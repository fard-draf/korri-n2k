//! Address supervisor built on top of [`AddressManager`] for Tokio runtime.
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull incoming traffic, unfiltered.
//!
//! This implementation uses `tokio::sync::mpsc` channels.

use core::fmt::Debug;
use core::future::poll_fn;
use core::task::{Context, Poll};
use futures_util::{future::select, future::Either, pin_mut};
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::error::ClaimFault;
use crate::infra::codec::traits::PgnData;
use crate::protocol::management::address_claiming::engine::ClaimStatus;
use crate::protocol::management::address_supervisor::{
    handle_command, AddressHandleError, AddressSupervisorRunError, ClaimedAddress,
    SupervisorCommand,
};
use crate::protocol::management::{address_manager::AddressManager, iso_name::IsoName};
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;
use crate::protocol::transport::traits::can_bus::CanBus;
use crate::protocol::transport::traits::korri_timer::KorriTimer;

/// Service assembling the supervisor components.
///
/// `N` is the number of Controller Applications, taken from the
/// [`AddressManager`]. Each one gets its own handle and command channel; the
/// incoming frames channel is shared.
pub struct AddressService<'a, C: CanBus, T: KorriTimer, const N: usize = 1>
where
    C::Error: Debug,
{
    manager: AddressManager<'a, C, T, N>,
    command_rx: [Option<Receiver<SupervisorCommand>>; N],
    frame_tx: Option<Sender<CanFrame>>,
    handles: [Option<AddressHandle>; N],
    frames: Option<AddressFrames>,
    claimed: [Arc<ClaimedAddress>; N],
}

impl<'a, C, T> AddressService<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Convenience helper: build the manager and the service in one call.
    /// The claim itself happens under [`AddressRunner::drive`].
    pub fn with_name(
        can_bus: C,
        timer: T,
        my_name: IsoName,
        strategy: crate::protocol::management::address_claiming::AddressClaimStrategy<'a>,
        cmd_cap: usize,
        frame_cap: usize,
    ) -> Result<Self, ClaimFault> {
        let manager = AddressManager::new(can_bus, timer, my_name, strategy)?;
        Ok(Self::new(manager, cmd_cap, frame_cap))
    }

    /// Split into handle/receiver/runner components.
    pub fn into_parts(self) -> AddressServiceParts<'a, C, T> {
        let MultiAddressServiceParts {
            handles: [handle],
            frames,
            runner,
        } = self.into_multi_parts();
        AddressServiceParts {
            handle,
            frames,
            runner,
        }
    }
}

impl<'a, C, T, const N: usize> AddressService<'a, C, T, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Wrap an already-initialised [`AddressManager`].
    ///
    /// `cmd_cap` sizes the command channel of **each** Controller Application.
    pub fn new(manager: AddressManager<'a, C, T, N>, cmd_cap: usize, frame_cap: usize) -> Self {
        let claimed: [Arc<ClaimedAddress>; N] =
            core::array::from_fn(|_| Arc::new(ClaimedAddress::new()));

        let mut handles: [Option<AddressHandle>; N] = core::array::from_fn(|_| None);
        let command_rx = core::array::from_fn(|index| {
            if cmd_cap == 0 {
                return None;
            }
            let (tx, rx) = channel(cmd_cap);
            handles[index] = Some(AddressHandle {
                sender: tx,
                claimed: Arc::clone(&claimed[index]),
            });
            Some(rx)
        });

        let (frame_tx, frame_rx) = if frame_cap > 0 {
            let (tx, rx) = channel(frame_cap);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        Self {
            manager,
            command_rx,
            frame_tx,
            handles,
            frames: frame_rx.map(|rx| AddressFrames { receiver: rx }),
            claimed,
        }
    }

    /// Split into one handle per Controller Application, in the order given to
    /// [`AddressManager::with_controllers`], the shared receiver, and the runner.
    pub fn into_multi_parts(self) -> MultiAddressServiceParts<'a, C, T, N> {
        MultiAddressServiceParts {
            handles: self.handles,
            frames: self.frames,
            runner: AddressRunner {
                manager: self.manager,
                command_rx: self.command_rx,
                frame_tx: self.frame_tx,
                claimed: self.claimed,
            },
        }
    }
}

/// Bundle returned by [`AddressService::into_parts`].
pub struct AddressServiceParts<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    pub handle: Option<AddressHandle>,
    pub frames: Option<AddressFrames>,
    pub runner: AddressRunner<'a, C, T>,
}

/// Bundle returned by [`AddressService::into_multi_parts`].
pub struct MultiAddressServiceParts<'a, C, T, const N: usize>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// One per Controller Application, in construction order.
    pub handles: [Option<AddressHandle>; N],
    pub frames: Option<AddressFrames>,
    pub runner: AddressRunner<'a, C, T, N>,
}

/// Runner that drives the supervisor loop.
pub struct AddressRunner<'a, C, T, const N: usize = 1>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    manager: AddressManager<'a, C, T, N>,
    command_rx: [Option<Receiver<SupervisorCommand>>; N],
    frame_tx: Option<Sender<CanFrame>>,
    claimed: [Arc<ClaimedAddress>; N],
}

impl<'a, C, T, const N: usize> AddressRunner<'a, C, T, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Own the single `select` of the supervisor: wait, poll the engines
    /// synchronously, execute, come back. No transition is ever cancelled.
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        let mut rx: Option<CanFrame> = None;
        // `rx` is a local emission looped back, not bus traffic.
        let mut local = false;
        let mut loopback: [Option<CanFrame>; N] = [None; N];
        // Rotates so one busy Controller Application cannot starve the others.
        let mut next_command = 0usize;

        loop {
            let outputs = self.manager.poll_all(rx.as_ref());

            // Published before the emission below: a lost address must stop
            // producers now, not after an `await` on a possibly slow bus.
            for (claimed, output) in self.claimed.iter().zip(&outputs) {
                claimed.set(Some(output.status));
            }

            // The engines saw it first; the application gets it too, unfiltered.
            // This `take` is also the "rx = None once consumed" the engine expects.
            // A loopback was already forwarded when it was sent.
            if let Some(frame) = rx.take() {
                if !local {
                    forward_frame(&mut self.frame_tx, frame);
                }
            }

            let frame_tx = &mut self.frame_tx;
            let wake_at_ms = self
                .manager
                .emit_all(&outputs, &mut loopback, &mut |frame| {
                    forward_local(frame_tx, frame, N)
                })
                .await
                .map_err(AddressSupervisorRunError::Send)?;

            // A sibling's claim reaches the engines before the bus is read
            // again, as the bus would have shown it to another device.
            if let Some(frame) = loopback.iter_mut().find_map(Option::take) {
                rx = Some(frame);
                local = true;
                continue;
            }
            local = false;

            let mut command = None;
            let mut closed_channel = None;

            // Scoped: both futures borrow `self` and must die before
            // `command_rx` is cleared or a command is executed.
            {
                let command_rx = &mut self.command_rx;
                let command_future = poll_fn(|cx| poll_commands(command_rx, next_command, cx));
                pin_mut!(command_future);

                // `None` waits on the bus and on commands alone. It never means
                // "done": a conflict must still be able to wake the engines.
                let recv = self.manager.recv_until(wake_at_ms);
                pin_mut!(recv);

                // `select` polls its first argument first: a frame ready at
                // the same instant as the deadline wins, as the engine wants.
                match select(recv, command_future).await {
                    Either::Left((frame, _)) => {
                        rx = frame.map_err(AddressSupervisorRunError::Receive)?;
                    }
                    Either::Right(((index, received), _)) => match received {
                        Some(cmd) => command = Some((index, cmd)),
                        None => closed_channel = Some(index),
                    },
                }
            }

            // A closed channel returns `None` instantly and forever:
            // drop it so the next round no longer polls it.
            if let Some(index) = closed_channel {
                self.command_rx[index] = None;
            }

            if let Some((index, cmd)) = command {
                next_command = (index + 1) % N;
                let frame_tx = &mut self.frame_tx;
                rx = handle_command(&mut self.manager, index, cmd, &mut |frame| {
                    forward_local(frame_tx, frame, N)
                })
                .await?;
                local = rx.is_some();
            }
        }
    }
}

/// Hand the frame to the application, or drop it.
///
/// Never awaits: a full channel must not delay the action the engine just
/// decided. Application traffic is sacrificial, address management is not.
fn forward_frame(frame_tx: &mut Option<Sender<CanFrame>>, frame: CanFrame) {
    let Some(tx) = frame_tx.as_ref() else {
        return;
    };
    match tx.try_send(frame) {
        Ok(()) => {}
        // dropped silently. A count belongs on `AddressFrames`,
        // where the consumer that cares about the gap actually lives.
        Err(TrySendError::Full(_)) => {}
        Err(TrySendError::Closed(_)) => *frame_tx = None,
    }
}

/// A local emission is bus traffic for the siblings' applications, which share
/// `AddressFrames`. A lone Controller Application already knows what it sent.
fn forward_local(frame_tx: &mut Option<Sender<CanFrame>>, frame: &CanFrame, controllers: usize) {
    if controllers > 1 {
        forward_frame(frame_tx, *frame);
    }
}

/// First command ready, scanning from `start`, with the index of its channel.
/// `(index, None)` reports a closed channel. Never ready without a channel.
fn poll_commands(
    command_rx: &mut [Option<Receiver<SupervisorCommand>>],
    start: usize,
    cx: &mut Context<'_>,
) -> Poll<(usize, Option<SupervisorCommand>)> {
    let count = command_rx.len();
    for offset in 0..count {
        let index = (start + offset) % count;
        if let Some(rx) = command_rx[index].as_mut() {
            if let Poll::Ready(received) = rx.poll_recv(cx) {
                return Poll::Ready((index, received));
            }
        }
    }
    Poll::Pending
}

/// Clear the published addresses when the runner goes away.
///
/// The runner is the only thing defending those addresses. Once it is gone,
/// nobody answers a competing claim, so a handle still reporting `Some(142)`
/// would invite the application to emit from an address it no longer holds.
///
/// `Drop` rather than a cleanup at the end of `drive`: it also covers the task
/// being cancelled and the future being dropped, which no `?` can catch.
impl<'a, C, T, const N: usize> Drop for AddressRunner<'a, C, T, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    fn drop(&mut self) {
        for claimed in &self.claimed {
            claimed.set(None);
        }
    }
}

/// Transmission handle (optional).
pub struct AddressHandle {
    sender: Sender<SupervisorCommand>,
    claimed: Arc<ClaimedAddress>,
}

impl AddressHandle {
    /// The current claim state, or `None` before the runner starts or after it stops.
    ///
    /// Best effort: see [`ClaimedAddress`].
    pub fn claim_status(&self) -> Option<ClaimStatus> {
        self.claimed.status()
    }

    /// The address this handle emits from, or `None` while none is held.
    ///
    /// Best effort: see [`ClaimedAddress`].
    pub fn claimed_address(&self) -> Option<u8> {
        self.claimed.get()
    }

    /// Queue a command built by the caller. Same contract as
    /// [`AddressHandle::send_pgn`]: it confirms queueing, not emission.
    pub async fn send_command(&self, command: SupervisorCommand) -> Result<(), AddressHandleError> {
        self.sender
            .send(command)
            .await
            .map_err(|_| AddressHandleError::RunnerGone)
    }

    /// Queue a frame. Returns once the runner has taken it from the channel,
    /// not once it reached the bus: see [`AddressHandle::send_pgn`].
    pub async fn send_raw_frame(&self, frame: &CanFrame) -> Result<(), AddressHandleError> {
        let command = SupervisorCommand::SendRawFrame(*frame);
        self.sender
            .send(command)
            .await
            .map_err(|_| AddressHandleError::RunnerGone)
    }

    /// Queue a PGN for the runner to emit.
    ///
    /// **This confirms queueing, not emission.** The runner may still refuse the
    /// command: no address acquired, or a conflict between now and execution.
    /// A refusal is dropped, not returned here. Check
    /// [`AddressHandle::claimed_address`] before queueing if that matters.
    pub async fn send_pgn<P: PgnData>(
        &self,
        pgn_data: &P,
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
    ) -> Result<(), AddressHandleError> {
        // Serialized straight into the command: a second buffer would cost
        // another 223 bytes of stack and a copy, for nothing.
        let mut payload = [0u8; MAX_FAST_PACKET_PAYLOAD];
        let len = pgn_data
            .to_payload(&mut payload)
            .map_err(|_| AddressHandleError::Serialization)?;

        let command = SupervisorCommand::SendPayload {
            pgn,
            priority,
            destination,
            len,
            payload,
        };

        self.sender
            .send(command)
            .await
            .map_err(|_| AddressHandleError::RunnerGone)
    }
}

/// Optional receiver returning incoming frames.
///
/// Nothing is filtered out. Address-claim traffic is included, so network
/// discovery can read it too. A frame is dropped rather than queued when this
/// channel is full: address management is never delayed by a slow consumer.
pub struct AddressFrames {
    receiver: Receiver<CanFrame>,
}

impl AddressFrames {
    pub async fn recv(&mut self) -> Option<CanFrame> {
        self.receiver.recv().await
    }
}

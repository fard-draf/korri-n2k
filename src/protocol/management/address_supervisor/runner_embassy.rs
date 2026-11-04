//! Address supervisor built on top of [`AddressManager`].
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull incoming traffic, unfiltered.
//!
//! Firmware decides which features it needs by providing pre-allocated
//! `embassy_sync::channel::Channel` instances. No allocation is performed by the
//! library and there is no dependency on a particular BSP.

use core::fmt::Debug;
use core::future::poll_fn;
use core::task::{Context, Poll};

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};
use futures_util::{future::select, future::Either, pin_mut};

use crate::error::ClaimFault;
use crate::infra::codec::traits::PgnData;
use crate::protocol::management::address_claiming::{engine::ClaimStatus, AddressClaimStrategy};
use crate::protocol::management::address_manager::AddressManager;
use crate::protocol::management::address_supervisor::{
    handle_command, AddressHandleError, AddressSupervisorRunError, ClaimedAddress,
    SupervisorCommand,
};
use crate::protocol::management::iso_name::IsoName;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;
use crate::protocol::transport::traits::can_bus::CanBus;
use crate::protocol::transport::traits::korri_timer::KorriTimer;

type CommandChannel<'a, const CMD_CAP: usize> =
    &'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>;
type FrameChannel<'a, const FRAME_CAP: usize> =
    &'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>;

/// Service assembling the supervisor components.
///
/// `N` is the number of Controller Applications, taken from the
/// [`AddressManager`]. Each one gets its own command channel and
/// [`ClaimedAddress`]; the incoming frames channel is shared.
pub struct AddressService<
    'a,
    C: CanBus,
    T: KorriTimer,
    const CMD_CAP: usize,
    const FRAME_CAP: usize,
    const N: usize = 1,
> where
    C::Error: Debug,
{
    manager: AddressManager<'a, C, T, N>,
    command_channels: [Option<CommandChannel<'a, CMD_CAP>>; N],
    frame_channel: Option<FrameChannel<'a, FRAME_CAP>>,
    claimed: [&'a ClaimedAddress; N],
}

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
    AddressService<'a, C, T, CMD_CAP, FRAME_CAP>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Wrap an already-initialised [`AddressManager`].
    /// `claimed` is a `static` you own: no allocation happens in the library.
    /// One per Controller Application.
    pub fn new(
        manager: AddressManager<'a, C, T>,
        command_channel: Option<CommandChannel<'a, CMD_CAP>>,
        frame_channel: Option<FrameChannel<'a, FRAME_CAP>>,
        claimed: &'a ClaimedAddress,
    ) -> Self {
        Self::with_controllers(manager, [command_channel], frame_channel, [claimed])
    }

    /// Convenience helper: build the manager and the service in one call.
    /// The claim itself happens under [`AddressRunner::drive`].
    pub fn with_name(
        can_bus: C,
        timer: T,
        my_name: IsoName,
        strategy: AddressClaimStrategy<'a>,
        command_channel: Option<CommandChannel<'a, CMD_CAP>>,
        frame_channel: Option<FrameChannel<'a, FRAME_CAP>>,
        claimed: &'a ClaimedAddress,
    ) -> Result<Self, ClaimFault> {
        let manager = AddressManager::new(can_bus, timer, my_name, strategy)?;
        Ok(Self::new(manager, command_channel, frame_channel, claimed))
    }

    /// Split into handle/receiver/runner components.
    pub fn into_parts(self) -> AddressServiceParts<'a, C, T, CMD_CAP, FRAME_CAP> {
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

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize, const N: usize>
    AddressService<'a, C, T, CMD_CAP, FRAME_CAP, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Wrap a manager holding `N` Controller Applications.
    ///
    /// Index `i` of `command_channels` and `claimed` belongs to the `i`-th
    /// controller given to [`AddressManager::with_controllers`].
    ///
    /// # Panics
    ///
    /// If two Controller Applications share a command channel or a
    /// `ClaimedAddress`. One would emit from the other's address, or report it.
    /// A wiring mistake, caught at start-up rather than on the bus.
    pub fn with_controllers(
        manager: AddressManager<'a, C, T, N>,
        command_channels: [Option<CommandChannel<'a, CMD_CAP>>; N],
        frame_channel: Option<FrameChannel<'a, FRAME_CAP>>,
        claimed: [&'a ClaimedAddress; N],
    ) -> Self {
        for index in 0..N {
            for other in 0..index {
                assert!(
                    !core::ptr::eq(claimed[index], claimed[other]),
                    "each Controller Application needs its own ClaimedAddress"
                );
                if let (Some(a), Some(b)) = (command_channels[index], command_channels[other]) {
                    assert!(
                        !core::ptr::eq(a, b),
                        "each Controller Application needs its own command channel"
                    );
                }
            }
        }
        Self {
            manager,
            command_channels,
            frame_channel,
            claimed,
        }
    }

    /// Split into one handle per Controller Application, in construction
    /// order, the shared receiver, and the runner.
    pub fn into_multi_parts(self) -> MultiAddressServiceParts<'a, C, T, CMD_CAP, FRAME_CAP, N> {
        let handles = core::array::from_fn(|index| {
            self.command_channels[index].map(|channel| AddressHandle {
                sender: channel.sender(),
                claimed: self.claimed[index],
            })
        });
        let frames = self.frame_channel.map(|channel| AddressFrames {
            receiver: channel.receiver(),
        });
        MultiAddressServiceParts {
            handles,
            frames,
            runner: AddressRunner {
                manager: self.manager,
                command_channels: self.command_channels,
                frame_channel: self.frame_channel,
                claimed: self.claimed,
            },
        }
    }
}

/// Bundle returned by [`AddressService::into_parts`].
pub struct AddressServiceParts<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    pub handle: Option<AddressHandle<'a, CMD_CAP>>,
    pub frames: Option<AddressFrames<'a, FRAME_CAP>>,
    pub runner: AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP>,
}

/// Bundle returned by [`AddressService::into_multi_parts`].
pub struct MultiAddressServiceParts<
    'a,
    C,
    T,
    const CMD_CAP: usize,
    const FRAME_CAP: usize,
    const N: usize,
> where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// One per Controller Application, in construction order.
    pub handles: [Option<AddressHandle<'a, CMD_CAP>>; N],
    pub frames: Option<AddressFrames<'a, FRAME_CAP>>,
    pub runner: AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP, N>,
}

/// Runner that drives the supervisor loop.
pub struct AddressRunner<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize, const N: usize = 1>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    manager: AddressManager<'a, C, T, N>,
    command_channels: [Option<CommandChannel<'a, CMD_CAP>>; N],
    frame_channel: Option<FrameChannel<'a, FRAME_CAP>>,
    claimed: [&'a ClaimedAddress; N],
}

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize, const N: usize>
    AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Own the single `select` of the supervisor: wait, poll the engines
    /// synchronously, execute, come back. No transition is ever cancelled.
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        let frame_channel = self.frame_channel;
        let command_channels = self.command_channels;
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
            // Never awaits: a full channel must not delay the action the engine
            // just decided. Application traffic is sacrificial, address
            // management is not.
            if let Some(frame) = rx.take() {
                if let (false, Some(frame_ch)) = (local, frame_channel) {
                    // dropped silently. A count belongs on
                    // `AddressFrames`, where the consumer that cares lives.
                    let _ = frame_ch.try_send(frame);
                }
            }

            let wake_at_ms = self
                .manager
                .emit_all(&outputs, &mut loopback, &mut |frame| {
                    forward_local(frame_channel, frame, N)
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

            // Scoped: both futures borrow `self` and must die before
            // a command is executed.
            {
                // An embassy channel never closes: absent means never ready.
                let command_future =
                    poll_fn(|cx| poll_commands(&command_channels, next_command, cx));
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
                    Either::Right((received, _)) => command = Some(received),
                }
            }

            if let Some((index, cmd)) = command {
                next_command = (index + 1) % N;
                rx = handle_command(&mut self.manager, index, cmd, &mut |frame| {
                    forward_local(frame_channel, frame, N)
                })
                .await?;
                local = rx.is_some();
            }
        }
    }
}

/// A local emission is bus traffic for the siblings' applications, which share
/// `AddressFrames`. A lone Controller Application already knows what it sent.
/// Never awaits, like any forwarding.
fn forward_local<const FRAME_CAP: usize>(
    frame_channel: Option<FrameChannel<'_, FRAME_CAP>>,
    frame: &CanFrame,
    controllers: usize,
) {
    if let (true, Some(channel)) = (controllers > 1, frame_channel) {
        let _ = channel.try_send(*frame);
    }
}

/// First command ready, scanning from `start`, with the index of its channel.
/// Never ready without a channel.
fn poll_commands<const CMD_CAP: usize>(
    command_channels: &[Option<CommandChannel<'_, CMD_CAP>>],
    start: usize,
    cx: &mut Context<'_>,
) -> Poll<(usize, SupervisorCommand)> {
    let count = command_channels.len();
    for offset in 0..count {
        let index = (start + offset) % count;
        if let Some(channel) = command_channels[index] {
            if let Poll::Ready(command) = channel.poll_receive(cx) {
                return Poll::Ready((index, command));
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
impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize, const N: usize> Drop
    for AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP, N>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    fn drop(&mut self) {
        for claimed in self.claimed {
            claimed.set(None);
        }
    }
}

/// Transmission handle (optional).
pub struct AddressHandle<'a, const CMD_CAP: usize> {
    sender: Sender<'a, CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>,
    claimed: &'a ClaimedAddress,
}

impl<'a, const CMD_CAP: usize> AddressHandle<'a, CMD_CAP> {
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
    pub async fn send_command(&self, command: SupervisorCommand) {
        self.sender.send(command).await;
    }

    /// Queue a frame. Returns once the runner has taken it from the channel,
    /// not once it reached the bus: see [`AddressHandle::send_pgn`].
    pub async fn send_raw_frame(&self, frame: &CanFrame) {
        self.sender
            .send(SupervisorCommand::SendRawFrame(*frame))
            .await;
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

        self.sender.send(command).await;
        Ok(())
    }
}

/// Optional receiver returning incoming frames.
///
/// Nothing is filtered out. Address-claim traffic is included, so network
/// discovery can read it too. A frame is dropped rather than queued when this
/// channel is full: address management is never delayed by a slow consumer.
pub struct AddressFrames<'a, const FRAME_CAP: usize> {
    receiver: Receiver<'a, CriticalSectionRawMutex, CanFrame, FRAME_CAP>,
}

impl<'a, const FRAME_CAP: usize> AddressFrames<'a, FRAME_CAP> {
    pub async fn recv(&mut self) -> CanFrame {
        self.receiver.receive().await
    }
}

//! Address supervisor built on top of [`AddressManager`].
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull application traffic filtered by the manager.
//!
//! Firmware decides which features it needs by providing pre-allocated
//! `embassy_sync::channel::Channel` instances. No allocation is performed by the
//! library and there is no dependency on a particular BSP.

use core::fmt::Debug;
use core::future::pending;

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};
use futures_util::{future::select, future::Either, pin_mut};

use crate::error::{ClaimFault, SendPgnError};
use crate::infra::codec::traits::PgnData;
use crate::protocol::management::address_claiming::engine::ClaimAction;
use crate::protocol::management::address_claiming::AddressClaimStrategy;
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

/// Service assembling the supervisor components.
pub struct AddressService<
    'a,
    C: CanBus,
    T: KorriTimer,
    const CMD_CAP: usize,
    const FRAME_CAP: usize,
> where
    C::Error: Debug,
{
    manager: AddressManager<'a, C, T>,
    command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
    frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
    claimed: &'a ClaimedAddress,
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
        command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
        frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
        claimed: &'a ClaimedAddress,
    ) -> Self {
        Self {
            manager,
            command_channel,
            frame_channel,
            claimed,
        }
    }

    /// Convenience helper: build the manager and the service in one call.
    /// The claim itself happens under [`AddressRunner::drive`].
    pub fn with_name(
        can_bus: C,
        timer: T,
        my_name: IsoName,
        strategy: AddressClaimStrategy<'a>,
        command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
        frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
        claimed: &'a ClaimedAddress,
    ) -> Result<Self, ClaimFault> {
        let manager = AddressManager::new(can_bus, timer, my_name, strategy)?;
        Ok(Self::new(manager, command_channel, frame_channel, claimed))
    }

    /// Split into handle/receiver/runner components.
    pub fn into_parts(self) -> AddressServiceParts<'a, C, T, CMD_CAP, FRAME_CAP> {
        let handle = self.command_channel.map(|channel| AddressHandle {
            sender: channel.sender(),
            claimed: self.claimed,
        });
        let frames = self.frame_channel.map(|channel| AddressFrames {
            receiver: channel.receiver(),
        });
        AddressServiceParts {
            handle,
            frames,
            runner: AddressRunner {
                manager: self.manager,
                command_channel: self.command_channel,
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

/// Runner that drives the supervisor loop.
pub struct AddressRunner<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    manager: AddressManager<'a, C, T>,
    command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
    frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
    claimed: &'a ClaimedAddress,
}

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
    AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Own the single `select` of the supervisor: wait, poll the engine
    /// synchronously, execute, come back. No transition is ever cancelled.
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        let frame_channel = self.frame_channel;
        let command_channel = self.command_channel;
        let mut rx: Option<CanFrame> = None;

        loop {
            let action = self.manager.poll(rx.as_ref());
            self.claimed.set(self.manager.claimed_address());

            // The engine saw it first; the application gets it too, unfiltered.
            // This `take` is also the "rx = None after a Send" the engine expects.
            // Never awaits: a full channel must not delay the action the engine
            // just decided. Application traffic is sacrificial, address
            // management is not.
            if let Some(frame) = rx.take() {
                if let Some(frame_ch) = frame_channel {
                    // dropped silently. A count belongs on
                    // `AddressFrames`, where the consumer that cares lives.
                    let _ = frame_ch.try_send(frame);
                }
            }

            match action {
                ClaimAction::Send(frame) | ClaimAction::CannotClaim(frame) => {
                    self.manager
                        .emit_claim(&frame)
                        .await
                        .map_err(AddressSupervisorRunError::Send)?;
                }

                ClaimAction::Claimed(_) => {}

                ClaimAction::Wait(delay_ms) => {
                    let mut command = None;

                    // Scoped: both futures borrow `self` and must die before
                    // a command is executed.
                    {
                        // An embassy channel never closes: absent means never ready.
                        let command_future = async {
                            match command_channel {
                                Some(cmd_ch) => cmd_ch.receive().await,
                                None => pending::<SupervisorCommand>().await,
                            }
                        };
                        pin_mut!(command_future);

                        let recv = self.manager.recv_until(delay_ms);
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

                    if let Some(cmd) = command {
                        self.run_command(cmd).await?;
                    }
                }
            }
        }
    }

    /// Run one command. Only a dead bus stops the runner: a rejected command is
    /// the caller's mistake and must not take address management down with it.
    async fn run_command(
        &mut self,
        command: SupervisorCommand,
    ) -> Result<(), AddressSupervisorRunError<C::Error>> {
        match handle_command(&mut self.manager, command).await {
            Ok(()) => Ok(()),
            Err(SendPgnError::Send(err)) => Err(AddressSupervisorRunError::Send(err)),
            Err(_rejected) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("supervisor command rejected");
                // dropped, and only visible under `defmt`. A rejection
                // channel when a consumer needs programmatic feedback.
                Ok(())
            }
        }
    }
}

/// Transmission handle (optional).
pub struct AddressHandle<'a, const CMD_CAP: usize> {
    sender: Sender<'a, CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>,
    claimed: &'a ClaimedAddress,
}

impl<'a, const CMD_CAP: usize> AddressHandle<'a, CMD_CAP> {
    /// The address this handle emits from, or `None` while none is held.
    ///
    /// Best effort: see [`ClaimedAddress`].
    pub fn claimed_address(&self) -> Option<u8> {
        self.claimed.get()
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
    /// command — no address acquired, or a conflict between now and execution —
    /// and a refusal is dropped, not returned here. Check
    /// [`AddressHandle::claimed_address`] before queueing if that matters.
    pub async fn send_pgn<P: PgnData>(
        &self,
        pgn_data: &P,
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
    ) -> Result<(), AddressHandleError> {
        let mut buffer = [0u8; MAX_FAST_PACKET_PAYLOAD];
        let len = pgn_data
            .to_payload(&mut buffer)
            .map_err(|_| AddressHandleError::Serialization)?;

        let mut payload = [0u8; MAX_FAST_PACKET_PAYLOAD];
        payload[..len].copy_from_slice(&buffer[..len]);

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

/// Optional receiver returning application frames filtered by the supervisor.
pub struct AddressFrames<'a, const FRAME_CAP: usize> {
    receiver: Receiver<'a, CriticalSectionRawMutex, CanFrame, FRAME_CAP>,
}

impl<'a, const FRAME_CAP: usize> AddressFrames<'a, FRAME_CAP> {
    pub async fn recv(&mut self) -> CanFrame {
        self.receiver.receive().await
    }
}

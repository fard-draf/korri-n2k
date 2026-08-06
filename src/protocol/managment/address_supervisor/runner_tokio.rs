//! Address supervisor built on top of [`AddressManager`] for Tokio runtime.
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull application traffic filtered by the manager.
//!
//! This implementation uses `tokio::sync::mpsc` channels.

use core::fmt::Debug;
use core::future::pending;
use futures_util::{future::select, future::Either, pin_mut};
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::error::{ClaimFault, SendPgnError};
use crate::infra::codec::traits::PgnData;
use crate::protocol::managment::address_claiming::engine::ClaimAction;
use crate::protocol::managment::address_supervisor::{
    handle_command, AddressHandleError, AddressSupervisorRunError, ClaimedAddress,
    SupervisorCommand,
};
use crate::protocol::managment::{address_manager::AddressManager, iso_name::IsoName};
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;
use crate::protocol::transport::traits::can_bus::CanBus;
use crate::protocol::transport::traits::korri_timer::KorriTimer;

/// Service assembling the supervisor components.
pub struct AddressService<'a, C: CanBus, T: KorriTimer>
where
    C::Error: Debug,
{
    manager: AddressManager<'a, C, T>,
    command_rx: Option<Receiver<SupervisorCommand>>,
    frame_tx: Option<Sender<CanFrame>>,
    handle: Option<AddressHandle>,
    frames: Option<AddressFrames>,
    claimed: Arc<ClaimedAddress>,
}

impl<'a, C, T> AddressService<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Wrap an already-initialised [`AddressManager`].
    pub fn new(manager: AddressManager<'a, C, T>, cmd_cap: usize, frame_cap: usize) -> Self {
        let (cmd_tx, cmd_rx) = if cmd_cap > 0 {
            let (tx, rx) = channel(cmd_cap);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let (frame_tx, frame_rx) = if frame_cap > 0 {
            let (tx, rx) = channel(frame_cap);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let claimed = Arc::new(ClaimedAddress::new());

        Self {
            manager,
            command_rx: cmd_rx,
            frame_tx,
            handle: cmd_tx.map(|tx| AddressHandle {
                sender: tx,
                claimed: Arc::clone(&claimed),
            }),
            frames: frame_rx.map(|rx| AddressFrames { receiver: rx }),
            claimed,
        }
    }

    /// Convenience helper: build the manager and the service in one call.
    /// The claim itself happens under [`AddressRunner::drive`].
    pub fn with_name(
        can_bus: C,
        timer: T,
        my_name: IsoName,
        strategy: crate::protocol::managment::address_claiming::AddressClaimStrategy<'a>,
        cmd_cap: usize,
        frame_cap: usize,
    ) -> Result<Self, ClaimFault> {
        let manager = AddressManager::new(can_bus, timer, my_name, strategy)?;
        Ok(Self::new(manager, cmd_cap, frame_cap))
    }

    /// Split into handle/receiver/runner components.
    pub fn into_parts(self) -> AddressServiceParts<'a, C, T> {
        AddressServiceParts {
            handle: self.handle,
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

/// Runner that drives the supervisor loop.
pub struct AddressRunner<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    manager: AddressManager<'a, C, T>,
    command_rx: Option<Receiver<SupervisorCommand>>,
    frame_tx: Option<Sender<CanFrame>>,
    claimed: Arc<ClaimedAddress>,
}

impl<'a, C, T> AddressRunner<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Own the single `select` of the supervisor: wait, poll the engine
    /// synchronously, execute, come back. No transition is ever cancelled.
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        let mut rx: Option<CanFrame> = None;

        loop {
            let action = self.manager.poll(rx.as_ref());
            self.claimed.set(self.manager.claimed_address());

            // The engine saw it first; the application gets it too, unfiltered.
            // This `take` is also the "rx = None after a Send" the engine expects.
            if let Some(frame) = rx.take() {
                self.forward_frame(frame).await;
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
                    let mut channel_closed = false;

                    // Scoped: both futures borrow `self` and must die before
                    // `command_rx` is cleared or a command is executed.
                    {
                        let command_rx = &mut self.command_rx;
                        let command_future = async {
                            match command_rx {
                                Some(rx) => rx.recv().await,
                                None => pending::<Option<SupervisorCommand>>().await,
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
                            Either::Right((received, _)) => match received {
                                Some(cmd) => command = Some(cmd),
                                None => channel_closed = true,
                            },
                        }
                    }

                    // A closed channel returns `None` instantly and forever:
                    // drop it so the next round falls back on `pending()`.
                    if channel_closed {
                        self.command_rx = None;
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
                // ponytail: reported then dropped; add a rejection channel when a
                // consumer needs programmatic feedback.
                Ok(())
            }
        }
    }

    async fn forward_frame(&mut self, frame: CanFrame) {
        let closed = match self.frame_tx {
            Some(ref tx) => tx.send(frame).await.is_err(),
            None => false,
        };
        if closed {
            self.frame_tx = None;
        }
    }
}

/// Transmission handle (optional).
pub struct AddressHandle {
    sender: Sender<SupervisorCommand>,
    claimed: Arc<ClaimedAddress>,
}

impl AddressHandle {
    /// The address this handle emits from, or `None` while none is held.
    ///
    /// Best effort: see [`ClaimedAddress`].
    pub fn claimed_address(&self) -> Option<u8> {
        self.claimed.get()
    }

    pub async fn send_raw_frame(&self, frame: &CanFrame) {
        let command = SupervisorCommand::SendRawFrame(frame.clone());
        // Fire-and-forget: if the runner is gone the frame is silently dropped.
        let _ = self.sender.send(command).await;
    }

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

        // Fire-and-forget: if the runner is gone the frame is silently dropped.
        let _ = self.sender.send(command).await;
        Ok(())
    }
}

/// Optional receiver returning application frames filtered by the supervisor.
pub struct AddressFrames {
    receiver: Receiver<CanFrame>,
}

impl AddressFrames {
    pub async fn recv(&mut self) -> Option<CanFrame> {
        self.receiver.recv().await
    }
}

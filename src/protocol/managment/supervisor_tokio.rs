//! Address supervisor built on top of [`AddressManager`] for Tokio runtime.
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull application traffic filtered by the manager.
//!
//! This implementation uses `tokio::sync::mpsc` channels.

use core::fmt::Debug;
use futures_util::{future::select, future::Either, pin_mut};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::error::{AddressManagerError, ClaimError, SendPgnError};
use crate::infra::codec::traits::PgnData;
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

        Self {
            manager,
            command_rx: cmd_rx,
            frame_tx,
            handle: cmd_tx.map(|tx| AddressHandle { sender: tx }),
            frames: frame_rx.map(|rx| AddressFrames { receiver: rx }),
        }
    }

    /// Convenience helper: claim an address then build the service.
    pub async fn claim(
        can_bus: C,
        timer: T,
        my_name: IsoName,
        strategy: crate::protocol::managment::address_claiming::AddressClaimStrategy<'a>,
        cmd_cap: usize,
        frame_cap: usize,
    ) -> Result<Self, ClaimError<C::Error>> {
        let manager = AddressManager::new(can_bus, timer, my_name, strategy).await?;
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
}

impl<'a, C, T> AddressRunner<'a, C, T>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        loop {
            let mut channel_closed = false;

            if let Some(ref mut cmd_rx) = self.command_rx {
                let mut command_to_process = None;
                let mut frame_to_forward = None;
                let mut recv_error = None;

                {
                    let cmd_future = cmd_rx.recv();
                    let recv_future = self.manager.recv();
                    pin_mut!(cmd_future);
                    pin_mut!(recv_future);

                    match select(recv_future, cmd_future).await {
                        Either::Left((result, _pending_cmd)) => match result {
                            Ok(Some(frame)) => frame_to_forward = Some(frame),
                            Ok(None) => {}
                            Err(err) => recv_error = Some(err),
                        },
                        Either::Right((command, _pending_recv)) => {
                            if let Some(cmd) = command {
                                command_to_process = Some(cmd);
                            } else {
                                channel_closed = true;
                            }
                        }
                    }
                }

                if channel_closed {
                    self.command_rx = None;
                }

                if let Some(err) = recv_error {
                    return Err(AddressSupervisorRunError::Receive(err));
                }

                if let Some(frame) = frame_to_forward {
                    self.forward_frame(frame).await;
                }

                if let Some(command) = command_to_process {
                    handle_command(&mut self.manager, command).await?;
                }
            } else {
                let result = self.manager.recv().await;
                match result {
                    Ok(Some(frame)) => self.forward_frame(frame).await,
                    Ok(None) => {}
                    Err(err) => return Err(AddressSupervisorRunError::Receive(err)),
                }
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
}

impl AddressHandle {
    pub async fn send_frame(&self, frame: &CanFrame) {
        let command = SupervisorCommand::SendFrame(frame.clone());
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

/// Commands queued by producer tasks.
// No alloc, no box!
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum SupervisorCommand {
    SendFrame(CanFrame),
    SendPayload {
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
        len: usize,
        payload: [u8; MAX_FAST_PACKET_PAYLOAD],
    },
}

#[derive(Debug)]
pub enum AddressHandleError {
    Serialization,
}

#[derive(Debug)]
pub enum AddressSupervisorRunError<E: Debug> {
    Receive(E),
    Send(E),
    SendPgn(SendPgnError<E>),
}

async fn handle_command<'a, C: CanBus, T: KorriTimer>(
    manager: &mut AddressManager<'a, C, T>,
    command: SupervisorCommand,
) -> Result<(), AddressSupervisorRunError<C::Error>>
where
    C::Error: Debug,
{
    match command {
        SupervisorCommand::SendFrame(frame) => manager
            .send(&frame)
            .await
            .map_err(AddressSupervisorRunError::Send),
        SupervisorCommand::SendPayload {
            pgn,
            priority,
            destination,
            len,
            payload,
        } => manager
            .send_payload(pgn, priority, destination, &payload[..len])
            .await
            .map_err(AddressSupervisorRunError::SendPgn),
    }
}

//! Address supervisor built on top of [`AddressManager`].
//!
//! It keeps the claiming state-machine alive and optionally offers:
//!
//! * a transmission handle (`AddressHandle`) to queue frames/PGNs;
//! * a frame receiver (`AddressFrames`) to pull application traffic filtered by the manager.
//!
//! Firmware decides which features it needs by providing pre-allocated
//! [`embassy_sync::Channel`] instances. No allocation is performed by the
//! library and there is no dependency on a particular BSP.

use core::fmt::Debug;

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};
use futures_util::{future::select, future::Either, pin_mut};

use crate::error::{ClaimError, SendPgnError};
use crate::infra::codec::traits::PgnData;
use crate::protocol::managment::address_manager::AddressManager;
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
    manager: AddressManager<C, T>,
    command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
    frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
}

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
    AddressService<'a, C, T, CMD_CAP, FRAME_CAP>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    /// Wrap an already-initialised [`AddressManager`].
    pub fn new(
        manager: AddressManager<C, T>,
        command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
        frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
    ) -> Self {
        Self {
            manager,
            command_channel,
            frame_channel,
        }
    }

    /// Convenience helper: claim an address then build the service.
    pub async fn claim(
        can_bus: C,
        timer: T,
        my_name: u64,
        preferred_address: u8,
        command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
        frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
    ) -> Result<Self, ClaimError<C::Error>> {
        let manager = AddressManager::new(can_bus, timer, my_name, preferred_address).await?;
        Ok(Self::new(manager, command_channel, frame_channel))
    }

    /// Split into handle/receiver/runner components.
    pub fn into_parts(self) -> AddressServiceParts<'a, C, T, CMD_CAP, FRAME_CAP> {
        let handle = self.command_channel.map(|channel| AddressHandle {
            sender: channel.sender(),
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
    manager: AddressManager<C, T>,
    command_channel: Option<&'a Channel<CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>>,
    frame_channel: Option<&'a Channel<CriticalSectionRawMutex, CanFrame, FRAME_CAP>>,
}

impl<'a, C, T, const CMD_CAP: usize, const FRAME_CAP: usize>
    AddressRunner<'a, C, T, CMD_CAP, FRAME_CAP>
where
    C: CanBus,
    C::Error: Debug,
    T: KorriTimer,
{
    pub async fn drive(mut self) -> Result<(), AddressSupervisorRunError<C::Error>> {
        let frame_channel = self.frame_channel;
        let command_channel = self.command_channel;

        loop {
            match command_channel {
                Some(cmd_ch) => {
                    let mut command_to_process = None;
                    let mut frame_to_forward = None;
                    let mut recv_error = None;

                    {
                        let cmd_future = cmd_ch.receive();
                        let recv_future = self.manager.recv();
                        pin_mut!(cmd_future);
                        pin_mut!(recv_future);

                        match select(recv_future, cmd_future).await {
                            Either::Left((result, pending_cmd)) => {
                                match result {
                                    Ok(Some(frame)) => frame_to_forward = Some(frame),
                                    Ok(None) => {}
                                    Err(err) => recv_error = Some(err),
                                }
                                drop(pending_cmd);
                            }
                            Either::Right((command, pending_recv)) => {
                                command_to_process = Some(command);
                                drop(pending_recv);
                            }
                        }
                    }

                    if let Some(err) = recv_error {
                        return Err(AddressSupervisorRunError::Receive(err));
                    }

                    if let Some(frame) = frame_to_forward {
                        if let Some(frame_ch) = frame_channel {
                            frame_ch.send(frame).await;
                        }
                    }

                    if let Some(command) = command_to_process {
                        handle_command(&mut self.manager, command).await?;
                    }
                }
                None => {
                    let result = self.manager.recv().await;
                    match result {
                        Ok(Some(frame)) => {
                            if let Some(frame_ch) = frame_channel {
                                frame_ch.send(frame).await;
                            }
                        }
                        Ok(None) => {}
                        Err(err) => return Err(AddressSupervisorRunError::Receive(err)),
                    }
                }
            }
        }
    }
}

/// Transmission handle (optional).
pub struct AddressHandle<'a, const CMD_CAP: usize> {
    sender: Sender<'a, CriticalSectionRawMutex, SupervisorCommand, CMD_CAP>,
}

impl<'a, const CMD_CAP: usize> AddressHandle<'a, CMD_CAP> {
    pub async fn send_frame(&self, frame: &CanFrame) {
        let command = SupervisorCommand::SendFrame(frame.clone());
        self.sender.send(command).await;
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

/// Commands queued by producer tasks.
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

async fn handle_command<C: CanBus, T: KorriTimer>(
    manager: &mut AddressManager<C, T>,
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

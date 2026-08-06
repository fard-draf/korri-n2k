use core::fmt::Debug;

use crate::{
    error::SendPgnError,
    protocol::{
        managment::address_manager::AddressManager,
        transport::{
            can_frame::CanFrame,
            fast_packet::MAX_FAST_PACKET_PAYLOAD,
            traits::{can_bus::CanBus, korri_timer::KorriTimer},
        },
    },
};

#[cfg(all(feature = "embassy", not(feature = "tokio")))]
pub mod runner_embassy;

#[cfg(feature = "tokio")]
pub mod runner_tokio;

// One path whatever the runtime: only one runner is ever compiled in.
#[cfg(all(feature = "embassy", not(feature = "tokio")))]
pub use runner_embassy::{AddressFrames, AddressHandle, AddressRunner, AddressService};
#[cfg(feature = "tokio")]
pub use runner_tokio::{AddressFrames, AddressHandle, AddressRunner, AddressService};

/// Commands queued by producer tasks.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum SupervisorCommand {
    /// Emit a frame the caller built entirely, including its CAN identifier.
    /// Escape hatch for PGNs the codec does not know (proprietary, out-of-manifest).
    /// Rejected unless the frame's source address matches the claimed one.
    SendRawFrame(CanFrame),

    /// Emit a payload: the library fills the source address and the Fast Packet framing.
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

/// Failures that stop the runner. A rejected command is not one of them.
#[derive(Debug)]
pub enum AddressSupervisorRunError<E: Debug> {
    Receive(E),
    Send(E),
}

/// Execute one command. The error is returned to the runner, which decides
/// whether it is fatal (`Send`, the bus is gone) or merely a rejection.
async fn handle_command<'a, C: CanBus, T: KorriTimer>(
    manager: &mut AddressManager<'a, C, T>,
    command: SupervisorCommand,
) -> Result<(), SendPgnError<C::Error>>
where
    C::Error: Debug,
{
    match command {
        // The caller supplied the source address: it must still be ours.
        // A reclaim between build and send would make it someone else's.
        SupervisorCommand::SendRawFrame(frame) => match manager.claimed_address() {
            Some(claimed) if frame.id.source_address() == claimed => {
                manager.emit_claim(&frame).await.map_err(SendPgnError::Send)
            }
            _ => Err(SendPgnError::NotClaimed),
        },
        SupervisorCommand::SendPayload {
            pgn,
            priority,
            destination,
            len,
            payload,
        } => {
            manager
                .send_payload(pgn, priority, destination, &payload[..len])
                .await
        }
    }
}

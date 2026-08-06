use core::fmt::Debug;

use portable_atomic::{AtomicU8, Ordering};

use crate::{
    error::SendPgnError,
    protocol::{
        constants::address::NULL_ADDR_254,
        management::address_manager::AddressManager,
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
///
/// One entry costs 240 bytes whatever the payload: the buffer is inline, so no
/// allocation happens. Size the channel accordingly: `CMD_CAP = 8` reserves
/// about 2 KB.
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
    /// The PGN could not be written into the command buffer.
    Serialization,
    /// The runner is gone: nothing will ever execute this command.
    RunnerGone,
}

/// The address a handle emits from, shared with the runner that owns the engine.
///
/// **Best effort, not a lock.** Reading `Some(42)` then sending races a reclaim
/// that may happen in between; the command is refused in that case. The runner's
/// own guard stays the authority. This only lets a caller avoid asking for what
/// it knows will be refused.
///
/// One instance per Controller Application: it hangs off the handle, so a node
/// holding several NAMEs reads each address through its own handle.
pub struct ClaimedAddress(AtomicU8);

impl Default for ClaimedAddress {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaimedAddress {
    /// Starts addressless. `no_std` friendly: usable as a `static`.
    pub const fn new() -> Self {
        Self(AtomicU8::new(NULL_ADDR_254))
    }

    /// The current address, or `None` while none is held.
    pub fn get(&self) -> Option<u8> {
        match self.0.load(Ordering::Relaxed) {
            NULL_ADDR_254 => None,
            address => Some(address),
        }
    }

    /// Published by the runner after every engine step.
    pub(crate) fn set(&self, address: Option<u8>) {
        self.0
            .store(address.unwrap_or(NULL_ADDR_254), Ordering::Relaxed);
    }
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
            // `len` is a public field of the command, and under embassy the
            // caller owns the channel: an out-of-range value must not panic.
            let payload = payload.get(..len).ok_or(SendPgnError::Serialization)?;
            manager
                .send_payload(pgn, priority, destination, payload)
                .await
        }
    }
}

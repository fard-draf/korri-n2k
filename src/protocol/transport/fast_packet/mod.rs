//! NMEA 2000 Fast Packet support: encapsulates payloads larger than eight bytes
//! across successive CAN frames.
//!
//! The generated PGN tables are re-exported here: an assembler that does not know
//! which PGNs are multi-frame cannot do its job, so this dependency on `messages`
//! already exists conceptually.
pub use crate::protocol::messages::{FAST_PACKET_PGNS, FAST_PACKET_PGNS_ALL};

/// Maximum payload a Fast Packet can transport once reassembled.
pub const MAX_FAST_PACKET_PAYLOAD: usize = 223;

pub mod assembler;
pub mod builder;

#[cfg(test)]
pub mod tests;

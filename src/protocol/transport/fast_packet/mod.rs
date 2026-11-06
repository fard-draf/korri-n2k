//! NMEA 2000 Fast Packet support: encapsulates payloads larger than eight bytes
//! across successive CAN frames.
//!
//! Reassembly is not wired into the receive path: run [`assembler::FastPacketAssembler`]
//! yourself. Decoding the first fragment of a long PGN succeeds with wrong
//! values, so branch on `handles()`, never on the decode result.
//!
//! ```rust,ignore
//! let mut assembler = FastPacketAssembler::new();
//!
//! if !assembler.handles(pgn) {
//!     // Single frame: decode the eight bytes directly.
//! } else {
//!     // `process_frame` takes eight bytes and never sees the DLC: pad a short
//!     // frame (a last frame often is) with 0xFF rather than stale bytes.
//!     let mut data = [0xFF; 8];
//!     let len = frame.len.min(8);
//!     data[..len].copy_from_slice(&frame.data[..len]);
//!     if let ProcessResult::MessageComplete(msg) =
//!         assembler.process_frame(now_ms, pgn, frame.id.source_address(), &data)
//!     {
//!         let fix = Pgn129029::from_payload(&msg.payload[..msg.len])?;
//!     }
//! }
//! ```
//!
//! `new()` covers the Fast Packet PGNs of your manifest. A gateway forwarding
//! payloads it cannot decode wants `FastPacketAssembler::with_pgns(FAST_PACKET_PGNS_ALL)`
//! (`full-pgns` is not a substitute: 152 of CANboat's 182). Pass your own sorted
//! table for proprietary PGNs.
//!
//! Example: `cargo run --example fast_packet_rx --features std`.
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

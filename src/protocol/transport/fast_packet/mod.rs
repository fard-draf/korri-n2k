//! NMEA 2000 Fast Packet support: encapsulates payloads larger than eight bytes
//! across successive CAN frames.
/// Maximum payload a Fast Packet can transport once reassembled.
pub const MAX_FAST_PACKET_PAYLOAD: usize = 223;

pub mod assembler;
pub mod builder;

#[cfg(test)]
pub mod tests;

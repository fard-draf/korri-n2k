//! In-memory representation of an SAE J1939 / NMEA 2000 CAN frame.
use crate::protocol::transport::can_id::CanId;

#[derive(Clone, Debug)]
/// Raw NMEA 2000 frame as read from the CAN bus.
pub struct CanFrame {
    /// Full 29-bit CAN identifier stored inside a `u32`.
    pub id: CanId,
    /// Payload buffer. Classic CAN frames always provide eight bytes.
    pub data: [u8; 8],
    /// Number of valid payload bytes (Data Length Code, 0 to 8).
    pub len: usize,
}

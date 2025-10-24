//! NMEA 2000 transport layer: CAN frame representations, 29-bit identifier
//! management, Fast Packet encoding, and bus abstraction traits.
//!
//! ## NMEA 2000 Timing Constants
//!
//! These constants define recommended delays and timeouts for reliable,
//! standards-compliant transmissions on an NMEA 2000 network.

pub mod can_frame;
pub mod can_id;
pub mod fast_packet;
pub mod traits;

/// Recommended minimal delay between two frames of the same Fast Packet message (ms).
///
/// The NMEA 2000 specification permits back-to-back frames, yet a 1–2 ms delay is
/// recommended in practice to avoid saturating embedded CAN TX buffers (notably
/// ESP32 TWAI with a three-frame buffer).
///
/// The delay also improves interoperability with devices that have limited receive capacity.
///
/// # NMEA 2000 Compliance
///
/// - No minimal delay is mandated between Fast Packet frames.
/// - The protocol allows interleaving with other high-priority messages.
/// - This delay ensures compatibility with common hardware implementations.
///
/// # Recommended Values
///
/// - **1 ms**: Minimum to avoid TX buffer saturation.
/// - **2 ms**: Suggested default for maximum compatibility.
/// - **5 ms**: Conservative choice for resource-constrained systems.
pub const FAST_PACKET_INTER_FRAME_DELAY_MS: u32 = 2;

/// Recommended timeout for sending a single CAN frame (ms).
///
/// Prevents indefinite blocking when the bus is faulty, disconnected, or saturated.
///
/// # Timeout rationale
///
/// On an NMEA 2000 bus @ 250 kbps with CAN arbitration:
/// - Maximum time for one frame (8 bytes): ~0.5 ms (no contention)
/// - With arbitration and retransmissions: ~10–20 ms
/// - Safety margin ×5 → 100 ms
///
/// # Implementation notes
///
/// [`CanBus`](traits::can_bus::CanBus) implementations **SHOULD**
/// enforce a timeout on `send()` to avoid infinite waits.
///
/// # Example
///
/// ```rust,ignore
/// use embassy_time::{with_timeout, Duration};
/// use korri_n2k::protocol::transport::CAN_SEND_TIMEOUT_MS;
///
/// async fn send_with_timeout(&mut self, frame: &CanFrame) -> Result<(), Error> {
///     with_timeout(
///         Duration::from_millis(CAN_SEND_TIMEOUT_MS as u64),
///         self.can.transmit_async(&twai_frame)
///     )
///     .await
///     .map_err(|_| Error::Timeout)?
/// }
/// ```
pub const CAN_SEND_TIMEOUT_MS: u32 = 100;

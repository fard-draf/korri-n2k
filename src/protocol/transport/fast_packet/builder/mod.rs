//! CAN frame generator for Fast Packet messages. Automatically builds the required
//! frame sequence (single or multi-packet) from an application payload.
use crate::error::CanIdBuildError;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
use crate::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;
#[cfg(target_has_atomic = "8")]
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(target_has_atomic = "8")]
static GLOBAL_SEQUENCE_ID: AtomicU8 = AtomicU8::new(0);

#[cfg(not(target_has_atomic = "8"))]
// Warning: this branch is only safe when the caller guarantees exclusive access
// (single-thread execution or interrupts disabled during construction). On MCUs without
// atomics, wrap the call in a critical section if multiple contexts can emit concurrently.
static mut GLOBAL_SEQUENCE_ID: u8 = 0;

fn next_sequence_id() -> u8 {
    #[cfg(target_has_atomic = "8")]
    {
        GLOBAL_SEQUENCE_ID
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |prev| {
                Some((prev + 1) & 0x07)
            })
            .unwrap()
            & 0x07
    }

    #[cfg(not(target_has_atomic = "8"))]
    unsafe {
        let current = GLOBAL_SEQUENCE_ID & 0x07;
        GLOBAL_SEQUENCE_ID = (current + 1) & 0x07;
        current
    }
}

#[derive(Debug)]
/// Shared parameters for all frames composing a Fast Packet message.
pub struct FastPacketBuilder<'a> {
    pgn: u32,
    source_address: u8,
    destination: Option<u8>,
    payload: &'a [u8],
    sequence_id: u8,
}

/// Lazy iterator returning frames one by one as they are encoded.
pub struct FrameIterator<'a> {
    builder: FastPacketBuilder<'a>,
    frame_index: u8,
    bytes_sent: usize,
}

impl<'a> Iterator for FrameIterator<'a> {
    type Item = Result<CanFrame, CanIdBuildError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes_sent >= self.builder.payload.len() {
            return None;
        }

        let mut id_builder = CanId::builder(self.builder.pgn, self.builder.source_address);

        if let Some(destination) = self.builder.destination {
            id_builder = id_builder.to_destination(destination);
        }

        let id = match id_builder.build() {
            Ok(id) => id,
            Err(e) => return Some(Err(e)),
        };
        let total_len = self.builder.payload.len();

        if total_len > MAX_FAST_PACKET_PAYLOAD {
            self.bytes_sent = total_len;
            return Some(Err(CanIdBuildError::InvalidData));
        }

        // Payload ≤ 8 bytes: single-frame message (no Fast Packet).
        if total_len <= 8 {
            let mut data = [0xFF; 8];
            data[..total_len].copy_from_slice(self.builder.payload);

            self.bytes_sent = total_len;

            return Some(Ok(CanFrame {
                id,
                data,
                len: total_len,
            }));
        }

        // Fast Packet case: segment the message.
        let header = ((self.builder.sequence_id & 0x07) << 5) | (self.frame_index & 0x1F);
        let frame = if self.bytes_sent == 0 {
            // First frame: header + six data bytes.
            let mut data = [0xFF; 8];
            // Byte 0: sequence identifier.
            data[0] = header;
            // Byte 1: total useful payload length.
            data[1] = self.builder.payload.len() as u8;
            // Bytes 2-7: first six payload bytes.
            let bytes_to_copy = 6.min(self.builder.payload.len());
            data[2..2 + bytes_to_copy].copy_from_slice(&self.builder.payload[0..bytes_to_copy]);

            self.bytes_sent += bytes_to_copy;

            CanFrame {
                id,
                data,
                len: 2 + bytes_to_copy,
            }
        } else {
            let mut data = [0xFF; 8];
            data[0] = header;

            let remaining_bytes = self.builder.payload.len() - self.bytes_sent;
            let bytes_to_copy = 7.min(remaining_bytes);
            let payload_slice =
                &self.builder.payload[self.bytes_sent..self.bytes_sent + bytes_to_copy];
            data[1..1 + bytes_to_copy].copy_from_slice(payload_slice);

            self.bytes_sent += bytes_to_copy;

            CanFrame {
                id,
                data,
                len: 1 + bytes_to_copy,
            }
        };

        self.frame_index = self.frame_index.wrapping_add(1);

        Some(Ok(frame))
    }
}

impl<'a> FastPacketBuilder<'a> {
    /// Create a Fast Packet encoder (or single-frame builder) depending on payload size.
    ///
    /// # Concurrency
    /// On platforms without 8-bit atomics, the sequence identifier relies on a `static mut`.
    /// Callers must ensure no concurrent Fast Packet builder runs at the same time (disable
    /// interrupts or enter a critical section).
    pub fn new(pgn: u32, source_address: u8, destination: Option<u8>, payload: &'a [u8]) -> Self {
        Self {
            pgn,
            source_address,
            destination,
            payload,
            sequence_id: next_sequence_id(),
        }
    }

    /// Override the 3-bit Fast Packet sequence identifier.
    ///
    /// # Recommended usage
    /// Testing, replaying captured traffic, or controlled playback scenarios.
    /// In production let `FastPacketBuilder::new` handle auto-increment to avoid collisions.
    pub fn with_sequence_id(mut self, sequence_id: u8) -> Self {
        self.sequence_id = sequence_id & 0x07;
        self
    }

    /// Start the iteration; each call to `next` yields the next frame.
    pub fn build(self) -> FrameIterator<'a> {
        FrameIterator {
            builder: self,
            frame_index: 0,
            bytes_sent: 0,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

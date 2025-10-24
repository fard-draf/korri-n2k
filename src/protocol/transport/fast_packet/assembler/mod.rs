//! NMEA 2000 Fast Packet assembler: rebuilds application messages by
//! aggregating the CAN frames of a multi-packet session.
use super::MAX_FAST_PACKET_PAYLOAD;

//==================================================================================Constants

/// Maximum number of Fast Packet sessions handled in parallel (distinct sources).
const MAX_CONCURRENT_SESSIONS: usize = 4;

//==================================================================================Enums and Structs
#[derive(Debug)]
pub enum ProcessResult {
    /// Frame not recognized as Fast Packet or discarded (invalid sequence,
    /// session pool exhausted, etc.).
    Ignored,
    /// Frame successfully integrated but additional fragments are still missing.
    FragmentConsumed,
    /// All expected fragments were received; the complete message is now available.
    MessageComplete(CompletedMessage),
}

/// Safe container returning a reassembled message without exposing
/// the assembler's internal buffer.
#[derive(Debug, PartialEq, Eq)]
pub struct CompletedMessage {
    /// Reassembled payload.
    pub payload: [u8; MAX_FAST_PACKET_PAYLOAD],
    /// Effective message length (number of valid bytes).
    pub len: usize,
}

/// Possible states for a reassembly session.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SessionState {
    Inactive,
    InProgress,
}

/// Internal structure tracking the state of a Fast Packet session.
#[derive(Debug, Clone, Copy)]
struct FastPacketSession {
    state: SessionState,
    source_address: u8,
    sequence_id: u8,
    buffer: [u8; MAX_FAST_PACKET_PAYLOAD],
    expected_size: usize,
    current_size: usize,
    last_frame_index: u8,
}

impl FastPacketSession {
    /// Create a session in the inactive state.
    const fn new() -> Self {
        Self {
            state: SessionState::Inactive,
            source_address: 0,
            sequence_id: 0,
            buffer: [0; MAX_FAST_PACKET_PAYLOAD],
            expected_size: 0,
            current_size: 0,
            last_frame_index: 0,
        }
    }

    /// Reset the session and make it available again.
    fn reset(&mut self) {
        self.state = SessionState::Inactive;
        self.sequence_id = 0;
        self.expected_size = 0;
        self.current_size = 0;
        self.last_frame_index = 0;
        // No need to wipe the buffer; upcoming copies will overwrite it.
    }
}

/// Main assembler: owns a fixed pool of reusable sessions.
#[derive(Debug, Copy, Clone)]
pub struct FastPacketAssembler {
    sessions: [FastPacketSession; MAX_CONCURRENT_SESSIONS],
}

impl Default for FastPacketAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl FastPacketAssembler {
    /// Instantiate the assembler with an inactive session pool.
    pub const fn new() -> Self {
        Self {
            sessions: [FastPacketSession::new(); MAX_CONCURRENT_SESSIONS],
        }
    }

    //==================================================================================Process Functions
    /// Process a CAN frame that may belong to a Fast Packet session.
    ///
    /// * `source_address` – logical address of the sender (session key)
    /// * `data` – raw 8-byte payload of the received CAN frame
    ///
    /// Returns a `ProcessResult` indicating whether the frame was ignored,
    /// consumed, or completed the message.
    pub fn process_frame(&mut self, source_address: u8, data: &[u8; 8]) -> ProcessResult {
        let frame_index = data[0] & 0x1F;
        let sequence_id = (data[0] >> 5) & 0x07;

        if frame_index == 0 {
            // First frame: carries the total expected size.
            let expected_size = data[1] as usize;

            if !(8..=MAX_FAST_PACKET_PAYLOAD).contains(&expected_size) {
                return ProcessResult::Ignored;
            }

            let ideal_session_index = self.sessions.iter().position(|s| {
                s.source_address == source_address && s.state == SessionState::Inactive
            });

            let session_index = ideal_session_index.or_else(|| {
                self.sessions
                    .iter()
                    .position(|s| s.state == SessionState::Inactive)
            });

            if let Some(index) = session_index {
                let session = &mut self.sessions[index];

                // Initialize the session.
                session.state = SessionState::InProgress;
                session.source_address = source_address;
                session.expected_size = expected_size;
                session.sequence_id = sequence_id as u8;
                session.last_frame_index = 0;

                // First frame transports six useful bytes after the header.
                let data_len = 6;
                session.buffer[0..data_len].copy_from_slice(&data[2..]);
                session.current_size = data_len;

                return ProcessResult::FragmentConsumed;
            } else {
                return ProcessResult::Ignored;
            }
        } else {
            // Continuation frame.
            if let Some(session) = self.sessions.iter_mut().find(|s| {
                s.state == SessionState::InProgress
                    && s.source_address == source_address
                    && s.sequence_id == sequence_id as u8
            }) {
                if frame_index != session.last_frame_index.wrapping_add(1) {
                    session.reset();
                    return ProcessResult::Ignored;
                }

                session.last_frame_index = frame_index;

                let bytes_needed = session.expected_size - session.current_size;
                // Subsequent frames provide up to seven bytes of payload.
                let bytes_in_frame = 7;
                let copy_len = bytes_needed.min(bytes_in_frame);

                let data_slice = &data[1..(1 + copy_len)];
                let buffer_slice =
                    &mut session.buffer[session.current_size..(session.current_size + copy_len)];

                buffer_slice.copy_from_slice(data_slice);
                session.current_size += copy_len;

                if session.current_size >= session.expected_size {
                    // Copy the complete message into a dedicated return structure.
                    let mut payload_buffer = [0; MAX_FAST_PACKET_PAYLOAD];
                    let payload_len = session.expected_size;
                    payload_buffer[..payload_len].copy_from_slice(&session.buffer[..payload_len]);

                    let completed_message = CompletedMessage {
                        payload: payload_buffer,
                        len: payload_len,
                    };

                    // Release the session for future messages.
                    session.reset();

                    return ProcessResult::MessageComplete(completed_message);
                } else {
                    return ProcessResult::FragmentConsumed;
                }
            }
        }

        ProcessResult::Ignored
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

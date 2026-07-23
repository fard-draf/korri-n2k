//! NMEA 2000 Fast Packet assembler: rebuilds application messages by
//! aggregating the CAN frames of a multi-packet session.
use super::MAX_FAST_PACKET_PAYLOAD;

//==================================================================================Constants

/// Maximum number of Fast Packet sessions handled in parallel (distinct sources).
const MAX_CONCURRENT_SESSIONS: usize = 4;

/// Maximum time without addressed message for a fast packet session.
const SESSION_TIMEOUT_MS: u32 = 500;

//==================================================================================Enums and Structs
// TODO: pass the completed message as an out-parameter of `process_frame` instead of
// returning it here. The caller would then own the buffer once, outside the receive
// loop, and this enum would drop from 240 bytes to 1.
#[allow(clippy::large_enum_variant)]
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
    pgn: u32,
    sequence_id: u8,
    buffer: [u8; MAX_FAST_PACKET_PAYLOAD],
    expected_size: usize,
    current_size: usize,
    last_frame_index: u8,
    last_seen_ms: u32,
}

impl FastPacketSession {
    /// Create a session in the inactive state.
    const fn new() -> Self {
        Self {
            state: SessionState::Inactive,
            source_address: 0,
            pgn: 0,
            sequence_id: 0,
            buffer: [0; MAX_FAST_PACKET_PAYLOAD],
            expected_size: 0,
            current_size: 0,
            last_frame_index: 0,
            last_seen_ms: 0,
        }
    }

    /// Reset the session and make it available again.
    fn reset(&mut self) {
        self.state = SessionState::Inactive;
        self.pgn = 0;
        self.sequence_id = 0;
        self.expected_size = 0;
        self.current_size = 0;
        self.last_frame_index = 0;
    }

    fn is_free(&self, now_ms: u32) -> bool {
        self.state == SessionState::Inactive
            || now_ms.wrapping_sub(self.last_seen_ms) > SESSION_TIMEOUT_MS
    }

    fn is_expired(&self, now_ms: u32) -> bool {
        self.state == SessionState::InProgress
            && now_ms.wrapping_sub(self.last_seen_ms) > SESSION_TIMEOUT_MS
    }
}

/// Main assembler: owns a fixed pool of reusable sessions.
#[derive(Debug, Copy, Clone)]
pub struct FastPacketAssembler {
    sessions: [FastPacketSession; MAX_CONCURRENT_SESSIONS],
    expired_sessions: u32,
    pool_exhausted: u32,
    rejected_frames: u32,
    lost_fragments: u32,
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
            expired_sessions: 0,
            pool_exhausted: 0,
            rejected_frames: 0,
            lost_fragments: 0,
        }
    }

    //==================================================================================Diagnostics
    //
    // All counters are cumulative and never reset. `rejected_frames` says the
    // assembler was handed traffic it does not deal with; the other three say a
    // message was lost and should stay at zero on a healthy bus.

    /// Incomplete sessions reclaimed after going `SESSION_TIMEOUT_MS` without a
    /// new fragment. Each one is a message that will never be delivered.
    pub fn expired_sessions(&self) -> u32 {
        self.expired_sessions
    }

    /// Messages refused because every session slot was already in use.
    pub fn pool_exhausted(&self) -> u32 {
        self.pool_exhausted
    }

    /// First frames announcing a size outside the Fast Packet range. Not a
    /// loss: the frame simply is not a Fast Packet message this assembler
    /// handles.
    pub fn rejected_frames(&self) -> u32 {
        self.rejected_frames
    }

    /// Continuation frames dropped because they arrived out of sequence or
    /// belonged to no live session. Each one leaves a message incomplete.
    pub fn lost_fragments(&self) -> u32 {
        self.lost_fragments
    }

    //==================================================================================Process Functions
    /// Process a CAN frame that may belong to a Fast Packet session.
    ///
    /// * `source_address` – logical address of the sender (session key)
    /// * `data` – raw 8-byte payload of the received CAN frame
    ///
    /// Returns a `ProcessResult` indicating whether the frame was ignored,
    /// consumed, or completed the message.
    pub fn process_frame(
        &mut self,
        now_ms: u32,
        pgn: u32,
        source_address: u8,
        data: &[u8; 8],
    ) -> ProcessResult {
        let frame_index = data[0] & 0x1F;
        let sequence_id = (data[0] >> 5) & 0x07;

        if frame_index == 0 {
            // First frame: carries the total expected size.
            let expected_size = data[1] as usize;

            if !(8..=MAX_FAST_PACKET_PAYLOAD).contains(&expected_size) {
                self.rejected_frames += 1;
                return ProcessResult::Ignored;
            }

            let session_index = self
                .sessions
                .iter()
                .position(|s| s.source_address == source_address && s.is_free(now_ms))
                .or_else(|| self.sessions.iter().position(|s| s.is_free(now_ms)));

            if let Some(index) = session_index {
                let session = &mut self.sessions[index];
                if session.state == SessionState::InProgress && session.is_free(now_ms) {
                    self.expired_sessions += 1;
                }

                // Initialize the session.
                session.state = SessionState::InProgress;
                session.source_address = source_address;
                session.expected_size = expected_size;
                session.sequence_id = sequence_id;
                session.last_frame_index = 0;
                session.pgn = pgn;
                session.last_seen_ms = now_ms;

                // First frame transports six useful bytes after the header.
                let data_len = 6;
                session.buffer[0..data_len].copy_from_slice(&data[2..]);
                session.current_size = data_len;

                return ProcessResult::FragmentConsumed;
            } else {
                self.pool_exhausted += 1;
                return ProcessResult::Ignored;
            }
        } else {
            // Continuation frame.
            if let Some(session) = self.sessions.iter_mut().find(|s| {
                s.state == SessionState::InProgress
                    && s.pgn == pgn
                    && s.source_address == source_address
                    && s.sequence_id == sequence_id
                    && !s.is_expired(now_ms)
            }) {
                if frame_index != session.last_frame_index.wrapping_add(1) {
                    session.reset();
                    self.lost_fragments += 1;
                    return ProcessResult::Ignored;
                }

                session.last_frame_index = frame_index;
                session.last_seen_ms = now_ms;

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

        self.lost_fragments += 1;
        ProcessResult::Ignored
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

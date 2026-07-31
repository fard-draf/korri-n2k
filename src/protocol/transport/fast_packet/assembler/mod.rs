//! NMEA 2000 Fast Packet assembler: rebuilds application messages by
//! aggregating the CAN frames of a multi-packet session.
use super::{FAST_PACKET_PGNS, MAX_FAST_PACKET_PAYLOAD};

//==================================================================================Constants

/// Maximum number of Fast Packet sessions handled in parallel (distinct sources).
const MAX_CONCURRENT_SESSIONS: usize = 4;

/// Maximum time without addressed message for a fast packet session.
const SESSION_TIMEOUT_MS: u32 = 500;

/// Useful bytes a first frame carries, after the sequence and size header.
const FIRST_FRAME_PAYLOAD: usize = 6;

/// Useful bytes a continuation frame carries, after the sequence header.
const CONTINUATION_PAYLOAD: usize = 7;

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
    fast_packet_pgns: &'static [u32],
    expired_sessions: u32,
    pool_exhausted: u32,
    rejected_frames: u32,
    lost_fragments: u32,
    unknown_pgn: u32,
}

impl Default for FastPacketAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl FastPacketAssembler {
    /// Constructor for a node: the Fast Packet PGNs of its manifest.
    ///
    /// A gateway wants `with_pgns`.
    pub const fn new() -> Self {
        Self::with_pgns(FAST_PACKET_PGNS)
    }

    /// Constructor for a gateway: an ascending table of your choosing.
    ///
    /// `FAST_PACKET_PGNS_ALL` holds every Fast Packet canboat declares, 182 against
    /// the 152 of `full-pgns`. Extend it for proprietary PGNs.
    pub const fn with_pgns(pgns: &'static [u32]) -> Self {
        Self {
            sessions: [FastPacketSession::new(); MAX_CONCURRENT_SESSIONS],
            fast_packet_pgns: pgns,
            expired_sessions: 0,
            pool_exhausted: 0,
            rejected_frames: 0,
            lost_fragments: 0,
            unknown_pgn: 0,
        }
    }

    /// Returns true when this assembler's table lists the PGN as Fast Packet.
    ///
    /// Branch on this, not on `Ignored`, which cannot tell a single frame from a
    /// lost fragment.
    pub fn handles(&self, pgn: u32) -> bool {
        self.fast_packet_pgns.binary_search(&pgn).is_ok()
    }

    //==================================================================================Diagnostics
    //
    // All counters are cumulative and never reset. `rejected_frames` and
    // `unknown_pgn` say the assembler was handed traffic it does not deal with; the
    // other three say a message was lost and should stay at zero on a healthy bus.

    /// Incomplete sessions reclaimed after going `SESSION_TIMEOUT_MS` without a
    /// new fragment. Each one is a message that will never be delivered.
    pub fn expired_sessions(&self) -> u32 {
        self.expired_sessions
    }

    /// Messages refused because every session slot was already in use.
    pub fn pool_exhausted(&self) -> u32 {
        self.pool_exhausted
    }

    /// First frames announcing a size of zero, or one above
    /// `MAX_FAST_PACKET_PAYLOAD`. Not a loss: no Fast Packet can carry that, so
    /// the frame is not one this assembler handles.
    pub fn rejected_frames(&self) -> u32 {
        self.rejected_frames
    }

    /// Continuation frames dropped because they arrived out of sequence or
    /// belonged to no live session. Each one leaves a message incomplete.
    pub fn lost_fragments(&self) -> u32 {
        self.lost_fragments
    }

    /// Frames whose PGN is absent from this assembler's table. Stays at zero for a
    /// caller that pre-filters with `handles`, so it counts an integration mistake
    /// rather than a bus fault.
    pub fn unknown_pgn(&self) -> u32 {
        self.unknown_pgn
    }

    //==================================================================================Process Functions
    /// Process a CAN frame that may belong to a Fast Packet session.
    ///
    /// * `source_address` – logical address of the sender (session key)
    /// * `data` – raw 8-byte payload of the received CAN frame
    ///
    /// Returns a `ProcessResult` indicating whether the frame was ignored,
    /// consumed, or completed the message. A PGN outside the table is ignored.
    pub fn process_frame(
        &mut self,
        now_ms: u32,
        pgn: u32,
        source_address: u8,
        data: &[u8; 8],
    ) -> ProcessResult {
        if !self.handles(pgn) {
            self.unknown_pgn += 1;
            return ProcessResult::Ignored;
        }

        let frame_index = data[0] & 0x1F;
        let sequence_id = (data[0] >> 5) & 0x07;

        if frame_index == 0 {
            // First frame: carries the total expected size.
            let expected_size = data[1] as usize;

            if !(1..=MAX_FAST_PACKET_PAYLOAD).contains(&expected_size) {
                self.rejected_frames += 1;
                return ProcessResult::Ignored;
            }

            // A payload of six bytes or less rides entirely in this frame, so there
            // is no continuation to wait for. Deliver it without touching the
            // session pool: canboat declares such Fast Packets (130824 announces two
            // bytes) and a live bus sends them at rate.
            if expected_size <= FIRST_FRAME_PAYLOAD {
                let mut payload = [0; MAX_FAST_PACKET_PAYLOAD];
                payload[..expected_size].copy_from_slice(&data[2..2 + expected_size]);
                return ProcessResult::MessageComplete(CompletedMessage {
                    payload,
                    len: expected_size,
                });
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

                session.buffer[0..FIRST_FRAME_PAYLOAD].copy_from_slice(&data[2..]);
                session.current_size = FIRST_FRAME_PAYLOAD;

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
                let copy_len = bytes_needed.min(CONTINUATION_PAYLOAD);

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

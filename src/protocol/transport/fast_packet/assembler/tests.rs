//! Fast Packet reassembly tests covering sequencing, sessions, and concurrency.
// ASSEMBLER
use super::super::FAST_PACKET_PGNS_ALL;
use super::*;

// Helper to make test assertions easier to read
impl PartialEq for ProcessResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ProcessResult::Ignored, ProcessResult::Ignored) => true,
            (ProcessResult::FragmentConsumed, ProcessResult::FragmentConsumed) => true,
            (ProcessResult::MessageComplete(a), ProcessResult::MessageComplete(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for ProcessResult {}

#[test]
/// Rebuild a complete message from three valid fragments.
fn test_full_fast_packet_reassembly() {
    let mut assembler = FastPacketAssembler::new();
    let source_address = 42;
    let fake_timer_ms: u32 = 40;
    let pgn = 129540;
    // --- Frame 1 (start) ---
    // Total length = 15 bytes
    // Data: 6 bytes
    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    let result = assembler.process_frame(fake_timer_ms, pgn, source_address, &frame0);
    assert_eq!(result, ProcessResult::FragmentConsumed);

    // --- Frame 2 (continuation) ---
    // Data: 7 bytes
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 11, 12, 13];
    let result = assembler.process_frame(fake_timer_ms, pgn, source_address, &frame1);
    assert_eq!(result, ProcessResult::FragmentConsumed);
    // --- Frame 3 (final) ---
    // Data: 2 bytes (remaining bytes are padding)
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(fake_timer_ms, pgn, source_address, &frame2);

    // --- Verification ---
    let mut expected_payload_array = [0; MAX_FAST_PACKET_PAYLOAD];
    let expected_data: [u8; 15] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    expected_payload_array[..15].copy_from_slice(&expected_data);

    let expected_message = CompletedMessage {
        payload: expected_payload_array,
        len: 15,
    };

    assert_eq!(result, ProcessResult::MessageComplete(expected_message));
}

#[test]
/// Ignore an out-of-sequence frame and reset the session.
fn test_out_of_sequence_packet() {
    let mut assembler = FastPacketAssembler::new();
    let source_address = 10;
    let fake_timer_ms: u32 = 10;
    let pgn = 129540;

    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(fake_timer_ms, pgn, source_address, &frame0);
    // Send frame index 2 while skipping frame index 1
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(fake_timer_ms, pgn, source_address, &frame2);
    // The assembler must drop the frame and abandon the message
    assert_eq!(result, ProcessResult::Ignored);
    // Ensure the session was released
    assert_eq!(assembler.sessions[0].state, SessionState::Inactive);
}

#[test]
/// Handles two concurrent sessions without collision.
fn test_multiple_concurrent_sessions() {
    let mut assembler = FastPacketAssembler::new();
    let source_a = 10;
    let source_b = 20;
    let fake_timer_ms: u32 = 10;
    let pgn = 129540;
    // Start message A
    let frame_a0: [u8; 8] = [0, 10, 1, 2, 3, 4, 5, 6];
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source_a, &frame_a0),
        ProcessResult::FragmentConsumed
    );
    // Start message B
    let frame_b0: [u8; 8] = [0, 9, 100, 101, 102, 103, 104, 105];
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source_b, &frame_b0),
        ProcessResult::FragmentConsumed
    );
    // Finish message A
    let frame_a1: [u8; 8] = [1, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    let mut payload_a = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_a[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let expected_a = CompletedMessage {
        payload: payload_a,
        len: 10,
    };
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source_a, &frame_a1),
        ProcessResult::MessageComplete(expected_a)
    );
    // Finish message B
    let frame_b1: [u8; 8] = [1, 106, 107, 108, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut payload_b = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_b[..9].copy_from_slice(&[100, 101, 102, 103, 104, 105, 106, 107, 108]);
    let expected_b = CompletedMessage {
        payload: payload_b,
        len: 9,
    };
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source_b, &frame_b1),
        ProcessResult::MessageComplete(expected_b)
    );
}

#[test]
/// Two Fast Packet streams from the same source but different sequence IDs must not interfere.
fn test_interleaved_sequences_same_source() {
    // Explicit table: this test is about sequencing, not about the manifest.
    let pgn = 129740;
    let mut assembler = FastPacketAssembler::with_pgns(&[129740]);
    let source = 7;
    let fake_timer_ms: u32 = 10;

    // Message A: sequence 1 (upper bits = 0b001)
    let frame_a0: [u8; 8] = [0b001_00000, 10, 1, 2, 3, 4, 5, 6];
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source, &frame_a0),
        ProcessResult::FragmentConsumed
    );

    // Message B: sequence 2 (upper bits = 0b010)
    let frame_b0: [u8; 8] = [0b010_00000, 9, 21, 22, 23, 24, 25, 26];
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source, &frame_b0),
        ProcessResult::FragmentConsumed
    );

    // Continue message B (completed before A)
    let frame_b1: [u8; 8] = [0b010_00001, 27, 28, 29, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut payload_b = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_b[..9].copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28, 29]);
    let expected_b = CompletedMessage {
        payload: payload_b,
        len: 9,
    };
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source, &frame_b1),
        ProcessResult::MessageComplete(expected_b)
    );

    // Continue message A
    let frame_a1: [u8; 8] = [0b001_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    let mut payload_a = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_a[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let expected_a = CompletedMessage {
        payload: payload_a,
        len: 10,
    };
    assert_eq!(
        assembler.process_frame(fake_timer_ms, pgn, source, &frame_a1),
        ProcessResult::MessageComplete(expected_a)
    );
}

//==================================================================================Session timeout

#[test]
/// A session must survive a long uptime: the timeout compares elapsed time,
/// not the absolute clock. Regression test for `last_seen_ms` left at zero,
/// which made every session look expired past `SESSION_TIMEOUT_MS` of uptime.
fn test_reassembly_unaffected_by_uptime() {
    let pgn = 129540;
    let source = 42;

    for now_ms in [0, 400, SESSION_TIMEOUT_MS + 1, 60_000, 3_600_000] {
        let mut assembler = FastPacketAssembler::new();

        let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
        assert_eq!(
            assembler.process_frame(now_ms, pgn, source, &frame0),
            ProcessResult::FragmentConsumed,
            "first frame refused at now_ms={now_ms}"
        );

        let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
        let mut payload = [0; MAX_FAST_PACKET_PAYLOAD];
        payload[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        assert_eq!(
            assembler.process_frame(now_ms, pgn, source, &frame1),
            ProcessResult::MessageComplete(CompletedMessage { payload, len: 10 }),
            "message not reassembled at now_ms={now_ms}"
        );
    }
}

#[test]
/// A fragment arriving past the timeout is dropped: the sender went silent and
/// the partial message must not be completed with stale data.
fn test_session_expires_after_timeout() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;
    let start = 1_000;

    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(start, pgn, source, &frame0);

    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    assert_eq!(
        assembler.process_frame(start + SESSION_TIMEOUT_MS + 1, pgn, source, &frame1),
        ProcessResult::Ignored
    );
}

#[test]
/// A fragment arriving exactly on the timeout boundary is still accepted.
fn test_session_survives_up_to_timeout() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;
    let start = 1_000;

    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(start, pgn, source, &frame0);

    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    assert!(matches!(
        assembler.process_frame(start + SESSION_TIMEOUT_MS, pgn, source, &frame1),
        ProcessResult::MessageComplete(_)
    ));
}

#[test]
/// Each fragment refreshes the deadline, so a slow message completes even when
/// its total duration exceeds the timeout. Only silence between two fragments
/// kills a session.
fn test_timeout_is_refreshed_by_each_fragment() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;
    let step = SESSION_TIMEOUT_MS - 100;

    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(0, pgn, source, &frame0);

    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 11, 12, 13];
    assert_eq!(
        assembler.process_frame(step, pgn, source, &frame1),
        ProcessResult::FragmentConsumed
    );

    // Total elapsed time is now above the timeout, but no single gap was.
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    assert!(2 * step > SESSION_TIMEOUT_MS);
    assert!(matches!(
        assembler.process_frame(2 * step, pgn, source, &frame2),
        ProcessResult::MessageComplete(_)
    ));
}

#[test]
/// Elapsed time must be computed with wrapping arithmetic, so a session started
/// just before the `u32` millisecond counter rolls over still completes.
fn test_timeout_across_u32_wrap() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;
    let start = u32::MAX - 100;

    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(start, pgn, source, &frame0);

    // 151 ms later, on the other side of the wrap.
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    assert!(matches!(
        assembler.process_frame(50, pgn, source, &frame1),
        ProcessResult::MessageComplete(_)
    ));
}

//==================================================================================Session pool

#[test]
/// An expired session releases its slot to a new sender, and the loss is counted.
fn test_expired_session_slot_is_reused_and_counted() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];

    for source in 0..MAX_CONCURRENT_SESSIONS as u8 {
        assembler.process_frame(0, pgn, source, &frame0);
    }
    assert_eq!(assembler.expired_sessions(), 0);

    // Every session is now stale; a new sender must still be served.
    let latecomer = MAX_CONCURRENT_SESSIONS as u8 + 10;
    assert_eq!(
        assembler.process_frame(SESSION_TIMEOUT_MS + 1, pgn, latecomer, &frame0),
        ProcessResult::FragmentConsumed
    );
    assert_eq!(
        assembler.expired_sessions(),
        1,
        "reclaiming a live session must be counted as a lost message"
    );
}

#[test]
/// A full pool of fresh sessions refuses a new sender rather than destroying
/// a message in flight.
fn test_pool_exhaustion_preserves_active_sessions() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let now = 100;
    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];

    for source in 0..MAX_CONCURRENT_SESSIONS as u8 {
        assembler.process_frame(now, pgn, source, &frame0);
    }

    let latecomer = MAX_CONCURRENT_SESSIONS as u8 + 10;
    assert_eq!(
        assembler.process_frame(now, pgn, latecomer, &frame0),
        ProcessResult::Ignored
    );
    assert_eq!(
        assembler.expired_sessions(),
        0,
        "no live session was reclaimed, so nothing was stolen"
    );
    assert_eq!(
        assembler.pool_exhausted(),
        1,
        "a message refused for lack of a slot must be counted, never silent"
    );

    // The sessions already in flight must be intact.
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    for source in 0..MAX_CONCURRENT_SESSIONS as u8 {
        assert!(
            matches!(
                assembler.process_frame(now, pgn, source, &frame1),
                ProcessResult::MessageComplete(_)
            ),
            "session of source {source} was disturbed by the refused sender"
        );
    }
}

//==================================================================================PGN keying

#[test]
/// Two Fast Packet streams from the same source sharing a sequence ID but
/// carrying different PGNs must not be merged.
fn test_same_source_same_sequence_different_pgn() {
    let mut assembler = FastPacketAssembler::new();
    let source = 7;
    let now = 10;
    let (pgn_a, pgn_b) = (129540, 129029);

    let a0: [u8; 8] = [0b001_00000, 10, 1, 2, 3, 4, 5, 6];
    let b0: [u8; 8] = [0b001_00000, 9, 21, 22, 23, 24, 25, 26];
    assert_eq!(
        assembler.process_frame(now, pgn_a, source, &a0),
        ProcessResult::FragmentConsumed
    );
    assert_eq!(
        assembler.process_frame(now, pgn_b, source, &b0),
        ProcessResult::FragmentConsumed
    );

    let a1: [u8; 8] = [0b001_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];
    let mut payload_a = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_a[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(
        assembler.process_frame(now, pgn_a, source, &a1),
        ProcessResult::MessageComplete(CompletedMessage {
            payload: payload_a,
            len: 10
        })
    );

    let b1: [u8; 8] = [0b001_00001, 27, 28, 29, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut payload_b = [0; MAX_FAST_PACKET_PAYLOAD];
    payload_b[..9].copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28, 29]);
    assert_eq!(
        assembler.process_frame(now, pgn_b, source, &b1),
        ProcessResult::MessageComplete(CompletedMessage {
            payload: payload_b,
            len: 9
        })
    );
}

#[test]
/// Refusals pile up in the counter, and a message accepted on a free slot must
/// not be counted as a refusal.
fn test_pool_exhaustion_counter_tracks_every_refusal() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let now = 100;
    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];

    for source in 0..MAX_CONCURRENT_SESSIONS as u8 {
        assembler.process_frame(now, pgn, source, &frame0);
    }
    assert_eq!(
        assembler.pool_exhausted(),
        0,
        "a fitting message is not a refusal"
    );

    for extra in 0..3u8 {
        assembler.process_frame(now, pgn, 100 + extra, &frame0);
    }
    assert_eq!(assembler.pool_exhausted(), 3);
}

//==================================================================================Diagnostic counters

#[test]
/// A first frame announcing a size no Fast Packet can carry is not a loss: it is
/// traffic the assembler does not handle. It must never be mixed with the
/// counters that report dropped data.
fn test_rejected_frames_is_not_a_loss() {
    // Explicit table: this test is about the announced size, not about the manifest.
    let pgn = 130824;
    let mut assembler = FastPacketAssembler::with_pgns(&[130824]);
    let source = 12;

    // A message of no bytes at all.
    let empty: [u8; 8] = [0b000_00000, 0, 1, 2, 3, 4, 5, 6];
    // Announced size above the payload buffer.
    let too_long: [u8; 8] = [0b000_00000, 255, 1, 2, 3, 4, 5, 6];

    for frame in [&empty, &too_long] {
        assert_eq!(
            assembler.process_frame(100, pgn, source, frame),
            ProcessResult::Ignored
        );
    }

    assert_eq!(assembler.rejected_frames(), 2);
    assert_eq!(
        assembler.lost_fragments(),
        0,
        "an unhandled frame must not be reported as lost data"
    );
    assert_eq!(assembler.expired_sessions(), 0);
    assert_eq!(assembler.pool_exhausted(), 0);
    assert_eq!(
        assembler.unknown_pgn(),
        0,
        "the PGN is in the table, only its announced size is wrong"
    );
}

#[test]
/// An out-of-sequence continuation loses the message being assembled.
fn test_lost_fragments_counts_out_of_sequence() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;

    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(100, pgn, source, &frame0);

    // Frame index 2 while index 1 was never seen.
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(
        assembler.process_frame(100, pgn, source, &frame2),
        ProcessResult::Ignored
    );

    assert_eq!(assembler.lost_fragments(), 1);
    assert_eq!(assembler.rejected_frames(), 0);
}

#[test]
/// A continuation frame with no live session behind it is a lost fragment,
/// whether the session expired or never existed.
fn test_lost_fragments_counts_orphan_continuation() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;
    let source = 42;
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];

    // No session was ever opened for this source.
    assert_eq!(
        assembler.process_frame(100, pgn, source, &frame1),
        ProcessResult::Ignored
    );
    assert_eq!(assembler.lost_fragments(), 1);

    // A session that timed out no longer accepts its own fragments.
    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(200, pgn, source, &frame0);
    assert_eq!(
        assembler.process_frame(200 + SESSION_TIMEOUT_MS + 1, pgn, source, &frame1),
        ProcessResult::Ignored
    );
    assert_eq!(assembler.lost_fragments(), 2);
}

#[test]
/// A clean exchange must leave every counter at zero.
fn test_counters_stay_zero_on_healthy_traffic() {
    let mut assembler = FastPacketAssembler::new();
    let pgn = 129540;

    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 0xFF, 0xFF, 0xFF];

    for source in 0..MAX_CONCURRENT_SESSIONS as u8 {
        assembler.process_frame(100, pgn, source, &frame0);
        assert!(matches!(
            assembler.process_frame(150, pgn, source, &frame1),
            ProcessResult::MessageComplete(_)
        ));
    }

    assert_eq!(assembler.expired_sessions(), 0);
    assert_eq!(assembler.pool_exhausted(), 0);
    assert_eq!(assembler.rejected_frames(), 0);
    assert_eq!(assembler.lost_fragments(), 0);
}

//==================================================================================PGN Table

#[test]
/// The generated table must be sorted and deduplicated: `handles` binary-searches it.
fn test_generated_table_is_sorted_and_deduplicated() {
    assert!(
        FAST_PACKET_PGNS_ALL.windows(2).all(|w| w[0] < w[1]),
        "the codegen must emit a strictly ascending table"
    );
    assert!(FAST_PACKET_PGNS_ALL.contains(&129029));
    // Polymorphic PGN: four canboat variants, one entry.
    assert!(FAST_PACKET_PGNS_ALL.contains(&130821));
    // Single-frame PGN at 10 Hz, the traffic that used to starve the session pool.
    assert!(!FAST_PACKET_PGNS_ALL.contains(&127250));
}

#[test]
/// A single-frame PGN whose payload mimics a first fragment must not open a session.
fn test_single_frame_pgn_mimicking_a_first_fragment_is_ignored() {
    let mut assembler = FastPacketAssembler::new();

    // 127250 heading: `data[0] & 0x1F == 0` and `data[1]` inside 8..=223, so every
    // check but the PGN table reads this as the first frame of a Fast Packet.
    let heading: [u8; 8] = [0x00, 0x2C, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFC];

    assert!(!assembler.handles(127250));
    assert_eq!(
        assembler.process_frame(100, 127250, 35, &heading),
        ProcessResult::Ignored
    );
    assert_eq!(assembler.unknown_pgn(), 1);
    assert_eq!(assembler.rejected_frames(), 0);
    assert_eq!(assembler.lost_fragments(), 0);
}

#[test]
/// An empty table ignores every frame, and `handles` says so before the call.
fn test_empty_table_ignores_everything() {
    let mut assembler = FastPacketAssembler::with_pgns(&[]);
    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];

    for pgn in [129540, 129029, 127250] {
        assert!(!assembler.handles(pgn));
        assert_eq!(
            assembler.process_frame(100, pgn, 1, &frame0),
            ProcessResult::Ignored
        );
    }
    assert_eq!(assembler.unknown_pgn(), 3);
}

//==================================================================================Short payloads

#[test]
/// A payload of six bytes or less is complete on its first frame: there is no
/// continuation to wait for. 130824 announces two bytes and sends nothing else.
fn test_payload_fitting_in_the_first_frame_completes_at_once() {
    let mut assembler = FastPacketAssembler::with_pgns(&[130824]);

    for size in 1..=FIRST_FRAME_PAYLOAD {
        let mut frame = [0u8; 8];
        frame[1] = size as u8;
        frame[2..].copy_from_slice(&[11, 22, 33, 44, 55, 66]);

        let mut expected = [0; MAX_FAST_PACKET_PAYLOAD];
        expected[..size].copy_from_slice(&[11, 22, 33, 44, 55, 66][..size]);

        assert_eq!(
            assembler.process_frame(100, 130824, 9, &frame),
            ProcessResult::MessageComplete(CompletedMessage {
                payload: expected,
                len: size,
            }),
            "a {size}-byte payload must be delivered on its first frame"
        );
    }

    // No session was ever opened, so the pool stayed untouched.
    assert!(assembler
        .sessions
        .iter()
        .all(|s| s.state == SessionState::Inactive));
    assert_eq!(assembler.rejected_frames(), 0);
    assert_eq!(assembler.lost_fragments(), 0);
}

#[test]
/// A seven-byte payload needs a second frame: one byte does not fit in the first.
/// canboat declares 130817 as Fast Packet with a length of seven.
fn test_seven_byte_payload_completes_on_the_second_frame() {
    let mut assembler = FastPacketAssembler::with_pgns(&[130817]);

    let frame0: [u8; 8] = [0b000_00000, 7, 1, 2, 3, 4, 5, 6];
    assert_eq!(
        assembler.process_frame(100, 130817, 9, &frame0),
        ProcessResult::FragmentConsumed
    );

    let frame1: [u8; 8] = [0b000_00001, 7, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut expected = [0; MAX_FAST_PACKET_PAYLOAD];
    expected[..7].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(
        assembler.process_frame(100, 130817, 9, &frame1),
        ProcessResult::MessageComplete(CompletedMessage {
            payload: expected,
            len: 7,
        })
    );
    assert_eq!(assembler.rejected_frames(), 0);
}

#[test]
/// Only a size no Fast Packet can carry is rejected: zero, or past the buffer.
fn test_only_impossible_sizes_are_rejected() {
    let mut assembler = FastPacketAssembler::with_pgns(&[130824]);

    for size in [0u8, (MAX_FAST_PACKET_PAYLOAD + 1) as u8, 255] {
        let mut frame = [0u8; 8];
        frame[1] = size;
        assert_eq!(
            assembler.process_frame(100, 130824, 9, &frame),
            ProcessResult::Ignored,
            "size {size} must be rejected"
        );
    }
    assert_eq!(assembler.rejected_frames(), 3);
    assert_eq!(assembler.lost_fragments(), 0);
}

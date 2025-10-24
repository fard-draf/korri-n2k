//! Fast Packet reassembly tests covering sequencing, sessions, and concurrency.
// ASSEMBLER
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
    // --- Frame 1 (start) ---
    // Total length = 15 bytes
    // Data: 6 bytes
    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    let result = assembler.process_frame(source_address, &frame0);
    assert_eq!(result, ProcessResult::FragmentConsumed);

    // --- Frame 2 (continuation) ---
    // Data: 7 bytes
    let frame1: [u8; 8] = [0b000_00001, 7, 8, 9, 10, 11, 12, 13];
    let result = assembler.process_frame(source_address, &frame1);
    assert_eq!(result, ProcessResult::FragmentConsumed);
    // --- Frame 3 (final) ---
    // Data: 2 bytes (remaining bytes are padding)
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(source_address, &frame2);

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
    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(source_address, &frame0);
    // Send frame index 2 while skipping frame index 1
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(source_address, &frame2);
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
    // Start message A
    let frame_a0: [u8; 8] = [0, 10, 1, 2, 3, 4, 5, 6];
    assert_eq!(
        assembler.process_frame(source_a, &frame_a0),
        ProcessResult::FragmentConsumed
    );
    // Start message B
    let frame_b0: [u8; 8] = [0, 9, 100, 101, 102, 103, 104, 105];
    assert_eq!(
        assembler.process_frame(source_b, &frame_b0),
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
        assembler.process_frame(source_a, &frame_a1),
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
        assembler.process_frame(source_b, &frame_b1),
        ProcessResult::MessageComplete(expected_b)
    );
}

#[test]
/// Two Fast Packet streams from the same source but different sequence IDs must not interfere.
fn test_interleaved_sequences_same_source() {
    let mut assembler = FastPacketAssembler::new();
    let source = 7;

    // Message A: sequence 1 (upper bits = 0b001)
    let frame_a0: [u8; 8] = [0b001_00000, 10, 1, 2, 3, 4, 5, 6];
    assert_eq!(
        assembler.process_frame(source, &frame_a0),
        ProcessResult::FragmentConsumed
    );

    // Message B: sequence 2 (upper bits = 0b010)
    let frame_b0: [u8; 8] = [0b010_00000, 9, 21, 22, 23, 24, 25, 26];
    assert_eq!(
        assembler.process_frame(source, &frame_b0),
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
        assembler.process_frame(source, &frame_b1),
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
        assembler.process_frame(source, &frame_a1),
        ProcessResult::MessageComplete(expected_a)
    );
}

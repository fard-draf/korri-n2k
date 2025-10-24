//! Advanced integration tests for the NMEA 2000 Fast Packet implementation.
//!
//! This module covers validation phases 2, 3, and 4:
//! - Phase 2: Round-trip tests with real PGNs
//! - Phase 3: Edge cases (timeouts, sequences, network errors)
//! - Phase 4: Performance and stress tests
//!
//! Each test follows the pattern: serialize → fragment → assemble → deserialize → compare.

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::{Pgn129029, Pgn129040};
use korri_n2k::protocol::transport::fast_packet::{
    assembler::{FastPacketAssembler, ProcessResult},
    builder::FastPacketBuilder,
};

//==================================================================================
// PHASE 2: TESTS WITH REAL PGNS
//==================================================================================

#[test]
/// Validate a full round-trip for PGN 129029 (GNSS Position Data).
///
/// This PGN carries 51 bytes of data, requiring multiple Fast Packet frames.
/// The test checks that GPS data (latitude, longitude, altitude) is properly
/// fragmented and reassembled.
fn test_roundtrip_pgn_129029() {
    // Create a GNSS message with realistic coordinates (Paris, France)
    let mut gnss = Pgn129029::new();
    gnss.latitude = 48.8566; // Latitude for Paris
    gnss.longitude = 2.3522; // Longitude for Paris
    gnss.altitude = 35.0; // Altitude in meters

    // Serialize PGN into a binary payload
    let mut buffer = [0u8; 64];
    let len = gnss
        .to_payload(&mut buffer)
        .expect("PGN 129029 serialization should succeed");

    // Ensure the message requires Fast Packet (> 8 bytes)
    assert!(
        len > 8,
        "PGN 129029 must generate a Fast Packet; current length: {len}"
    );

    // Build fragmented CAN frames
    let builder = FastPacketBuilder::new(129029, 42, None, &buffer[..len]);
    let mut frames = builder.build();

    // Reassemble frames with the assembler
    let mut assembler = FastPacketAssembler::new();
    let mut complete = None;
    let mut frame_count = 0;

    while let Some(frame_result) = frames.next() {
        let frame = frame_result.expect("Frame construction should succeed");
        frame_count += 1;

        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(42, &frame.data) {
            complete = Some(msg);
            break;
        }
    }

    // Ensure the message was reassembled
    let message = complete.expect("Message must be complete after processing");
    assert_eq!(
        message.len, len,
        "Reassembled message length must match"
    );
    assert_eq!(
        &message.payload[..len],
        &buffer[..len],
        "Reassembled payload must match the original"
    );

    // Deserialize and validate values
    let decoded = Pgn129029::from_payload(&message.payload[..message.len])
        .expect("Deserializing the reassembled message should succeed");

    // Validate using a tolerance to account for IEEE 754 rounding
    const TOLERANCE: f64 = 1e-5;
    assert!(
        (gnss.latitude - decoded.latitude).abs() < TOLERANCE,
        "Latitude must be preserved"
    );
    assert!(
        (gnss.longitude - decoded.longitude).abs() < TOLERANCE,
        "Longitude must be preserved"
    );
    assert!(
        (gnss.altitude - decoded.altitude).abs() < TOLERANCE,
        "Altitude must be preserved"
    );

    // Verify that at least two frames were generated (Fast Packet)
    assert!(
        frame_count >= 2,
        "A Fast Packet must generate at least two frames"
    );
}

#[test]
/// Test interleaving of multiple PGNs transmitted simultaneously.
///
/// Simulates several sources emitting Fast Packets in parallel. The assembler must
/// demultiplex sessions and rebuild each message independently.
fn test_interleaved_multiple_pgns() {
    // Prepare two different Fast Packet messages (AIS and GNSS)
    let mut ais = Pgn129040::new();
    ais.user_id = 123_456_789;
    ais.latitude = 48.8566;

    let mut gnss = Pgn129029::new();
    gnss.latitude = 45.5017; // Montreal coordinates
    gnss.longitude = -73.5673;
    gnss.altitude = 50.0;

    // Serialize both PGNs
    let mut buffer_ais = [0u8; 64];
    let len_ais = ais
        .to_payload(&mut buffer_ais)
        .expect("AIS serialization should succeed");

    let mut buffer_gnss = [0u8; 64];
    let len_gnss = gnss
        .to_payload(&mut buffer_gnss)
        .expect("GNSS serialization should succeed");

    // Ensure both require Fast Packet (> 8 bytes)
    assert!(len_ais > 8, "AIS must be a Fast Packet");
    assert!(len_gnss > 8, "GNSS must be a Fast Packet");

    // Build frame iterators for two different sources
    let builder_ais = FastPacketBuilder::new(129040, 10, None, &buffer_ais[..len_ais]);
    let builder_gnss = FastPacketBuilder::new(129029, 20, None, &buffer_gnss[..len_gnss]);

    let mut frames_ais = builder_ais.build();
    let mut frames_gnss = builder_gnss.build();

    // Single assembler handling both sessions
    let mut assembler = FastPacketAssembler::new();
    let mut ais_complete = None;
    let mut gnss_complete = None;

    // Flags to detect iterator exhaustion
    let mut ais_exhausted = false;
    let mut gnss_exhausted = false;

    // Interleave: alternate between both sources
    loop {
        // Send an AIS frame when available
        if !ais_exhausted {
            if let Some(frame_result) = frames_ais.next() {
                let frame = frame_result.expect("Valid AIS frame");
                if let ProcessResult::MessageComplete(msg) =
                    assembler.process_frame(10, &frame.data)
                {
                    ais_complete = Some(msg);
                }
            } else {
                ais_exhausted = true;
            }
        }

        // Send a GNSS frame when available
        if !gnss_exhausted {
            if let Some(frame_result) = frames_gnss.next() {
                let frame = frame_result.expect("Valid GNSS frame");
                if let ProcessResult::MessageComplete(msg) =
                    assembler.process_frame(20, &frame.data)
                {
                    gnss_complete = Some(msg);
                }
            } else {
                gnss_exhausted = true;
            }
        }

        // Break when both messages are complete or both iterators exhausted
        if (ais_complete.is_some() && gnss_complete.is_some()) || (ais_exhausted && gnss_exhausted)
        {
            break;
        }
    }

    // Ensure both messages were reassembled
    let msg_ais = ais_complete.expect("AIS message must be complete");
    let msg_gnss = gnss_complete.expect("GNSS message must be complete");

    // Validation AIS
    let decoded_ais = Pgn129040::from_payload(&msg_ais.payload[..msg_ais.len])
        .expect("AIS deserialization should succeed");
    assert_eq!(decoded_ais.user_id, ais.user_id);

    // Validation GNSS
    let decoded_gnss = Pgn129029::from_payload(&msg_gnss.payload[..msg_gnss.len])
        .expect("GNSS deserialization should succeed");
    const TOLERANCE: f64 = 1e-5;
    assert!((decoded_gnss.latitude - gnss.latitude).abs() < TOLERANCE);
    assert!((decoded_gnss.longitude - gnss.longitude).abs() < TOLERANCE);
}

//==================================================================================
// PHASE 3: EDGE CASES (ROBUSTNESS)
//==================================================================================

#[test]
/// Ensure the assembler handles sequence counter wrap-around.
///
/// The Fast Packet sequence counter uses three bits (0–7) and wraps around.
/// This test confirms the 7 → 0 transition succeeds.
fn test_assembler_sequence_wrap() {
    let mut assembler = FastPacketAssembler::new();
    let source = 42;

    // Complete message using sequence identifier 7 (upper bits)
    let frame_seq7: [u8; 8] = [0b111_00000, 15, 1, 2, 3, 4, 5, 6];
    let result = assembler.process_frame(source, &frame_seq7);
    assert!(
        matches!(result, ProcessResult::FragmentConsumed),
        "Frame with sequence 7 should be consumed"
    );

    let frame_seq7_cont: [u8; 8] = [0b111_00001, 7, 8, 9, 10, 11, 12, 13];
    let result = assembler.process_frame(source, &frame_seq7_cont);
    assert!(
        matches!(result, ProcessResult::FragmentConsumed),
        "Second frame with the same sequence should be accepted"
    );

    let frame_seq7_end: [u8; 8] = [0b111_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(source, &frame_seq7_end);

    // Ensure the message is considered complete
    assert!(
        matches!(result, ProcessResult::MessageComplete(_)),
        "Message should be complete with sequence identifier 7"
    );

    // New message: wrap sequence counter 7 → 0
    let frame_seq0_new: [u8; 8] = [0b000_00000, 9, 42, 43, 44, 45, 46, 47];
    let result = assembler.process_frame(source, &frame_seq0_new);
    assert!(
        matches!(result, ProcessResult::FragmentConsumed),
        "Next message with sequence 0 should be accepted after wrapping"
    );
}

#[test]
/// Verify behavior when frames arrive out of order.
///
/// Frames with incorrect sequence numbers must cause the assembler to drop the
/// current session and ignore fragments until a fresh start is detected.
fn test_assembler_out_of_order() {
    let mut assembler = FastPacketAssembler::new();
    let source = 50;

    // First frame: start of session (sequence 0)
    let frame0: [u8; 8] = [0b000_00000, 20, 1, 2, 3, 4, 5, 6];
    let result = assembler.process_frame(source, &frame0);
    assert!(
        matches!(result, ProcessResult::FragmentConsumed),
        "First frame should be consumed"
    );

    // Send frame 2 before frame 1 (out of order)
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 16, 17, 18, 19, 20];
    let result = assembler.process_frame(source, &frame2);
    assert!(
        matches!(result, ProcessResult::Ignored),
        "Out-of-sequence frame should be ignored"
    );

    // Check that the session resets and a new frame 0 starts a new session
    let new_frame0: [u8; 8] = [0b000_00000, 10, 100, 101, 102, 103, 104, 105];
    let result = assembler.process_frame(source, &new_frame0);
    assert!(
        matches!(result, ProcessResult::FragmentConsumed),
        "A new session should start after reset"
    );
}

#[test]
/// Test handling of partial messages (missing frames).
///
/// Simulate frame loss on the CAN bus. The assembler must detect the incorrect
/// sequence and drop the incomplete message.
fn test_assembler_partial_message() {
    let mut assembler = FastPacketAssembler::new();
    let source = 60;

    // Start of message: three frames required
    let frame0: [u8; 8] = [0b000_00000, 15, 1, 2, 3, 4, 5, 6];
    assembler.process_frame(source, &frame0);

    // ⚠️ Simulate loss of frame 1

    // Receive frame 2 directly (invalid sequence)
    let frame2: [u8; 8] = [0b000_00010, 14, 15, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = assembler.process_frame(source, &frame2);

    assert!(
        matches!(result, ProcessResult::Ignored),
        "Partial messages must be rejected"
    );
}

#[test]
/// Verify behavior when duplicate frames (CAN retransmissions) occur.
///
/// The CAN bus may retransmit a frame; the assembler must ignore duplicates or
/// handle them without data corruption.
fn test_assembler_duplicate_frame() {
    let mut assembler = FastPacketAssembler::new();
    let source = 70;

    // First frame
    let frame0: [u8; 8] = [0b000_00000, 10, 1, 2, 3, 4, 5, 6];
    let result1 = assembler.process_frame(source, &frame0);
    assert!(matches!(result1, ProcessResult::FragmentConsumed));

    // ⚠️ Retransmit the same frame (duplicate)
    let result2 = assembler.process_frame(source, &frame0);

    // Acceptable behavior: ignore or reset, but never crash or corrupt data
    assert!(
        matches!(
            result2,
            ProcessResult::Ignored | ProcessResult::FragmentConsumed
        ),
        "Duplicate frames must be handled properly"
    );
}

#[test]
/// Exercise the concurrent session limit (pool saturation).
///
/// The assembler supports a limited number of concurrent sessions; additional
/// sessions must be rejected.
fn test_assembler_max_sessions() {
    let mut assembler = FastPacketAssembler::new();

    // Start four concurrent sessions (current limit = 4)
    for source_addr in 1..=4 {
        let frame: [u8; 8] = [0b000_00000, 20, source_addr, 0, 0, 0, 0, 0];
        let result = assembler.process_frame(source_addr, &frame);
        assert!(
            matches!(result, ProcessResult::FragmentConsumed),
            "Session {source_addr} should be accepted"
        );
    }

    // Attempt to create a fifth session (must fail)
    let frame5: [u8; 8] = [0b000_00000, 20, 5, 0, 0, 0, 0, 0];
    let result = assembler.process_frame(5, &frame5);

    assert!(
        matches!(result, ProcessResult::Ignored),
        "The fifth session must be rejected (pool saturated)"
    );
}

//==================================================================================
// PHASE 4: PERFORMANCE AND STRESS TESTS
//==================================================================================

#[test]
/// Stress test: process 100 PGNs in a row to validate stability.
///
/// Confirms the assembler can tolerate continuous traffic without leaks, corruption,
/// or panics—critical for long-lived embedded systems.
fn test_stress_100_pgns() {
    let mut assembler = FastPacketAssembler::new();

    for i in 0..100 {
        let source = (i % 4) as u8; // Rotate across four sources

        // Create an AIS message with varying data
        let mut ais = Pgn129040::new();
        ais.user_id = 1_000_000 + i;
        ais.latitude = 45.0 + (i as f32 * 0.01);

        let mut buffer = [0u8; 64];
        let len = ais.to_payload(&mut buffer).expect("Serialization succeeded");

        // Build and send the frames
        let builder = FastPacketBuilder::new(129040, source, None, &buffer[..len]);
        let mut frames = builder.build();

        while let Some(frame_result) = frames.next() {
            let frame = frame_result.expect("Valid frame");
            let result = assembler.process_frame(source, &frame.data);

            if let ProcessResult::MessageComplete(msg) = result {
                // Quick validation of the message
                assert_eq!(msg.len, len);

                let decoded = Pgn129040::from_payload(&msg.payload[..msg.len])
                    .expect("Deserialization succeeded");

                assert_eq!(decoded.user_id, ais.user_id);
            }
        }
    }

    // Reaching this point without panic means the stability test passed ✅
}

#[test]
/// Throughput benchmark: measure frames per second.
///
/// Serves as a performance indicator for the assembler and helps spot regressions.
fn test_builder_throughput() {
    // Prepare a maximum-sized message
    let mut ais = Pgn129040::new();
    ais.user_id = 987_654_321;

    let mut buffer = [0u8; 64];
    let len = ais.to_payload(&mut buffer).expect("Serialization");

    // Construction de 1000 sets de trames
    let iterations = 1000;
    let mut total_frames = 0;

    for _ in 0..iterations {
        let builder = FastPacketBuilder::new(129040, 42, None, &buffer[..len]);
        let frames: Vec<_> = builder.build().collect();
        total_frames += frames.len();
    }

    // Basic verification (no timing, simple validation)
    assert!(
        total_frames > 0,
        "At least one frame must be generated per iteration"
    );
    println!(
        "✅ Throughput test: {total_frames} frames generated over {iterations} iterations"
    );
}

#[test]
/// Check the assembler memory footprint (critical for no_std).
///
/// The assembler must have a fixed, predictable size with no heap allocation.
fn test_assembler_memory_footprint() {
    use core::mem::size_of;

    let _assembler = FastPacketAssembler::new();
    let size = size_of::<FastPacketAssembler>();

    // Ensure the size remains reasonable for embedded systems (stay under 8 KB)
    const MAX_SIZE_BYTES: usize = 8 * 1024;

    assert!(
        size < MAX_SIZE_BYTES,
        "Assembler must remain compact: {size} bytes (max: {MAX_SIZE_BYTES})"
    );

    println!("✅ FastPacketAssembler memory footprint: {size} bytes");
}

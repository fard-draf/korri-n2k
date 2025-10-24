//! Integration tests combining the Fast Packet builder and assembler.
use crate::protocol::transport::fast_packet::{
    assembler::{FastPacketAssembler, ProcessResult},
    builder::FastPacketBuilder,
};

#[test]
/// Validate a round-trip for a modest 15-byte payload.
fn test_roundtrip_15_bytes() {
    let original = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

    // Fragmentation
    let builder = FastPacketBuilder::new(129540, 42, None, &original);
    let mut iter = builder.build();

    // Reassembly
    let mut assembler = FastPacketAssembler::new();
    let mut result = None;

    while let Some(frame_result) = iter.next() {
        let frame = frame_result.unwrap();
        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(42, &frame.data) {
            result = Some(msg);
            break;
        }
    }

    let msg = result.expect("Message complet");
    assert_eq!(msg.len, 15);
    assert_eq!(&msg.payload[..15], &original);
}

#[test]
/// Maximum payload: 223 bytes fragmented and reassembled.
fn test_roundtrip_max_payload() {
    let original = [0x42; 223];

    // PGN 129540 is PDU2 (broadcast)
    let builder = FastPacketBuilder::new(129540, 30, None, &original);
    let mut iter = builder.build();

    let mut assembler = FastPacketAssembler::new();
    let mut result = None;

    while let Some(frame_result) = iter.next() {
        let frame = frame_result.unwrap();
        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(30, &frame.data) {
            result = Some(msg);
            break;
        }
    }

    let msg = result.unwrap();
    assert_eq!(msg.len, 223);
    assert_eq!(&msg.payload[..223], &original);
}

#[test]
/// Interleaved sessions must remain independent.
fn test_roundtrip_with_interleaved_frames() {
    let payload_a = [0xAA; 20];
    let payload_b = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    ];

    let builder_a = FastPacketBuilder::new(129540, 10, None, &payload_a);
    let builder_b = FastPacketBuilder::new(129025, 20, None, &payload_b);

    let mut iter_a = builder_a.build();
    let mut iter_b = builder_b.build();

    let mut assembler = FastPacketAssembler::new();

    // Interleave: A, B, A, B, A...
    loop {
        let mut done_a = false;
        let mut done_b = false;

        if let Some(frame_result) = iter_a.next() {
            assembler.process_frame(10, &frame_result.unwrap().data);
        } else {
            done_a = true;
        }

        if let Some(frame_result) = iter_b.next() {
            let result = assembler.process_frame(20, &frame_result.unwrap().data);
            if let ProcessResult::MessageComplete(msg) = result {
                // Stream B completes first (shorter payload)
                assert_eq!(msg.len, 15);
                assert_eq!(&msg.payload[..15], &payload_b);
            }
        } else {
            done_b = true;
        }

        if done_a && done_b {
            break;
        }
    }
}

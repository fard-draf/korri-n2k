//! Tests for the Fast Packet frame generator (`FrameIterator`).
// BUILDER
use super::*;
use crate::{error::CanIdBuildError, protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD};

#[test]
/// Short payload: remains a single classic CAN frame (no Fast Packet).
fn test_builder_single_frame() {
    let payload = [1, 2, 3, 4, 5];
    let builder = FastPacketBuilder::new(129025, 42, None, &payload);
    let mut iter = builder.build();

    let frame = iter.next().unwrap().unwrap();
    assert_eq!(frame.len, 5);
    assert_eq!(&frame.data[..5], &payload);

    // Should be the only frame
    assert!(iter.next().is_none());
}

#[test]
/// Ten-byte payload split across two Fast Packet frames.
fn test_builder_two_frames() {
    // 10 bytes → 2 frames (6+4)
    let payload = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let builder = FastPacketBuilder::new(129040, 50, None, &payload).with_sequence_id(0);
    let mut iter = builder.build();

    // Trame 0
    let frame0 = iter.next().unwrap().unwrap();
    assert_eq!(frame0.data[0], 0); // seq_id
    assert_eq!(frame0.data[1], 10); // length
    assert_eq!(&frame0.data[2..8], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(frame0.len, 8);

    // Trame 1
    let frame1 = iter.next().unwrap().unwrap();
    assert_eq!(frame1.data[0], 1); // seq_id
    assert_eq!(&frame1.data[1..5], &[7, 8, 9, 10]);
    assert_eq!(frame1.len, 5);

    assert!(iter.next().is_none());
}

#[test]
/// Maximum payload: ensures 32 frames are produced.
fn test_builder_max_payload() {
    let payload = [0xAA; 223]; // Max Fast Packet
                               // PGN 129540 (GNSS Sats) is PDU2 (broadcast), no destination needed
    let builder = FastPacketBuilder::new(129540, 25, None, &payload);
    let mut iter = builder.build();

    // First frame
    let frame0 = iter.next().unwrap().unwrap();
    assert_eq!(frame0.data[1], 223); // total length

    // Count the frames
    let mut count = 1;
    while iter.next().is_some() {
        count += 1;
    }

    // 6 + 31*7 = 223 → 32 frames
    assert_eq!(count, 32);
}

#[test]
/// Destination-aware PGNs keep their target in the generated frames.
fn test_builder_with_destination() {
    let payload = [1, 2, 3];
    let builder = FastPacketBuilder::new(59904, 42, Some(50), &payload);
    let mut iter = builder.build();

    let frame = iter.next().unwrap().unwrap();
    assert_eq!(frame.id.destination(), Some(50));
}

#[test]
/// Oversized payload: returns an error and stops the iteration.
fn test_builder_payload_too_large() {
    let payload = [0x11; MAX_FAST_PACKET_PAYLOAD + 1];
    let builder = FastPacketBuilder::new(129540, 42, None, &payload);
    let mut iter = builder.build();

    let err = iter.next().unwrap().unwrap_err();
    assert!(matches!(err, CanIdBuildError::InvalidData));
    assert!(iter.next().is_none());
}

#[test]
/// Consecutive messages must receive distinct Fast Packet sequence identifiers.
fn test_builder_sequence_id_progresses_between_messages() {
    let payload = [0x55; 10];

    let mut first_iter = FastPacketBuilder::new(129040, 50, None, &payload).build();
    let first_header = first_iter.next().unwrap().unwrap().data[0];

    let mut second_iter = FastPacketBuilder::new(129040, 50, None, &payload).build();
    let second_header = second_iter.next().unwrap().unwrap().data[0];

    // Bits 5-7: sequence identifier, bits 0-4: frame index (0 for the first frame)
    assert_eq!(first_header & 0x1F, 0);
    assert_eq!(second_header & 0x1F, 0);
    assert_ne!(first_header & 0xE0, second_header & 0xE0);
}

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

#[test]
/// A Fast Packet PGN keeps its header however short its payload is.
fn test_builder_short_fast_packet_pgn_keeps_header() {
    let payload = [0xA1, 0xA2, 0xA3];
    let mut iter = FastPacketBuilder::new(129540, 42, None, &payload)
        .with_sequence_id(0)
        .build();

    let frame = iter.next().unwrap().unwrap();
    assert_eq!(frame.data[0], 0); // sequence 0, frame 0
    assert_eq!(frame.data[1], 3); // total length
    assert_eq!(&frame.data[2..5], &payload);
    assert!(iter.next().is_none());
}

#[test]
/// A 7-byte Fast Packet PGN is 6 bytes in the first frame and 1 in the second.
fn test_builder_seven_byte_fast_packet_pgn_takes_two_frames() {
    let payload = [1, 2, 3, 4, 5, 6, 7];
    let mut iter = FastPacketBuilder::new(129540, 42, None, &payload)
        .with_sequence_id(0)
        .build();

    let frame0 = iter.next().unwrap().unwrap();
    assert_eq!(frame0.data[1], 7);
    assert_eq!(&frame0.data[2..8], &payload[..6]);

    let frame1 = iter.next().unwrap().unwrap();
    assert_eq!(frame1.data[0], 1); // sequence 0, frame 1
    assert_eq!(frame1.data[1], 7);

    assert!(iter.next().is_none());
}

#[test]
/// A PGN outside the Fast Packet table stays a raw frame up to 8 bytes.
fn test_builder_non_fast_packet_pgn_stays_single_frame_at_eight_bytes() {
    let payload = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut iter = FastPacketBuilder::new(129025, 42, None, &payload).build();

    let frame = iter.next().unwrap().unwrap();
    assert_eq!(frame.len, 8);
    assert_eq!(frame.data, payload);
    assert!(iter.next().is_none());
}

#[test]
/// A PGN outside the table still falls back to Fast Packet above 8 bytes.
fn test_builder_unlisted_pgn_above_eight_bytes_is_fast_packet() {
    let payload = [0x33; 20];
    let mut iter = FastPacketBuilder::new(129025, 42, None, &payload)
        .with_sequence_id(0)
        .build();

    let frame = iter.next().unwrap().unwrap();
    assert_eq!(frame.data[1], 20);
    assert_eq!(frame.len, 8);
}

#[test]
/// An identifier error ends the iteration: `for f in it { if let Ok(f) = f {..} }` must not spin.
fn test_builder_identifier_error_is_yielded_once() {
    // PGN 59904 is destination-specific: no destination is an error.
    let mut iter = FastPacketBuilder::new(59904, 42, None, &[1u8; 20]).build();

    assert!(matches!(
        iter.next(),
        Some(Err(CanIdBuildError::InvalidForBroadcast))
    ));
    assert!(iter.next().is_none());
}

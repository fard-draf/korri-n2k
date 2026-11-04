//! CANboat 9.1.0 appended a Sequence ID to PGN 129794 (75 -> 76 bytes) and
//! published `MinLength: 75` for devices still sending the older layout.
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn129794;

fn full_payload() -> ([u8; 76], usize) {
    let mut pgn = Pgn129794::new();
    pgn.sequence_id = 42;
    let mut buf = [0u8; 76];
    let len = pgn.to_payload(&mut buf).expect("encode");
    (buf, len)
}

#[test]
fn full_length_reads_the_trailing_field() {
    let (buf, len) = full_payload();
    assert_eq!(len, 76);
    assert_eq!(Pgn129794::from_payload(&buf).unwrap().sequence_id, 42);
}

#[test]
fn min_length_decodes_with_missing_tail_not_available() {
    let (buf, _) = full_payload();
    let pgn = Pgn129794::from_payload(&buf[..75]).expect("MinLength payload");
    assert_eq!(pgn.sequence_id, 0xFF); // absent, read as "not available"
}

#[test]
fn below_min_length_is_rejected() {
    let (buf, _) = full_payload();
    assert!(Pgn129794::from_payload(&buf[..74]).is_err());
}

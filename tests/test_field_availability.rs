//! Field sentinel availability (`FieldDescriptor::availability` and `string_fix_trimmed`).
use korri_n2k::core::{Availability, FieldDescriptor};
use korri_n2k::infra::codec::engine::string_fix_trimmed;
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::{descriptor, Pgn129794};

fn field(pgn: u32, id: &str) -> &'static FieldDescriptor {
    descriptor(pgn)
        .unwrap_or_else(|| panic!("missing descriptor for {pgn}"))
        .fields
        .iter()
        .find(|f| f.id.eq_ignore_ascii_case(id))
        .unwrap_or_else(|| panic!("missing field {id} in PGN {pgn}"))
}

#[test]
fn unsigned_32bit_depth_sentinels() {
    let depth = field(128267, "Depth");
    assert_eq!(
        depth.availability(4_294_967_295),
        Availability::NotAvailable
    );
    assert_eq!(depth.availability(4_294_967_294), Availability::OutOfRange);
    assert_eq!(depth.availability(4_294_967_293), Availability::Reserved);
    assert_eq!(depth.availability(520), Availability::Available);
}

#[test]
fn signed_16bit_offset_sentinels() {
    let offset = field(128267, "Offset");
    assert_eq!(offset.availability(32767), Availability::NotAvailable);
    assert_eq!(offset.availability(32766), Availability::OutOfRange);
    assert_eq!(offset.availability(32765), Availability::Reserved);
    // 0x8000 (-32768) is a valid negative reading, not a sentinel.
    assert_eq!(offset.availability(0x8000), Availability::Available);
}

#[test]
fn lookup_and_exception_sentinels() {
    let dev_lower = field(60928, "DeviceInstanceLower");
    assert_eq!(dev_lower.availability(7), Availability::Available);

    let wind_ref = field(130306, "Reference");
    assert_eq!(wind_ref.availability(7), Availability::NotAvailable);
    assert_eq!(wind_ref.availability(2), Availability::Available);
}

#[test]
fn mmsi_zero_and_all_ones_are_not_available() {
    let user_id = field(129038, "UserId");
    assert_eq!(user_id.availability(0), Availability::NotAvailable);
    assert_eq!(
        user_id.availability(4_294_967_295),
        Availability::NotAvailable
    );
    assert_eq!(user_id.availability(227_123_456), Availability::Available);
}

#[test]
fn min_length_padded_tail_reads_as_not_available() {
    let mut pgn = Pgn129794::new();
    pgn.sequence_id = 42;
    let mut buf = [0u8; 76];
    let len = pgn.to_payload(&mut buf).expect("encode 129794");
    assert_eq!(len, 76);

    let decoded = Pgn129794::from_payload(&buf[..75]).expect("decode 75-byte 129794");
    let seq_desc = field(129794, "SequenceId");
    assert_eq!(
        seq_desc.availability(decoded.sequence_id as u64),
        Availability::NotAvailable
    );
}

#[test]
fn string_fix_trimming_rules() {
    assert_eq!(string_fix_trimmed(b"ABC@@@"), b"ABC");
    assert_eq!(string_fix_trimmed(&[0xFF; 8]), b"");
    assert_eq!(string_fix_trimmed(b"\0trailing"), b"");
    assert_eq!(string_fix_trimmed(b"KORRI\xFF\xFF"), b"KORRI");
}

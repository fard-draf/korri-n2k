//! Ensure PGN 127489 decodes its two 16-bit BITLOOKUP fields.
//! A bitmask is a plain unsigned integer, so a 16-bit one must round-trip
//! through `PgnValue::U16`. The generated setter used to demand `U8` for every
//! BITLOOKUP regardless of width, which made those fields fail to deserialize.

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn127489;

/// Discrete Status 1 sits at bit 160 (byte 20), Status 2 at bit 176 (byte 22),
/// both 16 bits little-endian, in a 26-byte fast-packet payload.
fn payload(status_1: u16, status_2: u16) -> [u8; 26] {
    let mut p = [0u8; 26];
    p[20..22].copy_from_slice(&status_1.to_le_bytes());
    p[22..24].copy_from_slice(&status_2.to_le_bytes());
    p
}

#[test]
fn wide_bitlookup_fields_survive_a_decode() {
    let pgn = Pgn127489::from_payload(&payload(0xBEEF, 0x1234)).expect("decode 127489");

    // Both bytes must reach the field: a U8-only setter would drop the high one.
    assert_eq!(pgn.discrete_status1, 0xBEEF);
    assert_eq!(pgn.discrete_status2, 0x1234);
}

#[test]
fn wide_bitlookup_fields_round_trip() {
    let decoded = Pgn127489::from_payload(&payload(0xBEEF, 0x1234)).expect("decode 127489");

    let mut buffer = [0u8; 26];
    decoded.to_payload(&mut buffer).expect("encode 127489");

    assert_eq!(&buffer[20..24], &payload(0xBEEF, 0x1234)[20..24]);
}

#[test]
fn bitlookup_fields_are_sixteen_bits_wide() {
    let pgn = Pgn127489::new();
    let _: u16 = pgn.discrete_status1;
    let _: u16 = pgn.discrete_status2;
}

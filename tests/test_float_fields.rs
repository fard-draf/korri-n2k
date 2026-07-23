//! CANboat FLOAT fields are raw IEEE-754 singles. The engine handled no such
//! field, so every PGN carrying one failed with `UnsupportedFieldKind`, and the
//! generator typed them as scaled signed integers.
#![cfg(feature = "full-pgns")]

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn130321;

/// PGN 130321 (Salinity), 26 bytes. `Salinity` is a FLOAT at bit offset 120.
fn payload(salinity: f32) -> [u8; 26] {
    let mut p = [0u8; 26];
    p[0] = 0; // Mode + reserved
    p[15..19].copy_from_slice(&salinity.to_bits().to_le_bytes());
    p
}

#[test]
fn float_field_decodes_as_ieee754() {
    let pgn = Pgn130321::from_payload(&payload(35.5)).expect("decode 130321");
    assert_eq!(pgn.salinity, 35.5f32);
}

#[test]
fn negative_float_survives_a_round_trip() {
    let pgn = Pgn130321::from_payload(&payload(-1.25)).expect("decode 130321");
    assert_eq!(pgn.salinity, -1.25f32);

    let mut out = [0u8; 26];
    pgn.to_payload(&mut out).expect("encode 130321");
    assert_eq!(&out[15..19], &(-1.25f32).to_bits().to_le_bytes());
}

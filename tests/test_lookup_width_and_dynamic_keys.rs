//! Two generator/engine disagreements over a field's width.
//!
//! A lookup enum sizes its repr on the CANboat `MaxValue`, which is a declared
//! range, not the width of the field carrying it: `ENTERTAINMENT_PLAY_STATUS`
//! declares 65535 yet rides in 8 bits. The accessors now follow the wire.
//!
//! `DYNAMIC_FIELD_KEY` and friends are fixed-width integers despite the naming;
//! they were mapped to `Unimplemented` and rejected outright.
#![cfg(feature = "full-pgns")]

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::lookups::EntertainmentPlayStatus;
use korri_n2k::protocol::messages::{Pgn130569, Pgn130833};

#[test]
fn eight_bit_lookup_with_a_wide_repr_decodes() {
    // PlayStatus sits at bit 56 (byte 7). 2 = Stop in ENTERTAINMENT_PLAY_STATUS.
    let mut payload = [0u8; 40];
    payload[7] = 2;

    let pgn = Pgn130569::from_payload(&payload).expect("decode 130569");
    assert_eq!(pgn.play_status, EntertainmentPlayStatus::Stop);
}

#[test]
fn unnamed_value_of_a_narrow_lookup_is_kept() {
    let mut payload = [0u8; 40];
    payload[7] = 200; // outside the enumerated set, still only 8 bits wide

    let pgn = Pgn130569::from_payload(&payload).expect("decode 130569");
    assert_eq!(pgn.play_status, EntertainmentPlayStatus::Unrecognized(200));
}

#[test]
fn dynamic_field_key_is_a_plain_integer() {
    // Manufacturer 381 (B&G) + industry 4 select the variant; DataType is a
    // 12-bit DYNAMIC_FIELD_KEY at bit 16, Length a 4-bit field at bit 28.
    let mut payload = [0u8; 30];
    let header: u16 = 381 | (0b11 << 11) | (4 << 13);
    payload[0..2].copy_from_slice(&header.to_le_bytes());
    let key: u16 = 0x123;
    let length: u16 = 9;
    payload[2..4].copy_from_slice(&(key | (length << 12)).to_le_bytes());

    let decoded = Pgn130833::from_payload(&payload).expect("decode 130833");
    let Pgn130833::BGUserAndRemoteRename(pgn) = decoded else {
        panic!("wrong variant: {decoded:?}");
    };
    assert_eq!(pgn.data_type, 0x123);
    assert_eq!(pgn.length, 9);
}

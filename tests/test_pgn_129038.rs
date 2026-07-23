//! AIS Class A position reports must decode. Two defects made them impossible:
//! the 19-bit `CommunicationState` was a BINARY field the engine rejected for
//! not being byte-aligned, and `TimeStamp=30` — an ordinary count of seconds —
//! was refused because CANboat only names the values 60..=63.

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::lookups::TimeStamp;
use korri_n2k::protocol::messages::Pgn129038;

/// MessageId=1, TimeStamp=30 s, CommunicationState=0x12345 (19 bits).
const FRAME: [u8; 28] = [
    0x01, 0x0B, 0x85, 0x8C, 0x0E, 0xC0, 0x3B, 0x47, 0x03, 0x80, 0x47, 0xA1, 0x19, 0x79, 0x08, 0x07,
    0x26, 0x02, 0x45, 0x23, 0x01, 0x08, 0x07, 0x00, 0x00, 0x00, 0x00, 0x07,
];

#[test]
fn realistic_frame_decodes() {
    let pgn = Pgn129038::from_payload(&FRAME).expect("decode 129038");

    // Sub-byte BINARY field, read as a scalar.
    assert_eq!(pgn.communication_state, 0x12345);
    // Value outside the enumerated set is kept, not rejected.
    assert_eq!(pgn.time_stamp, TimeStamp::Unrecognized(30));
}

#[test]
fn named_timestamp_still_maps_to_its_variant() {
    let mut frame = FRAME;
    frame[13] = (frame[13] & 0x03) | (60 << 2);

    let pgn = Pgn129038::from_payload(&frame).expect("decode 129038");
    assert_eq!(pgn.time_stamp, TimeStamp::NotAvailable);
}

#[test]
fn communication_state_round_trips() {
    let pgn = Pgn129038::from_payload(&FRAME).expect("decode 129038");

    let mut out = [0u8; 28];
    pgn.to_payload(&mut out).expect("encode 129038");

    // Bits 144..163 hold CommunicationState; bytes 18..20 cover them.
    assert_eq!(&out[18..20], &FRAME[18..20]);
}

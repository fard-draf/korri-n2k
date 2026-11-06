//! Regressions for the codec defects found by the 2026-11 audit.
//!
//! Every expected byte string is written by hand from the CANboat layout: the
//! encoder is never its own oracle. Tests on PGNs the default manifest does not
//! carry are gated on `full-pgns`.
#![allow(unused_imports)]

use korri_n2k::core::{PgnBytes, PgnValue};
use korri_n2k::infra::codec::engine::for_each_field;
use korri_n2k::infra::codec::traits::{FieldAccess, PgnData};
use korri_n2k::protocol::messages::*;

/// `PgnBytes` holding `content` (LAU: encoding byte, then text).
#[cfg(feature = "full-pgns")]
fn bytes(content: &[u8]) -> PgnBytes {
    let mut b = PgnBytes::default();
    b.copy_from_slice(content);
    b
}

// ---------------------------------------------------------------------------
// AUD-002 — a field that follows a repeating group is laid out after it.
// ---------------------------------------------------------------------------

/// 130571 Library Data Group: 14 bytes of fixed fields (ID count at bytes 10-11),
/// then the group `IdType`(8) `Id`(32) `Name`(LAU), then `Artist`(LAU).
#[cfg(feature = "full-pgns")]
const LIBRARY_DATA_GROUP: [u8; 25] = {
    let mut v = [0u8; 25];
    v[10] = 1; // ID count
    let tail = [2, 1, 2, 3, 4, 3, 1, b'N', 3, 1, b'A'];
    let mut i = 0;
    while i < tail.len() {
        v[14 + i] = tail[i];
        i += 1;
    }
    v
};

#[cfg(feature = "full-pgns")]
#[test]
fn aud_002_group_tail_field_is_decoded_after_the_group() {
    let decoded = Pgn130571::from_payload(&LIBRARY_DATA_GROUP).expect("valid 130571");
    assert_eq!(decoded.repetitive_count("id_types"), Some(1));
    assert_eq!(
        decoded.repetitive_field("id_types", 0, "IdType"),
        Some(PgnValue::U8(2))
    );
    assert_eq!(
        decoded.repetitive_field("id_types", 0, "Id"),
        Some(PgnValue::U32(0x0403_0201))
    );
    assert_eq!(
        decoded.repetitive_field("id_types", 0, "Name"),
        Some(PgnValue::Bytes(bytes(&[1, b'N'])))
    );
    assert_eq!(
        decoded.field("Artist"),
        Some(PgnValue::Bytes(bytes(&[1, b'A'])))
    );
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_002_group_tail_field_is_encoded_after_the_group() {
    let mut pgn = Pgn130571::new();
    assert_eq!(pgn.set_repetitive_count("id_types", 1), Some(()));
    assert_eq!(
        pgn.repetitive_field_mut("id_types", 0, "IdType", PgnValue::U8(2)),
        Some(())
    );
    assert_eq!(
        pgn.repetitive_field_mut("id_types", 0, "Id", PgnValue::U32(0x0403_0201)),
        Some(())
    );
    assert_eq!(
        pgn.repetitive_field_mut("id_types", 0, "Name", PgnValue::Bytes(bytes(&[1, b'N']))),
        Some(())
    );
    assert_eq!(
        pgn.field_mut("Artist", PgnValue::Bytes(bytes(&[1, b'A']))),
        Some(())
    );
    let mut buffer = [0u8; 64];
    let written = pgn.to_payload(&mut buffer).expect("encode 130571");
    assert_eq!(written, 25);
    assert_eq!(&buffer[10..12], &[1, 0], "ID count");
    assert_eq!(&buffer[14..written], &LIBRARY_DATA_GROUP[14..]);
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_002_visitor_reports_group_fields_before_the_tail_field() {
    let mut seen = Vec::new();
    for_each_field(
        descriptor(130571).expect("130571 descriptor"),
        &LIBRARY_DATA_GROUP,
        |field, reading| seen.push((field.id, reading.repetition.map(|(_, index)| index))),
    )
    .expect("visit 130571");
    assert_eq!(
        &seen[6..],
        &[
            ("IdCount", None),
            ("TotalIdCount", None),
            ("IdType", Some(0)),
            ("Id", Some(0)),
            ("Name", Some(0)),
            ("Artist", None),
        ]
    );
}

// ---------------------------------------------------------------------------
// AUD-003 — repetitions past the generated capacity.
// ---------------------------------------------------------------------------

/// 130944 Airmar POST with `pairs` TestId/TestResult pairs (pair i = `i`, `100 + i`).
#[cfg(feature = "full-pgns")]
fn airmar_post(pairs: u8) -> Vec<u8> {
    let mut payload = vec![0x87, 0x98, 0xfe, pairs];
    for i in 0..pairs {
        payload.extend([i, 100 + i]);
    }
    payload
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_003_more_repetitions_than_the_capacity_is_an_error() {
    let payload = airmar_post(33);
    assert!(Pgn130944::from_payload(&payload).is_err());
    let visited = for_each_field(
        descriptor_for(130944, &payload).expect("130944 descriptor"),
        &payload,
        |_, _| {},
    );
    assert!(visited.is_err());
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_003_the_capacity_itself_is_kept_whole() {
    let payload = airmar_post(32);
    let decoded = Pgn130944::from_payload(&payload).expect("32 pairs fit");
    let mut buffer = [0u8; 128];
    let written = decoded.to_payload(&mut buffer).expect("encode 32 pairs");
    assert_eq!(&buffer[..written], &payload[..]);
    let mut last = None;
    for_each_field(
        descriptor_for(130944, &payload).expect("130944 descriptor"),
        &payload,
        |field, reading| {
            if field.id == "TestResult" {
                last = Some(reading.value);
            }
        },
    )
    .expect("visit 32 pairs");
    assert_eq!(last, Some(PgnValue::U8(131)));
}

//! Regressions for the codec defects found by the 2026-11 audit.
//!
//! Every expected byte string is written by hand from the CANboat layout: the
//! encoder is never its own oracle. Tests on PGNs the default manifest does not
//! carry are gated on `full-pgns`.
#![allow(unused_imports)]

use korri_n2k::core::{Availability, PgnBytes, PgnValue};
use korri_n2k::infra::codec::engine::for_each_field;
use korri_n2k::infra::codec::traits::{FieldAccess, PgnData};
use korri_n2k::protocol::messages::*;

/// `PgnBytes` holding `content` (LAU: encoding byte, then text).
fn bytes_of(content: &[u8]) -> PgnBytes {
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
        Some(PgnValue::Bytes(bytes_of(&[1, b'N'])))
    );
    assert_eq!(
        decoded.field("Artist"),
        Some(PgnValue::Bytes(bytes_of(&[1, b'A'])))
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
        pgn.repetitive_field_mut("id_types", 0, "Name", PgnValue::Bytes(bytes_of(&[1, b'N']))),
        Some(())
    );
    assert_eq!(
        pgn.field_mut("Artist", PgnValue::Bytes(bytes_of(&[1, b'A']))),
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
// ---------------------------------------------------------------------------
// AUD-005 — the polymorphic enum forwards the repeating-group accessors.
// ---------------------------------------------------------------------------

#[cfg(feature = "full-pgns")]
#[test]
fn aud_005_polymorphic_enum_delegates_the_repeating_group() {
    let payload = airmar_post(2);
    let mut decoded = Pgn130944::from_payload(&payload).expect("valid 130944");
    assert_eq!(decoded.repetitive_count("test_ids"), Some(2));
    assert_eq!(
        decoded.repetitive_field("test_ids", 1, "TestResult"),
        Some(PgnValue::U8(101))
    );
    assert_eq!(
        decoded.repetitive_field_mut("test_ids", 1, "TestResult", PgnValue::U8(7)),
        Some(())
    );
    assert_eq!(
        decoded.repetitive_field("test_ids", 1, "TestResult"),
        Some(PgnValue::U8(7))
    );
    assert_eq!(decoded.set_repetitive_count("test_ids", 1), Some(()));
    assert_eq!(decoded.repetitive_count("test_ids"), Some(1));
    assert_eq!(decoded.repetitive_field("test_ids", 1, "TestResult"), None);
}

// ---------------------------------------------------------------------------
// AUD-008 — a short fixed string is required in full; a long text is a capacity.
// ---------------------------------------------------------------------------

/// 129044 Datum is 20 bytes: two 4-byte strings around three 32-bit numbers.
#[test]
fn aud_008_short_fixed_string_truncated_by_the_payload_is_rejected() {
    for len in 16..20 {
        let payload = vec![0u8; len];
        assert!(
            Pgn129044::from_payload(&payload).is_err(),
            "{len} bytes decoded"
        );
        let visited = for_each_field(
            descriptor(129044).expect("129044 descriptor"),
            &payload,
            |_, _| {},
        );
        assert!(visited.is_err(), "{len} bytes visited");
    }
}

#[test]
fn aud_008_full_length_datum_is_accepted() {
    let payload = [0u8; 20];
    assert!(Pgn129044::from_payload(&payload).is_ok());
    let mut visited = 0;
    for_each_field(
        descriptor(129044).expect("129044 descriptor"),
        &payload,
        |_, _| visited += 1,
    )
    .expect("visit 129044");
    assert_eq!(visited, 5);
}

/// Navico 130821 reserves 230 bytes for its text and sends fewer: the capture
/// of the replay test relies on it. Header `13 99` is manufacturer 275, industry 4.
#[test]
fn aud_008_short_navico_text_keeps_decoding() {
    let payload = [0x13, 0x99, 150, b'h', b'i'];
    let decoded = Pgn130821::from_payload(&payload).expect("short 130821");
    assert_eq!(decoded.field("A"), Some(PgnValue::U8(150)));
    match decoded.field("Message") {
        Some(PgnValue::Bytes(text)) => assert_eq!(&text.as_slice()[..2], b"hi"),
        other => panic!("unexpected message {other:?}"),
    }
    let mut texts = Vec::new();
    for_each_field(
        descriptor_for(130821, &payload).expect("130821 descriptor"),
        &payload,
        |field, reading| {
            if field.id == "Message" {
                texts.push(reading.value);
            }
        },
    )
    .expect("visit short 130821");
    assert_eq!(texts, vec![PgnValue::Bytes(bytes_of(b"hi"))]);
}

/// Simnet 130856: header `41 9f` is manufacturer 1857, industry 4; then
/// alarm id (16 bits), two bytes, and a 223-byte text sent shorter.
#[cfg(feature = "full-pgns")]
#[test]
fn aud_008_short_simnet_text_keeps_decoding() {
    let payload = [0x41, 0x9f, 1, 0, 2, 3, b'h', b'i'];
    let decoded = Pgn130856::from_payload(&payload).expect("short 130856");
    match decoded.field("Text") {
        Some(PgnValue::Bytes(text)) => assert_eq!(&text.as_slice()[..2], b"hi"),
        other => panic!("unexpected text {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// AUD-009 — NaN and text-less variable strings are "not available".
// ---------------------------------------------------------------------------

/// Availability of every field of `payload`, in wire order.
fn availabilities(pgn: u32, payload: &[u8]) -> Vec<(&'static str, Availability, PgnValue)> {
    let mut seen = Vec::new();
    for_each_field(
        descriptor_for(pgn, payload).expect("descriptor"),
        payload,
        |field, reading| seen.push((field.id, reading.availability, reading.value)),
    )
    .expect("visit");
    seen
}

/// 126998 carries three LAU strings. An empty one is `02 01` (length 2, ASCII)
/// per CANboat; a lone `00` is the older form.
#[test]
fn aud_009_empty_lau_strings_are_not_available() {
    for payload in [[2, 1, 2, 1, 2, 1].as_slice(), [0, 0, 0].as_slice()] {
        let seen = availabilities(126998, payload);
        assert_eq!(seen.len(), 3);
        for (id, availability, _) in &seen {
            assert_eq!(*availability, Availability::NotAvailable, "{id}");
        }
    }
    // The value is still exposed: the encoding byte is kept.
    let seen = availabilities(126998, &[2, 1, 2, 1, 2, 1]);
    assert_eq!(seen[0].2, PgnValue::Bytes(bytes_of(&[1])));
}

#[test]
fn aud_009_lau_string_with_text_is_available() {
    let seen = availabilities(126998, &[3, 1, b'A', 2, 1, 2, 1]);
    assert_eq!(seen[0].1, Availability::Available);
    assert_eq!(seen[1].1, Availability::NotAvailable);
    assert_eq!(seen[2].1, Availability::NotAvailable);
}

/// 130321 Salinity Station Data: 23 bytes of fixed fields (salinity FLOAT at
/// bytes 15-18), then two LAU strings.
#[cfg(feature = "full-pgns")]
fn salinity_station(salinity: [u8; 4]) -> Vec<u8> {
    let mut payload = vec![0u8; 15];
    payload.extend(salinity);
    payload.extend([0, 0]); // water temperature
    payload.extend([2, 1]); // station id: empty
    payload.extend([3, 1, b'S']); // station name: "S"
    payload
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_009_nan_float_is_not_available() {
    let nan = salinity_station([0x00, 0x00, 0xC0, 0x7F]);
    let seen = availabilities(130321, &nan);
    let salinity = seen.iter().find(|(id, _, _)| *id == "Salinity").unwrap();
    assert_eq!(salinity.1, Availability::NotAvailable);
    assert!(matches!(salinity.2, PgnValue::F32(v) if v.is_nan()));
    let station_id = seen.iter().find(|(id, _, _)| *id == "StationId").unwrap();
    assert_eq!(station_id.1, Availability::NotAvailable);
    let station_name = seen.iter().find(|(id, _, _)| *id == "StationName").unwrap();
    assert_eq!(station_name.1, Availability::Available);

    // 35.0 psu is IEEE 0x420C0000: a real measurement stays available.
    let measured = salinity_station([0x00, 0x00, 0x0C, 0x42]);
    let seen = availabilities(130321, &measured);
    let salinity = seen.iter().find(|(id, _, _)| *id == "Salinity").unwrap();
    assert_eq!(salinity.1, Availability::Available);
    assert_eq!(salinity.2, PgnValue::F32(35.0));
}

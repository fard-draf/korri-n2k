//! Regressions for the codec defects found by the 2026-11 audit.
//!
//! Every expected byte string is written by hand from the CANboat layout: the
//! encoder is never its own oracle. Tests on PGNs the default manifest does not
//! carry are gated on `full-pgns`.
#![allow(unused_imports)]

use korri_n2k::core::{Availability, PgnBytes, PgnValue};
use korri_n2k::error::SerializationError;
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

// ---------------------------------------------------------------------------
// AUD-001 — a `PgnBytes::len` past its buffer is refused, never indexed.
// ---------------------------------------------------------------------------

const OVERSIZED_LENGTHS: [usize; 3] = [231, 255, usize::MAX];

#[test]
fn aud_001_lau_length_past_capacity_is_invalid_data() {
    for len in OVERSIZED_LENGTHS {
        let mut pgn = Pgn126998::new();
        pgn.installation_description1.len = len;
        let mut buffer = [0u8; 1024];
        assert!(
            matches!(
                pgn.to_payload(&mut buffer),
                Err(SerializationError::InvalidData)
            ),
            "len {len}"
        );
    }
}

#[test]
fn aud_001_lau_at_capacity_is_written() {
    let mut pgn = Pgn126998::new();
    pgn.installation_description1.len = 230;
    pgn.installation_description1.data[0] = 1;
    pgn.installation_description1.data[1..230].fill(b'x');
    let mut buffer = [0u8; 1024];
    let written = pgn.to_payload(&mut buffer).expect("230 bytes fit");
    // length byte (231 = 230 + 1), encoding, 229 characters, then two empty strings
    // of `02 01` each.
    assert_eq!(buffer[0], 231);
    assert_eq!(buffer[1], 1);
    assert!(buffer[2..231].iter().all(|&b| b == b'x'));
    assert_eq!(written, 231 + 4);
}

#[cfg(feature = "full-pgns")]
#[test]
fn aud_001_lz_length_past_capacity_is_invalid_data() {
    for len in OVERSIZED_LENGTHS {
        let mut zone_name = Pgn126720FusionSetZoneName::new();
        zone_name.name.len = len;
        let pgn = Pgn126720::FusionSetZoneName(zone_name);
        let mut buffer = [0u8; 1024];
        assert!(
            matches!(
                pgn.to_payload(&mut buffer),
                Err(SerializationError::InvalidData)
            ),
            "len {len}"
        );
    }
}

#[test]
fn aud_001_pgn_bytes_views_stay_inside_the_buffer() {
    for len in OVERSIZED_LENGTHS {
        let mut bytes = PgnBytes {
            len,
            ..PgnBytes::default()
        };
        assert_eq!(bytes.as_slice().len(), 230);
        assert_eq!(bytes.as_mut_slice().len(), 230);
    }
}

// ---------------------------------------------------------------------------
// AUD-007 — an empty LAU string is written `02 01`, and a lone `00` is still read.
// ---------------------------------------------------------------------------

#[test]
fn aud_007_empty_lau_strings_are_written_as_length_two_and_ascii() {
    let mut buffer = [0u8; 64];
    let written = Pgn126998::new().to_payload(&mut buffer).expect("encode");
    assert_eq!(&buffer[..written], &[2, 1, 2, 1, 2, 1]);
}

#[test]
fn aud_007_empty_lau_between_two_texts_keeps_its_place() {
    let mut pgn = Pgn126998::new();
    pgn.installation_description1 = bytes_of(&[1, b'A']);
    pgn.manufacturer_information = bytes_of(&[1, b'B', b'C']);
    let mut buffer = [0u8; 64];
    let written = pgn.to_payload(&mut buffer).expect("encode");
    assert_eq!(&buffer[..written], &[3, 1, b'A', 2, 1, 4, 1, b'B', b'C']);
}

#[test]
fn aud_007_lone_zero_lau_is_still_read_as_empty() {
    let decoded = Pgn126998::from_payload(&[0, 0, 0]).expect("legacy empty strings");
    assert!(decoded.installation_description1.is_empty());
    assert!(decoded.installation_description2.is_empty());
    assert!(decoded.manufacturer_information.is_empty());
    let decoded = Pgn126998::from_payload(&[2, 1, 2, 1, 2, 1]).expect("canonical empty strings");
    assert_eq!(decoded.installation_description1, bytes_of(&[1]));
}

// ---------------------------------------------------------------------------
// LZ — the terminating zero is written, and read when present or absent.
// ---------------------------------------------------------------------------

/// 126720 Fusion Set Zone Name: header `a3 99` (Fusion, marine), id 34, zone 1,
/// then the LZ name: length 1, `X`, terminating zero.
#[cfg(feature = "full-pgns")]
const ZONE_NAME_X: [u8; 8] = [0xa3, 0x99, 0x22, 0x00, 1, 1, b'X', 0];

#[cfg(feature = "full-pgns")]
fn zone_name(payload: &[u8]) -> PgnBytes {
    match Pgn126720::from_payload(payload).expect("126720 zone name") {
        Pgn126720::FusionSetZoneName(inner) => inner.name,
        other => panic!("unexpected variant {other:?}"),
    }
}

#[cfg(feature = "full-pgns")]
#[test]
fn lz_terminating_zero_is_written() {
    let decoded = Pgn126720::from_payload(&ZONE_NAME_X).expect("valid 126720");
    let mut buffer = [0u8; 64];
    let written = decoded.to_payload(&mut buffer).expect("encode");
    assert_eq!(&buffer[..written], &ZONE_NAME_X);
}

#[cfg(feature = "full-pgns")]
#[test]
fn lz_empty_string_is_length_zero_then_terminator() {
    let mut inner = Pgn126720FusionSetZoneName::new();
    inner.zone = 1;
    let mut buffer = [0u8; 64];
    let written = Pgn126720::FusionSetZoneName(inner)
        .to_payload(&mut buffer)
        .expect("encode");
    assert_eq!(&buffer[4..written], &[1, 0, 0]);
}

#[cfg(feature = "full-pgns")]
#[test]
fn lz_missing_terminator_is_still_read() {
    assert_eq!(zone_name(&ZONE_NAME_X), bytes_of(b"X"));
    assert_eq!(zone_name(&ZONE_NAME_X[..7]), bytes_of(b"X"));
    // The terminator is not part of the text, whether or not it is sent.
    assert_eq!(zone_name(&[0xa3, 0x99, 0x22, 0x00, 1, 0]), bytes_of(b""));
    assert_eq!(zone_name(&[0xa3, 0x99, 0x22, 0x00, 1, 0, 0]), bytes_of(b""));
}

// ---------------------------------------------------------------------------
// AUD-006 — a quantized value that does not fit its field is refused.
// ---------------------------------------------------------------------------

/// 130306 Wind Data: sid, wind speed (u16, 0.01 m/s), wind angle (u16, 0.0001 rad).
fn wind_speed_bytes(speed: f32) -> Result<[u8; 2], SerializationError> {
    let mut pgn = Pgn130306::new();
    pgn.wind_speed = speed;
    let mut buffer = [0u8; 8];
    pgn.to_payload(&mut buffer)?;
    Ok([buffer[1], buffer[2]])
}

/// 127257 Attitude: sid, then yaw, pitch and roll as i16 at 0.0001 rad.
fn roll_bytes(roll: f32) -> Result<[u8; 2], SerializationError> {
    let mut pgn = Pgn127257::new();
    pgn.roll = roll;
    let mut buffer = [0u8; 8];
    pgn.to_payload(&mut buffer)?;
    Ok([buffer[5], buffer[6]])
}

#[test]
fn aud_006_unsigned_value_past_its_width_is_refused() {
    // 700 m/s is 70000 units: the old writer kept the low 16 bits, 44.64 m/s.
    assert!(matches!(
        wind_speed_bytes(700.0),
        Err(SerializationError::ValueOutOfRange {
            field_id: "WindSpeed"
        })
    ));
    // 655.36 m/s is 65536 units, one past the last raw value.
    assert!(matches!(
        wind_speed_bytes(655.36),
        Err(SerializationError::ValueOutOfRange { .. })
    ));
}

#[test]
fn aud_006_negative_value_in_an_unsigned_field_is_refused() {
    assert!(matches!(
        wind_speed_bytes(-1.0),
        Err(SerializationError::ValueOutOfRange { .. })
    ));
    assert!(matches!(
        wind_speed_bytes(-0.01),
        Err(SerializationError::ValueOutOfRange { .. })
    ));
}

#[test]
fn aud_006_non_finite_quantized_value_is_refused() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(
            matches!(
                wind_speed_bytes(value),
                Err(SerializationError::ValueOutOfRange { .. })
            ),
            "{value}"
        );
        assert!(
            matches!(
                roll_bytes(value),
                Err(SerializationError::ValueOutOfRange { .. })
            ),
            "{value}"
        );
    }
}

#[test]
fn aud_006_unsigned_limits_are_written() {
    assert_eq!(wind_speed_bytes(0.0).unwrap(), [0x00, 0x00]);
    assert_eq!(wind_speed_bytes(0.01).unwrap(), [0x01, 0x00]);
    // 655.35 m/s is raw 65535, the "not available" sentinel: still representable.
    assert_eq!(wind_speed_bytes(655.35).unwrap(), [0xFF, 0xFF]);
}

#[test]
fn aud_006_signed_limits_are_written_and_one_past_is_refused() {
    assert_eq!(roll_bytes(3.2767).unwrap(), [0xFF, 0x7F]);
    assert_eq!(roll_bytes(-3.2768).unwrap(), [0x00, 0x80]);
    assert_eq!(roll_bytes(-0.0001).unwrap(), [0xFF, 0xFF]);
    assert!(matches!(
        roll_bytes(3.2768),
        Err(SerializationError::ValueOutOfRange { field_id: "Roll" })
    ));
    assert!(matches!(
        roll_bytes(-3.2769),
        Err(SerializationError::ValueOutOfRange { .. })
    ));
}

/// 130321: NaN is the IEEE-754 way to say "no data", a FLOAT is not quantized.
#[cfg(feature = "full-pgns")]
#[test]
fn aud_006_nan_in_a_float_field_is_still_written() {
    let mut pgn = Pgn130321::new();
    assert_eq!(pgn.field_mut("Salinity", PgnValue::F32(f32::NAN)), Some(()));
    let mut buffer = [0u8; 64];
    let written = pgn.to_payload(&mut buffer).expect("NaN salinity");
    assert!(written >= 19);
    assert_eq!(&buffer[15..19], &[0x00, 0x00, 0xC0, 0x7F]);
}

#[test]
fn aud_006_error_stays_small() {
    assert!(core::mem::size_of::<SerializationError>() <= 48);
}

/// 129029 GNSS Position Data: sid, date (16), time (32), then latitude, longitude
/// and altitude as 64-bit signed values. A receiver without a fix sends the
/// "not available" raw `7F FF FF FF FF FF FF FF`; a gateway must pass it on.
#[test]
fn aud_006_wide_field_sentinel_survives_a_round_trip() {
    let mut payload = vec![0u8; 43];
    let latitude = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F];
    payload[7..15].copy_from_slice(&latitude);
    let decoded = Pgn129029::from_payload(&payload).expect("decode 129029");
    let mut buffer = [0u8; 230];
    let written = decoded.to_payload(&mut buffer).expect("re-encode 129029");
    assert_eq!(&buffer[7..15], &latitude);
    assert!(written >= 15);
}

// ---------------------------------------------------------------------------
// AUD-004 — the repeating accessors never index past the array.
// ---------------------------------------------------------------------------

/// `field_mut` on a counter stores the raw value, 255 included (the "not
/// available" sentinel). 129029's group holds 32 stations.
#[test]
fn aud_004_counter_past_the_capacity_does_not_arm_out_of_range_access() {
    let mut pgn = Pgn129029::new();
    assert_eq!(
        pgn.field_mut("ReferenceStations", PgnValue::U8(255)),
        Some(())
    );
    assert_eq!(pgn.repetitive_count("reference_station_types"), Some(255));
    assert!(pgn
        .repetitive_field("reference_station_types", 31, "ReferenceStationId")
        .is_some());
    assert_eq!(
        pgn.repetitive_field_mut(
            "reference_station_types",
            31,
            "ReferenceStationId",
            PgnValue::U16(7)
        ),
        Some(())
    );
    for index in [32, 254, usize::MAX] {
        assert_eq!(
            pgn.repetitive_field("reference_station_types", index, "ReferenceStationId"),
            None,
            "read {index}"
        );
        assert_eq!(
            pgn.repetitive_field_mut(
                "reference_station_types",
                index,
                "ReferenceStationId",
                PgnValue::U16(1)
            ),
            None,
            "write {index}"
        );
    }
}

#[test]
fn aud_004_indices_past_the_counter_stay_refused() {
    let mut pgn = Pgn129029::new();
    assert_eq!(
        pgn.set_repetitive_count("reference_station_types", 2),
        Some(())
    );
    assert!(pgn
        .repetitive_field("reference_station_types", 1, "ReferenceStationId")
        .is_some());
    assert_eq!(
        pgn.repetitive_field("reference_station_types", 2, "ReferenceStationId"),
        None
    );
}

// ---------------------------------------------------------------------------
// Unsigned 24-bit fields with a resolution: `f32` cannot tell their top raw
// values apart, so the "not available" raw `FF FF FF` came back as `FF FF FE`.
// ---------------------------------------------------------------------------

/// 130316 Temperature, Extended Range: sid, instance, source, temperature (24 bits,
/// 0.001 K), set temperature (16 bits, 0.1 K). Every byte is the CANboat
/// "not available" pattern.
#[cfg(feature = "full-pgns")]
#[test]
fn unsigned_24_bit_not_available_raw_is_written_back_unchanged() {
    let payload = [0xFF; 8];
    let decoded = Pgn130316::from_payload(&payload).expect("valid 130316");
    // The field is an `f64`: an `f32` loses the unit at the top of the range.
    let temperature: f64 = decoded.temperature;
    assert!((temperature - 16_777.215).abs() < 0.01);
    let mut out = [0u8; 8];
    assert_eq!(decoded.to_payload(&mut out).unwrap(), 8);
    assert_eq!(out, payload);
}

/// Raws the `f32` merged with their neighbour: 0xFA0000 (the first one) and the
/// two just under the "not available" raw, 0xFFFFFD and 0xFFFFFE.
#[cfg(feature = "full-pgns")]
#[test]
fn unsigned_24_bit_top_raws_are_written_back_unchanged() {
    for temperature in [[0x00, 0x00, 0xFA], [0xFD, 0xFF, 0xFF], [0xFE, 0xFF, 0xFF]] {
        let payload = [
            0x01,
            0x02,
            0x03,
            temperature[0],
            temperature[1],
            temperature[2],
            0x34,
            0x12,
        ];
        let decoded = Pgn130316::from_payload(&payload).expect("valid 130316");
        let mut out = [0u8; 8];
        assert_eq!(decoded.to_payload(&mut out).unwrap(), 8);
        assert_eq!(out, payload);
    }
}

/// 65287 Maretron Trip Volume, manufacturer 137 and industry code 4 (`89 98`), sid,
/// volume instance, fluid type and reserved (`FF`), then the 24-bit trip volume.
#[cfg(feature = "full-pgns")]
#[test]
fn unsigned_24_bit_trip_volume_not_available_is_written_back_unchanged() {
    let payload = [0x89, 0x98, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let decoded = Pgn65287::from_payload(&payload).expect("valid 65287");
    let Pgn65287::MaretronTripVolume(trip) = &decoded else {
        panic!("expected the Maretron variant, got {decoded:?}");
    };
    let volume: f64 = trip.trip_volume;
    assert!((volume - 16_777.215).abs() < 0.01);
    let mut out = [0u8; 8];
    assert_eq!(decoded.to_payload(&mut out).unwrap(), 8);
    assert_eq!(out, payload);
}

// ---------------------------------------------------------------------------
// Integers without a resolution are range-checked like the scaled ones: the
// writer used to keep their low bits only.
// ---------------------------------------------------------------------------

/// 127505 Fluid Level: the instance is a 4-bit unsigned number in the low nibble
/// of byte 0, the "not available" raw being 0xF.
fn fluid_level_first_byte(instance: u8) -> Result<u8, SerializationError> {
    let mut pgn = Pgn127505::new();
    assert_eq!(pgn.field_mut("Instance", PgnValue::U8(instance)), Some(()));
    let mut buffer = [0u8; 8];
    pgn.to_payload(&mut buffer)?;
    Ok(buffer[0])
}

#[test]
fn integer_wider_than_its_field_is_refused() {
    // 158 = 0x9E used to go out as 0xE (14) on four bits.
    assert!(matches!(
        fluid_level_first_byte(158),
        Err(SerializationError::ValueOutOfRange {
            field_id: "Instance"
        })
    ));
    assert!(matches!(
        fluid_level_first_byte(16),
        Err(SerializationError::ValueOutOfRange { .. })
    ));
}

#[test]
fn integer_limits_of_a_narrow_field_are_written() {
    assert_eq!(fluid_level_first_byte(0).unwrap() & 0x0F, 0x0);
    assert_eq!(fluid_level_first_byte(1).unwrap() & 0x0F, 0x1);
    // 15 is the "not available" sentinel of four bits: still representable.
    assert_eq!(fluid_level_first_byte(15).unwrap() & 0x0F, 0xF);
}

/// 60928 ISO Address Claim: the unique number is 21 bits (`FF FF 1F` at most in
/// bytes 0 to 2, the top three bits of byte 2 belong to the manufacturer code).
#[test]
fn integer_of_21_bits_stops_at_its_largest_raw() {
    let mut pgn = Pgn60928::new();
    pgn.unique_number = 0x1F_FFFF;
    let mut buffer = [0u8; 8];
    pgn.to_payload(&mut buffer).expect("largest 21-bit raw");
    assert_eq!(buffer[..2], [0xFF, 0xFF]);
    assert_eq!(buffer[2] & 0x1F, 0x1F);

    pgn.unique_number = 0x20_0000;
    assert!(matches!(
        pgn.to_payload(&mut buffer),
        Err(SerializationError::ValueOutOfRange {
            field_id: "UniqueNumber"
        })
    ));
}

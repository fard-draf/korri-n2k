//! End-to-end tests for the generic PGN serialization/deserialization engine.
use crate::core::{FieldDescriptor, FieldKind, PgnDescriptor, PgnValue};

use crate::{
    infra::codec::{
        engine::{deserialize_into, serialize},
        traits::FieldAccess,
    },
    protocol::{
        lookups::{
            AcLine, Acceptability, DeviceClass, DeviceFunction, IndustryCode, ManufacturerCode,
            RangeResidualMode, SatelliteStatus, YesNo,
        },
        messages::{
            LineInfo, Pgn127503, Pgn129025, Pgn129029, Pgn129040, Pgn129044, Pgn129540,
            Pgn130821NavicoAsciiData, Pgn59904, Pgn60160, Pgn60928,
        },
    },
};

#[test]
/// Validate a synthetic PGN mixing multiple numeric types.
fn test_round_trip_multiple_way_pgn() {
    #[derive(Debug, Default, PartialEq)]
    struct PgnFloatTest {
        value_f32: f32,
        value_f64: f64,
        value_i16: i16,
        value_u32_scaled: f32,
    }

    impl FieldAccess for PgnFloatTest {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "value_f32" => Some(PgnValue::F32(self.value_f32)),
                "value_f64" => Some(PgnValue::F64(self.value_f64)),
                "value_i16" => Some(PgnValue::I16(self.value_i16)),
                "value_u32_scaled" => Some(PgnValue::F32(self.value_u32_scaled)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match id {
                "value_f32" => {
                    if let PgnValue::F32(val) = value {
                        self.value_f32 = val;
                        Some(())
                    } else {
                        None
                    }
                }
                "value_f64" => {
                    if let PgnValue::F64(val) = value {
                        self.value_f64 = val;
                        Some(())
                    } else {
                        None
                    }
                }
                "value_i16" => {
                    if let PgnValue::I16(val) = value {
                        self.value_i16 = val;
                        Some(())
                    } else {
                        None
                    }
                }
                "value_u32_scaled" => {
                    if let PgnValue::F32(val) = value {
                        self.value_u32_scaled = val;
                        Some(())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }
    impl PgnFloatTest {
        pub const TEST_FLOAT_DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 99999,
            name: "MockedPgn",
            description: "MockedPgn",
            priority: Some(2),
            fastpacket: false,
            length: Some(18),
            field_count: Some(4),
            trans_interval: None,
            trans_irregular: None,
            fields: &[
                FieldDescriptor {
                    id: "value_f32",
                    name: "ValueF32",
                    kind: FieldKind::Number,
                    bits_length: Some(32),
                    bits_length_var: None,
                    bits_offset: Some(0),
                    is_signed: Some(true),
                    resolution: Some(1e-2),
                    enum_direct_name: None,
                    enum_indirect_name: None,
                    enum_indirect_field_order: None,
                    physical_unit: None,
                    physical_qtity: None,
                },
                FieldDescriptor {
                    id: "value_f64",
                    name: "ValueF64",
                    kind: FieldKind::Number,
                    bits_length: Some(64),
                    bits_length_var: None,
                    bits_offset: Some(32),
                    is_signed: Some(true),
                    resolution: Some(1e-10),
                    enum_direct_name: None,
                    enum_indirect_name: None,
                    enum_indirect_field_order: None,
                    physical_unit: None,
                    physical_qtity: None,
                },
                FieldDescriptor {
                    id: "value_i16",
                    name: "ValueI16",
                    kind: FieldKind::Number,
                    bits_length: Some(16),
                    bits_length_var: None,
                    bits_offset: Some(96),
                    is_signed: Some(true),
                    resolution: None,
                    enum_direct_name: None,
                    enum_indirect_name: None,
                    enum_indirect_field_order: None,
                    physical_unit: None,
                    physical_qtity: None,
                },
                FieldDescriptor {
                    id: "value_u32_scaled",
                    name: "ValueU32Scaled",
                    kind: FieldKind::Number,
                    bits_length: Some(32),
                    bits_length_var: None,
                    bits_offset: Some(112),
                    is_signed: None,
                    resolution: Some(1e-1),
                    enum_direct_name: None,
                    enum_indirect_name: None,
                    enum_indirect_field_order: None,
                    physical_unit: None,
                    physical_qtity: None,
                },
            ],
            repeating_field_sets: &[],
        };
    }
    let mocked_pgn = PgnFloatTest {
        value_f32: 9.12345678,
        value_f64: 1.23456789123456789,
        value_i16: -2542,
        value_u32_scaled: 429_496.4,
    };
    let mut buffer = [0xFF; PgnFloatTest::TEST_FLOAT_DESCRIPTOR.length.unwrap() as usize];
    let bit_writed = serialize(
        &mocked_pgn,
        &mut buffer,
        &PgnFloatTest::TEST_FLOAT_DESCRIPTOR,
    )
    .unwrap();
    let payload_slice = &buffer[..bit_writed];
    let mut pgn_rounded = PgnFloatTest::default();
    deserialize_into::<PgnFloatTest>(
        &mut pgn_rounded,
        payload_slice,
        &PgnFloatTest::TEST_FLOAT_DESCRIPTOR,
    )
    .unwrap();

    assert!((mocked_pgn.value_f32 - pgn_rounded.value_f32).abs() < 1e-2);
    assert!((mocked_pgn.value_f64 - pgn_rounded.value_f64).abs() < 1e-9);
    assert!((mocked_pgn.value_i16 - pgn_rounded.value_i16).abs() == 0);
    assert!((mocked_pgn.value_u32_scaled - pgn_rounded.value_u32_scaled).abs() < 1e-1);
}

#[test]
fn test_string_lz_roundtrip() {
    #[derive(Debug, PartialEq, Copy, Clone)]
    struct PgnStringLz {
        text: crate::core::PgnBytes,
    }

    impl Default for PgnStringLz {
        fn default() -> Self {
            Self {
                text: crate::core::PgnBytes::default(),
            }
        }
    }

    impl FieldAccess for PgnStringLz {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "Text" => Some(PgnValue::Bytes(self.text)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("Text", PgnValue::Bytes(bytes)) => {
                    self.text = bytes;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnStringLz {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42420,
            name: "MockStringLz",
            description: "Mocked STRING_LZ field",
            priority: Some(6),
            fastpacket: false,
            length: None,
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "Text",
                name: "Text",
                kind: FieldKind::StringLz,
                bits_length: None,
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    let mut payload = PgnStringLz::default();
    let mut text_bytes = crate::core::PgnBytes::default();
    text_bytes.copy_from_slice(b"KORRI");
    payload.text = text_bytes;

    let mut buffer = [0xFF; 64];
    let bytes_written = serialize(&payload, &mut buffer, &PgnStringLz::DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, payload.text.len() + 1);
    assert_eq!(buffer[0], payload.text.len() as u8);
    assert_eq!(&buffer[1..1 + payload.text.len()], payload.text.as_slice());

    let mut decoded = PgnStringLz::default();
    deserialize_into(
        &mut decoded,
        &buffer[..bytes_written],
        &PgnStringLz::DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(decoded.text.len(), payload.text.len());
    assert_eq!(decoded.text.as_slice(), payload.text.as_slice());
}

#[test]
fn test_string_lau_roundtrip() {
    #[derive(Debug, PartialEq, Copy, Clone)]
    struct PgnStringLau {
        description: crate::core::PgnBytes,
    }

    impl Default for PgnStringLau {
        fn default() -> Self {
            Self {
                description: crate::core::PgnBytes::default(),
            }
        }
    }

    impl FieldAccess for PgnStringLau {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "Description" => Some(PgnValue::Bytes(self.description)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("Description", PgnValue::Bytes(bytes)) => {
                    self.description = bytes;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnStringLau {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42421,
            name: "MockStringLau",
            description: "Mocked STRING_LAU field",
            priority: Some(6),
            fastpacket: false,
            length: None,
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "Description",
                name: "Description",
                kind: FieldKind::StringLau,
                bits_length: None,
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    let mut payload = PgnStringLau::default();
    let mut raw = [0u8; 8];
    raw[0] = 1; // ASCII
    let text = b"KORRI";
    raw[1..1 + text.len()].copy_from_slice(text);
    let mut bytes = crate::core::PgnBytes::default();
    bytes.copy_from_slice(&raw[..1 + text.len()]);
    payload.description = bytes;

    let mut buffer = [0xFF; 64];
    let bytes_written = serialize(&payload, &mut buffer, &PgnStringLau::DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, payload.description.len() + 1);
    assert_eq!(buffer[0], payload.description.len() as u8);
    assert_eq!(buffer[1], 1);
    assert_eq!(
        &buffer[2..2 + text.len()],
        &payload.description.as_slice()[1..]
    );

    let mut decoded = PgnStringLau::default();
    deserialize_into(
        &mut decoded,
        &buffer[..bytes_written],
        &PgnStringLau::DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(decoded.description.len(), payload.description.len());
    assert_eq!(
        decoded.description.as_slice(),
        payload.description.as_slice()
    );
}

#[test]
/// PGN 129025: latitude/longitude positions preserved within tolerance.
fn test_round_trip_pgn_129025() {
    let pgn = Pgn129025::new();
    let la_tolerance = 1e-6;
    let lg_tolerance = 1e-5;

    let mut buffer = [0xFF; Pgn129025::PGN_129025_DESCRIPTOR.length.unwrap() as usize];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn129025::PGN_129025_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn129025::new();
    assert!(deserialize_into::<Pgn129025>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn129025::PGN_129025_DESCRIPTOR
    )
    .is_ok());

    assert!((pgn.latitude - pgn_rounded.latitude).abs() < la_tolerance);
    assert!((pgn.longitude - pgn_rounded.longitude).abs() < lg_tolerance);
}

#[test]
/// PGN 60928: full NAME field and conversion round-trips.
fn test_round_trip_pgn_60928() {
    let mut pgn = Pgn60928::new();
    pgn.unique_number = 2122;
    pgn.manufacturer_code = ManufacturerCode::Airmar;
    pgn.device_instance_lower = 4;
    pgn.device_instance_upper = 11;
    pgn.device_class = DeviceClass::SystemTools;
    // device_function is an INDIRECT_LOOKUP stored as a u8
    // Use helper to assign the full DeviceFunction value
    pgn.set_device_function(DeviceFunction::Diagnostic);
    pgn.system_instance = 4;
    pgn.arbitrary_address_capable = YesNo::Yes;

    let mut pgn_rounded = Pgn60928::new();
    let mut buffer = [0xFF; Pgn60928::PGN_60928_DESCRIPTOR.length.unwrap() as usize];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn60928::PGN_60928_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];
    assert!(deserialize_into::<Pgn60928>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn60928::PGN_60928_DESCRIPTOR
    )
    .is_ok());

    assert_eq!(pgn.unique_number, pgn_rounded.unique_number);
    assert_eq!(pgn.manufacturer_code, pgn_rounded.manufacturer_code);
    assert_eq!(pgn.device_instance_lower, pgn_rounded.device_instance_lower);
    assert_eq!(pgn.device_instance_upper, pgn_rounded.device_instance_upper);
    assert_eq!(pgn.device_function, pgn_rounded.device_function);
    assert_eq!(pgn.device_class, pgn_rounded.device_class);
    assert_eq!(pgn.system_instance, pgn_rounded.system_instance);
    assert_eq!(pgn.industry_group, pgn_rounded.industry_group);
    assert_eq!(
        pgn.arbitrary_address_capable,
        pgn_rounded.arbitrary_address_capable
    );
}

#[test]
/// PGN 59904 : message Request minimal.
fn test_round_trip_pgn_59904() {
    let pgn = Pgn59904 { pgn: 129025 };
    let mut buffer = [0xFF; Pgn59904::PGN_59904_DESCRIPTOR.length.unwrap() as usize];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn59904::PGN_59904_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn59904::new();
    deserialize_into::<Pgn59904>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn59904::PGN_59904_DESCRIPTOR,
    )
    .unwrap();
    assert_eq!(pgn, pgn_rounded)
}

#[test]
/// Proprietary ASCII PGN: fixed-length string field with padding.
fn test_round_trip_stringfixe_pgn_130821() {
    let mut pgn = Pgn130821NavicoAsciiData::new();
    pgn.manufacturer_code = ManufacturerCode::ArksEnterprisesInc;
    pgn.industry_code = IndustryCode::MarineIndustry;

    pgn.a = 150;
    let message = "Lorem ipsum dolor sit amet".as_bytes();
    pgn.message[..message.len()].copy_from_slice(message);
    let mut buffer = [0xFF;
        Pgn130821NavicoAsciiData::PGN_130821_NAVICO_ASCII_DATA_DESCRIPTOR
            .length
            .unwrap() as usize];
    let bytes_written = serialize(
        &pgn,
        &mut buffer,
        &Pgn130821NavicoAsciiData::PGN_130821_NAVICO_ASCII_DATA_DESCRIPTOR,
    )
    .unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn130821NavicoAsciiData::new();
    assert!(deserialize_into(
        &mut pgn_rounded,
        payload_slice,
        &Pgn130821NavicoAsciiData::PGN_130821_NAVICO_ASCII_DATA_DESCRIPTOR
    )
    .is_ok());
    assert_eq!(pgn, pgn_rounded);
    assert_ne!(
        &pgn_rounded.message[..message.len()],
        "Corem ipsum dolor sit amet".as_bytes()
    );
    assert_eq!(
        &pgn_rounded.message[..message.len()],
        "Lorem ipsum dolor sit amet".as_bytes()
    );
}

#[test]
/// PGN 129044: binary fields combined with floating resolutions.
fn test_round_trip_stringfixe_pgn_129044() {
    let mut pgn = Pgn129044::new();
    let mess_local_datum = "Fr".as_bytes();
    pgn.local_datum[..mess_local_datum.len()].copy_from_slice(mess_local_datum);
    pgn.delta_latitude = 47.996033;
    pgn.delta_longitude = -4.102478;
    pgn.delta_altitude = 15001.0;
    let mess_reference_datum = "Ref".as_bytes();
    pgn.reference_datum[..mess_reference_datum.len()].copy_from_slice(mess_reference_datum);

    let mut buffer = [0xFF; Pgn129044::PGN_129044_DESCRIPTOR.length.unwrap() as usize];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn129044::PGN_129044_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn129044::new();
    assert!(deserialize_into(
        &mut pgn_rounded,
        payload_slice,
        &Pgn129044::PGN_129044_DESCRIPTOR
    )
    .is_ok());
    assert_eq!(pgn, pgn_rounded);
    assert_ne!(pgn_rounded.delta_latitude, 47.99604);
}

//==================================================================================129040
#[test]
/// PGN 129040: round-trip a simple MMSI (u32) field.
fn test_round_trip_pgn_129040_mmsi() {
    let mut pgn = Pgn129040::new();
    pgn.user_id = 123456789;

    let mut buffer = [0xFF; 64];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn129040::PGN_129040_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn129040::new();
    deserialize_into::<Pgn129040>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn129040::PGN_129040_DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(pgn.user_id, pgn_rounded.user_id);
}

//==================================================================================60160
#[test]
/// PGN 60160: binary slice field plus SID identifier.
fn test_round_trip_pgn_60160_binary_field() {
    let mut pgn = Pgn60160::new();
    pgn.sid = 0x5A;
    pgn.data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];

    let mut buffer = [0xFF; Pgn60160::PGN_60160_DESCRIPTOR.length.unwrap() as usize];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn60160::PGN_60160_DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, 8);
    let payload_slice = &buffer[..bytes_written];

    let mut decoded = Pgn60160::new();
    deserialize_into::<Pgn60160>(&mut decoded, payload_slice, &Pgn60160::PGN_60160_DESCRIPTOR)
        .unwrap();

    assert_eq!(decoded.sid, pgn.sid);
    assert_eq!(decoded.data, pgn.data);
}

//==================================================================================129029

#[test]
/// PGN 129029: exercise Date/Time fields with different resolutions.
fn test_round_trip_pgn_129029_date_time() {
    // Validate Date (u16) and Time (u32) fields
    let mut pgn = Pgn129029::new();
    pgn.date = 19000; // Days since 1970-01-01
    pgn.time = 3600.0; // Seconds since midnight × 10000 (3600.0 s = 1 h)

    let mut buffer = [0xFF; 64];
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn129029::PGN_129029_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn129029::new();
    deserialize_into::<Pgn129029>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn129029::PGN_129029_DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(pgn.date, pgn_rounded.date);

    // Time resolution 0.0001 → tolerance must be ≥ resolution
    let time_tolerance = 1e-3; // 1 millisecond (10× resolution)
    assert!(
        (pgn.time - pgn_rounded.time).abs() < time_tolerance,
        "Time mismatch: {} vs {} (diff: {})",
        pgn.time,
        pgn_rounded.time,
        (pgn.time - pgn_rounded.time).abs()
    );
}

#[test]
/// PGN 129029: ensures repeating fields (reference stations) serialize correctly.
fn test_round_trip_pgn_129029_repetitive_fields() {
    let mut pgn = Pgn129029::new();
    pgn.date = 19000;
    pgn.time = 3600.0;
    pgn.latitude = 48.8566;
    pgn.longitude = 2.3522;

    // Add three reference stations
    pgn.reference_station_types_count = 3;
    pgn.reference_station_types[0].reference_station_id = 101;
    pgn.reference_station_types[0].age_of_dgnss_corrections = 5.2;
    pgn.reference_station_types[1].reference_station_id = 202;
    pgn.reference_station_types[1].age_of_dgnss_corrections = 3.7;
    pgn.reference_station_types[2].reference_station_id = 303;
    pgn.reference_station_types[2].age_of_dgnss_corrections = 8.1;

    let mut buffer = [0xFF; 223]; // Max Fast Packet size
    let bytes_written = serialize(&pgn, &mut buffer, &Pgn129029::PGN_129029_DESCRIPTOR).unwrap();
    let payload_slice = &buffer[..bytes_written];

    let mut pgn_rounded = Pgn129029::new();
    deserialize_into::<Pgn129029>(
        &mut pgn_rounded,
        payload_slice,
        &Pgn129029::PGN_129029_DESCRIPTOR,
    )
    .unwrap();

    // Validate regular fields
    assert_eq!(pgn.date, pgn_rounded.date);
    assert!((pgn.time - pgn_rounded.time).abs() < 1e-3);

    // Validate the counter
    assert_eq!(
        pgn.reference_station_types_count,
        pgn_rounded.reference_station_types_count
    );

    // Validate reference station entries
    for i in 0..pgn.reference_station_types_count {
        assert_eq!(
            pgn.reference_station_types[i].reference_station_id,
            pgn_rounded.reference_station_types[i].reference_station_id,
            "Station {} ID mismatch",
            i
        );
        assert!(
            (pgn.reference_station_types[i].age_of_dgnss_corrections
                - pgn_rounded.reference_station_types[i].age_of_dgnss_corrections)
                .abs()
                < 0.1,
            "Station {} age mismatch",
            i
        );
    }
}

#[test]
/// PGN 129540: verifies serialization of satellites-in-view repeating data.
fn test_round_trip_pgn_129540_repetitive_fields() {
    let mut pgn = Pgn129540::new();
    pgn.sid = 7;
    pgn.range_residual_mode = RangeResidualMode::RangeResidualsWereUsedToCalculateData;

    pgn.sats_in_view = 3;
    pgn.prns_count = 3;

    let samples = [
        (
            12u8,
            0.5236f32,
            1.0472f32,
            45.67f32,
            12.34567f32,
            3u8,
            0x0Fu8,
        ),
        (
            24u8,
            0.7854f32,
            2.0944f32,
            38.12f32,
            -8.76543f32,
            9u8,
            0x0Au8,
        ),
        (
            36u8, -0.1571f32, 3.1416f32, 52.0f32, 0.00005f32, 1u8, 0x05u8,
        ),
    ];

    for (idx, sample) in samples.iter().enumerate() {
        pgn.prns[idx].prn = sample.0;
        pgn.prns[idx].elevation = sample.1;
        pgn.prns[idx].azimuth = sample.2;
        pgn.prns[idx].snr = sample.3;
        pgn.prns[idx].range_residuals = sample.4;
        pgn.prns[idx].status = SatelliteStatus::NotTracked;

        pgn.prns[idx].reserved11 = sample.6;
    }

    let mut buffer = [0xFF; 223];
    let bytes_written =
        serialize(&pgn, &mut buffer, &Pgn129540::PGN_129540_DESCRIPTOR).expect("serialize");
    let payload_slice = &buffer[..bytes_written];

    let mut decoded = Pgn129540::new();
    deserialize_into(
        &mut decoded,
        payload_slice,
        &Pgn129540::PGN_129540_DESCRIPTOR,
    )
    .expect("deserialize");

    assert_eq!(decoded.sid, pgn.sid);
    assert_eq!(decoded.range_residual_mode, pgn.range_residual_mode);
    assert_eq!(decoded.sats_in_view, pgn.prns_count as u8);
    assert_eq!(decoded.prns_count, pgn.prns_count);

    for idx in 0..pgn.prns_count {
        let expected = &pgn.prns[idx];
        let actual = &decoded.prns[idx];
        assert_eq!(actual.prn, expected.prn, "PRN mismatch at {}", idx);
        assert!(
            (actual.elevation - expected.elevation).abs() < 2e-4,
            "Elevation mismatch at {}",
            idx
        );
        assert!(
            (actual.azimuth - expected.azimuth).abs() < 2e-4,
            "Azimuth mismatch at {}",
            idx
        );
        assert!(
            (actual.snr - expected.snr).abs() < 0.02,
            "SNR mismatch at {}",
            idx
        );
        assert!(
            (actual.range_residuals - expected.range_residuals).abs() < 2e-5,
            "Range residual mismatch at {}",
            idx
        );
        assert_eq!(actual.status, expected.status, "Status mismatch at {}", idx);
    }
}

//==================================================================================BitLookup
#[test]
/// Unit test for 8-bit BitLookup: ensures bitmasks serialize/deserialize correctly.
/// BitLookup fields allow multiple bits to be active simultaneously (e.g. 0b10110101).
fn test_bitlookup_u8_roundtrip() {
    #[derive(Debug, Default, PartialEq)]
    struct PgnBitLookupU8 {
        flags: u8,
    }

    impl FieldAccess for PgnBitLookupU8 {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "flags" => Some(PgnValue::U8(self.flags)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("flags", PgnValue::U8(val)) => {
                    self.flags = val;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnBitLookupU8 {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42000,
            name: "MockBitLookupU8",
            description: "Test BitLookup 8 bits",
            priority: Some(6),
            fastpacket: false,
            length: Some(1),
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "flags",
                name: "Flags",
                kind: FieldKind::BitLookup,
                bits_length: Some(8),
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    // Test with multiple bits set: 0b10110101 = 0xB5 = 181
    let pgn = PgnBitLookupU8 { flags: 0b10110101 };

    let mut buffer = [0xFF; 1];
    let bytes_written = serialize(&pgn, &mut buffer, &PgnBitLookupU8::DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, 1);
    assert_eq!(buffer[0], 0xB5);

    let mut decoded = PgnBitLookupU8::default();
    deserialize_into(
        &mut decoded,
        &buffer[..bytes_written],
        &PgnBitLookupU8::DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(decoded.flags, pgn.flags);
    assert_eq!(decoded.flags, 0b10110101);
}

#[test]
/// Unit test for a 16-bit BitLookup: most common layout across NMEA 2000 PGNs.
/// Verify that all 16 bits of the bitmask survive the round-trip.
fn test_bitlookup_u16_roundtrip() {
    #[derive(Debug, Default, PartialEq)]
    struct PgnBitLookupU16 {
        alert_bits: u16,
    }

    impl FieldAccess for PgnBitLookupU16 {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "alert_bits" => Some(PgnValue::U16(self.alert_bits)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("alert_bits", PgnValue::U16(val)) => {
                    self.alert_bits = val;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnBitLookupU16 {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42001,
            name: "MockBitLookupU16",
            description: "Test BitLookup 16 bits (example: SIMNET Alert Bits)",
            priority: Some(6),
            fastpacket: false,
            length: Some(2),
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "alert_bits",
                name: "Alert Bits",
                kind: FieldKind::BitLookup,
                bits_length: Some(16),
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    // Test with a complex pattern: bits 0, 3, 7, 8, 12, 15 set
    // 0b1001000110001001 = 0x9189
    let pgn = PgnBitLookupU16 {
        alert_bits: 0b1001000110001001,
    };

    let mut buffer = [0xFF; 2];
    let bytes_written = serialize(&pgn, &mut buffer, &PgnBitLookupU16::DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, 2);

    let mut decoded = PgnBitLookupU16::default();
    deserialize_into(
        &mut decoded,
        &buffer[..bytes_written],
        &PgnBitLookupU16::DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(decoded.alert_bits, pgn.alert_bits);
    assert_eq!(decoded.alert_bits, 0x9189);

    // Ensure the specific bits remain set
    assert_eq!(decoded.alert_bits & (1 << 0), 1 << 0); // Bit 0
    assert_eq!(decoded.alert_bits & (1 << 3), 1 << 3); // Bit 3
    assert_eq!(decoded.alert_bits & (1 << 7), 1 << 7); // Bit 7
    assert_eq!(decoded.alert_bits & (1 << 8), 1 << 8); // Bit 8
    assert_eq!(decoded.alert_bits & (1 << 12), 1 << 12); // Bit 12
    assert_eq!(decoded.alert_bits & (1 << 15), 1 << 15); // Bit 15
}

#[test]
/// Unit test for 32-bit BitLookup: validates support for extended bitmasks.
/// Useful for systems with many binary states (e.g. 32 on/off sensors).
fn test_bitlookup_u32_roundtrip() {
    #[derive(Debug, Default, PartialEq)]
    struct PgnBitLookupU32 {
        status_flags: u32,
    }

    impl FieldAccess for PgnBitLookupU32 {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "status_flags" => Some(PgnValue::U32(self.status_flags)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("status_flags", PgnValue::U32(val)) => {
                    self.status_flags = val;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnBitLookupU32 {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42002,
            name: "MockBitLookupU32",
            description: "Test BitLookup 32 bits",
            priority: Some(6),
            fastpacket: false,
            length: Some(4),
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "status_flags",
                name: "Status Flags",
                kind: FieldKind::BitLookup,
                bits_length: Some(32),
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    // Test with alternating high/low bits set: 0xA5A5A5A5
    let pgn = PgnBitLookupU32 {
        status_flags: 0xA5A5A5A5,
    };

    let mut buffer = [0xFF; 4];
    let bytes_written = serialize(&pgn, &mut buffer, &PgnBitLookupU32::DESCRIPTOR).unwrap();
    assert_eq!(bytes_written, 4);

    let mut decoded = PgnBitLookupU32::default();
    deserialize_into(
        &mut decoded,
        &buffer[..bytes_written],
        &PgnBitLookupU32::DESCRIPTOR,
    )
    .unwrap();

    assert_eq!(decoded.status_flags, pgn.status_flags);
    assert_eq!(decoded.status_flags, 0xA5A5A5A5);
}

#[test]
/// Edge cases for BitLookup: all bits cleared, all bits set.
/// Ensures robustness across extreme values.
fn test_bitlookup_edge_cases() {
    #[derive(Debug, Default, PartialEq)]
    struct PgnBitLookupEdge {
        flags: u16,
    }

    impl FieldAccess for PgnBitLookupEdge {
        fn field(&self, id: &'static str) -> Option<PgnValue> {
            match id {
                "flags" => Some(PgnValue::U16(self.flags)),
                _ => None,
            }
        }

        fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {
            match (id, value) {
                ("flags", PgnValue::U16(val)) => {
                    self.flags = val;
                    Some(())
                }
                _ => None,
            }
        }
    }

    impl PgnBitLookupEdge {
        pub const DESCRIPTOR: PgnDescriptor = PgnDescriptor {
            id: 42003,
            name: "MockBitLookupEdge",
            description: "Test BitLookup edge cases",
            priority: Some(6),
            fastpacket: false,
            length: Some(2),
            field_count: Some(1),
            trans_interval: None,
            trans_irregular: Some(true),
            fields: &[FieldDescriptor {
                id: "flags",
                name: "Flags",
                kind: FieldKind::BitLookup,
                bits_length: Some(16),
                bits_length_var: None,
                bits_offset: Some(0),
                is_signed: None,
                resolution: None,
                enum_direct_name: None,
                enum_indirect_name: None,
                enum_indirect_field_order: None,
                physical_unit: None,
                physical_qtity: None,
            }],
            repeating_field_sets: &[],
        };
    }

    // Test 1: all bits cleared
    let pgn_zero = PgnBitLookupEdge { flags: 0x0000 };
    let mut buffer = [0xFF; 2];
    serialize(&pgn_zero, &mut buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    let mut decoded = PgnBitLookupEdge::default();
    deserialize_into(&mut decoded, &buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    assert_eq!(decoded.flags, 0x0000);

    // Test 2: all bits set
    let pgn_ones = PgnBitLookupEdge { flags: 0xFFFF };
    serialize(&pgn_ones, &mut buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    deserialize_into(&mut decoded, &buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    assert_eq!(decoded.flags, 0xFFFF);

    // Test 3: single bit set (bit 10)
    let pgn_single = PgnBitLookupEdge { flags: 1 << 10 };
    serialize(&pgn_single, &mut buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    deserialize_into(&mut decoded, &buffer, &PgnBitLookupEdge::DESCRIPTOR).unwrap();
    assert_eq!(decoded.flags, 1 << 10);
}

//==================================================================================127503
#[test]
/// PGN 127503: validates serialization of AC input entries.
fn test_round_trip_pgn_127503_repetitive_fields() {
    let mut pgn = Pgn127503::new();
    pgn.instance = 1;
    pgn.number_of_lines = 2;
    pgn.lines_count = 2;

    let lines = [
        LineInfo {
            line: AcLine::Line1,
            acceptability: Acceptability::Good,
            reserved: 0,
            voltage: 230.50,
            current: 10.5,
            frequency: 50.0,
            breaker_size: 16.0,
            real_power: 1500,
            reactive_power: 250,
            power_factor: 0.95,
        },
        LineInfo {
            line: AcLine::Line2,
            acceptability: Acceptability::Good,
            reserved: 0,
            voltage: 115.25,
            current: 8.4,
            frequency: 60.0,
            breaker_size: 10.0,
            real_power: 980,
            reactive_power: 120,
            power_factor: 0.87,
        },
    ];

    for (idx, line) in lines.iter().enumerate() {
        pgn.lines[idx] = *line;
    }

    let mut buffer = [0xFF; 223];
    let bytes_written =
        serialize(&pgn, &mut buffer, &Pgn127503::PGN_127503_DESCRIPTOR).expect("serialize");
    let payload_slice = &buffer[..bytes_written];

    let mut decoded = Pgn127503::new();
    deserialize_into(
        &mut decoded,
        payload_slice,
        &Pgn127503::PGN_127503_DESCRIPTOR,
    )
    .expect("deserialize");

    assert_eq!(decoded.instance, pgn.instance);
    assert_eq!(decoded.number_of_lines, pgn.lines_count as u8);
    assert_eq!(decoded.lines_count, pgn.lines_count);

    for idx in 0..pgn.lines_count {
        let expected = &pgn.lines[idx];
        let actual = &decoded.lines[idx];
        assert_eq!(actual.line, expected.line, "Line mismatch at {}", idx);
        assert_eq!(
            actual.acceptability, expected.acceptability,
            "Acceptability mismatch at {}",
            idx
        );
        assert!(
            (actual.voltage - expected.voltage).abs() < 0.02,
            "Voltage mismatch at {}",
            idx
        );
        assert!(
            (actual.current - expected.current).abs() < 0.11,
            "Current mismatch at {}",
            idx
        );
        assert!(
            (actual.frequency - expected.frequency).abs() < 0.02,
            "Frequency mismatch at {}",
            idx
        );
        assert!(
            (actual.breaker_size - expected.breaker_size).abs() < 0.11,
            "Breaker size mismatch at {}",
            idx
        );
        assert_eq!(
            actual.real_power, expected.real_power,
            "Real power mismatch at {}",
            idx
        );
        assert_eq!(
            actual.reactive_power, expected.reactive_power,
            "Reactive power mismatch at {}",
            idx
        );
        assert!(
            (actual.power_factor - expected.power_factor).abs() < 0.02,
            "Power factor mismatch at {}",
            idx
        );
    }
}

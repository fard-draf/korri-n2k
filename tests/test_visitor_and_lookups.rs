//! Field visitor (`for_each_field`) and CANboat lookup label resolution
//! (`lookup_name`, `lookup_name_indirect`).
use korri_n2k::core::{Availability, FieldKind, PgnValue, MAX_PGN_BYTES};
use korri_n2k::infra::codec::engine::{for_each_field, string_fix_trimmed};
use korri_n2k::infra::codec::traits::{FieldAccess, PgnData};
use korri_n2k::protocol::lookups::{
    lookup_name, lookup_name_indirect, DeviceClass, DeviceFunction, ManufacturerCode, YesNo1bit,
};
use korri_n2k::protocol::messages::*;

struct Rng(u64);

impl Rng {
    fn next_byte(&mut self) -> u8 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 33) as u8
    }
}

fn values_match(a: &PgnValue, b: &PgnValue, resolution: Option<f32>) -> bool {
    let tol = resolution.unwrap_or(0.0) as f64 * 0.5 + 1e-6;
    match (a, b) {
        (PgnValue::F32(x), PgnValue::F32(y)) => {
            (x.is_nan() && y.is_nan()) || ((*x as f64 - *y as f64).abs() <= tol)
        }
        (PgnValue::F64(x), PgnValue::F64(y)) => {
            (x.is_nan() && y.is_nan()) || ((*x - *y).abs() <= tol)
        }
        _ => a == b,
    }
}

fn check_pgn_equivalence<T: PgnData + FieldAccess>(pgn_id: u32, seed_payload_prefix: &[u8]) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ pgn_id as u64);
    let mut raw_buf = [0u8; 256];
    let mut checked = 0usize;

    for _ in 0..200 {
        for b in raw_buf.iter_mut() {
            *b = rng.next_byte();
        }
        raw_buf[..seed_payload_prefix.len()].copy_from_slice(seed_payload_prefix);

        let Ok(inst) = T::from_payload(&raw_buf) else {
            continue;
        };
        let mut encoded = [0u8; 256];
        let len = inst.to_payload(&mut encoded).expect("to_payload");
        let slice = &encoded[..len];

        let decoded = T::from_payload(slice).expect("re-decode");
        let desc = descriptor_for(pgn_id, slice)
            .unwrap_or_else(|| panic!("descriptor_for({pgn_id}) returned None"));

        let mut visited = 0usize;
        for_each_field(desc, slice, |field_desc, reading| {
            visited += 1;
            assert!(!matches!(
                field_desc.kind,
                FieldKind::Reserved | FieldKind::Spare
            ));
            let expected = match reading.repetition {
                None => decoded.field(field_desc.id),
                Some((array_id, idx)) => decoded.repetitive_field(array_id, idx, field_desc.id),
            }
            .unwrap_or_else(|| {
                panic!(
                    "PGN {pgn_id}: missing FieldAccess value for {} (rep={:?})",
                    field_desc.id, reading.repetition
                )
            });
            assert!(
                values_match(&reading.value, &expected, field_desc.resolution),
                "PGN {pgn_id} field {}: visitor={:?} != field()={:?}",
                field_desc.id,
                reading.value,
                expected
            );
        })
        .expect("for_each_field");

        assert!(visited > 0, "PGN {pgn_id}: visitor yielded no fields");
        checked += 1;
        if checked >= 5 {
            break;
        }
    }
    assert!(checked > 0, "PGN {pgn_id}: no valid instance generated");
}

#[test]
fn visitor_matches_field_access_across_default_manifest() {
    check_pgn_equivalence::<Pgn59904>(59904, &[]);
    check_pgn_equivalence::<Pgn60160>(60160, &[]);
    check_pgn_equivalence::<Pgn60416>(60416, &[16]);
    check_pgn_equivalence::<Pgn60416>(60416, &[17]);
    check_pgn_equivalence::<Pgn60416>(60416, &[19]);
    check_pgn_equivalence::<Pgn60416>(60416, &[32]);
    check_pgn_equivalence::<Pgn60416>(60416, &[255]);
    check_pgn_equivalence::<Pgn60928>(60928, &[]);
    check_pgn_equivalence::<Pgn126985>(126985, &[]);
    check_pgn_equivalence::<Pgn126992>(126992, &[]);
    check_pgn_equivalence::<Pgn126993>(126993, &[]);
    check_pgn_equivalence::<Pgn126996>(126996, &[]);
    check_pgn_equivalence::<Pgn126998>(126998, &[]);
    check_pgn_equivalence::<Pgn127237>(127237, &[]);
    check_pgn_equivalence::<Pgn127245>(127245, &[]);
    check_pgn_equivalence::<Pgn127250>(127250, &[]);
    check_pgn_equivalence::<Pgn127251>(127251, &[]);
    check_pgn_equivalence::<Pgn127257>(127257, &[]);
    check_pgn_equivalence::<Pgn127488>(127488, &[]);
    check_pgn_equivalence::<Pgn127489>(127489, &[]);
    check_pgn_equivalence::<Pgn127497>(127497, &[]);
    check_pgn_equivalence::<Pgn127503>(127503, &[]);
    check_pgn_equivalence::<Pgn127505>(127505, &[]);
    check_pgn_equivalence::<Pgn127508>(127508, &[]);
    check_pgn_equivalence::<Pgn127750>(127750, &[]);
    check_pgn_equivalence::<Pgn128001>(128001, &[]);
    check_pgn_equivalence::<Pgn128259>(128259, &[]);
    check_pgn_equivalence::<Pgn128267>(128267, &[]);
    check_pgn_equivalence::<Pgn128275>(128275, &[]);
    check_pgn_equivalence::<Pgn129025>(129025, &[]);
    check_pgn_equivalence::<Pgn129026>(129026, &[]);
    check_pgn_equivalence::<Pgn129029>(129029, &[]);
    check_pgn_equivalence::<Pgn129038>(129038, &[]);
    check_pgn_equivalence::<Pgn129039>(129039, &[]);
    check_pgn_equivalence::<Pgn129040>(129040, &[]);
    check_pgn_equivalence::<Pgn129044>(129044, &[]);
    check_pgn_equivalence::<Pgn129283>(129283, &[]);
    check_pgn_equivalence::<Pgn129284>(129284, &[]);
    check_pgn_equivalence::<Pgn129540>(129540, &[]);
    check_pgn_equivalence::<Pgn129794>(129794, &[]);
    check_pgn_equivalence::<Pgn129809>(129809, &[]);
    check_pgn_equivalence::<Pgn129810>(129810, &[]);
    check_pgn_equivalence::<Pgn130306>(130306, &[]);
    check_pgn_equivalence::<Pgn130310>(130310, &[]);
    check_pgn_equivalence::<Pgn130311>(130311, &[]);
    check_pgn_equivalence::<Pgn130821>(130821, &[0x13, 0x99]);
}

#[test]
fn visitor_repeating_groups_and_all_ones_counter() {
    let mut pgn = Pgn129029::new();
    pgn.date = 19000;
    pgn.time = 3600.0;
    pgn.latitude = 47.7223;
    pgn.longitude = -4.0022;
    pgn.reference_stations = 2;
    pgn.reference_station_types_count = 2;
    pgn.reference_station_types[0].reference_station_id = 11;
    pgn.reference_station_types[0].age_of_dgnss_corrections = 1.5;
    pgn.reference_station_types[1].reference_station_id = 22;
    pgn.reference_station_types[1].age_of_dgnss_corrections = 2.5;

    let mut buf = [0xFF; MAX_PGN_BYTES];
    let len = pgn.to_payload(&mut buf).expect("encode 129029");
    let desc = descriptor(129029).unwrap();

    let mut stations = Vec::new();
    for_each_field(desc, &buf[..len], |f, r| {
        if let Some((arr, idx)) = r.repetition {
            if f.id == "ReferenceStationId" {
                stations.push((arr, idx, r.raw.unwrap()));
            }
        }
    })
    .expect("visit 129029 with 2 stations");
    assert_eq!(
        stations,
        vec![
            ("reference_station_types", 0, 11),
            ("reference_station_types", 1, 22),
        ]
    );

    // Counter set to 0xFF (N/A sentinel) -> 0 occurrences visited.
    buf[42] = 0xFF;
    let mut rep_count = 0usize;
    for_each_field(desc, &buf[..len], |_, r| {
        if r.repetition.is_some() {
            rep_count += 1;
        }
    })
    .expect("visit 129029 with 0xFF counter");
    assert_eq!(rep_count, 0);
}

#[test]
fn lookup_labels_and_60928_indirect_resolution() {
    assert_eq!(lookup_name("ManufacturerCode", 229), Some("Garmin"));
    assert_eq!(lookup_name("ManufacturerCode", 2047), None);
    assert_eq!(
        lookup_name_indirect("DeviceFunction", 10, 130),
        Some("Diagnostic")
    );
    assert_eq!(lookup_name_indirect("DeviceFunction", 10, 255), None);

    let mut claim = Pgn60928::new();
    claim.unique_number = 42;
    claim.manufacturer_code = ManufacturerCode::Garmin;
    claim.device_class = DeviceClass::SystemTools;
    claim.set_device_function(DeviceFunction::Diagnostic);
    claim.arbitrary_address_capable = YesNo1bit::Yes;

    let mut buf = [0u8; 8];
    claim.to_payload(&mut buf).expect("encode 60928");
    let desc = descriptor(60928).unwrap();

    let mut raw_by_index = [None::<u64>; 16];
    let mut mfg_label = None;
    let mut class_label = None;
    let mut func_label = None;

    let mut field_index = 0usize;
    for field_desc in desc.fields {
        if matches!(field_desc.kind, FieldKind::Reserved | FieldKind::Spare) {
            field_index += 1;
            continue;
        }
        let _ = field_index;
    }

    for_each_field(desc, &buf, |f, r| {
        let idx = desc
            .fields
            .iter()
            .position(|cand| core::ptr::eq(cand, f))
            .unwrap();
        raw_by_index[idx] = r.raw;

        if let (Some(enum_name), Some(raw)) = (f.enum_direct_name, r.raw) {
            let label = lookup_name(enum_name, raw as u32);
            match f.id {
                "ManufacturerCode" => mfg_label = label,
                "DeviceClass" => class_label = label,
                _ => {}
            }
        }
    })
    .expect("visit 60928");

    // Resolve indirect lookups using the 1-based `enum_indirect_field_order`.
    for (idx, f) in desc.fields.iter().enumerate() {
        if let (Some(indir_name), Some(order), Some(sec)) = (
            f.enum_indirect_name,
            f.enum_indirect_field_order,
            raw_by_index[idx],
        ) {
            let primary = raw_by_index[(order - 1) as usize].unwrap() as u8;
            if f.id == "DeviceFunction" {
                func_label = lookup_name_indirect(indir_name, primary, sec as u8);
            }
        }
    }

    assert_eq!(mfg_label, Some("Garmin"));
    assert_eq!(class_label, Some("System tools"));
    assert_eq!(func_label, Some("Diagnostic"));
}

#[test]
fn string_fix_availability_in_visitor() {
    let mut datum = Pgn129044::new();
    datum.local_datum = *b"WGS@";
    datum.reference_datum = [0xFF; 4];

    let mut buf = [0xFF; 24];
    let len = datum.to_payload(&mut buf).expect("encode 129044");
    let desc = descriptor(129044).unwrap();

    let mut local = None;
    let mut reference = None;
    for_each_field(desc, &buf[..len], |f, r| match f.id {
        "LocalDatum" => {
            if let PgnValue::Bytes(b) = r.value {
                local = Some((r.availability, string_fix_trimmed(b.as_slice()).to_vec()));
            }
        }
        "ReferenceDatum" => {
            if let PgnValue::Bytes(b) = r.value {
                reference = Some((r.availability, string_fix_trimmed(b.as_slice()).to_vec()));
            }
        }
        _ => {}
    })
    .expect("visit 129044");

    assert_eq!(local, Some((Availability::Available, b"WGS".to_vec())));
    assert_eq!(reference, Some((Availability::NotAvailable, b"".to_vec())));
}

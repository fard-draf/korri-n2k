//! FUSION_SETTING_VALUE is the first INDIRECT_LOOKUP whose fields are 32 bits wide:
//! the raw value must survive a round trip, not be cut to its low byte.
#![cfg(feature = "full-pgns")]
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::lookups::*;
use korri_n2k::protocol::messages::*;

fn round_trip(mut p: Pgn126720FusionSetSetting) -> Pgn126720FusionSetSetting {
    // `new()` does not set the Match fields that select the variant.
    p.manufacturer_code = ManufacturerCode::from(419u16);
    p.industry_code = IndustryCode::from(4u8);
    p.proprietary_id = FusionMessageId::from(15u16);
    let mut buf = [0u8; 64];
    let n = Pgn126720::FusionSetSetting(p).to_payload(&mut buf).unwrap();
    match Pgn126720::from_payload(&buf[..n]).unwrap() {
        Pgn126720::FusionSetSetting(q) => q,
        other => panic!("wrong variant {other:?}"),
    }
}

#[test]
fn wide_raw_value_survives() {
    let mut p = Pgn126720FusionSetSetting::new();
    p.id = FusionSetting::from(7u32);
    p.value = 0x1234_5678;
    let q = round_trip(p);
    assert_eq!(q.value, 0x1234_5678);
    assert_eq!(q.get_value(), None); // not a byte: no indirect value applies
}

#[test]
fn indirect_value_round_trips() {
    let mut p = Pgn126720FusionSetSetting::new();
    p.set_value(FusionSettingValue::from_values(7, 1).unwrap());
    let q = round_trip(p);
    assert_eq!(q.get_value(), FusionSettingValue::from_values(7, 1));
    assert_eq!((u32::from(q.id), q.value), (7, 1));
}

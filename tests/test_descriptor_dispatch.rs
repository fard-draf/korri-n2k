//! Runtime PGN descriptor lookup (`descriptor` and `descriptor_for`).
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::{descriptor, descriptor_for, Pgn130821, Pgn60416};

fn pgn60416_variant_name(msg: &Pgn60416) -> &'static str {
    match msg {
        Pgn60416::IsoTransportProtocolConnectionManagementRequestToSend(_) => {
            "IsoTransportProtocolConnectionManagementRequestToSend"
        }
        Pgn60416::IsoTransportProtocolConnectionManagementClearToSend(_) => {
            "IsoTransportProtocolConnectionManagementClearToSend"
        }
        Pgn60416::IsoTransportProtocolConnectionManagementEndOfMessage(_) => {
            "IsoTransportProtocolConnectionManagementEndOfMessage"
        }
        Pgn60416::IsoTransportProtocolConnectionManagementBroadcastAnnounce(_) => {
            "IsoTransportProtocolConnectionManagementBroadcastAnnounce"
        }
        Pgn60416::IsoTransportProtocolConnectionManagementAbort(_) => {
            "IsoTransportProtocolConnectionManagementAbort"
        }
    }
}

fn pgn130821_variant_name(msg: &Pgn130821) -> &'static str {
    match msg {
        Pgn130821::NavicoAsciiData(_) => "NavicoAsciiData",
        Pgn130821::FurunoUnknown130821(_) => "FurunoUnknown130821",
        Pgn130821::MaretronAlertText(_) => "MaretronAlertText",
        Pgn130821::BepMarineProprietaryPgn130821(_) => "BepMarineProprietaryPgn130821",
    }
}

#[test]
fn simple_polymorphic_and_unknown_pgn_lookup() {
    let d = descriptor(128267).expect("128267 is a simple PGN in the default manifest");
    assert_eq!(d.id, 128267);
    assert_eq!(
        descriptor_for(128267, &[]).map(|d| (d.id, d.name)),
        Some((128267, d.name))
    );

    assert!(descriptor(60416).is_none());
    assert!(descriptor(130821).is_none());
    assert!(descriptor(999_999).is_none());
    assert!(descriptor_for(999_999, &[0xFF; 8]).is_none());
}

#[test]
fn polymorphic_60416_matches_from_payload() {
    for (code, expected) in [
        (
            16u8,
            "IsoTransportProtocolConnectionManagementRequestToSend",
        ),
        (17, "IsoTransportProtocolConnectionManagementClearToSend"),
        (19, "IsoTransportProtocolConnectionManagementEndOfMessage"),
        (
            32,
            "IsoTransportProtocolConnectionManagementBroadcastAnnounce",
        ),
        (255, "IsoTransportProtocolConnectionManagementAbort"),
    ] {
        let mut payload = [0u8; 8];
        payload[0] = code;
        let desc = descriptor_for(60416, &payload).expect("matching 60416 variant");
        assert_eq!(desc.id, 60416);
        assert_eq!(desc.name, expected);

        let decoded = Pgn60416::from_payload(&payload).expect("from_payload matches");
        assert_eq!(desc.name, pgn60416_variant_name(&decoded));
    }

    let invalid = [0u8; 8];
    assert!(descriptor_for(60416, &invalid).is_none());
    assert!(Pgn60416::from_payload(&invalid).is_err());
}

#[test]
fn polymorphic_130821_matches_from_payload() {
    // Byte-by-byte check from the spec: 275 (0x0113) | reserved (0b11 << 11) | marine (4 << 13) = 0x9913 -> [0x13, 0x99].
    let mut navico = [0u8; 32];
    navico[0] = 0x13;
    navico[1] = 0x99;
    let desc = descriptor_for(130821, &navico).expect("Navico 130821");
    assert_eq!(desc.name, "NavicoAsciiData");

    for (mfg, expected) in [
        (275u16, "NavicoAsciiData"),
        (1855, "FurunoUnknown130821"),
        (137, "MaretronAlertText"),
        (295, "BepMarineProprietaryPgn130821"),
    ] {
        let header: u16 = mfg | (0b11 << 11) | (4 << 13);
        let mut payload = [0u8; 223];
        payload[0..2].copy_from_slice(&header.to_le_bytes());

        let desc = descriptor_for(130821, &payload).expect("matching 130821 variant");
        assert_eq!(desc.id, 130821);
        assert_eq!(desc.name, expected);

        let decoded = Pgn130821::from_payload(&payload).expect("from_payload matches");
        assert_eq!(desc.name, pgn130821_variant_name(&decoded));
    }

    let unknown = [0u8; 16];
    assert!(descriptor_for(130821, &unknown).is_none());
    assert!(Pgn130821::from_payload(&unknown).is_err());
}

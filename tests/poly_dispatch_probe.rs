//! A polymorphic PGN must be selected by every `Match` field, not by the first
//! one alone. On Simnet 65305 all five variants share manufacturer 1857, so
//! dispatching on field #1 silently decoded every message as the first variant.
#![cfg(feature = "full-pgns")]
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn65305;

/// Simnet 65305 header: manufacturer 1857 (11b), reserved (2b), industry 4 (3b),
/// model (8b), then `Report` (8b) — the field that actually discriminates.
fn payload(report: u8) -> [u8; 8] {
    let header: u16 = 1857 | (0b11 << 11) | (4 << 13);
    let mut p = [0u8; 8];
    p[0..2].copy_from_slice(&header.to_le_bytes());
    p[2] = 0; // Model: AC
    p[3] = report;
    p[4] = 2; // Status: Manual — valid for the variants exercised here
    p
}

#[test]
fn report_field_selects_the_variant() {
    let status = Pgn65305::from_payload(&payload(2)).expect("Report=2");
    let request = Pgn65305::from_payload(&payload(3)).expect("Report=3");

    assert!(matches!(status, Pgn65305::SimnetDeviceStatus(_)));
    assert!(
        matches!(request, Pgn65305::SimnetDeviceStatusRequest(_)),
        "Report=3 decoded as the wrong variant: {request:?}"
    );
}

#[test]
fn unknown_discriminator_is_rejected() {
    // Report=200 matches no variant: must fail, not fall back on the first one.
    assert!(Pgn65305::from_payload(&payload(200)).is_err());
}

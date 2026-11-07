//! CAN identifier builder: what it refuses instead of truncating (AUD-012 a).
//! Every expected identifier is written by hand from the J1939 layout:
//! priority 3 bits | reserved | data page | PF | PS | source.
use korri_n2k::error::CanIdBuildError;
use korri_n2k::protocol::transport::can_id::{CanId, CanIdBuilder};

/// PGN 129029 is `0x1F805`: data page 1, PF 0xF8, PS 0x05.
const GNSS_POSITION: u32 = 129029;

#[test]
fn a_pgn_past_18_bits_is_refused_not_truncated() {
    // 0x40000 would lose its only set bit and become PGN 0 (PF 0 needs a destination).
    assert!(matches!(
        CanId::builder(0x4_0000, 5).to_destination(9).build(),
        Err(CanIdBuildError::InvalidData)
    ));
    // An alias of a valid PGN: bit 18 set on top of PGN 129029.
    assert!(matches!(
        CanId::builder(GNSS_POSITION | 0x4_0000, 5).build(),
        Err(CanIdBuildError::InvalidData)
    ));
    assert!(matches!(
        CanId::builder(u32::MAX, 5).build(),
        Err(CanIdBuildError::InvalidData)
    ));
}

#[test]
fn the_largest_pgn_is_built() {
    let id = CanId::builder(0x3_FFFF, 5)
        .with_priority(3)
        .build()
        .unwrap();
    // priority 3 | R 1 | DP 1 | PF 0xFF | PS 0xFF | source 5
    assert_eq!(id.0, (3 << 26) | (1 << 25) | (1 << 24) | 0xFFFF05);
    assert_eq!(id.pgn(), 0x3_FFFF);
}

#[test]
fn a_priority_assigned_past_3_bits_is_refused() {
    for priority in [8, 255] {
        let mut builder: CanIdBuilder = CanId::builder(GNSS_POSITION, 5);
        builder.priority = priority;
        assert!(
            matches!(builder.build(), Err(CanIdBuildError::InvalidData)),
            "priority {priority}"
        );
    }
    let mut builder = CanId::builder(GNSS_POSITION, 5);
    builder.priority = 7;
    assert_eq!(builder.build().unwrap().0, (7 << 26) | (1 << 24) | 0xF80505);
}

#[test]
fn the_priority_setters_still_mask_to_three_bits() {
    let id = CanId::builder(GNSS_POSITION, 5)
        .with_priority(9)
        .build()
        .unwrap();
    assert_eq!(id.priority(), 1);
}

#[test]
fn the_pdu1_pdu2_boundary_sits_at_pf_0xf0() {
    // PF 0xEF is the last addressed PGN: destination mandatory, PS must be 0.
    let addressed = CanId::builder(0xEF00, 5)
        .to_destination(0x42)
        .build()
        .unwrap();
    assert_eq!(addressed.0, (6 << 26) | 0xEF4205);
    assert!(matches!(
        CanId::builder(0xEF00, 5).build(),
        Err(CanIdBuildError::InvalidForBroadcast)
    ));
    // PF 0xF0 is the first broadcast PGN: PS belongs to the PGN.
    let broadcast = CanId::builder(0xF001, 5).build().unwrap();
    assert_eq!(broadcast.0, (6 << 26) | 0xF00105);
    assert!(matches!(
        CanId::builder(0xF001, 5).to_destination(0x42).build(),
        Err(CanIdBuildError::InvalidForFocusedMessage { .. })
    ));
}

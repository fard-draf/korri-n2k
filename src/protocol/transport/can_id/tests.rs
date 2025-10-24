//! Unit tests for the `CanId` accessors and builder.
use super::*;

//==================================================================================CAN_ID
#[test]
/// Extracts the source address from the raw ID.
fn test_source_address() {
    let can_id = CanId(0xFAE225D1);
    // let test = 0b0010_1110_0010;
    assert_eq!(can_id.source_address(), 0xD1);
}

#[test]
/// Verifies extraction of the 3-bit priority field.
fn test_priority() {
    let can_id = CanId(0xFAE225D1);
    assert_eq!(can_id.priority(), 0b110)
}

#[test]
/// Rebuilds the correct PGN (PDU1/PDU2 cases).
fn test_pgn() {
    let can_id = CanId(0xFAE225D1);
    assert_eq!(can_id.pgn(), 0x2E200)
}
//==================================================================================CAN_ID_BUILDER
#[test]
/// Validates builder scenarios: broadcast, addressed, and error handling.
fn test_builder() {
    // Example 1: Broadcast (destination = None), PGN 129029 (GNSS Position)
    let position_id = CanId::builder(129029, 35) // PGN, Source
        .with_priority(3)
        .build();
    // `destination` defaults to None, so build() applies PDU2 rules.
    assert!(position_id.is_ok());

    // Example 2: Addressed message (destination = Some), PGN 59904 (ISO Request)
    let request_id = CanId::builder(59904, 35) // PGN, Source
        .with_priority(6)
        .to_destination(80) // Explicit destination
        .build();
    // `to_destination` sets Some(80), so build() applies PDU1 logic.
    assert!(request_id.is_ok());

    // Example 3: Misconfiguration
    let invalid_id = CanId::builder(129029, 35) // PDU2 PGN
        .to_destination(80) // Yet we supply a destination
        .build();
    // build() must return Err because a PDU2 PGN cannot be addressed.
    assert!(invalid_id.is_err());
}

#[test]
/// The priority must be capped to 3 bits to avoid touching the reserved field.
fn test_priority_masks_extra_bits() {
    let can_id = CanId::builder(129029, 35)
        .priority(0b1111_0000)
        .build()
        .expect("CanId must build");

    // Bits 5..29 must remain untouched by stray priority bits
    assert_eq!(can_id.0 & (1 << 29), 0, "Reserved bit 29 must remain clear");
    assert_eq!(can_id.priority(), 0);
}

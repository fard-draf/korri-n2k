//! Ensure PGNs 129038 and 129039 are generated correctly.
//! These PGNs expose a 19-bit `communicationState` field (not byte-aligned).

use korri_n2k::protocol::messages::{Pgn129038, Pgn129039};

#[test]
fn test_pgn_129038_generation() {
    let pgn = Pgn129038::new();

    // `communicationState` must be a u32 (19 bits)
    let _comm_state: u32 = pgn.communication_state;

    // Ensure the structure was generated correctly
    assert_eq!(pgn.communication_state, 0);
}

#[test]
fn test_pgn_129039_generation() {
    let pgn = Pgn129039::new();

    // `communicationState` must be a u32 (19 bits)
    let _comm_state: u32 = pgn.communication_state;

    // Ensure the structure was generated correctly
    assert_eq!(pgn.communication_state, 0);
}

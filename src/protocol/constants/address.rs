//! J1939 / NMEA 2000 source and destination addresses.
//!
//! J1939-81 splits the 8-bit address space as follows:
//!
//! | Range     | Purpose                                  |
//! |-----------|------------------------------------------|
//! | 0..=127   | Preferred addresses assigned by SAE      |
//! | 128..=247 | Arbitrary (self-configurable) addresses  |
//! | 248..=253 | Preferred addresses for service tools    |
//! | 254       | NULL — no address claimed                |
//! | 255       | GLOBAL — broadcast                       |
//!
//! The `128..=247` arbitrary range is a theoretical maximum: J1939-81 narrows it
//! per industry group. "Maritime Industry" uses `128..=207`, which is the only
//! range this crate keeps. Other industry groups are out of scope, so no
//! runtime selection is implemented.

//==================================================================================SPECIAL_VALUES

/// Broadcast address: the frame targets every node on the bus.
///
/// Mandatory destination for an Address Claim (PGN 60928).
pub const GLOBAL: u8 = 255;

/// Null address ("Cannot Claim").
///
/// Sent as the source address by a node that lost arbitration and has no
/// claimable address left.
pub const NULL: u8 = 254;

//==================================================================================MARINE_DYNAMIC_RANGE

/// First address scanned by an AAC (Arbitrary Address Capable) node.
pub const MARINE_DYNAMIC_START: u8 = 128;

/// Last address scanned by an AAC (Arbitrary Address Capable) node.
pub const MARINE_DYNAMIC_END: u8 = 207;

/// Number of distinct addresses in the marine dynamic range.
pub const MARINE_DYNAMIC_COUNT: usize =
    (MARINE_DYNAMIC_END as usize - MARINE_DYNAMIC_START as usize) + 1;

/// Highest address a node may claim, whatever the strategy
/// (`Fixed`, `SelfConfigurable`, `Arbitrary`).
///
/// It currently equals [`MARINE_DYNAMIC_END`], but it answers a different
/// question: "can this address be claimed?" rather than "does it belong to the
/// range scanned in AAC mode?". Both are kept apart so that one can change
/// without dragging the other along.
pub const MAX_CLAIMABLE: u8 = MARINE_DYNAMIC_END;

//==================================================================================PREDICATES

/// Returns `true` when a node may claim this address.
///
/// The `Fixed` and `SelfConfigurable` strategies accept the whole
/// `0..=MAX_CLAIMABLE` range, SAE preferred addresses included.
pub const fn is_claimable(address: u8) -> bool {
    address <= MAX_CLAIMABLE
}

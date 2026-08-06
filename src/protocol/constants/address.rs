//! J1939 / NMEA 2000 source and destination addresses.
//!
//! | Range     | Purpose                                  |
//! |-----------|------------------------------------------|
//! | 0..=251   | Claimable source addresses               |
//! | 252..=253 | Reserved                                 |
//! | 254       | NULL — no address claimed                |
//! | 255       | GLOBAL — broadcast                       |
//!
//! # Why the whole `0..=251` range is claimable
//!
//! J1939-81 reserves `0..=127` for preferred addresses assigned by the SAE and
//! keeps `128..=247` for self-configurable nodes. **NMEA 2000 does not inherit
//! that split**: the SAE assignments describe the on-road industry groups, not
//! the marine one, so a marine node may claim anywhere in `0..=251`.
//!
//! This is not a reading of the standard alone. The reference C++ stack defines
//! `N2kMaxCanBusAddress` as `251`, its documentation states that "each device
//! will get device source address (0-251)", and a 28-minute capture of an
//! 18-device bus shows every node — all of them flagged Arbitrary Address
//! Capable — sitting between 0 and 43.
//!
//! Restricting claims to `128..=207` would therefore park a node in an empty
//! part of the bus, where no arbitration ever happens.

//==================================================================================SPECIAL_VALUES

/// Broadcast address: the frame targets every node on the bus.
///
/// Mandatory destination for an Address Claim (PGN 60928).
pub const GLOBAL: u8 = 255;

/// Null address ("Cannot Claim").
///
/// Sent as the source address by a node that lost arbitration and has no
/// claimable address left.
pub const NULL_ADDR_254: u8 = 254;

//==================================================================================CLAIMABLE_RANGE

/// Lowest address a node may claim.
pub const MIN_CLAIMABLE: u8 = 0;

/// Highest address a node may claim, whatever the strategy
/// (`Fixed`, `SelfConfigurable`, `Arbitrary`).
pub const MAX_CLAIMABLE: u8 = 251;

/// Number of distinct claimable addresses.
pub const CLAIMABLE_COUNT: usize = (MAX_CLAIMABLE as usize - MIN_CLAIMABLE as usize) + 1;

//==================================================================================PREDICATES

/// Returns `true` when a node may claim this address.
pub const fn is_claimable(address: u8) -> bool {
    address <= MAX_CLAIMABLE
}

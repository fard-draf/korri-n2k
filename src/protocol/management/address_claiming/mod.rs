//! SAE J1939 / NMEA 2000 address-claim algorithm:
//! emit PGN 60928, listen for conflicts, and fall back to alternative addresses when needed.
use crate::error::ExtractionError;
use crate::protocol::constants::addr_mgmt_pgns::{ADDR_CLAIM_ID_BASE, CLAIM_PGN_60928};
use crate::protocol::constants::address::NULL_ADDR_254;
use crate::protocol::constants::{addr_mgmt_pgns, address};
use crate::protocol::management::iso_name::IsoName;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
pub mod engine;

//==================================================================================ADDRESS_CLAIM_STRATEGY
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressClaimStrategy<'a> {
    /// SAC (Single Address Capable) with only one source address who may be attempted
    Fixed { preferred: u8 },

    /// SAC (Single Address Capable) with a finite, predefined set of valid addresses.
    SelfConfigurable { addresses: &'a [u8] },

    /// AAC (Arbitrary Address Capable): preferred address followed by dynamic-address candidates.
    Arbitrary { preferred: u8 },
}

//==================================================================================ADDRESS_CLAIM_ITERATOR
/// Generates candidate addresses following the J1939 rules.
enum AddressClaimIterator<'a> {
    Fixed {
        remaining: Option<u8>,
    },
    SelfConfigurable {
        remaining: &'a [u8],
    },
    Arbitrary {
        preferred: u8,
        tried_preferred: bool,
        /// Distance walked from `preferred`, wrapping over the claimable range.
        offset: usize,
    },
}

impl<'a> AddressClaimIterator<'a> {
    fn new(strategy: AddressClaimStrategy<'a>) -> Self {
        match strategy {
            AddressClaimStrategy::Fixed { preferred } => Self::Fixed {
                remaining: Some(preferred),
            },
            AddressClaimStrategy::SelfConfigurable { addresses } => Self::SelfConfigurable {
                remaining: addresses,
            },
            AddressClaimStrategy::Arbitrary { preferred } => Self::Arbitrary {
                preferred,
                tried_preferred: false,
                offset: 0,
            },
        }
    }

    pub(crate) fn try_next_addr(&mut self) -> u8 {
        self.next().unwrap_or(address::NULL_ADDR_254)
    }
}

impl<'a> Iterator for AddressClaimIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Fixed { remaining } => remaining.take().filter(|&a| address::is_claimable(a)),

            Self::SelfConfigurable { remaining } => loop {
                let (&candidate, rest) = remaining.split_first()?;
                *remaining = rest;
                if address::is_claimable(candidate) {
                    return Some(candidate);
                }
            },
            // Preferred address first, then the rest of the claimable range
            // walked upwards and wrapping around. This mirrors what real nodes
            // do — increment on conflict — instead of jumping into a dedicated
            // block where no other device sits.
            Self::Arbitrary {
                preferred,
                tried_preferred,
                offset,
            } => {
                if !*tried_preferred {
                    *tried_preferred = true;
                    if address::is_claimable(*preferred) {
                        return Some(*preferred);
                    }
                }

                while *offset < address::CLAIMABLE_COUNT {
                    *offset += 1;
                    let candidate =
                        ((*preferred as usize + *offset) % address::CLAIMABLE_COUNT) as u8;

                    if candidate != *preferred {
                        return Some(candidate);
                    }
                }
                None
            }
        }
    }
}

//==================================================================================ADDRESS_CLAIM_FRAME
/// Build a claim frame (PGN 60928) for the provided NAME.
/// Infaillible!
pub fn build_address_claim_frame(my_name: IsoName, address_to_claim: u8) -> CanFrame {
    let myname_as_le_bytes = my_name.raw().to_le_bytes();

    CanFrame {
        id: CanId(ADDR_CLAIM_ID_BASE | address_to_claim as u32),
        data: myname_as_le_bytes,
        len: myname_as_le_bytes.len(),
    }
}

//==================================================================================TOOLS
pub enum ClaimRelation {
    Unrelated,
    OwnClaim,
    WeWin,
    WeLose,
    PeerCannotClaim,
}

/// Classify claim relation between incoming frame and us.
pub fn classify_claim(frame: &CanFrame, my_name: IsoName, local_address: u8) -> ClaimRelation {
    if frame.id.pgn() != CLAIM_PGN_60928 {
        return ClaimRelation::Unrelated;
    }
    if frame.len != 8usize {
        return ClaimRelation::Unrelated;
    }
    let their_name = IsoName::from_raw(u64::from_le_bytes(frame.data));
    #[cfg(feature = "defmt")]
    defmt::debug!(
        "Claim RX: SA={}, Their NAME={:#X}, My NAME={:#X}",
        frame.id.source_address(),
        their_name,
        my_name
    );
    if their_name != my_name {
        if frame.id.source_address() == NULL_ADDR_254 {
            // PeerCannotClaim
            return ClaimRelation::PeerCannotClaim;
        }
        if frame.id.source_address() == local_address {
            // WeLoose
            if their_name < my_name {
                return ClaimRelation::WeLose;
            }
            // WeWin
            if their_name > my_name {
                return ClaimRelation::WeWin;
            }
        }
    }
    // OwnClaim
    if their_name == my_name {
        return ClaimRelation::OwnClaim;
    }
    // Everything else
    return ClaimRelation::Unrelated;
}

/// Extracts the NAME from an Address Claim frame (PGN 60928).
pub(super) fn extract_name_from_claim(frame: &CanFrame) -> Result<IsoName, ExtractionError> {
    if frame.id.pgn() != addr_mgmt_pgns::CLAIM_PGN_60928 {
        return Err(ExtractionError::InvalidIncomingFrame);
    }
    if frame.len != 8usize {
        return Err(ExtractionError::InvalidDataLen);
    }

    Ok(IsoName::from_raw(u64::from_le_bytes(frame.data)))
}

//==================================================================================TESTS
#[cfg(test)]
pub mod tests;

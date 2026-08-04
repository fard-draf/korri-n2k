//! SAE J1939 / NMEA 2000 address-claim algorithm:
//! emit PGN 60928, listen for conflicts, and fall back to alternative addresses when needed.
use crate::error::ClaimError::SendError;
use crate::error::{CanIdBuildError, ExtractionError};
use crate::protocol::constants::{addr_mgmt_pgns, address};
use crate::protocol::managment::address_claiming::engine::AddressClaimer;
use crate::protocol::managment::iso_name::IsoName;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
use crate::{
    error::ClaimError, protocol::transport::traits::can_bus::CanBus,
    protocol::transport::traits::korri_timer::KorriTimer,
};
use futures_util::future::{select, Either};
use futures_util::pin_mut;
mod engine;

/// Execute a full address-claim cycle and return the acquired address.
pub async fn claim_address<'a, C: CanBus, T: KorriTimer>(
    can_bus: &mut C,
    timer: &mut T,
    my_name: super::iso_name::IsoName,
    strategy: AddressClaimStrategy<'a>,
) -> Result<u8, ClaimError<C::Error>>
where
    C::Error: core::fmt::Debug,
{
    let mut addr_claimer = AddressClaimer::new(my_name);
    let mut rx: Option<CanFrame> = None;

    loop {
        let now_ms = timer.now_ms();
        match addr_claimer.poll(now_ms, rx.as_ref(), strategy) {
            Ok(claim_action) => match claim_action {
                engine::ClaimAction::Send(frame) => {
                    can_bus.send(&frame).await.map_err(SendError)?;
                    rx = None;
                }
                engine::ClaimAction::Wait(delay) => {
                    let recv = can_bus.recv();
                    pin_mut!(recv);
                    let timer = timer.delay_ms(delay);
                    pin_mut!(timer);
                    match select(timer.as_mut(), recv).await {
                        Either::Left(_) => rx = None,
                        Either::Right((f, _)) => {
                            rx = Some(f.map_err(|e| ClaimError::ReceiveError(e))?)
                        }
                    }
                }
                engine::ClaimAction::Done(addr) => return Ok(addr),
                engine::ClaimAction::CannotClaim(frame) => {
                    can_bus.send(&frame).await.map_err(SendError)?;
                    return Ok(address::NULL);
                }
            },
            Err(e) => return Err(ClaimError::Fault(e)),
        }
    }
}

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
pub fn build_address_claim_frame(
    my_name: IsoName,
    address_to_claim: u8,
) -> Result<CanFrame, CanIdBuildError> {
    let myname_as_le_bytes = my_name.raw().to_le_bytes();
    Ok(CanFrame {
        id: {
            match CanId::builder(addr_mgmt_pgns::ADDR_CLAIMED, address_to_claim)
                .to_destination(address::GLOBAL)
                .with_priority(6)
                .build()
            {
                Ok(can_id) => can_id,
                Err(_) => return Err(CanIdBuildError::InvalidData),
            }
        },
        data: myname_as_le_bytes,
        len: myname_as_le_bytes.len(),
    })
}

//==================================================================================TOOLS
/// Check whether an incoming claim frame conflicts with our current address.
fn is_conflicting_claim(
    incoming_frame: &CanFrame,
    my_claimed_address: u8,
    my_name: IsoName,
) -> bool {
    // All three conditions must be true for a conflict.
    // The `&&` operator ensures every predicate is checked in one expression.
    incoming_frame.id.pgn() == addr_mgmt_pgns::ADDR_CLAIMED
        && incoming_frame.id.source_address() != address::NULL
        && incoming_frame.id.source_address() == my_claimed_address
        && extract_name_from_claim(incoming_frame).is_ok_and(|their_name| their_name != my_name)
}

/// Extracts the NAME from an Address Claim frame (PGN 60928).
pub(super) fn extract_name_from_claim(frame: &CanFrame) -> Result<IsoName, ExtractionError> {
    if frame.id.pgn() != addr_mgmt_pgns::ADDR_CLAIMED {
        return Err(ExtractionError::InvalidIncomingFrame);
    }
    if frame.len != 8usize {
        return Err(ExtractionError::InvalidDataLen);
    }

    Ok(IsoName::from_raw(u64::from_le_bytes(frame.data)))
}

pub(super) fn is_addr_capable_and_isoname_match(
    my_name: u64,
    strategy: AddressClaimStrategy<'_>,
) -> bool {
    let is_addr_capable = ((my_name >> 63) & 0x01) as u8;
    match strategy {
        AddressClaimStrategy::Fixed { preferred: _ } => is_addr_capable == 0,
        AddressClaimStrategy::SelfConfigurable { addresses: _ } => is_addr_capable == 0,
        AddressClaimStrategy::Arbitrary { preferred: _ } => is_addr_capable == 1,
    }
}
//==================================================================================TESTS
#[cfg(test)]
pub mod tests;

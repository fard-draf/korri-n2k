//! SAE J1939 / NMEA 2000 address-claim algorithm:
//! emit PGN 60928, listen for conflicts, and fall back to alternative addresses when needed.
use crate::error::{CanIdBuildError, ClaimFault, ExtractionError};
use crate::protocol::constants::address;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
use crate::{
    error::ClaimError, protocol::transport::traits::can_bus::CanBus,
    protocol::transport::traits::korri_timer::KorriTimer,
};
use futures_util::future::{select, Either};
use futures_util::pin_mut;
mod machine;

/// Execute a full address-claim cycle and return the acquired address.
///
/// Strategy:
/// 1. Try the preferred address first.
/// 2. If the equipment is Arbitrary Address Capable (AAC), walk upwards from
///    the preferred address over the whole claimable range, wrapping around
///    (see [`address`]).
/// 3. After each attempt, listen for competing claims for 250 ms.
/// 4. Defend the address if the local NAME wins, otherwise move to the next one.
pub async fn claim_address<'a, C: CanBus, T: KorriTimer>(
    can_bus: &mut C,
    timer: &mut T,
    my_name: u64,
    strategy: AddressClaimStrategy<'a>,
) -> Result<u8, ClaimError<C::Error>>
where
    C::Error: core::fmt::Debug,
{
    // Determine consistency between IsoName and Addr Claim Strategy.
    if !is_addr_capable_and_isoname_match(my_name, strategy) {
        return Err(ClaimError::Fault(ClaimFault::InconsistentStrategy));
    };

    // Iterate over allowed addresses (preferred, then 1..251).
    let addr_iterator = AddressClaimIterator::<'a>::new(strategy);
    for address_to_claim in addr_iterator {
        // Step 1: propose our claim.
        #[cfg(feature = "defmt")]
        defmt::info!("Trying to claim address: {}", address_to_claim);

        let claim_frame =
            build_address_claim_frame(my_name, address_to_claim).map_err(ClaimFault::BuildErr)?;
        can_bus
            .send(&claim_frame)
            .await
            .map_err(ClaimError::SendError)?;

        #[cfg(feature = "defmt")]
        defmt::info!("Sent claim frame, waiting 250ms for conflicts...");

        // Step 2: 250 ms listening window for conflicts.
        let timer = timer.delay_ms(250);
        pin_mut!(timer);

        'listen_loop: loop {
            let need_defense = {
                let recv = can_bus.recv();
                pin_mut!(recv);

                match select(timer.as_mut(), recv).await {
                    Either::Left(_) => {
                        #[cfg(feature = "defmt")]
                        defmt::info!(
                            "Timer expired, address {} claimed successfully!",
                            address_to_claim
                        );
                        return Ok(address_to_claim);
                    }

                    Either::Right((incoming_frame, _)) => match incoming_frame {
                        Ok(incoming_frame) => {
                            // Ignore everything except Address Claim frames (PGN 60928)
                            if incoming_frame.id.pgn() != 60928 {
                                #[cfg(feature = "defmt")]
                                defmt::trace!(
                                    "Ignoring non-claim frame: PGN={}",
                                    incoming_frame.id.pgn()
                                );
                                false
                            } else {
                                #[cfg(feature = "defmt")]
                                defmt::debug!(
                                    "Received claim frame: PGN={}, SA={}",
                                    incoming_frame.id.pgn(),
                                    incoming_frame.id.source_address()
                                );

                                let their_name = extract_name_from_claim(&incoming_frame)
                                    .map_err(ClaimFault::Extraction)?;

                                #[cfg(feature = "defmt")]
                                defmt::debug!(
                                    "Claim RX: SA={}, Their NAME={:#X}, My NAME={:#X}",
                                    incoming_frame.id.source_address(),
                                    their_name,
                                    my_name
                                );

                                if is_conflicting_claim(&incoming_frame, address_to_claim, my_name)
                                {
                                    #[cfg(feature = "defmt")]
                                    defmt::warn!(
                                        "CONFLICT DETECTED! Their name: {:#X}, My name: {:#X}",
                                        their_name,
                                        my_name
                                    );

                                    if my_name > their_name {
                                        #[cfg(feature = "defmt")]
                                        defmt::warn!(
                                            "I LOSE (higher name), trying next address..."
                                        );
                                        match strategy {
                                            AddressClaimStrategy::Fixed { preferred: _ } => {
                                                return Err(ClaimError::Fault(
                                                    ClaimFault::NoAddressAvailable,
                                                ));
                                            }
                                            AddressClaimStrategy::SelfConfigurable {
                                                addresses: _,
                                            } => break 'listen_loop,
                                            AddressClaimStrategy::Arbitrary { preferred: _ } => {
                                                break 'listen_loop
                                            }
                                        }
                                    } else {
                                        #[cfg(feature = "defmt")]
                                        defmt::info!("I WIN (lower name), defending address...");
                                        true
                                    }
                                } else {
                                    #[cfg(feature = "defmt")]
                                    defmt::debug!("NOT a conflict (same NAME or different SA)");
                                    false
                                }
                            }
                        }

                        Err(e) => {
                            #[cfg(feature = "defmt")]
                            defmt::error!("Receive error occurred");
                            return Err(ClaimError::ReceiveError(e));
                        }
                    },
                }
            }; // recv borrow is dropped here

            // Optional defensive transmission (outside the `recv` borrow scope).
            if need_defense {
                let defense_frame = build_address_claim_frame(my_name, address_to_claim)
                    .map_err(ClaimFault::BuildErr)?;
                can_bus
                    .send(&defense_frame)
                    .await
                    .map_err(ClaimError::SendError)?;
            }
        }
    }

    // Iterator exhausted: no address available.
    Err(ClaimError::Fault(ClaimFault::NoAddressAvailable))
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

                    if address::is_claimable(candidate) {
                        if candidate != *preferred {
                            return Some(candidate);
                        }
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
    my_name: u64,
    address_to_claim: u8,
) -> Result<CanFrame, CanIdBuildError> {
    let myname_as_le_bytes = my_name.to_le_bytes();
    Ok(CanFrame {
        id: {
            match CanId::builder(60928, address_to_claim)
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
fn is_conflicting_claim(incoming_frame: &CanFrame, my_claimed_address: u8, my_name: u64) -> bool {
    // All three conditions must be true for a conflict.
    // The `&&` operator ensures every predicate is checked in one expression.
    incoming_frame.id.pgn() == 60928
        && incoming_frame.id.source_address() == my_claimed_address
        && extract_name_from_claim(incoming_frame).is_ok_and(|their_name| their_name != my_name)
}

/// Extracts the NAME from an Address Claim frame (PGN 60928).
pub(super) fn extract_name_from_claim(frame: &CanFrame) -> Result<u64, ExtractionError> {
    if frame.id.pgn() != 60928 {
        return Err(ExtractionError::InvalidIncomingFrame);
    }
    if frame.len != 8usize {
        return Err(ExtractionError::InvalidDataLen);
    }

    Ok(u64::from_le_bytes(frame.data))
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

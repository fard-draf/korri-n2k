//! SAE J1939 / NMEA 2000 address-claim algorithm:
//! emit PGN 60928, listen for conflicts, and fall back to alternative addresses when needed.
use crate::error::{CanIdBuildError, ExtractionError};
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
use crate::{
    error::ClaimError, protocol::transport::traits::can_bus::CanBus,
    protocol::transport::traits::korri_timer::KorriTimer,
};
use futures_util::future::{select, Either};
use futures_util::pin_mut;

/// Execute a full address-claim cycle and return the acquired address.
///
/// Strategy:
/// 1. Try the preferred address first.
/// 2. If the equipment is Arbitrary Address Capable (AAC), iterate over the 128–247 range.
/// 3. After each attempt, listen for competing claims for 250 ms.
/// 4. Defend the address if the local NAME wins, otherwise move to the next one.
pub async fn claim_address<C: CanBus, T: KorriTimer>(
    can_bus: &mut C,
    timer: &mut T,
    my_name: u64,
    preferred_address: u8,
) -> Result<u8, ClaimError<C::Error>>
where
    C::Error: core::fmt::Debug,
{
    // Determine AAC capabilities (bit 63 of the NAME).
    let is_arbitrary_capable = (my_name >> 63) & 1 == 1;
    // Iterate over allowed addresses (preferred, then 128-247).
    let addr_iterator = AddressClaimIterator::new(preferred_address, is_arbitrary_capable);

    for address_to_claim in addr_iterator {
        // Step 1: propose our claim.
        #[cfg(feature = "defmt")]
        defmt::info!("Trying to claim address: {}", address_to_claim);

        let claim_frame = build_address_claim_frame(my_name, address_to_claim)?;
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

                                let their_name = extract_name_from_claim(&incoming_frame)?;

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

                                        if is_arbitrary_capable {
                                            // Lost arbitration, try the next address
                                            break 'listen_loop;
                                        } else {
                                            return Ok(254);
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
                let defense_frame = build_address_claim_frame(my_name, address_to_claim)?;
                can_bus
                    .send(&defense_frame)
                    .await
                    .map_err(ClaimError::SendError)?;
            }
        }
    }

    // Iterator exhausted: no address available.
    Err(ClaimError::NoAddressAvailable)
}

//==================================================================================ADDRESS_CLAIM_ITERATOR
/// Generates candidate addresses following the J1939 rules.
struct AddressClaimIterator {
    preferred: u8,
    next_arbitrary: u16,
    state: AddressClaimState,
    arbitrary_capable: bool,
}

#[derive(PartialEq)]
/// Iteration states (preferred address, then AAC range).
enum AddressClaimState {
    TryPreferred,
    TryArbitrary,
    Done,
}

impl AddressClaimIterator {
    /// Prepare the iterator with the preferred address and AAC capability flag.
    pub fn new(preferred_address: u8, arbitrary_capable: bool) -> Self {
        Self {
            preferred: preferred_address,
            next_arbitrary: 128,
            state: AddressClaimState::TryPreferred,
            arbitrary_capable,
        }
    }
}

impl Iterator for AddressClaimIterator {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state {
                AddressClaimState::TryPreferred => {
                    // After trying the preferred address, move on.
                    self.state = if self.arbitrary_capable {
                        AddressClaimState::TryArbitrary
                    } else {
                        // Non-AAC equipment only gets a single attempt.
                        AddressClaimState::Done
                    };

                    if self.preferred <= 247 {
                        return Some(self.preferred);
                    }
                }
                AddressClaimState::TryArbitrary => {
                    // Safeguard in case of inconsistent usage (should not happen).
                    if !self.arbitrary_capable {
                        self.state = AddressClaimState::Done;
                        continue;
                    }
                    // Iterate through the standard 128-247 range.
                    if self.next_arbitrary > 247 {
                        self.state = AddressClaimState::Done;
                        continue;
                    }

                    let addr_to_try = self.next_arbitrary as u8;
                    self.next_arbitrary += 1;

                    // Skip the preferred address (already tested).
                    if addr_to_try == self.preferred {
                        continue;
                    }
                    return Some(addr_to_try);
                }
                AddressClaimState::Done => {
                    return None;
                }
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
                .to_destination(255)
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

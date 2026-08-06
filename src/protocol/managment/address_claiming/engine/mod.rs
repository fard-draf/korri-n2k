use crate::{
    error::ClaimFault::{self},
    protocol::{
        constants::{
            address::{self, NULL_ADDR_254},
            iso_delay::{CANNOT_CLAIM_RETRY_DELAY_MS, CLAIM_DELAY_MS, NO_DEADLINE_DELAY_MS},
        },
        managment::{
            address_claiming::{
                build_address_claim_frame, classify_claim, AddressClaimIterator,
                AddressClaimStrategy, ClaimRelation,
            },
            iso_name::IsoName,
        },
        transport::can_frame::CanFrame,
    },
};

#[derive(PartialEq, Eq, Debug)]
pub enum ClaimAction {
    CannotClaim(CanFrame),
    Send(CanFrame),
    Wait(u32),
    Claimed(u8),
}

#[derive(PartialEq, Eq, Debug)]
enum State {
    UnClaimed,
    Claiming { frame: CanFrame, deadline_ms: u64 },
    Claimed { frame: CanFrame },
    CannotClaim { retry_at_ms: u64 },
}

pub struct AddressClaimEngine<'a> {
    my_name: IsoName,
    addr_iterator: Option<AddressClaimIterator<'a>>,
    strategy: AddressClaimStrategy<'a>,
    state: State,
}

impl<'a> AddressClaimEngine<'a> {
    pub fn new(my_name: IsoName, strategy: AddressClaimStrategy<'a>) -> Result<Self, ClaimFault> {
        if !IsoName::is_addr_capable_and_isoname_match(&my_name, strategy) {
            return Err(ClaimFault::InconsistentStrategy);
        }
        Ok(Self {
            my_name: my_name,
            addr_iterator: None,
            strategy,
            state: State::UnClaimed,
        })
    }

    fn start_claim(&mut self, now_ms: u64, strategy: AddressClaimStrategy<'a>) -> ClaimAction {
        let iterator = self
            .addr_iterator
            .insert(AddressClaimIterator::new(strategy));
        let addr_to_claim = iterator.try_next_addr();
        let claim_frame = build_address_claim_frame(self.my_name, addr_to_claim);

        if addr_to_claim == NULL_ADDR_254 {
            self.state = State::CannotClaim {
                retry_at_ms: now_ms + CANNOT_CLAIM_RETRY_DELAY_MS as u64,
            };
            return ClaimAction::CannotClaim(claim_frame);
        }

        self.state = State::Claiming {
            frame: claim_frame,
            deadline_ms: now_ms + CLAIM_DELAY_MS as u64,
        };
        return ClaimAction::Send(claim_frame);
    }

    // Strategy:
    // 1. Try the preferred address first.
    // 2. If the equipment is Arbitrary Address Capable (AAC), walk upwards from
    //    the preferred address over the whole claimable range, wrapping around.
    // 3. After each attempt, listen for competing claims for 250 ms.
    // 4. Defend the address if the local NAME wins, otherwise move to the next one.
    pub fn poll(&mut self, now_ms: u64, rx: Option<&CanFrame>) -> ClaimAction {
        // TODO!: check the pseudo-random delay for claiming
        // guards
        match self.state {
            // not started | first call
            State::UnClaimed => self.start_claim(now_ms, self.strategy),
            State::Claiming { frame, deadline_ms } => {
                // The frame comes first: only a conflict changes the state.
                // TODO!: implement COMMANDED_ADDRESS 65240 0xFED8, today an Unrelated frame.
                if let Some(recv_frame) = rx {
                    match classify_claim(recv_frame, self.my_name, frame.id.source_address()) {
                        // we win -> re-send the current claim, deadline untouched
                        ClaimRelation::WeWin => return ClaimAction::Send(frame),
                        ClaimRelation::WeLose => return self.handle_loosing_conflict(now_ms),
                        // Unrelated | OwnClaim | PeerCannotClaim: no effect, fall through
                        // to the deadline so a harmless frame cannot stall the acquisition.
                        _ => {}
                    }
                }
                // Then the deadline. Hits 0 once now_ms >= deadline_ms.
                let remaining_ms = deadline_ms.saturating_sub(now_ms);
                if remaining_ms == 0 {
                    self.state = State::Claimed { frame };
                    return ClaimAction::Claimed(frame.id.source_address());
                }
                return ClaimAction::Wait(remaining_ms as u32);
            }

            State::Claimed { frame } => {
                let Some(recv_frame) = rx else {
                    return ClaimAction::Wait(NO_DEADLINE_DELAY_MS as u32);
                };
                match classify_claim(recv_frame, self.my_name, frame.id.source_address()) {
                    ClaimRelation::Unrelated => {
                        return ClaimAction::Wait(NO_DEADLINE_DELAY_MS as u32)
                    }
                    ClaimRelation::OwnClaim => {
                        return ClaimAction::Wait(NO_DEADLINE_DELAY_MS as u32)
                    }
                    ClaimRelation::WeWin => return ClaimAction::Send(frame),
                    ClaimRelation::WeLose => {
                        return self.handle_loosing_conflict(now_ms);
                    }
                    ClaimRelation::PeerCannotClaim => {
                        return ClaimAction::Wait(NO_DEADLINE_DELAY_MS as u32)
                    }
                };
            }

            State::CannotClaim { retry_at_ms } => {
                if now_ms < retry_at_ms {
                    return ClaimAction::Wait(retry_at_ms.saturating_sub(now_ms) as u32);
                } else {
                    return self.start_claim(now_ms, self.strategy);
                }
            }
        }
    }

    fn handle_loosing_conflict(&mut self, now_ms: u64) -> ClaimAction {
        let addr_to_claim: u8 = {
            if let Some(addr_it) = &mut self.addr_iterator {
                addr_it.try_next_addr()
            } else {
                address::NULL_ADDR_254
            }
        };

        let claim_frame = build_address_claim_frame(self.my_name, addr_to_claim);

        if addr_to_claim == address::NULL_ADDR_254 {
            self.state = State::CannotClaim {
                retry_at_ms: now_ms + CANNOT_CLAIM_RETRY_DELAY_MS as u64,
            };
            return ClaimAction::CannotClaim(claim_frame);
        }

        self.state = State::Claiming {
            frame: claim_frame.clone(),
            deadline_ms: now_ms + CLAIM_DELAY_MS as u64,
        };

        return ClaimAction::Send(claim_frame);
    }
}

#[cfg(test)]
pub mod tests;

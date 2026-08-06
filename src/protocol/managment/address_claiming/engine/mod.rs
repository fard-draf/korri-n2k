use crate::{
    error::ClaimFault::{self},
    protocol::{
        constants::{
            addr_mgmt_pgns::{CLAIM_PGN_60928, REQUEST_PGN_59904, REQUEST_PGN_LEN},
            address::{self, GLOBAL, NULL_ADDR_254},
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

    fn start_claim(&mut self, now_ms: u64) -> ClaimAction {
        let iterator = self
            .addr_iterator
            .insert(AddressClaimIterator::new(self.strategy));
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
            State::UnClaimed => match rx {
                Some(recv_frame)
                    if is_addressed_claim_request_message(recv_frame, NULL_ADDR_254) =>
                {
                    let claim_frame = build_address_claim_frame(self.my_name, NULL_ADDR_254);
                    return ClaimAction::Send(claim_frame);
                }
                _ => return self.start_claim(now_ms),
            },
            State::Claiming { frame, deadline_ms } => {
                // The frame comes first: only a conflict changes the state.
                // TODO!: implement COMMANDED_ADDRESS 65240 0xFED8, today an Unrelated frame.
                if let Some(recv_frame) = rx {
                    if is_addressed_claim_request_message(recv_frame, frame.id.source_address()) {
                        return ClaimAction::Send(frame);
                    }
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
                if is_addressed_claim_request_message(recv_frame, frame.id.source_address()) {
                    return ClaimAction::Send(frame);
                }
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

            State::CannotClaim { retry_at_ms } => match rx {
                Some(recv_frame)
                    if is_addressed_claim_request_message(recv_frame, NULL_ADDR_254) =>
                {
                    let claim_frame = build_address_claim_frame(self.my_name, NULL_ADDR_254);
                    return ClaimAction::Send(claim_frame);
                }
                _ => {
                    if now_ms < retry_at_ms {
                        return ClaimAction::Wait(retry_at_ms.saturating_sub(now_ms) as u32);
                    } else {
                        return self.start_claim(now_ms);
                    }
                }
            },
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

    /// return an Option with Some(addr) if claimed address, otherwise None.
    pub fn claimed_address(&self) -> Option<u8> {
        match self.state {
            State::Claimed { frame } => Some(frame.id.source_address()),
            _ => None,
        }
    }
}

fn is_addressed_claim_request_message(frame: &CanFrame, curr_addr: u8) -> bool {
    if frame.id.pgn() != REQUEST_PGN_59904 || frame.len < REQUEST_PGN_LEN {
        return false;
    }
    // The requested PGN is 3 bytes; the padding bytes are not part of it.
    let requested_pgn = u32::from_le_bytes([frame.data[0], frame.data[1], frame.data[2], 0]);

    requested_pgn == CLAIM_PGN_60928
        && frame
            .id
            .destination()
            .is_some_and(|request_addr| request_addr == curr_addr || request_addr == GLOBAL)
}

#[cfg(test)]
pub mod tests;

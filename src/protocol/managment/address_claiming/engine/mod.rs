use crate::{
    error::ClaimFault::{self},
    protocol::{
        managment::address_claiming::{
            build_address_claim_frame, extract_name_from_claim, is_addr_capable_and_isoname_match,
            is_conflicting_claim, AddressClaimIterator, AddressClaimStrategy,
        },
        transport::can_frame::CanFrame,
    },
};

#[derive(PartialEq, Eq, Debug)]
pub enum ClaimAction {
    Send(CanFrame),
    Wait(u32),
    Done(u8),
}

#[derive(PartialEq, Eq, Debug)]
enum State {
    Idle,
    Listening { frame: CanFrame, deadline_ms: u64 },
}

pub struct AddressClaimer<'a> {
    my_name: Option<u64>,
    addr_iterator: Option<AddressClaimIterator<'a>>,
    state: State,
}

impl<'a> AddressClaimer<'a> {
    pub fn new(my_name: u64) -> Self {
        Self {
            my_name: Some(my_name),
            addr_iterator: None,
            state: State::Idle,
        }
    }

    // Strategy:
    // 1. Try the preferred address first.
    // 2. If the equipment is Arbitrary Address Capable (AAC), walk upwards from
    //    the preferred address over the whole claimable range, wrapping around.
    // 3. After each attempt, listen for competing claims for 250 ms.
    // 4. Defend the address if the local NAME wins, otherwise move to the next one.
    pub fn poll(
        &mut self,
        now_ms: u64,
        rx: Option<&CanFrame>,
        strategy: AddressClaimStrategy<'a>,
    ) -> Result<ClaimAction, ClaimFault> {
        if let Some(my_name) = self.my_name {
            // guards
            if !is_addr_capable_and_isoname_match(my_name, strategy) {
                return Err(ClaimFault::InconsistentStrategy);
            }
            match self.state {
                // not started | first call
                State::Idle => {
                    let iterator = self
                        .addr_iterator
                        .insert(AddressClaimIterator::new(strategy));
                    let Some(addr_to_claim) = iterator.next() else {
                        return Err(ClaimFault::NoAddressAvailable);
                    };
                    let claim_frame = build_address_claim_frame(my_name, addr_to_claim)
                        .map_err(ClaimFault::BuildErr)?;
                    self.state = State::Listening {
                        frame: claim_frame.clone(),
                        deadline_ms: now_ms + 250,
                    };
                    return Ok(ClaimAction::Send(claim_frame));
                }
                State::Listening {
                    frame,
                    mut deadline_ms,
                } => {
                    // hit 0 if now_ms > deadline.
                    deadline_ms = deadline_ms.saturating_sub(now_ms);
                    // is timer finished ?
                    if deadline_ms == 0 {
                        #[cfg(feature = "defmt")]
                        defmt::info!(
                            "Timer expired, address {} claimed successfully!",
                            address_to_claim
                        );
                        self.state = State::Idle;
                        return Ok(ClaimAction::Done(frame.id.source_address()));
                    }
                    // listen     | rx == None && now <  deadline
                    if rx.is_none() && deadline_ms > 0 {
                        return Ok(ClaimAction::Wait(deadline_ms as u32));
                    }
                    // listen     | rx == Some
                    let Some(recv) = rx else {
                        return Ok(ClaimAction::Wait(deadline_ms as u32));
                    };
                    if recv.id.pgn() != 60928 {
                        #[cfg(feature = "defmt")]
                        defmt::trace!("Ignoring non-claim frame: PGN={}", recv.id.pgn());
                        return Ok(ClaimAction::Wait(deadline_ms as u32));
                    } else {
                        #[cfg(feature = "defmt")]
                        defmt::debug!(
                            "Received claim frame: PGN={}, SA={}",
                            recv.id.pgn(),
                            recv.id.source_address()
                        );
                        let their_name = match extract_name_from_claim(recv) {
                            Ok(name) => name,
                            // malformed frame is considered as a winning conflict, we win.
                            Err(_) => return Ok(ClaimAction::Send(frame)),
                        };
                        #[cfg(feature = "defmt")]
                        defmt::debug!(
                            "Claim RX: SA={}, Their NAME={:#X}, My NAME={:#X}",
                            recv.id.source_address(),
                            their_name,
                            my_name
                        );
                        if is_conflicting_claim(recv, frame.id.source_address(), my_name) {
                            #[cfg(feature = "defmt")]
                            defmt::warn!(
                                "CONFLICT DETECTED! Their name: {:#X}, My name: {:#X}",
                                their_name,
                                my_name
                            );
                            // listen     | conflict: my_name is lower -> wins
                            if my_name < their_name {
                                // we win -> sending a new claiming frame while keeping same deadline
                                #[cfg(feature = "defmt")]
                                defmt::info!("I WIN (lower name), defending address...");
                                return Ok(ClaimAction::Send(frame));
                            }
                            // listen     | conflict: my_name is higher -> loses
                            if my_name > their_name {
                                // we lose -> try next address if possible
                                #[cfg(feature = "defmt")]
                                defmt::warn!("I LOSE (higher name), trying next address...");
                                self.state = State::Idle;
                                match strategy {
                                    AddressClaimStrategy::Fixed { preferred: _ } => {
                                        return Err(ClaimFault::NoAddressAvailable)
                                    }
                                    _ => {
                                        // try next addr, available for arbitrary and self_configurable strategies
                                        let Some(addr_itrerator) = &mut self.addr_iterator else {
                                            // no addr available on the iterator
                                            return Err(ClaimFault::NoAddressAvailable);
                                        };
                                        let Some(next_addr) = addr_itrerator.next() else {
                                            // listen     | conflict -> lose, no more address available
                                            return Err(ClaimFault::NoAddressAvailable);
                                        };
                                        let claim_frame =
                                            build_address_claim_frame(my_name, next_addr)
                                                .map_err(ClaimFault::BuildErr)?;
                                        self.state = State::Listening {
                                            frame: claim_frame.clone(),
                                            deadline_ms: now_ms + 250,
                                        };

                                        return Ok(ClaimAction::Send(claim_frame));
                                    }
                                }
                            }
                        } else {
                            // listen     | frame without conflict
                            if deadline_ms > 0 {
                                return Ok(ClaimAction::Wait(deadline_ms as u32));
                            }
                        }
                    }
                }
            }
        } else {
            // impossible to continue without a name;
            return Err(ClaimFault::UnvailableName);
        }
        return Err(ClaimFault::NoAddressAvailable);
    }
}

#[cfg(test)]
pub mod tests;

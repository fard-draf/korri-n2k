use crate::{
    error::ClaimFault::{self},
    protocol::{
        constants::{
            addr_mgmt_pgns::{CLAIM_PGN_60928, REQUEST_PGN_59904, REQUEST_PGN_LEN},
            address::{self, GLOBAL, NULL_ADDR_254},
            iso_delay::{CANNOT_CLAIM_RETRY_DELAY_MS, CLAIM_DELAY_MS},
        },
        management::{
            address_claiming::{
                build_address_claim_frame, classify_claim, AddressClaimIterator,
                AddressClaimStrategy, ClaimRelation,
            },
            iso_name::IsoName,
        },
        transport::can_frame::CanFrame,
    },
};

/// What one `poll` decided, along three independent axes.
///
/// The three fields answer three different questions. None of them replaces
/// another. An emission can happen in the same step as a state change, and a
/// state change does not imply a deadline.
///
/// # Contract
///
/// * `tx` carries at most one frame to emit.
/// * Call [`AddressClaimEngine::tx_sent`] after a successful emission.
/// * `status` is the state **after** `rx` and the deadlines have been handled.
/// * `wake_at_ms` is an **absolute** deadline. It lives in the same domain as
///   the `now_ms` passed to [`AddressClaimEngine::poll`].
/// * `wake_at_ms: None` means no timer is pending. It never means the engine is
///   done. An incoming frame must still wake it.
/// * **If `tx` is present, emit it before leaving the loop on `status`.** A
///   `Claimed` handed back with a defence frame still owes that frame to the
///   bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimOutput {
    /// The frame to emit, if any.
    pub tx: Option<CanFrame>,
    /// The state the engine is in now.
    pub status: ClaimStatus,
    /// Absolute deadline at which `poll` must be called again, if any.
    pub wake_at_ms: Option<u64>,
}

/// The engine's state as the caller sees it.
///
/// There is no `Unclaimed` variant. The first `poll` always starts a campaign,
/// so the engine is never observed before one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimStatus {
    /// Emitting for the carried address and waiting out the arbitration window.
    /// The address is not usable yet.
    Claiming(u8),
    /// The carried address is held and may be emitted from.
    Claimed(u8),
    /// Every candidate address was refused. A retry is pending.
    CannotClaim,
}

impl ClaimStatus {
    /// The address the node may emit from, `None` in every other state.
    ///
    /// `Claiming` returns `None` on purpose. The arbitration window is still
    /// open, so the address is not ours yet.
    pub fn claimed_address(&self) -> Option<u8> {
        match self {
            Self::Claimed(address) => Some(*address),
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
enum State {
    Unclaimed,
    Claiming { frame: CanFrame, deadline_ms: u64 },
    Claimed { frame: CanFrame },
    CannotClaim { retry_at_ms: u64 },
}

pub struct AddressClaimEngine<'a> {
    my_name: IsoName,
    addr_iterator: Option<AddressClaimIterator<'a>>,
    strategy: AddressClaimStrategy<'a>,
    state: State,
    tx_pending: bool,
}

impl<'a> AddressClaimEngine<'a> {
    pub fn new(my_name: IsoName, strategy: AddressClaimStrategy<'a>) -> Result<Self, ClaimFault> {
        if !IsoName::is_addr_capable_and_isoname_match(&my_name, strategy) {
            return Err(ClaimFault::InconsistentStrategy);
        }
        Ok(Self {
            my_name,
            addr_iterator: None,
            strategy,
            state: State::Unclaimed,
            tx_pending: false,
        })
    }

    /// Build the output from the state, so `status` and `wake_at_ms` are never
    /// invented at a call site. Only `tx` is a decision. The other two are
    /// consequences. That is what keeps `Claimed` from ever carrying a claim
    /// deadline that is still running.
    fn output(&self, tx: Option<CanFrame>) -> ClaimOutput {
        let (status, wake_at_ms) = match self.state {
            // Unreachable: `output` only runs after a transition, and every
            // transition leaves `Unclaimed` behind for good.
            State::Unclaimed => unreachable!("output() called before the first campaign"),
            State::Claiming { frame, deadline_ms } => {
                let wake_at_ms = (!self.tx_pending).then_some(deadline_ms);
                (ClaimStatus::Claiming(frame.id.source_address()), wake_at_ms)
            }
            State::Claimed { frame } => (ClaimStatus::Claimed(frame.id.source_address()), None),
            State::CannotClaim { retry_at_ms } => {
                let wake_at_ms = (!self.tx_pending).then_some(retry_at_ms);
                (ClaimStatus::CannotClaim, wake_at_ms)
            }
        };

        ClaimOutput {
            tx,
            status,
            wake_at_ms,
        }
    }

    fn start_claim(&mut self, now_ms: u64) -> ClaimOutput {
        let iterator = self
            .addr_iterator
            .insert(AddressClaimIterator::new(self.strategy));
        let addr_to_claim = iterator.try_next_addr();
        let claim_frame = build_address_claim_frame(self.my_name, addr_to_claim);

        if addr_to_claim == NULL_ADDR_254 {
            self.state = State::CannotClaim {
                retry_at_ms: now_ms.saturating_add(CANNOT_CLAIM_RETRY_DELAY_MS as u64),
            };
        } else {
            self.state = State::Claiming {
                frame: claim_frame,
                deadline_ms: now_ms.saturating_add(CLAIM_DELAY_MS as u64),
            };
        }
        self.tx_pending = true;

        self.output(Some(claim_frame))
    }

    /// Start the timer after the frame was sent.
    pub fn tx_sent(&mut self, now_ms: u64) -> ClaimOutput {
        if self.tx_pending {
            match &mut self.state {
                State::Claiming { deadline_ms, .. } => {
                    *deadline_ms = now_ms.saturating_add(CLAIM_DELAY_MS as u64);
                }
                State::CannotClaim { retry_at_ms } => {
                    *retry_at_ms = now_ms.saturating_add(CANNOT_CLAIM_RETRY_DELAY_MS as u64);
                }
                _ => {}
            }
            self.tx_pending = false;
        }

        self.output(None)
    }

    // Strategy:
    // 1. Try the preferred address first.
    // 2. If the equipment is Arbitrary Address Capable (AAC), walk upwards from
    //    the preferred address over the whole claimable range, wrapping around.
    // 3. After each attempt, listen for competing claims for 250 ms.
    // 4. Defend the address if the local NAME wins, otherwise move to the next one.
    pub fn poll(&mut self, now_ms: u64, rx: Option<&CanFrame>) -> ClaimOutput {
        if self.tx_pending {
            let tx = match self.state {
                State::Claiming { frame, .. } => frame,
                State::CannotClaim { .. } => build_address_claim_frame(self.my_name, NULL_ADDR_254),
                _ => unreachable!("pending send without a frame"),
            };
            return self.output(Some(tx));
        }

        // TODO!: check the pseudo-random delay for claiming
        match self.state {
            // Not started. Whatever arrived, the campaign begins now.
            //
            // A request is not special-cased here. Answering it with a Cannot
            // Claim and staying put would leave the node addressless for as long
            // as requests keep coming: the runner reads the bus before the wake
            // deadline, so a steady stream always wins the race. The Address
            // Claim `start_claim` emits is itself the answer to the request, and
            // a strategy with no claimable address emits the Cannot Claim anyway.
            State::Unclaimed => self.start_claim(now_ms),

            State::Claiming { frame, deadline_ms } => {
                // TODO!: implement COMMANDED_ADDRESS 65240 0xFED8, today an Unrelated frame.
                let mut tx = None;

                if let Some(recv_frame) = rx {
                    if is_addressed_claim_request_message(recv_frame, frame.id.source_address()) {
                        tx = Some(frame);
                    } else {
                        match classify_claim(recv_frame, self.my_name, frame.id.source_address()) {
                            // A loss outranks the deadline, including at the exact
                            // millisecond it expires: winning the wait does not
                            // make the address ours if someone better claimed it.
                            ClaimRelation::WeLose => return self.handle_losing_conflict(now_ms),
                            // We win: defend, deadline untouched.
                            ClaimRelation::WeWin => tx = Some(frame),
                            // Unrelated | OwnClaim | PeerCannotClaim: no effect.
                            _ => {}
                        }
                    }
                }

                // The deadline is consumed whatever the frame did, so a bus that
                // requests or loses arbitration on every poll cannot starve the
                // acquisition.
                if deadline_ms.saturating_sub(now_ms) == 0 {
                    self.state = State::Claimed { frame };
                }

                self.output(tx)
            }

            State::Claimed { frame } => {
                let mut tx = None;

                if let Some(recv_frame) = rx {
                    if is_addressed_claim_request_message(recv_frame, frame.id.source_address()) {
                        tx = Some(frame);
                    } else {
                        match classify_claim(recv_frame, self.my_name, frame.id.source_address()) {
                            ClaimRelation::WeLose => return self.handle_losing_conflict(now_ms),
                            ClaimRelation::WeWin => tx = Some(frame),
                            // Unrelated | OwnClaim | PeerCannotClaim: no effect.
                            _ => {}
                        }
                    }
                }

                self.output(tx)
            }

            State::CannotClaim { retry_at_ms } => {
                // The retry comes first: answering requests must not postpone it.
                if retry_at_ms.saturating_sub(now_ms) == 0 {
                    return self.start_claim(now_ms);
                }

                let tx = match rx {
                    Some(recv_frame)
                        if is_addressed_claim_request_message(recv_frame, NULL_ADDR_254) =>
                    {
                        Some(build_address_claim_frame(self.my_name, NULL_ADDR_254))
                    }
                    _ => None,
                };

                self.output(tx)
            }
        }
    }

    fn handle_losing_conflict(&mut self, now_ms: u64) -> ClaimOutput {
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
                retry_at_ms: now_ms.saturating_add(CANNOT_CLAIM_RETRY_DELAY_MS as u64),
            };
        } else {
            self.state = State::Claiming {
                frame: claim_frame,
                deadline_ms: now_ms.saturating_add(CLAIM_DELAY_MS as u64),
            };
        }
        self.tx_pending = true;

        self.output(Some(claim_frame))
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

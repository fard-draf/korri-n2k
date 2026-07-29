use crate::{
    error::ClaimFault::{self},
    protocol::{
        constants::address::is_claimable,
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
#[allow(dead_code)]
impl<'a> AddressClaimer<'a> {
    pub fn new(my_name: u64) -> Self {
        Self {
            my_name: Some(my_name),
            addr_iterator: None,
            state: State::Idle,
        }
    }

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
                    // prepare to send 60928 // 0xEE00
                    self.addr_iterator = Some(AddressClaimIterator::<'a>::new(strategy));
                    if let Some(addr_iterator) = &mut self.addr_iterator {
                        if let Some(addr_to_claim) = addr_iterator.next() {
                            if !is_claimable(addr_to_claim) {
                                return Err(ClaimFault::UnvalidClaimAddress);
                            }
                            let claim_frame = build_address_claim_frame(my_name, addr_to_claim)
                                .map_err(ClaimFault::BuildErr)?;
                            self.state = State::Listening {
                                frame: claim_frame.clone(),
                                deadline_ms: now_ms + 250,
                            };
                            return Ok(ClaimAction::Send(claim_frame));
                        } else {
                            return Err(ClaimFault::NoAddressAvailable);
                        }
                    } else {
                        return Err(ClaimFault::RequestAddressClaimErr);
                    }
                }
                State::Listening {
                    frame,
                    mut deadline_ms,
                } => {
                    // hit 0 if now_ms > deadline.
                    deadline_ms = deadline_ms.saturating_sub(now_ms);
                    // is timer finished ?
                    if deadline_ms == 0 {
                        self.state = State::Idle;
                        return Ok(ClaimAction::Done(frame.id.source_address()));
                    }
                    // listen     | rx == None && now <  deadline
                    if rx.is_none() && deadline_ms > 0 {
                        return Ok(ClaimAction::Wait(deadline_ms as u32));
                    }
                    // listen     | rx == Some
                    if let Some(recv) = rx {
                        // deadline_ms = deadline_ms.saturating_sub(now_ms);
                        if recv.id.pgn() != 60928 {
                            return Ok(ClaimAction::Wait(deadline_ms as u32));
                        } else {
                            let their_name =
                                extract_name_from_claim(recv).map_err(ClaimFault::Extraction)?;
                            if is_conflicting_claim(recv, frame.id.source_address(), my_name) {
                                // listen     | conflict: my_name is lower -> wins
                                if my_name < their_name {
                                    // we win -> sending a new claiming frame while keeping same deadline
                                    return Ok(ClaimAction::Send(frame));
                                }
                                // listen     | conflict: my_name is higher -> loses
                                if my_name > their_name {
                                    self.state = State::Idle;
                                    match strategy {
                                        AddressClaimStrategy::Fixed { preferred: _ } => {
                                            return Err(ClaimFault::NoAddressAvailable)
                                        }
                                        _ => {
                                            // try next addr, available for arbitrary and self_configurable strategies
                                            if let Some(addr_iterator) = &mut self.addr_iterator {
                                                if let Some(next_addr) = addr_iterator.next() {
                                                    let claim_frame = build_address_claim_frame(
                                                        my_name, next_addr,
                                                    )
                                                    .map_err(|_e| {
                                                        ClaimFault::RequestAddressClaimErr
                                                    })?;
                                                    self.state = State::Listening {
                                                        frame: claim_frame.clone(),
                                                        deadline_ms: now_ms + 250,
                                                    };

                                                    return Ok(ClaimAction::Send(claim_frame));
                                                }
                                            } else {
                                                // no addr available on the iterator
                                                return Err(ClaimFault::NoAddressAvailable);
                                            }
                                        }
                                    }
                                    // listen     | conflict >> lose, no more address available
                                    return Err(ClaimFault::NoAddressAvailable);
                                }
                            } else {
                                // listen     | frame without conflict
                                if deadline_ms > 0 {
                                    return Ok(ClaimAction::Wait(deadline_ms as u32));
                                } else {
                                    return Ok(ClaimAction::Done(frame.id.source_address()));
                                }
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

// let my_name: u64 = 0x1234567890ABCDEF; // MSB is 0 → not arbitrary capable
// let their_name: u64 = 0x1234567890ABCDEE; // BuiltToLooseer than my_name → we lose

#[cfg(test)]

mod tests {
    use crate::protocol::{
        constants::address::{self, is_claimable},
        transport::can_id::CanIdBuilder,
    };

    use super::*;
    use log;

    const ADDR_1: u8 = 1;
    const ADDR_2: u8 = 2;
    const ADDR_3: u8 = 144;
    const ADDR_4: u8 = 244;
    const STARTING_TIME: u64 = 123456789;

    struct ClockTest {
        ms: u64,
    }

    impl ClockTest {
        fn new(start_value: u64) -> Self {
            Self { ms: start_value }
        }

        fn tick(&mut self, d: u64) -> u64 {
            self.ms += d;
            self.ms
        }
    }

    #[derive(Clone, Copy)]
    struct Name(u64);

    impl Default for Name {
        fn default() -> Self {
            Self(0x1234567890ABCDEF)
        }
    }

    impl Name {
        fn new(strategy: AddressClaimStrategy) -> Self {
            let mut name = Name::default();
            match strategy {
                AddressClaimStrategy::Fixed { preferred: _ } => name,
                AddressClaimStrategy::SelfConfigurable { addresses: _ } => name,
                AddressClaimStrategy::Arbitrary { preferred: _ } => {
                    name.0 |= 1u64 << 63;
                    assert_eq!(name.0, 0x9234567890ABCDEF);
                    name
                }
            }
        }

        // fn build_lower_name(&mut self) {
        //     self.0 &= 0xFFFF_FFFF_FFFF_FF00u64;
        // }

        // fn build_higher_name(&mut self) {
        //     self.0 |= 0x0000_0000_0000_00FFu64;
        // }

        fn conflict_priority_builder(mut self, name_priority: ConflictPriority) -> Self {
            match name_priority {
                ConflictPriority::BuiltToWin => self.0 &= !0xFFFF,
                ConflictPriority::Normal => {}
                ConflictPriority::BuiltToLoose => self.0 |= 0xFFFF,
            }
            return self;
        }
    }

    struct Instance<'a> {
        name: Name,
        strategy: AddressClaimStrategy<'a>,
        preferred_addr: u8,
        can_frame_origin: CanFrame,
        can_frame_next: Option<CanFrame>,
        claimer: AddressClaimer<'a>,
    }

    enum CanFrameClass {
        Claiming,
        Normal,
    }

    // Higher wins (VeryHigh is the lowest name)
    enum ConflictPriority {
        BuiltToWin,
        Normal,
        BuiltToLoose,
    }

    impl<'a> Instance<'a> {
        fn new(
            strategy: AddressClaimStrategy<'a>,
            can_frame_class: CanFrameClass,
            conflict_design: ConflictPriority,
        ) -> Self {
            let name = Name::new(strategy).conflict_priority_builder(conflict_design);
            let preferred_addr: u8 = match strategy {
                AddressClaimStrategy::Fixed { preferred } => preferred,
                AddressClaimStrategy::SelfConfigurable { addresses } => addresses[0],
                AddressClaimStrategy::Arbitrary { preferred } => preferred,
            };

            let can_frame: CanFrame = match can_frame_class {
                CanFrameClass::Claiming => {
                    build_address_claim_frame(name.0, preferred_addr).expect("must be valid")
                }
                CanFrameClass::Normal => {
                    let id = CanIdBuilder::new(129044, preferred_addr)
                        .build()
                        .expect("must be valid");
                    CanFrame {
                        id,
                        data: [1u8; 8],
                        len: 8,
                    }
                }
            };

            Self {
                name,
                strategy,
                preferred_addr,
                can_frame_origin: can_frame,
                can_frame_next: None,
                claimer: AddressClaimer::new(name.0),
            }
        }
    }

    #[test]
    fn test_addr_claim_machine_aac_without_rx() {
        let my_preferred = ADDR_1;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let mut timer = ClockTest::new(STARTING_TIME);

        assert_eq!(my_inst.claimer.state, State::Idle);

        assert_eq!(
            my_inst.claimer.poll(timer.ms, None, my_strategy).unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(10);

        assert_eq!(
            my_inst.claimer.poll(timer.ms, None, my_strategy).unwrap(),
            ClaimAction::Wait(240u32)
        );

        timer.tick(240);

        assert_eq!(
            my_inst.claimer.poll(timer.ms, None, my_strategy).unwrap(),
            ClaimAction::Done(my_preferred)
        );
    }

    #[test]
    fn test_addr_claim_machine_with_claiming_rx_different_sa() {
        let my_preferred = ADDR_1;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let their_preferred = ADDR_2;
        let their_strategy = AddressClaimStrategy::Arbitrary {
            preferred: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToWin,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // If it's a conflict situation, their_inst must win.
        assert!(their_inst.preferred_addr != my_inst.preferred_addr); // not the same addr
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Start
        assert_eq!(
            my_inst.claimer.poll(timer.ms, None, my_strategy).unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(10);
        // Despite the rx with lower name, there is no conflict due to the different preferred_addr.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_strategy)
                .unwrap(),
            ClaimAction::Wait(240u32)
        );

        timer.tick(240);
        // Claiming's done. Targeted address has been obtained.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_strategy)
                .unwrap(),
            ClaimAction::Done(my_preferred)
        );
    }

    #[test]
    fn test_addr_claim_machine_aac_with_conflict_we_win() {
        let my_preferred = ADDR_1;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let their_preferred = ADDR_1;
        let their_strategy = AddressClaimStrategy::Arbitrary {
            preferred: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToLoose,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        assert!(their_inst.can_frame_origin.id.pgn() == 60928); // claiming pgn
        assert!(their_inst.name.0 > my_inst.name.0); // we win
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Round started
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, None, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        assert_eq!(
            my_inst.claimer.state,
            State::Listening {
                frame: my_inst.can_frame_origin,
                deadline_ms: timer.ms + 250
            }
        );

        timer.tick(10);
        assert!(timer.ms == STARTING_TIME + 10);

        // Conflict -> same preferred_addr
        // * my_inst must win and resend her claiming_frame without reset her deadline.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        // Claiming frame is resended after a win conflict
        // * remaining 240 ms deadline.
        // * my_inst.claimer.state must be Sate::Listening.
        assert_eq!(
            my_inst.claimer.state,
            State::Listening {
                frame: my_inst.can_frame_origin,
                deadline_ms: 240 + timer.ms
            }
        );

        timer.tick(99);

        assert_eq!(
            my_inst.claimer.state,
            State::Listening {
                frame: my_inst.can_frame_origin,
                deadline_ms: 141 + timer.ms
            }
        );

        timer.tick(240);

        // Claiming's done. Targeted address has been obtained.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Done(my_inst.preferred_addr)
        );
    }

    #[test]
    fn test_addr_claim_machine_aac_with_conflict_we_lose_and_no_addr_available() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut addr_targeted = ADDR_1;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: addr_targeted,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let their_strategy = AddressClaimStrategy::Arbitrary {
            preferred: addr_targeted,
        };
        let mut their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToWin,
        );

        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // we loose
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Start
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, None, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(1);

        let expected_next_addr = my_inst.preferred_addr + 1;
        let expected_canframe =
            build_address_claim_frame(my_inst.name.0, expected_next_addr).unwrap();
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(expected_canframe)
        );

        // debug
        let mut count = 1;

        while addr_targeted < 252 {
            their_inst.strategy = AddressClaimStrategy::Arbitrary {
                preferred: addr_targeted,
            };
            their_inst.can_frame_next = Some(
                build_address_claim_frame(
                    their_inst.name.0,
                    (my_inst.preferred_addr + count) % 252,
                )
                .unwrap(),
            );

            let expected_next_addr = my_inst.preferred_addr + count + 1;
            // if !is_claimable(expected_next_addr) {
            //     break;
            // };
            let expected_canframe =
                build_address_claim_frame(my_inst.name.0, expected_next_addr % 252).unwrap();

            if addr_targeted == 251 {
                break;
            }

            assert_eq!(
                my_inst
                    .claimer
                    .poll(
                        timer.ms,
                        their_inst.can_frame_next.as_ref(),
                        my_inst.strategy
                    )
                    .unwrap(),
                ClaimAction::Send(expected_canframe)
            );
            addr_targeted += 1;
            count += 1;
            log::debug!("addr: {}", addr_targeted);
        }

        assert!(matches!(
            my_inst.claimer.poll(
                timer.ms,
                their_inst.can_frame_next.as_ref(),
                my_inst.strategy
            ),
            Err(ClaimFault::NoAddressAvailable)
        ));

        // assert!(timer.ms == STARTING_TIME + 260);
    }

    #[test]
    fn test_addr_claim_machine_aac_with_conflict_we_lose() {
        let my_preferred = ADDR_3;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let their_preferred = ADDR_3;
        let their_strategy = AddressClaimStrategy::Arbitrary {
            preferred: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToWin,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // * my_inst claim preferred_addr ADDR_3. Claiming frame is sended on t+0.
        // * at t+10ms, there is a conflict: same address is claimed by a lower name.
        // * conflict is lose by my_inst, so it must iterate on the address range to claim another if possible.
        // * address is available, sending a new claim frame. at t+10, deadline is reset, 250ms again.
        // * Once 250ms have elapsed and no claiming frame in conflict received, my_inst wins the next addr, here ADDR_3 + 1.

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // we loose
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Start
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, None, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(10);
        my_inst.can_frame_next = Some(
            build_address_claim_frame(my_inst.name.0, my_inst.preferred_addr + 1)
                .expect("must be valid"),
        );

        let captured_claim = my_inst
            .claimer
            .poll(timer.ms, their_rx, my_inst.strategy)
            .expect("must be valid");

        assert_ne!(captured_claim, ClaimAction::Send(my_inst.can_frame_origin));
        assert_eq!(
            captured_claim,
            ClaimAction::Send(my_inst.can_frame_next.expect("must be valid"))
        );

        assert_eq!(
            my_inst.claimer.state,
            State::Listening {
                frame: my_inst.can_frame_next.expect("must be valid"),
                deadline_ms: timer.ms + 250
            }
        );

        timer.tick(250);
        // Claiming's done. Next targeted address has been obtained.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Done(my_inst.preferred_addr + 1)
        );

        assert!(timer.ms == STARTING_TIME + 260);
    }

    #[test]
    fn test_addr_claim_machine_non_aac_fixed_addr_we_lose() {
        let my_preferred = ADDR_1;
        let my_strategy = AddressClaimStrategy::Fixed {
            preferred: my_preferred,
        };

        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        let their_preferred = ADDR_1;
        let their_strategy = AddressClaimStrategy::Fixed {
            preferred: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToWin,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // we loose
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Start
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, None, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(45);
        // Inject conflict claiming frame.
        // * we loose
        // * there is not other addr available
        assert!(matches!(
            my_inst.claimer.poll(timer.ms, their_rx, my_inst.strategy),
            Err(ClaimFault::NoAddressAvailable)
        ));
    }

    #[test]
    fn test_addr_claim_machine_non_aac_self_config_strategy_with_conflict_we_lose() {
        let my_preferred = &[ADDR_3, ADDR_1, ADDR_2, ADDR_4];
        let my_strategy = AddressClaimStrategy::SelfConfigurable {
            addresses: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );
        my_inst.can_frame_next =
            Some(build_address_claim_frame(my_inst.name.0, ADDR_1).expect("must be valid"));

        let their_preferred = &[ADDR_3, ADDR_2, ADDR_1, ADDR_4];
        let their_strategy = AddressClaimStrategy::SelfConfigurable {
            addresses: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::BuiltToWin,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // we loose
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        //Start
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        timer.tick(45);
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_next.unwrap())
        );

        assert_eq!(
            my_inst.claimer.state,
            State::Listening {
                frame: my_inst.can_frame_next.unwrap(),
                deadline_ms: timer.ms + 250
            }
        );

        timer.tick(250);
        // Claiming's done. Targeted address has been obtained.
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Done(ADDR_1)
        );
        assert!(timer.ms == STARTING_TIME + 295);
    }

    #[test]
    fn test_addr_claim_machine_aac_disturbed_with_no_effect() {
        // The aim of this test is to confirm that a non claiming frame is correctly ignored.

        // build the tested instance
        // * AAC: yes
        // * preferred: ADDR_3
        // * conflict design: normal
        // * claiming frame: yes
        let my_preferred = ADDR_3;
        let my_strategy = AddressClaimStrategy::Arbitrary {
            preferred: my_preferred,
        };
        let mut my_inst = Instance::new(
            my_strategy,
            CanFrameClass::Claiming,
            ConflictPriority::Normal,
        );

        // build a non distrubing frame
        // * AAC: yes
        // * preferred: ADDR_3
        // * conflict design: built to win
        // * claiming frame: no
        let their_preferred = ADDR_3;
        let their_strategy = AddressClaimStrategy::Arbitrary {
            preferred: their_preferred,
        };
        let their_inst = Instance::new(
            their_strategy,
            CanFrameClass::Normal,
            ConflictPriority::BuiltToWin,
        );
        let their_rx = Some(&their_inst.can_frame_origin);

        let mut timer = ClockTest::new(STARTING_TIME);

        // Pre-conditions
        assert!(their_inst.name.0 < my_inst.name.0); // we loose
        assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict but != claiming frame
        assert_eq!(my_inst.claimer.state, State::Idle); // correct beggining state
        assert!(timer.ms == STARTING_TIME);

        // Starting test.
        // * claiming address: ADDR_3
        // * rx: None
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, None, my_inst.strategy)
                .unwrap(),
            ClaimAction::Send(my_inst.can_frame_origin)
        );

        // Send a non-claiming frame on the same addr.
        // * rx: their_rx
        timer.tick(10);
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Wait(240)
        );

        // Timer advance of 239 to reach deadline == 1;
        timer.tick(239);
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Wait(1)
        );

        // Claiming's done. Targeted address has been obtained.
        timer.tick(1);
        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_rx, my_inst.strategy)
                .unwrap(),
            ClaimAction::Done(my_inst.preferred_addr)
        );
    }
}

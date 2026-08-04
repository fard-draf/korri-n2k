use crate::protocol::{
    constants::address::CLAIMABLE_COUNT,
    managment::address_claiming::engine::ClaimAction::CannotClaim, transport::can_id::CanIdBuilder,
};

use super::*;

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
struct Name(IsoName);

impl Default for Name {
    fn default() -> Self {
        Self(IsoName::from_raw(0x1234567890ABCDEF))
    }
}

impl Name {
    fn from_raw(raw: u64) -> Self {
        Self(IsoName::from_raw(raw))
    }

    fn raw(&self) -> u64 {
        self.0.raw()
    }
}

impl Name {
    fn new(strategy: AddressClaimStrategy) -> Self {
        let name = Name::default();
        match strategy {
            AddressClaimStrategy::Fixed { preferred: _ } => name,
            AddressClaimStrategy::SelfConfigurable { addresses: _ } => name,
            AddressClaimStrategy::Arbitrary { preferred: _ } => {
                let r_name = name.0.raw() | 1u64 << 63;
                assert_eq!(r_name, 0x9234567890ABCDEF);
                Name::from_raw(r_name)
            }
        }
    }

    fn conflict_priority_builder(mut self, name_priority: ConflictPriority) -> Self {
        match name_priority {
            ConflictPriority::BuiltToWin => self.0 = IsoName::from_raw(self.0.raw() & !0xFFFF),
            ConflictPriority::Normal => {}
            ConflictPriority::BuiltToLoose => self.0 = IsoName::from_raw(self.0.raw() | 0xFFFF),
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
    // * my_inst.claimer.state must be State::Listening.
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
    let my_strategy = AddressClaimStrategy::Arbitrary { preferred: ADDR_3 };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Arbitrary { preferred: ADDR_3 };
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
    let expected_canframe = build_address_claim_frame(my_inst.name.0, expected_next_addr).unwrap();
    assert_eq!(
        my_inst
            .claimer
            .poll(timer.ms, their_rx, my_inst.strategy)
            .unwrap(),
        ClaimAction::Send(expected_canframe)
    );

    // count of tested addr
    let mut tested_addr: u16 = 1;

    while tested_addr < CLAIMABLE_COUNT as u16 {
        their_inst.can_frame_next = Some(
            build_address_claim_frame(
                their_inst.name.0,
                ((my_inst.preferred_addr as u16 + tested_addr) % CLAIMABLE_COUNT as u16) as u8,
            )
            .unwrap(),
        );

        // this is the last addr available, next will return None.
        if tested_addr == 251 {
            let expected_canframe =
                build_address_claim_frame(my_inst.name.0, address::NULL).unwrap();
            assert_eq!(
                my_inst
                    .claimer
                    .poll(
                        timer.ms,
                        their_inst.can_frame_next.as_ref(),
                        my_inst.strategy
                    )
                    .unwrap(),
                ClaimAction::CannotClaim(expected_canframe)
            );

            break;
        }

        let expected_next_addr: u16 =
            (my_inst.preferred_addr as u16 + tested_addr + 1) % CLAIMABLE_COUNT as u16;

        assert!(expected_next_addr < CLAIMABLE_COUNT as u16);

        let expected_canframe =
            build_address_claim_frame(my_inst.name.0, expected_next_addr as u8).unwrap();

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

        tested_addr += 1;
    }
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
    let expected_frame = build_address_claim_frame(my_inst.name.0, address::NULL).unwrap();
    assert_eq!(
        my_inst
            .claimer
            .poll(timer.ms, their_rx, my_inst.strategy)
            .unwrap(),
        ClaimAction::CannotClaim(expected_frame)
    );
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

    // build a non disturbing frame
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
    assert_eq!(my_inst.claimer.state, State::Idle); // correct starting state
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

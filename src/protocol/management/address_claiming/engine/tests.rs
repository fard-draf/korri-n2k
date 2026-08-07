use crate::protocol::{
    constants::{addr_mgmt_pgns::COMMAND_ADDR_65240, address::CLAIMABLE_COUNT},
    transport::can_id::CanIdBuilder,
};

use super::*;

const ADDR_1: u8 = 1;
const ADDR_2: u8 = 2;
const ADDR_3: u8 = 144;
const ADDR_4: u8 = 244;
const STARTING_TIME: u64 = 123456789;

//==================================================================================OUTPUT_HELPERS
// Local, deliberately not a public builder: a test spells out all three axes so
// the derivation the engine performs in `output()` is asserted, not assumed.

/// A claim campaign step: a frame goes out and the arbitration window is open.
fn claiming(tx: CanFrame, address: u8, deadline_ms: u64) -> ClaimOutput {
    ClaimOutput {
        tx: Some(tx),
        status: ClaimStatus::Claiming(address),
        wake_at_ms: Some(deadline_ms),
    }
}

/// Waiting out the arbitration window with nothing to emit.
fn waiting_claim(address: u8, deadline_ms: u64) -> ClaimOutput {
    ClaimOutput {
        tx: None,
        status: ClaimStatus::Claiming(address),
        wake_at_ms: Some(deadline_ms),
    }
}

/// The address is held and the bus is quiet.
fn claimed(address: u8) -> ClaimOutput {
    ClaimOutput {
        tx: None,
        status: ClaimStatus::Claimed(address),
        wake_at_ms: None,
    }
}

/// The address is held and a frame is owed to the bus in the same step.
fn defending(tx: CanFrame, address: u8) -> ClaimOutput {
    ClaimOutput {
        tx: Some(tx),
        status: ClaimStatus::Claimed(address),
        wake_at_ms: None,
    }
}

/// No address left; `retry_at_ms` carries the next campaign.
fn cannot_claim(tx: Option<CanFrame>, retry_at_ms: u64) -> ClaimOutput {
    ClaimOutput {
        tx,
        status: ClaimStatus::CannotClaim,
        wake_at_ms: Some(retry_at_ms),
    }
}

//==================================================================================FIXTURES

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
            ConflictPriority::BuiltToLose => self.0 = IsoName::from_raw(self.0.raw() | 0xFFFF),
        }
        self
    }
}

/// Builds an ISO Request frame, `len` and `requested_pgn` left open to forge invalid ones.
fn build_request_frame(destination: u8, requested_pgn: u32, len: usize) -> CanFrame {
    let mut data = [0xFFu8; 8];
    data[0..REQUEST_PGN_LEN].copy_from_slice(&requested_pgn.to_le_bytes()[0..REQUEST_PGN_LEN]);

    CanFrame {
        id: CanIdBuilder::new(REQUEST_PGN_59904, ADDR_4)
            .to_destination(destination)
            .with_priority(6)
            .build()
            .expect("must be valid"),
        data,
        len,
    }
}

struct Instance<'a> {
    name: Name,
    preferred_addr: u8,
    can_frame_origin: CanFrame,
    can_frame_next: Option<CanFrame>,
    claimer: AddressClaimEngine<'a>,
}

enum CanFrameClass {
    Claiming,
    Normal,
}

enum ConflictPriority {
    BuiltToWin,
    Normal,
    BuiltToLose,
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
            CanFrameClass::Claiming => build_address_claim_frame(name.0, preferred_addr),
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

        let claimer = AddressClaimEngine::new(name.0, strategy)
            .expect("Instance::new() // Claimer must be valid");

        Self {
            name,
            preferred_addr,
            can_frame_origin: can_frame,
            can_frame_next: None,
            claimer,
        }
    }
}

//==================================================================================CAMPAIGN

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

    assert_eq!(my_inst.claimer.state, State::Unclaimed);

    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    timer.tick(10);

    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        waiting_claim(my_preferred, deadline_ms)
    );

    timer.tick(240);

    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));
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
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    // Start
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    timer.tick(10);
    // Despite the rx with lower name, there is no conflict due to the different preferred_addr.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        waiting_claim(my_preferred, deadline_ms)
    );

    timer.tick(240);
    // Claiming's done. Targeted address has been obtained.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claimed(my_preferred)
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
        ConflictPriority::BuiltToLose,
    );
    let their_rx = Some(&their_inst.can_frame_origin);

    let mut timer = ClockTest::new(STARTING_TIME);

    assert!(their_inst.can_frame_origin.id.pgn() == 60928); // claiming pgn
    assert!(their_inst.name.0 > my_inst.name.0); // we win
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    // Round started
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_origin,
            deadline_ms
        }
    );

    timer.tick(10);
    assert!(timer.ms == STARTING_TIME + 10);

    // Conflict -> same preferred_addr
    // * my_inst must win and resend her claiming_frame without resetting her deadline.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // Claiming frame is resent after a won conflict
    // * remaining 240 ms deadline.
    // * my_inst.claimer.state must be State::Claiming.
    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_origin,
            deadline_ms: 240 + timer.ms
        }
    );

    timer.tick(99);

    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_origin,
            deadline_ms: 141 + timer.ms
        }
    );

    timer.tick(240);

    // Claiming's done. Targeted address has been obtained.
    // `None` and not `their_rx`: the driver clears `rx` once consumed, so a
    // frame is never handed to `poll` twice.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claimed(my_inst.preferred_addr)
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
    assert!(their_inst.name.0 < my_inst.name.0); // we lose
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    // Start
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_inst.preferred_addr,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );

    timer.tick(1);
    let deadline_ms = timer.ms + CLAIM_DELAY_MS as u64;

    let expected_next_addr = my_inst.preferred_addr + 1;
    let expected_canframe = build_address_claim_frame(my_inst.name.0, expected_next_addr);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claiming(expected_canframe, expected_next_addr, deadline_ms)
    );

    // count of tested addr
    let mut tested_addr: u16 = 1;

    while tested_addr < CLAIMABLE_COUNT as u16 {
        their_inst.can_frame_next = Some(build_address_claim_frame(
            their_inst.name.0,
            ((my_inst.preferred_addr as u16 + tested_addr) % CLAIMABLE_COUNT as u16) as u8,
        ));

        // this is the last addr available, next will return None.
        if tested_addr == 251 {
            let expected_canframe =
                build_address_claim_frame(my_inst.name.0, address::NULL_ADDR_254);
            assert_eq!(
                my_inst
                    .claimer
                    .poll(timer.ms, their_inst.can_frame_next.as_ref(),),
                cannot_claim(
                    Some(expected_canframe),
                    timer.ms + CANNOT_CLAIM_RETRY_DELAY_MS as u64
                )
            );

            break;
        }

        let expected_next_addr: u16 =
            (my_inst.preferred_addr as u16 + tested_addr + 1) % CLAIMABLE_COUNT as u16;

        assert!(expected_next_addr < CLAIMABLE_COUNT as u16);

        let expected_canframe = build_address_claim_frame(my_inst.name.0, expected_next_addr as u8);

        assert_eq!(
            my_inst
                .claimer
                .poll(timer.ms, their_inst.can_frame_next.as_ref(),),
            claiming(expected_canframe, expected_next_addr as u8, deadline_ms)
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
    assert!(their_inst.name.0 < my_inst.name.0); // we lose
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    // Start
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );

    timer.tick(10);
    my_inst.can_frame_next = Some(build_address_claim_frame(
        my_inst.name.0,
        my_inst.preferred_addr + 1,
    ));
    let deadline_ms = timer.ms + CLAIM_DELAY_MS as u64;

    let captured_claim = my_inst.claimer.poll(timer.ms, their_rx);

    assert_ne!(
        captured_claim,
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );
    assert_eq!(
        captured_claim,
        claiming(
            my_inst.can_frame_next.expect("must be valid"),
            my_inst.preferred_addr + 1,
            deadline_ms
        )
    );

    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_next.expect("must be valid"),
            deadline_ms
        }
    );

    timer.tick(250);
    // Claiming's done. Next targeted address has been obtained.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claimed(my_inst.preferred_addr + 1)
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
    assert!(their_inst.name.0 < my_inst.name.0); // we lose
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    // Start
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );

    timer.tick(45);
    // Inject conflict claiming frame.
    // * we lose
    // * there is no other addr available
    let expected_frame = build_address_claim_frame(my_inst.name.0, address::NULL_ADDR_254);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        cannot_claim(
            Some(expected_frame),
            timer.ms + CANNOT_CLAIM_RETRY_DELAY_MS as u64
        )
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
    my_inst.can_frame_next = Some(build_address_claim_frame(my_inst.name.0, ADDR_1));

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
    assert!(their_inst.name.0 < my_inst.name.0); // we lose
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state
    assert!(timer.ms == STARTING_TIME);

    //Start
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claiming(
            my_inst.can_frame_origin,
            ADDR_3,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );

    timer.tick(45);
    let deadline_ms = timer.ms + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claiming(my_inst.can_frame_next.unwrap(), ADDR_1, deadline_ms)
    );

    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_next.unwrap(),
            deadline_ms
        }
    );

    timer.tick(250);
    // Claiming's done. Targeted address has been obtained.
    assert_eq!(my_inst.claimer.poll(timer.ms, their_rx), claimed(ADDR_1));
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
    assert!(their_inst.name.0 < my_inst.name.0); // we lose
    assert!(their_inst.preferred_addr == my_inst.preferred_addr); // conflict but != claiming frame
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct starting state
    assert!(timer.ms == STARTING_TIME);

    // Starting test.
    // * claiming address: ADDR_3
    // * rx: None
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // Send a non-claiming frame on the same addr.
    // * rx: their_rx
    timer.tick(10);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        waiting_claim(my_preferred, deadline_ms)
    );

    // Timer advance of 239 to reach deadline == 1;
    timer.tick(239);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        waiting_claim(my_preferred, deadline_ms)
    );

    // Claiming's done. Targeted address has been obtained.
    timer.tick(1);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claimed(my_inst.preferred_addr)
    );
}

//==================================================================================ISO_REQUEST

/// A request arriving before the campaign starts it. The Address Claim that
/// goes out is the answer, so no separate Cannot Claim is owed.
#[test]
fn test_iso_request_from_unclaimed_starts_the_campaign() {
    let my_strategy = AddressClaimStrategy::Arbitrary { preferred: ADDR_1 };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);

    let timer = ClockTest::new(STARTING_TIME);

    // Pre-conditions
    assert_eq!(my_inst.claimer.state, State::Unclaimed); // correct beginning state

    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        claiming(
            my_inst.can_frame_origin,
            ADDR_1,
            timer.ms + CLAIM_DELAY_MS as u64
        )
    );
}

/// A strategy with nothing claimable still owes the Cannot Claim, and the first
/// poll is where it goes out.
#[test]
fn test_a_strategy_with_no_claimable_address_answers_cannot_claim() {
    // Built without `Instance`: an empty address list has no preferred address.
    let name = Name::default().0;
    let strategy = AddressClaimStrategy::SelfConfigurable { addresses: &[] };
    let mut engine = AddressClaimEngine::new(name, strategy).expect("SAC name, SAC strategy");

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    let timer = ClockTest::new(STARTING_TIME);

    let expected_frame = build_address_claim_frame(name, address::NULL_ADDR_254);
    assert_eq!(
        engine.poll(timer.ms, Some(&request_rx)),
        cannot_claim(
            Some(expected_frame),
            timer.ms + CANNOT_CLAIM_RETRY_DELAY_MS as u64
        )
    );
}

/// Regression: a stream of requests arriving from the very first poll must not
/// keep the node addressless.
///
/// The runner reads the bus before the wake deadline, so an engine that answered
/// a request without starting its campaign would be handed the next request
/// instead of ever reaching `start_claim`.
#[test]
fn test_continuous_requests_from_unclaimed_do_not_starve_the_first_claim() {
    let my_strategy = AddressClaimStrategy::Arbitrary { preferred: ADDR_1 };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    let mut timer = ClockTest::new(STARTING_TIME);

    // Every poll is handed a request, starting with the first.
    for _ in 0..10 {
        my_inst.claimer.poll(timer.ms, Some(&request_rx));
        assert!(
            matches!(my_inst.claimer.state, State::Claiming { .. }),
            "a request stream must not keep the engine out of a campaign"
        );
        timer.tick(10);
    }

    // And the campaign still closes on time.
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        defending(my_inst.can_frame_origin, ADDR_1)
    );
}

#[test]
fn test_iso_request_while_claiming_resends_current_claim() {
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

    // Start
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // Pre-conditions
    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_origin,
            deadline_ms
        }
    );

    timer.tick(10);
    let request_rx = build_request_frame(my_preferred, CLAIM_PGN_60928, REQUEST_PGN_LEN);

    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // The deadline is not rearmed by an answer.
    assert_eq!(
        my_inst.claimer.state,
        State::Claiming {
            frame: my_inst.can_frame_origin,
            deadline_ms
        }
    );

    timer.tick(240);
    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));
}

#[test]
fn test_iso_request_while_claimed_resends_current_claim() {
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

    // Reach the Claimed state.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));

    // Pre-conditions
    assert_eq!(
        my_inst.claimer.state,
        State::Claimed {
            frame: my_inst.can_frame_origin
        }
    );

    // A globally broadcast request is answered with the current claim.
    let global_request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&global_request_rx)),
        defending(my_inst.can_frame_origin, my_preferred)
    );

    // A request addressed to us is answered the same way.
    let addressed_request_rx = build_request_frame(my_preferred, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&addressed_request_rx)),
        defending(my_inst.can_frame_origin, my_preferred)
    );

    // No deadline is created by an answer.
    assert_eq!(
        my_inst.claimer.state,
        State::Claimed {
            frame: my_inst.can_frame_origin
        }
    );
}

#[test]
fn test_iso_request_from_cannot_claim_answers_cannot_claim() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Fixed {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Fixed {
        preferred: my_preferred,
    };
    let their_inst = Instance::new(
        their_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToWin,
    );
    let their_rx = Some(&their_inst.can_frame_origin);

    let mut timer = ClockTest::new(STARTING_TIME);

    // Reach the CannotClaim state: Fixed strategy, lost conflict, no fallback address.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );
    let expected_frame = build_address_claim_frame(my_inst.name.0, address::NULL_ADDR_254);
    let retry_at_ms = STARTING_TIME + CANNOT_CLAIM_RETRY_DELAY_MS as u64;
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        cannot_claim(Some(expected_frame), retry_at_ms)
    );

    // Pre-conditions
    assert_eq!(my_inst.claimer.state, State::CannotClaim { retry_at_ms });

    timer.tick(10);
    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        cannot_claim(Some(expected_frame), retry_at_ms)
    );

    // The retry deadline survives the answer.
    assert_eq!(my_inst.claimer.state, State::CannotClaim { retry_at_ms });

    // Once elapsed, the retry still happens.
    timer.tick(CANNOT_CLAIM_RETRY_DELAY_MS as u64);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            timer.ms + CLAIM_DELAY_MS as u64
        )
    );
}

#[test]
fn test_iso_request_is_ignored_when_not_for_us() {
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

    // Reach the Claimed state.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            STARTING_TIME + CLAIM_DELAY_MS as u64
        )
    );
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));

    // Pre-conditions: ADDR_2 is neither ours nor the broadcast address.
    const _: () = assert!(ADDR_2 != ADDR_1 && ADDR_2 != GLOBAL);

    // Addressed to another node.
    let other_node_rx = build_request_frame(ADDR_2, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&other_node_rx)),
        claimed(my_preferred)
    );

    // Requesting another PGN than the address claim.
    let other_pgn_rx = build_request_frame(GLOBAL, COMMAND_ADDR_65240, REQUEST_PGN_LEN);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&other_pgn_rx)),
        claimed(my_preferred)
    );

    // Truncated payload: the requested PGN cannot be read.
    let truncated_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN - 1);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&truncated_rx)),
        claimed(my_preferred)
    );

    assert_eq!(
        my_inst.claimer.state,
        State::Claimed {
            frame: my_inst.can_frame_origin
        }
    );
}

#[test]
fn test_iso_request_payload_padding_is_not_part_of_the_pgn() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);

    let mut timer = ClockTest::new(STARTING_TIME);

    // Pre-conditions: three PGN bytes, then J1939 padding.
    assert_eq!(request_rx.data[0..3], [0x00, 0xEE, 0x00]);
    assert_eq!(request_rx.data[3..8], [0xFF; 5]);

    // Reach the Claimed state, where a request is answered with the claim.
    my_inst.claimer.poll(timer.ms, None);
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));

    // Reading a fourth byte would drown the PGN in the padding, and the request
    // would go unanswered.
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        defending(my_inst.can_frame_origin, my_preferred)
    );
}

//==================================================================================LIVENESS
// The deadline must survive traffic. Every case below hammers the engine with
// frames it has to answer and checks it still reaches the next state on time.

/// A request answered on every single poll must not postpone the acquisition.
#[test]
fn test_a_request_on_every_poll_does_not_starve_the_claim() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    let mut timer = ClockTest::new(STARTING_TIME);
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;

    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // 24 answered requests inside the 250 ms window: every one re-emits the
    // claim, none of them touches the deadline.
    for _ in 0..24 {
        timer.tick(10);
        assert_eq!(
            my_inst.claimer.poll(timer.ms, Some(&request_rx)),
            claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
        );
    }

    // The 250 ms are up. The request is still there, and the address is still won.
    timer.tick(10);
    assert!(timer.ms >= deadline_ms);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        defending(my_inst.can_frame_origin, my_preferred)
    );
}

/// A rival we beat, claiming on every poll, must not starve the acquisition either.
#[test]
fn test_continuous_we_win_conflicts_do_not_starve_the_claim() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let their_inst = Instance::new(
        their_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToLose,
    );
    let their_rx = Some(&their_inst.can_frame_origin);

    assert!(their_inst.name.0 > my_inst.name.0); // we win every round

    let mut timer = ClockTest::new(STARTING_TIME);
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;

    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    for _ in 0..24 {
        timer.tick(10);
        assert_eq!(
            my_inst.claimer.poll(timer.ms, their_rx),
            claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
        );
    }

    timer.tick(10);
    assert!(timer.ms >= deadline_ms);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        defending(my_inst.can_frame_origin, my_preferred)
    );
}

/// A request answered on every poll must not postpone the Cannot Claim retry.
#[test]
fn test_a_request_on_every_poll_does_not_starve_the_cannot_claim_retry() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Fixed {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Fixed {
        preferred: my_preferred,
    };
    let their_inst = Instance::new(
        their_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToWin,
    );

    let mut timer = ClockTest::new(STARTING_TIME);
    let retry_at_ms = STARTING_TIME + CANNOT_CLAIM_RETRY_DELAY_MS as u64;
    let cannot_claim_frame = build_address_claim_frame(my_inst.name.0, address::NULL_ADDR_254);

    // Reach CannotClaim.
    my_inst.claimer.poll(timer.ms, None);
    assert_eq!(
        my_inst
            .claimer
            .poll(timer.ms, Some(&their_inst.can_frame_origin)),
        cannot_claim(Some(cannot_claim_frame), retry_at_ms)
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);

    // 99 answered requests spread over the retry window.
    for _ in 0..99 {
        timer.tick(100);
        assert!(timer.ms < retry_at_ms);
        assert_eq!(
            my_inst.claimer.poll(timer.ms, Some(&request_rx)),
            cannot_claim(Some(cannot_claim_frame), retry_at_ms)
        );
    }

    // The retry outranks the request the moment it is due.
    timer.tick(100);
    assert!(timer.ms >= retry_at_ms);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, Some(&request_rx)),
        claiming(
            my_inst.can_frame_origin,
            my_preferred,
            timer.ms + CLAIM_DELAY_MS as u64
        )
    );
}

/// A loss landing exactly on the deadline outranks it: winning the wait does not
/// make the address ours if a better NAME took it in the same millisecond.
#[test]
fn test_a_lost_conflict_on_the_exact_deadline_outranks_it() {
    let my_preferred = ADDR_3;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let their_inst = Instance::new(
        their_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToWin,
    );
    let their_rx = Some(&their_inst.can_frame_origin);

    assert!(their_inst.name.0 < my_inst.name.0); // we lose

    let mut timer = ClockTest::new(STARTING_TIME);
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;

    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    // Land on the deadline to the millisecond.
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(timer.ms, deadline_ms);

    // The next address is claimed, not the contested one.
    let expected_frame = build_address_claim_frame(my_inst.name.0, my_preferred + 1);
    assert_eq!(
        my_inst.claimer.poll(timer.ms, their_rx),
        claiming(
            expected_frame,
            my_preferred + 1,
            timer.ms + CLAIM_DELAY_MS as u64
        )
    );

    // And the contested address was never handed out.
    assert_eq!(my_inst.claimer.claimed_address(), None);
}

/// The axes are independent: a request landing on the deadline yields a frame to
/// emit *and* the acquired address in the same output.
#[test]
fn test_a_request_on_the_deadline_yields_both_a_frame_and_the_address() {
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
    let deadline_ms = STARTING_TIME + CLAIM_DELAY_MS as u64;

    assert_eq!(
        my_inst.claimer.poll(timer.ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, deadline_ms)
    );

    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(timer.ms, deadline_ms);

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    let output = my_inst.claimer.poll(timer.ms, Some(&request_rx));

    // Under `ClaimAction` this frame was lost: one variant could not say both.
    assert_eq!(output.tx, Some(my_inst.can_frame_origin));
    assert_eq!(output.status, ClaimStatus::Claimed(my_preferred));
    assert_eq!(output.wake_at_ms, None);
}

/// A held address with a quiet bus has no deadline at all.
#[test]
fn test_claimed_without_activity_has_no_deadline() {
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

    my_inst.claimer.poll(timer.ms, None);
    timer.tick(CLAIM_DELAY_MS as u64);
    assert_eq!(my_inst.claimer.poll(timer.ms, None), claimed(my_preferred));

    // Hours later, still nothing to wake up for: only a frame can change this.
    timer.tick(6 * 3600 * 1000);
    let output = my_inst.claimer.poll(timer.ms, None);
    assert_eq!(output.tx, None);
    assert_eq!(output.wake_at_ms, None);
    assert_eq!(output.status, ClaimStatus::Claimed(my_preferred));
}

/// Deadlines are built with `saturating_add`: a clock reading at the very end of
/// the `u64` range pins them instead of overflowing.
#[test]
fn test_deadlines_near_u64_max_do_not_panic() {
    let my_preferred = ADDR_1;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let now_ms = u64::MAX - 10;

    assert_eq!(
        my_inst.claimer.poll(now_ms, None),
        claiming(my_inst.can_frame_origin, my_preferred, u64::MAX)
    );

    // Pinned at the maximum, and still reachable.
    assert_eq!(my_inst.claimer.poll(u64::MAX, None), claimed(my_preferred));

    // The Cannot Claim retry saturates the same way.
    let fixed_strategy = AddressClaimStrategy::Fixed {
        preferred: my_preferred,
    };
    let mut fixed_inst = Instance::new(
        fixed_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );
    let rival = Instance::new(
        fixed_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToWin,
    );

    fixed_inst.claimer.poll(now_ms, None);
    let expected_frame = build_address_claim_frame(fixed_inst.name.0, address::NULL_ADDR_254);
    assert_eq!(
        fixed_inst
            .claimer
            .poll(now_ms, Some(&rival.can_frame_origin)),
        cannot_claim(Some(expected_frame), u64::MAX)
    );
}

/// `status` and `claimed_address()` are two views of the same state and must
/// never disagree, whatever the step.
#[test]
fn test_status_and_claimed_address_never_disagree() {
    let my_preferred = ADDR_3;
    let my_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let mut my_inst = Instance::new(
        my_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::Normal,
    );

    let their_strategy = AddressClaimStrategy::Arbitrary {
        preferred: my_preferred,
    };
    let their_inst = Instance::new(
        their_strategy,
        CanFrameClass::Claiming,
        ConflictPriority::BuiltToWin,
    );

    let request_rx = build_request_frame(GLOBAL, CLAIM_PGN_60928, REQUEST_PGN_LEN);
    let mut timer = ClockTest::new(STARTING_TIME);

    // A script covering every state: start, wait, acquire, answer, lose, requeue.
    let script: [Option<&CanFrame>; 6] = [
        None,
        None,
        None,
        Some(&request_rx),
        Some(&their_inst.can_frame_origin),
        None,
    ];

    for rx in script {
        let output = my_inst.claimer.poll(timer.ms, rx);
        assert_eq!(
            output.status.claimed_address(),
            my_inst.claimer.claimed_address(),
            "status {:?} disagrees with claimed_address()",
            output.status
        );
        timer.tick(CLAIM_DELAY_MS as u64);
    }
}

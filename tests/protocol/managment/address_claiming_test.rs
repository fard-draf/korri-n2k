//! Tests for `claim_address`: nominal case, winning conflict, losing conflict.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{simulate_no_conflict, MockCanBus, MockTimer};
use korri_n2k::{
    error::{ClaimError, ClaimFault},
    protocol::{
        constants::address,
        managment::address_claiming::{claim_address, AddressClaimStrategy},
        transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus},
    },
};

use tokio::time::Duration;

/// Build a simulated competing Address Claim frame.
fn build_conflict_frame(name: u64, address: u8) -> CanFrame {
    let id = CanId::builder(60928, address)
        .to_destination(255)
        .with_priority(6)
        .build()
        .unwrap();
    CanFrame {
        id,
        data: name.to_le_bytes(),
        len: 8,
    }
}

#[tokio::test]
async fn test_claim_address_no_conflict() {
    // No other node responds; we retain the preferred address.
    let (mut dut_bus, host_bus) = MockCanBus::create_pair();

    // Spawn a task that simulates a quiet network (no conflict)
    tokio::spawn(simulate_no_conflict(host_bus));

    let mut timer = MockTimer::new();
    let my_name = 0x8234567890ABCDEF;
    let preferred = 42;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };

    // Invoke claim_address
    let result = claim_address(&mut dut_bus, &mut timer, my_name, strategy).await;

    // Assertions
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), preferred)
}

#[tokio::test]
async fn test_claim_address_with_conflict_win() {
    // Local NAME is smaller: we defend and keep the address.
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();

    let my_name = 0x8234567890ABCDEE; // AAC enable
    let their_name = 0x8234567890ABCDEF; // Larger than my_name → we win // AAC enable
    assert!(my_name < their_name);
    let preferred = 42;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };
    let mut timer = MockTimer::new();

    tokio::select! {
    claim_result = claim_address(&mut dut_bus, &mut timer, my_name, strategy) => {
        assert!(claim_result.is_ok());
        let claimed_address = claim_result.unwrap();
        assert_eq!(claimed_address, preferred, "Should keep preferred (win)");
        assert!(claimed_address > 0 && claimed_address <= 247, "Claimed address is outside the valid range");
        dbg!("claimed_address: {:?}", claimed_address);

    }


    _ = async {
        let frame1 = host_bus
            .recv()
            .await
            .expect("DUT did not send the initial claim");
        assert_eq!(frame1.id.source_address(), preferred);

        let conflict_frame = build_conflict_frame(their_name, preferred);
        host_bus
            .send(&conflict_frame)
            .await
            .expect("Failed to send conflict frame");

        // let frame2 = host_bus.recv().await.expect("DUT attempted an alternative action");
        // assert_ne!(frame2.id.source_address(), preferred_address);
        std::future::pending::<()>().await;
    } => {
        panic!("Simulator finished before `claim_address`; the test setup is likely incorrect");
    }
        }
}

#[tokio::test]
async fn test_claim_address_with_conflict_lose() {
    // Remote NAME has priority: walk up to the next free address.
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();

    let my_name: u64 = 0x9234567890ABCDEF; // MSB is 1 -> Arbitrary Capable
    let their_name: u64 = 0x1234567890ABCDEE; // Lower than my_name → we lose
    assert!(my_name > their_name);
    let preferred = 207;
    let preferred2 = 208;
    let preferred3 = 209;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };
    let mut timer = MockTimer::new();

    tokio::select! {
        claim_result = claim_address(&mut dut_bus, &mut timer, my_name, strategy) => {
            // assert!(claim_result.is_ok());
            let claimed_address = claim_result.unwrap();
            dbg!("claimed_address: {:?}", claimed_address);
            assert_ne!(claimed_address, preferred, "Preferred address should have been lost");
            assert_eq!(
                claimed_address, 210,
                "Should walk up past every contested address"
            );
            assert!(
                address::is_claimable(claimed_address),
                "Claimed address is outside the valid range"
            );

        }


        _ = async {
            let frame1 = host_bus
                .recv()
                .await
                .expect("DUT did not send the initial claim");
            assert_eq!(frame1.id.source_address(), preferred);

            let conflict_frame = build_conflict_frame(their_name, preferred);
            host_bus
                .send(&conflict_frame)
                .await
                .expect("Failed to send conflict frame");

            let conflict_frame2 = build_conflict_frame(their_name, preferred2);
            host_bus
                .send(&conflict_frame2)
                .await
                .expect("Failed to send conflict frame #2");

            let conflict_frame3 = build_conflict_frame(their_name, preferred3);
            host_bus
                .send(&conflict_frame3)
                .await
                .expect("Failed to send conflict frame #3");


            // let frame2 = host_bus.recv().await.expect("DUT attempted an alternative action");
            // assert_ne!(frame2.id.source_address(), preferred_address);
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator finished before `claim_address`; the test setup is likely incorrect")
        }

    }
}

#[tokio::test]
async fn test_claim_address_with_conflict_lose_and_with_no_address_available() {
    // Every address is taken: the algorithm must return `NoAddressAvailable`.
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();

    let my_name: u64 = 0x9234567890ABCDEF; // MSB is 1 -> Arbitrary
    let their_name: u64 = 0x1234567890ABCDEE; // Lower than my_name → we lose
    assert!(my_name > their_name);
    let preferred = 128;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };
    let mut timer = MockTimer::new();

    tokio::select! {
        claim_result = claim_address(&mut dut_bus, &mut timer, my_name, strategy) => {
            assert_eq!(claim_result.unwrap(), address::NULL);
        }

        _ = async {
            let frame1 = host_bus
                .recv()
                .await
                .expect("DUT did not send the initial claim");
            assert_eq!(frame1.id.source_address(), preferred);
            // Contest every address in the order the node walks them —
            // starting at `preferred` and wrapping — so each conflict lands
            // while the node is actually sitting on that address.
            for offset in 0..address::CLAIMABLE_COUNT {
                let address = ((preferred as usize + offset) % address::CLAIMABLE_COUNT) as u8;
                let conflict_frame = build_conflict_frame(their_name, address);
                host_bus
                    .send(&conflict_frame)
                    .await
                    .expect("Failed to send conflict frame");
            }
            // let frame2 = host_bus.recv().await.expect("DUT attempted an alternative action");
            // assert_ne!(frame2.id.source_address(), preferred_address);
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator finished before `claim_address`; the test setup is likely incorrect")
        }
    }
}

#[tokio::test]
async fn test_claim_address_non_arbitrary_loses_and_fails() {
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();

    let my_name: u64 = 0x1234567890ABCDEF; // MSB is 0 → not arbitrary capable
    let their_name: u64 = 0x1234567890ABCDEE; // Lower than my_name → we lose
    assert!(my_name > their_name);
    let preferred = 42;
    let strategy = AddressClaimStrategy::Fixed { preferred };
    let mut timer = MockTimer::new();

    tokio::select! {

        claim_result = claim_address(&mut dut_bus, &mut timer, my_name, strategy) => {
            assert_eq!(claim_result.unwrap(), address::NULL);
        }

        _ = async {
            //1. Wait for the DUT to claim its preferred address
            let frame1 = host_bus
                .recv()
                .await
                .expect("DUT did not send the initial claim");
            assert_eq!(frame1.id.source_address(), preferred);

            //2. Send a conflict from a higher-priority device (lower NAME)
            let conflict_frame = build_conflict_frame(their_name, preferred);
            host_bus
                .send(&conflict_frame)
                .await
                .expect("Sending conflict failed");

            //3. The DUT should not try another address; it uses the NULL address (254)
            // Verify by timing out on recv()
            if tokio::time::timeout(Duration::from_millis(50), host_bus.recv()).await.is_ok() {
                panic!("DUT should not have tried another address because it is not arbitrary-address capable");
            }

            // Keep the simulator alive so `claim_address` can complete.
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator finished before `claim_address`; the test setup is likely incorrect :(")
        }
    }
}

#[tokio::test]
async fn test_claim_address_non_arbitrary_conflict_and_win() {
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();

    let my_name: u64 = 0x1234567890ABCDEF; // MSB is 0 -> non arbitrary capable
    let their_name: u64 = 0x1934567890ABCDEE; // Greater than my_name → we win
    assert!(my_name < their_name);
    let preferred = 42;
    let strategy = AddressClaimStrategy::Fixed { preferred };
    let mut timer = MockTimer::new();

    tokio::select! {
        claim_result = claim_address(&mut dut_bus, &mut timer, my_name, strategy) => {
            // assert!(claim_result.is_ok());
            let claimed_address = claim_result.unwrap();
            dbg!("claimed_address: {:?}", claimed_address);
            assert_eq!(claimed_address, preferred, "Should retain preferred address");
        }


        _ = async {
            let frame1 = host_bus
                .recv()
                .await
                .expect("DUT did not send the initial claim");
            assert_eq!(frame1.id.source_address(), preferred);

            let conflict_frame = build_conflict_frame(their_name, preferred);
            host_bus
                .send(&conflict_frame)
                .await
                .expect("Failed to send conflict frame");

            let defense_frame = tokio::time::timeout(Duration::from_millis(20), host_bus.recv())
                .await
                .expect("DUT should have defended its address with a claim")
                .expect("Failed to read defense frame");

            assert_eq!(defense_frame.id.source_address(), preferred, "Defense must use the preferred address");
            assert_eq!(defense_frame.data, frame1.data, "Defense frame must reuse the same NAME");

            if tokio::time::timeout(Duration::from_millis(50), host_bus.recv()).await.is_ok() {
                panic!("DUT is not arbitrary-address capable and must not try a new address");
            }
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator finished before `claim_address`; the test setup is likely incorrect")
        }
    }
}

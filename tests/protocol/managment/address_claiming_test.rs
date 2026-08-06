//! Address claim driven by the runner: nominal case, lost conflict, exhaustion,
//! and ISO Request. Arbitration itself is covered by the engine unit tests.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::{
    constants::address,
    managment::{
        address_claiming::AddressClaimStrategy, address_manager::AddressManager,
        address_supervisor::AddressService, iso_name::IsoName,
    },
    transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus},
};

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

/// Build an ISO Request asking for the Address Claim PGN.
fn build_claim_request(destination: u8) -> CanFrame {
    let mut data = [0xFFu8; 8];
    data[0..3].copy_from_slice(&60928u32.to_le_bytes()[0..3]);
    let id = CanId::builder(59904, 42)
        .to_destination(destination)
        .with_priority(6)
        .build()
        .unwrap();
    CanFrame { id, data, len: 3 }
}

/// Build a service whose runner is ready to be driven.
fn service(
    bus: MockCanBus,
    my_name: u64,
    strategy: AddressClaimStrategy<'_>,
) -> AddressService<'_, MockCanBus, MockTimer> {
    let manager = AddressManager::new(bus, MockTimer::new(), IsoName::from_raw(my_name), strategy)
        .expect("strategy must match the NAME");
    AddressService::new(manager, 4, 4)
}

#[tokio::test(start_paused = true)]
async fn test_claim_no_conflict_keeps_the_preferred_address() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let preferred = 42;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };

    let parts = service(dut_bus, 0x8234567890ABCDEF, strategy).into_parts();
    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("runner ended unexpectedly: {:?}", result),
        _ = async {
            let claim = host_bus.recv().await.expect("initial claim expected");
            assert_eq!(claim.id.pgn(), 60928);
            assert_eq!(claim.id.source_address(), preferred);

            // Nothing answers: the address is kept and nothing else is emitted.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        } => {}
    }
}

#[tokio::test(start_paused = true)]
async fn test_claim_lost_conflict_walks_to_the_next_address() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let my_name: u64 = 0x9234567890ABCDEF; // MSB set -> Arbitrary Address Capable
    let their_name: u64 = 0x1234567890ABCDEF; // lower NAME -> we lose
    assert!(their_name < my_name);
    let preferred = 42;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };

    let parts = service(dut_bus, my_name, strategy).into_parts();
    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("runner ended unexpectedly: {:?}", result),
        _ = async {
            let claim = host_bus.recv().await.expect("initial claim expected");
            assert_eq!(claim.id.source_address(), preferred);

            host_bus
                .send(&build_conflict_frame(their_name, preferred))
                .await
                .expect("conflict must reach the DUT");

            let next = host_bus.recv().await.expect("a new candidate is expected");
            assert_eq!(next.id.pgn(), 60928);
            assert_eq!(next.id.source_address(), preferred + 1);
        } => {}
    }
}

#[tokio::test(start_paused = true)]
async fn test_claim_exhausted_emits_cannot_claim() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let my_name: u64 = 0x0234567890ABCDEF; // MSB clear -> single address capable
    let their_name: u64 = 0x0134567890ABCDEF; // lower NAME -> we lose
    assert!(their_name < my_name);
    let preferred = 142;
    let strategy = AddressClaimStrategy::Fixed { preferred };

    let parts = service(dut_bus, my_name, strategy).into_parts();
    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("runner ended unexpectedly: {:?}", result),
        _ = async {
            let claim = host_bus.recv().await.expect("initial claim expected");
            assert_eq!(claim.id.source_address(), preferred);

            host_bus
                .send(&build_conflict_frame(their_name, preferred))
                .await
                .expect("conflict must reach the DUT");

            // Fixed strategy: no fallback address, so the node declares Cannot Claim.
            let cannot = host_bus.recv().await.expect("CannotClaim expected");
            assert_eq!(cannot.id.pgn(), 60928);
            assert_eq!(cannot.id.source_address(), address::NULL_ADDR_254);
        } => {}
    }
}

#[tokio::test(start_paused = true)]
async fn test_iso_request_is_answered_through_the_runner() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let preferred = 42;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };

    let parts = service(dut_bus, 0x8234567890ABCDEF, strategy).into_parts();
    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("runner ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            host_bus
                .send(&build_claim_request(address::GLOBAL))
                .await
                .expect("request must reach the DUT");

            let answer = host_bus.recv().await.expect("the request must be answered");
            assert_eq!(answer.id.pgn(), 60928);
            assert_eq!(answer.id.source_address(), preferred);
        } => {}
    }
}

//! Address manager as a facade: construction, emission guard, and the address
//! it reports. The claim itself belongs to the runner.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::{
    error::{ClaimFault, SendPgnError},
    protocol::{
        management::{
            address_claiming::AddressClaimStrategy, address_manager::AddressManager,
            iso_name::IsoName,
        },
        messages::Pgn129025,
    },
};

const AAC_NAME: u64 = 0xF234567890ABCDEF; // MSB set -> Arbitrary Address Capable
const SAC_NAME: u64 = 0x0234567890ABCDEF; // MSB clear -> single address capable

fn manager(
    bus: MockCanBus,
    my_name: u64,
    strategy: AddressClaimStrategy<'_>,
) -> Result<AddressManager<'_, MockCanBus, MockTimer>, ClaimFault> {
    AddressManager::new(bus, MockTimer::new(), IsoName::from_raw(my_name), strategy)
}

#[tokio::test]
async fn test_manager_starts_without_an_address() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };

    let manager = manager(dut_bus, AAC_NAME, strategy).expect("strategy matches the NAME");

    // The constructor no longer claims: it cannot block on a saturated bus.
    assert_eq!(manager.claimed_address(), None);
}

#[tokio::test]
async fn test_manager_rejects_a_name_inconsistent_with_the_strategy() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();

    // Arbitrary requires the AAC bit; a SAC name cannot walk the address range.
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };
    let result = manager(dut_bus, SAC_NAME, strategy);

    assert!(matches!(result, Err(ClaimFault::InconsistentStrategy)));
}

#[tokio::test]
async fn test_send_pgn_is_refused_while_unclaimed() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };
    let mut manager = manager(dut_bus, AAC_NAME, strategy).expect("strategy matches the NAME");

    let mut position = Pgn129025::new();
    position.latitude = 47.6;
    position.longitude = -3.1;

    let result = manager.send_pgn(&position, 129025, None).await;

    // A swallowed `Ok(())` would let the caller believe it emitted.
    assert!(matches!(result, Err(SendPgnError::NotClaimed)));
    assert!(
        host_bus.try_recv().is_none(),
        "nothing may reach the bus before an address is acquired"
    );
}

#[tokio::test]
async fn test_send_payload_is_refused_while_unclaimed() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };
    let mut manager = manager(dut_bus, AAC_NAME, strategy).expect("strategy matches the NAME");

    let result = manager.send_payload(129025, 2, None, &[1, 2, 3, 4]).await;

    assert!(matches!(result, Err(SendPgnError::NotClaimed)));
    assert!(host_bus.try_recv().is_none());
}

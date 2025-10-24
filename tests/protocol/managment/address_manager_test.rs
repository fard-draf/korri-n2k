//! Address manager tests: initial claim, defense, reclaim, and filtering.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::{
    managment::address_manager::AddressManager,
    transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus},
};
use tokio::time::Duration;

/// Build a competing Address Claim frame.
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

/// Build a generic application frame (non-claim).
fn build_data_frame(pgn: u32, address: u8) -> CanFrame {
    let id = CanId::builder(pgn, address)
        .with_priority(3)
        .build()
        .unwrap();
    CanFrame {
        id,
        data: [1, 2, 3, 4, 5, 6, 7, 8],
        len: 8,
    }
}

#[tokio::test]
async fn test_address_manager_initial_claim() {
    // Ensure initialization obtains the preferred address when no conflict occurs.
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;

    let my_name = 0x1234567890ABCDEF;
    let preferred_address = 42;

    tokio::select! {
        result = AddressManager::new(dut_bus, timer, my_name, preferred_address) => {
            assert!(result.is_ok());
            let manager = result.unwrap();
            assert_eq!(manager.current_address(), preferred_address);
        }

        _ = async {
            // Simulate an idle network (no conflicts)
            let _claim = host_bus.recv().await.expect("Should receive initial claim");
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator terminated before AddressManager");
        }
    }
}

#[tokio::test]
async fn test_address_manager_defend_on_conflict_win() {
    // The manager must defend its address when it wins the conflict.
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;

    let my_name = 0x1234567890ABCDEE;
    let their_name = 0x1234567890ABCDEF; // Higher NAME → we win
    assert!(my_name < their_name);
    let preferred_address = 42;

    tokio::select! {
        _ = async {
            let mut manager = AddressManager::new(dut_bus, timer, my_name, preferred_address).await.unwrap();
            assert_eq!(manager.current_address(), preferred_address);

            // Receive in a loop so the manager processes claim frames
            loop {
                let _ = manager.recv().await;
            }
        } => {
            panic!("Manager task should not complete");
        }

        _ = async {
            // Wait for the initial claim
            let _initial_claim = host_bus.recv().await.expect("Should receive initial claim");

            // Inject a conflicting claim
            let conflict_frame = build_conflict_frame(their_name, preferred_address);
            host_bus.send(&conflict_frame).await.expect("Send conflict");

            // The manager should defend the address by issuing its own claim
            let defense_claim = tokio::time::timeout(
                Duration::from_millis(500),
                host_bus.recv()
            ).await.expect("Should receive defense claim within timeout").expect("Defense claim");

            assert_eq!(defense_claim.id.pgn(), 60928);
            assert_eq!(defense_claim.id.source_address(), preferred_address);
            assert_eq!(u64::from_le_bytes(defense_claim.data), my_name);
        } => {
            // Test complete
        }
    }
}

#[tokio::test]
async fn test_address_manager_reclaim_on_conflict_lose() {
    // Demonstrate automatic reclaim after losing an address.
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;

    let my_name = 0x9234567890ABCDEF; // AAC enabled
    let their_name = 0x1234567890ABCDEE; // Lower NAME → we lose
    assert!(my_name > their_name);
    let preferred_address = 247;

    tokio::select! {
        _ = async {
            let mut manager = AddressManager::new(dut_bus, timer, my_name, preferred_address).await.unwrap();
            assert_eq!(manager.current_address(), preferred_address);

            // Receive continuously; the manager handles conflicts automatically
            loop {
                let _ = manager.recv().await;
            }
        } => {
            panic!("Manager should not complete");
        }

        _ = async {
            // Wait for the initial claim (address 247)
            let initial_claim = host_bus.recv().await.expect("Should receive initial claim");
            assert_eq!(initial_claim.id.source_address(), 247);

            // Send a conflicting claim that forces the manager to yield
            let conflict_frame = build_conflict_frame(their_name, preferred_address);
            host_bus.send(&conflict_frame).await.expect("Send conflict");

            // The manager should reclaim another address (128)
            let reclaim = tokio::time::timeout(
                Duration::from_millis(500),
                host_bus.recv()
            ).await.expect("Should receive reclaim within timeout").expect("Reclaim");

            assert_eq!(reclaim.id.pgn(), 60928);
            assert_eq!(reclaim.id.source_address(), 128);
        } => {
            // Test complete
        }
    }
}

#[tokio::test]
async fn test_address_manager_filters_claim_frames() {
    // Application frames must be relayed to the caller.
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;

    let my_name = 0x1234567890ABCDEF;
    let preferred_address = 42;

    tokio::select! {
        _ = async {
            let mut manager = AddressManager::new(dut_bus, timer, my_name, preferred_address).await.unwrap();

            // Send non-claim data frames to the manager
            let data_frame = build_data_frame(129025, 50);
            let handled = manager.handle_frame(&data_frame).await.unwrap();

            // Data frames should reach the application layer
            assert!(handled.is_some());
            assert_eq!(handled.unwrap().id.pgn(), 129025);
        } => {
            // Test complete
        }

        _ = async {
            // Wait for the initial claim
            let _claim = host_bus.recv().await.expect("Should receive initial claim");
            std::future::pending::<()>().await;
        } => {
            panic!("Simulator terminated before test completion");
        }
    }
}

#[tokio::test]
async fn test_address_manager_ignores_own_claims() {
    // Claims originating from the same NAME must be ignored.
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;

    let my_name = 0x1234567890ABCDEF;
    let preferred_address = 42;

    tokio::select! {
        _ = async {
            let mut manager = AddressManager::new(dut_bus, timer, my_name, preferred_address).await.unwrap();

            // Send our own claim (same NAME)
            let own_claim = build_conflict_frame(my_name, preferred_address);
            let handled = manager.handle_frame(&own_claim).await.unwrap();

            // Our own claim must be ignored (no defense, no reclaim)
            assert!(handled.is_none());
            assert_eq!(manager.current_address(), preferred_address);
        } => {
            // Test complete
        }

        _ = async {
            // Wait for the initial claim
            let _claim = host_bus.recv().await.expect("Should receive initial claim");

            // Should not receive a defense for our own claim
            if tokio::time::timeout(Duration::from_millis(50), host_bus.recv()).await.is_ok() {
                panic!("Should not defend against own claim");
            }

            std::future::pending::<()>().await;
        } => {
            panic!("Simulator terminated before test completion");
        }
    }
}

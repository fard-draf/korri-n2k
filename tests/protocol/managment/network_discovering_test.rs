//! Validate network discovery by simulating Address Claim responses.
mod helpers {
    include!("../../helpers/mod.rs");
}
use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::managment::address_claiming::build_address_claim_frame;
use korri_n2k::protocol::managment::network_discovering::request_network_discovery;
use korri_n2k::protocol::transport::can_frame::CanFrame;
use korri_n2k::protocol::transport::can_id::CanId;
use korri_n2k::protocol::transport::traits::can_bus::CanBus;

#[tokio::test]
async fn test_request_network_discovery_three_devices() {
    // The function must discover three devices while ignoring duplicates and stray frames.
    // 1. Initialization
    let (mut dut_bus, mut host_bus) = MockCanBus::create_pair();
    let mut timer = MockTimer;

    // Prepare a stack buffer large enough to hold all results and a safety margin.
    // This ensures the function neither panics nor miscounts when extra space is available.
    let mut discovered_devices = [(0u8, 0u64); 5];

    // 2. Run the function and the simulator in parallel
    tokio::select! {
        // Branch 1: execute the function under test
        result = request_network_discovery(&mut dut_bus, &mut timer, &mut discovered_devices) => {
            // Step 4: assert on the result
            assert!(result.is_ok(), "Function returned an error");
            let count = result.unwrap();
            assert_eq!(count, 3, "Should discover 3 devices");

            // Sort results by address to keep the test deterministic since
            // frame arrival order is not guaranteed.
            discovered_devices[0..count].sort_by_key(|k| k.0);
            assert_eq!(discovered_devices[0], (42, 0xAAAAAAAAAAAAAAA1));
            assert_eq!(discovered_devices[1], (100, 0xBBBBBBBBBBBBBBB2));
            assert_eq!(discovered_devices[2], (200, 0xCCCCCCCCCCCCCCC3));
        }

        // Branch 2: network simulation
        _ = async {
            // Step 3: simulation logic
            // Wait for the discovery request as a synchronization point
            let request = host_bus
                .recv()
                .await
                .expect("DUT did not send a discovery request");
            assert_eq!(request.id.pgn(), 59904, "Unexpected PGN in discovery request");

            // Define the three simulated devices
            let device1 = (0xAAAAAAAAAAAAAAA1, 42);
            let device2 = (0xBBBBBBBBBBBBBBB2, 100);
            let device3 = (0xCCCCCCCCCCCCCCC3, 200);

            // Simulate responses by sending Address Claim frames
            host_bus.send(&build_address_claim_frame(device1.0, device1.1).unwrap()).await.unwrap();
            host_bus.send(&build_address_claim_frame(device2.0, device2.1).unwrap()).await.unwrap();
            host_bus.send(&build_address_claim_frame(device3.0, device3.1).unwrap()).await.unwrap();

            // Add a duplicate and an irrelevant frame to ensure they are ignored
            host_bus.send(&build_address_claim_frame(device1.0, device1.1).unwrap()).await.unwrap();
            let non_relevant_frame = CanFrame {
                id: CanId::builder(129025, 248)
                    .with_priority(2)
                    .build()
                    .unwrap(),
                data: [0u8; 8],
                len: 8,
            };
            host_bus.send(&non_relevant_frame).await.unwrap();
            // Keep this task pending so the function timeout concludes the select!
            std::future::pending::<()>().await;
        } => {
            // This branch must never finish first
            panic!("Simulator finished before the function under test")
        }
    }
}

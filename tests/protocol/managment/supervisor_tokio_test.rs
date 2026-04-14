mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::managment::address_manager::AddressManager;
use korri_n2k::protocol::managment::address_supervisor::{AddressService, AddressSupervisorRunError};
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus};
use tokio::time::Duration;

#[tokio::test]
async fn supervisor_tokio_queues_and_sends_pgn() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;
    let my_name = 0x1234_5678_90AB_CDEF;
    let preferred = 142u8;

    let manager = AddressManager::new(dut_bus, timer, my_name, preferred)
        .await
        .expect("claim must succeed");

    let service = AddressService::new(manager, 4, 0);
    let parts = service.into_parts();
    let handle = parts
        .handle
        .expect("handle must exist when command channel is provided");
    let runner_future = parts.runner.drive();
    tokio::pin!(runner_future);

    tokio::select! {
        result = &mut runner_future => {
            panic!("supervisor ended unexpectedly: {:?}", result);
        }
        _ = async {
            let claim_frame = host_bus
                .recv()
                .await
                .expect("supervisor must issue a claim frame");
            assert_eq!(claim_frame.id.pgn(), 60928);
            assert_eq!(claim_frame.id.source_address(), preferred);

            tokio::time::sleep(Duration::from_millis(300)).await;

            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            position.longitude = -3.1;

            handle
                .send_pgn(&position, 129025, 2, None)
                .await
                .expect("queueing PGN must succeed");

            let payload_frame = host_bus
                .recv()
                .await
                .expect("PGN frame expected on CAN bus");
            assert_eq!(payload_frame.id.pgn(), 129025);
            assert_eq!(payload_frame.id.source_address(), preferred);
        } => {}
    }
}

#[tokio::test]
async fn supervisor_tokio_exits_on_can_bus_error() {
    let (dut_bus, host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;
    let my_name = 0x1234_5678_90AB_CDEF;
    let preferred = 142u8;

    let manager = AddressManager::new(dut_bus, timer, my_name, preferred)
        .await
        .expect("claim must succeed");

    let service = AddressService::new(manager, 4, 4);
    let parts = service.into_parts();
    let runner_future = parts.runner.drive();

    // Drop host bus to close the underlying mpsc channel, which simulates a bus error on recv
    drop(host_bus);

    let result = runner_future.await;
    assert!(matches!(result, Err(AddressSupervisorRunError::Receive(()))));
}

#[tokio::test]
async fn supervisor_tokio_handles_cmd_channel_close() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;
    let my_name = 0x1234_5678_90AB_CDEF;
    let preferred = 142u8;

    let manager = AddressManager::new(dut_bus, timer, my_name, preferred)
        .await
        .expect("claim must succeed");

    let service = AddressService::new(manager, 4, 4);
    let parts = service.into_parts();
    
    // Keep frame receiver alive to check if it still receives CAN frames
    let mut frames = parts.frames.unwrap();
    let handle = parts.handle.unwrap();
    
    let runner_future = parts.runner.drive();
    tokio::pin!(runner_future);

    // Drop the sender channel
    drop(handle);

    tokio::select! {
        result = &mut runner_future => {
            panic!("supervisor ended unexpectedly: {:?}", result);
        }
        _ = async {
            // Consume the claim frame
            host_bus.recv().await.unwrap();

            // The supervisor should still be running. Send a dummy frame to the DUT.
            let id = CanId::builder(129025, 42).build().unwrap();
            let frame = CanFrame { id, data: [0, 1, 2, 3, 4, 5, 6, 7], len: 8 };
            host_bus.send(&frame).await.unwrap();

            // The supervisor should forward the frame to the frames channel
            let received = frames.recv().await.expect("should forward frame");
            assert_eq!(received.id.pgn(), 129025);
        } => {}
    }
}

#[tokio::test]
async fn supervisor_tokio_handles_frame_channel_close() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;
    let my_name = 0x1234_5678_90AB_CDEF;
    let preferred = 142u8;

    let manager = AddressManager::new(dut_bus, timer, my_name, preferred)
        .await
        .expect("claim must succeed");

    let service = AddressService::new(manager, 4, 4);
    let parts = service.into_parts();
    
    let handle = parts.handle.unwrap();
    let frames = parts.frames.unwrap();
    
    let runner_future = parts.runner.drive();
    tokio::pin!(runner_future);

    // Drop the frames receiver
    drop(frames);

    tokio::select! {
        result = &mut runner_future => {
            panic!("supervisor ended unexpectedly: {:?}", result);
        }
        _ = async {
            host_bus.recv().await.unwrap();

            // Send a dummy frame to the DUT. 
            // The supervisor will try to forward it, see the channel is closed, drop it and continue.
            let id = CanId::builder(129025, 42).build().unwrap();
            let frame = CanFrame { id, data: [0, 1, 2, 3, 4, 5, 6, 7], len: 8 };
            host_bus.send(&frame).await.unwrap();

            // Ensure the supervisor is still running and can still process commands
            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            position.longitude = -3.1;
            handle.send_pgn(&position, 129025, 2, None).await.unwrap();

            let payload_frame = host_bus.recv().await.unwrap();
            assert_eq!(payload_frame.id.pgn(), 129025);
        } => {}
    }
}

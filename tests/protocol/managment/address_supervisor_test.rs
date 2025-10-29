mod helpers {
    include!("../../helpers/mod.rs");
}

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::managment::address_manager::AddressManager;
use korri_n2k::protocol::managment::address_supervisor::{AddressService, SupervisorCommand};
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::traits::can_bus::CanBus;
use static_cell::StaticCell;
use tokio::time::Duration;

static COMMAND_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, SupervisorCommand, 4>> =
    StaticCell::new();

#[tokio::test]
async fn supervisor_queues_and_sends_pgn() {
    let command_channel = COMMAND_CHANNEL.init(Channel::new());

    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer;
    let my_name = 0x1234_5678_90AB_CDEF;
    let preferred = 142u8;

    let manager = AddressManager::new(dut_bus, timer, my_name, preferred)
        .await
        .expect("claim must succeed");

    let service = AddressService::<_, _, 4, 0>::new(manager, Some(&*command_channel), None);
    let parts = service.into_parts();
    let handle = parts
        .handle
        .expect("handle must exist when command channel is provided");
    let mut runner_future = parts.runner.drive();

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

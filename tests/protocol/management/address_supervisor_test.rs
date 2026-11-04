mod helpers {
    include!("../../helpers/mod.rs");
}

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::management::address_claiming::{
    engine::ClaimStatus, AddressClaimStrategy,
};
use korri_n2k::protocol::management::address_manager::AddressManager;
use korri_n2k::protocol::management::address_supervisor::{
    AddressService, ClaimedAddress, SupervisorCommand,
};
use korri_n2k::protocol::management::iso_name::IsoName;
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::traits::can_bus::CanBus;
use static_cell::StaticCell;
use tokio::time::Duration;

static COMMAND_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, SupervisorCommand, 4>> =
    StaticCell::new();
static CLAIMED: ClaimedAddress = ClaimedAddress::new();

#[tokio::test]
async fn supervisor_queues_and_sends_pgn() {
    let command_channel = COMMAND_CHANNEL.init(Channel::new());

    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let timer = MockTimer::new();
    let my_name = IsoName::from_raw(0xF234_5678_90AB_CDEF); // AAC enabled
    let preferred = 142u8;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };

    let manager =
        AddressManager::new(dut_bus, timer, my_name, strategy).expect("strategy matches the NAME");

    let service =
        AddressService::<_, _, 4, 0>::new(manager, Some(&*command_channel), None, &CLAIMED);
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
            assert_eq!(handle.claim_status(), Some(ClaimStatus::Claiming(preferred)));

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

            // The handle reports the address it emits from.
            assert_eq!(handle.claimed_address(), Some(preferred));
            assert_eq!(handle.claim_status(), Some(ClaimStatus::Claimed(preferred)));
        } => {}
    }
}

static MULTI_COMMANDS: [Channel<CriticalSectionRawMutex, SupervisorCommand, 2>; 2] =
    [Channel::new(), Channel::new()];
static MULTI_FRAMES: Channel<
    CriticalSectionRawMutex,
    korri_n2k::protocol::transport::can_frame::CanFrame,
    8,
> = Channel::new();
static MULTI_CLAIMED: [ClaimedAddress; 2] = [ClaimedAddress::new(), ClaimedAddress::new()];

/// Two Controller Applications on one bus aiming at the same address: the
/// loopback lets them see each other and settle on distinct addresses.
#[tokio::test]
async fn sibling_controllers_settle_on_distinct_addresses() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let preferred = 142u8;
    let strategy = AddressClaimStrategy::Arbitrary { preferred };
    let high = IsoName::from_raw(0xF234_5678_90AB_CDEF);
    let low = IsoName::from_raw(0xF234_5678_90AB_CDEE);

    let manager = AddressManager::with_controllers(
        dut_bus,
        MockTimer::new(),
        [(high, strategy), (low, strategy)],
    )
    .expect("strategies match the NAMEs");
    let parts = AddressService::<_, _, 2, 8, 2>::with_controllers(
        manager,
        [Some(&MULTI_COMMANDS[0]), Some(&MULTI_COMMANDS[1])],
        Some(&MULTI_FRAMES),
        [&MULTI_CLAIMED[0], &MULTI_CLAIMED[1]],
    )
    .into_multi_parts();
    let [Some(high_handle), Some(low_handle)] = parts.handles else {
        panic!("command channels were provided");
    };
    let mut frames = parts.frames.expect("frame channel was provided");
    {
        let runner_future = parts.runner.drive();
        tokio::pin!(runner_future);

        tokio::select! {
            result = &mut runner_future => panic!("supervisor ended unexpectedly: {:?}", result),
            _ = async {
                tokio::time::sleep(Duration::from_millis(400)).await;
                assert_eq!(low_handle.claimed_address(), Some(preferred));
                assert_eq!(high_handle.claimed_address(), Some(preferred + 1));
                while host_bus.try_recv().is_some() {}
                while MULTI_FRAMES.try_receive().is_ok() {}

                let position = Pgn129025::new();
                high_handle.send_pgn(&position, 129025, 2, None).await.unwrap();
                let frame = host_bus.recv().await.expect("PGN frame expected");
                assert_eq!(frame.id.source_address(), preferred + 1);
                let forwarded = frames.recv().await;
                assert_eq!(forwarded.id.pgn(), 129025);
                assert_eq!(forwarded.id.source_address(), preferred + 1);
            } => {}
        }
    }
    assert_eq!(high_handle.claimed_address(), None);
    assert_eq!(low_handle.claimed_address(), None);
}

static SHARED_CLAIMED: ClaimedAddress = ClaimedAddress::new();

/// One `ClaimedAddress` for two Controller Applications would make a handle
/// report, and its commands use, its sibling's address.
#[test]
#[should_panic(expected = "its own ClaimedAddress")]
fn a_shared_claimed_address_is_refused() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 142 };
    let manager = AddressManager::with_controllers(
        dut_bus,
        MockTimer::new(),
        [
            (IsoName::from_raw(0xF234_5678_90AB_CDEF), strategy),
            (IsoName::from_raw(0xF234_5678_90AB_CDEE), strategy),
        ],
    )
    .expect("strategies match the NAMEs");
    let _ = AddressService::<_, _, 2, 0, 2>::with_controllers(
        manager,
        [None, None],
        None,
        [&SHARED_CLAIMED, &SHARED_CLAIMED],
    );
}

/// Same for a command channel: one handle's commands would run under either
/// address.
#[test]
#[should_panic(expected = "its own command channel")]
fn a_shared_command_channel_is_refused() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 142 };
    let manager = AddressManager::with_controllers(
        dut_bus,
        MockTimer::new(),
        [
            (IsoName::from_raw(0xF234_5678_90AB_CDEF), strategy),
            (IsoName::from_raw(0xF234_5678_90AB_CDEE), strategy),
        ],
    )
    .expect("strategies match the NAMEs");
    let _ = AddressService::<_, _, 2, 0, 2>::with_controllers(
        manager,
        [Some(&MULTI_COMMANDS[0]), Some(&MULTI_COMMANDS[0])],
        None,
        [&MULTI_CLAIMED[0], &MULTI_CLAIMED[1]],
    );
}

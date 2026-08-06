//! Tokio runner: emission, frame forwarding, channel lifecycle, and the rule
//! that a rejected command must never take address management down.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::management::address_claiming::AddressClaimStrategy;
use korri_n2k::protocol::management::address_manager::AddressManager;
use korri_n2k::protocol::management::address_supervisor::{
    AddressService, AddressSupervisorRunError,
};
use korri_n2k::protocol::management::iso_name::IsoName;
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus};
use tokio::time::{sleep, Duration};

const AAC_NAME: u64 = 0xF234_5678_90AB_CDEF;
const PREFERRED: u8 = 142;

/// Long enough for a 250 ms claim window to close.
const CLAIM_SETTLED_MS: u64 = 400;

fn service(
    bus: MockCanBus,
    cmd_cap: usize,
    frame_cap: usize,
) -> AddressService<'static, MockCanBus, MockTimer> {
    let manager = AddressManager::new(
        bus,
        MockTimer::new(),
        IsoName::from_raw(AAC_NAME),
        AddressClaimStrategy::Arbitrary {
            preferred: PREFERRED,
        },
    )
    .expect("strategy must match the NAME");
    AddressService::new(manager, cmd_cap, frame_cap)
}

fn data_frame(pgn: u32, source: u8) -> CanFrame {
    CanFrame {
        id: CanId::builder(pgn, source).build().unwrap(),
        data: [0, 1, 2, 3, 4, 5, 6, 7],
        len: 8,
    }
}

#[tokio::test]
async fn supervisor_claims_then_sends_a_pgn() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            let claim = host_bus.recv().await.expect("supervisor must issue a claim");
            assert_eq!(claim.id.pgn(), 60928);
            assert_eq!(claim.id.source_address(), PREFERRED);

            // The claim now happens under the runner: wait for it to settle.
            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;

            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            position.longitude = -3.1;
            handle.send_pgn(&position, 129025, 2, None).await.expect("queueing must succeed");

            let payload = host_bus.recv().await.expect("PGN frame expected");
            assert_eq!(payload.id.pgn(), 129025);
            assert_eq!(payload.id.source_address(), PREFERRED);
        } => {}
    }
}

#[tokio::test]
async fn handle_reports_the_address_it_emits_from() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    // Nothing is held before the runner starts.
    assert_eq!(handle.claimed_address(), None);

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            // Still nothing during the 250 ms claim window: the address is not
            // ours until it closes.
            assert_eq!(handle.claimed_address(), None);

            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
            assert_eq!(handle.claimed_address(), Some(PREFERRED));
        } => {}
    }
}

#[tokio::test]
async fn supervisor_exits_on_can_bus_error() {
    let (dut_bus, host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 4).into_parts();

    // Dropping the host closes the underlying channel in both directions.
    drop(host_bus);

    // The very first thing the runner does is emit a claim, so the bus error
    // surfaces there — unwrapped, exactly as the driver reported it.
    let result = parts.runner.drive().await;
    assert!(matches!(result, Err(AddressSupervisorRunError::Send(()))));
}

#[tokio::test]
async fn supervisor_survives_a_rejected_command() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 4).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("a rejected command must not stop the runner: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            // Sent before any address is acquired: the manager refuses it.
            let mut position = Pgn129025::new();
            position.latitude = 1.0;
            handle.send_pgn(&position, 129025, 2, None).await.expect("queueing must succeed");

            // The runner is still alive and still claiming: once settled it emits.
            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
            handle.send_pgn(&position, 129025, 2, None).await.expect("queueing must succeed");

            let payload = host_bus.recv().await.expect("the runner must still emit");
            assert_eq!(payload.id.pgn(), 129025);
            assert_eq!(payload.id.source_address(), PREFERRED);
        } => {}
    }
}

#[tokio::test]
async fn supervisor_survives_a_closed_command_channel() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 4).into_parts();
    let mut frames = parts.frames.expect("frame channel requested");

    // Drop the only sender: `recv` now returns `None` instantly and forever.
    drop(parts.handle);

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            host_bus.send(&data_frame(129025, 42)).await.unwrap();
            let forwarded = frames.recv().await.expect("frame must be forwarded");
            assert_eq!(forwarded.id.pgn(), 129025);
        } => {}
    }
}

#[tokio::test]
async fn supervisor_survives_a_closed_frame_channel() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 4).into_parts();
    let handle = parts.handle.expect("command channel requested");

    drop(parts.frames);

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            // Forwarding fails silently; the runner must carry on.
            host_bus.send(&data_frame(129025, 42)).await.unwrap();
            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;

            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            handle.send_pgn(&position, 129025, 2, None).await.unwrap();

            let payload = host_bus.recv().await.expect("the runner must still emit");
            assert_eq!(payload.id.pgn(), 129025);
        } => {}
    }
}

#[tokio::test]
async fn supervisor_forwards_claim_frames_to_the_application() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 0, 4).into_parts();
    let mut frames = parts.frames.expect("frame channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            // A foreign claim reaches the engine *and* the application: address
            // management must not hide PGN 60928 from network discovery.
            let foreign_claim = CanFrame {
                id: CanId::builder(60928, 7).to_destination(255).with_priority(6).build().unwrap(),
                data: 0x1122_3344_5566_7788u64.to_le_bytes(),
                len: 8,
            };
            host_bus.send(&foreign_claim).await.unwrap();

            let forwarded = frames.recv().await.expect("60928 must be forwarded");
            assert_eq!(forwarded.id.pgn(), 60928);
            assert_eq!(forwarded.id.source_address(), 7);
        } => {}
    }
}

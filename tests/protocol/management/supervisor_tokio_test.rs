//! Tokio runner: emission, frame forwarding, channel lifecycle, and the rule
//! that a rejected command must never take address management down.
mod helpers {
    include!("../../helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::protocol::management::address_claiming::{
    build_address_claim_frame, engine::ClaimStatus, AddressClaimStrategy,
};
use korri_n2k::protocol::management::address_manager::AddressManager;
use korri_n2k::protocol::management::address_supervisor::{
    AddressService, AddressSupervisorRunError, SupervisorCommand,
};
use korri_n2k::protocol::management::iso_name::IsoName;
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;
use korri_n2k::protocol::transport::{can_frame::CanFrame, can_id::CanId, traits::can_bus::CanBus};
use tokio::time::{sleep, Duration};

const AAC_NAME: u64 = 0xF234_5678_90AB_CDEF;
const FIXED_NAME: u64 = 0x7234_5678_90AB_CDEF;
const PREFERRED: u8 = 142;

/// Long enough for a 250 ms claim window to close.
const CLAIM_SETTLED_MS: u64 = 400;

struct SlowSendBus {
    inner: MockCanBus,
}

impl CanBus for SlowSendBus {
    type Error = ();

    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
        sleep(Duration::from_millis(300)).await;
        self.inner.send(frame).await
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        self.inner.recv().await
    }
}

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

#[tokio::test(start_paused = true)]
async fn claim_timer_starts_after_slow_send() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let manager = AddressManager::new(
        SlowSendBus { inner: dut_bus },
        MockTimer::new(),
        IsoName::from_raw(AAC_NAME),
        AddressClaimStrategy::Arbitrary {
            preferred: PREFERRED,
        },
    )
    .expect("strategy must match the NAME");
    let parts = AddressService::new(manager, 1, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(350)).await;
    assert_eq!(handle.claimed_address(), None);

    sleep(Duration::from_millis(201)).await;
    assert_eq!(handle.claimed_address(), Some(PREFERRED));

    runner.abort();
    let _ = runner.await;
}

#[tokio::test]
async fn handle_reports_the_address_it_emits_from() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    // Nothing is held before the runner starts.
    assert_eq!(handle.claimed_address(), None);
    assert_eq!(handle.claim_status(), None);

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");

            // Still nothing during the 250 ms claim window: the address is not
            // ours until it closes.
            assert_eq!(handle.claimed_address(), None);
            assert_eq!(handle.claim_status(), Some(ClaimStatus::Claiming(PREFERRED)));

            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
            assert_eq!(handle.claimed_address(), Some(PREFERRED));
            assert_eq!(handle.claim_status(), Some(ClaimStatus::Claimed(PREFERRED)));
        } => {}
    }
}

#[tokio::test]
async fn handle_reports_cannot_claim_after_a_fixed_node_loses() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let manager = AddressManager::new(
        dut_bus,
        MockTimer::new(),
        IsoName::from_raw(FIXED_NAME),
        AddressClaimStrategy::Fixed {
            preferred: PREFERRED,
        },
    )
    .expect("fixed strategy must match the NAME");
    let parts = AddressService::new(manager, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");
            assert_eq!(handle.claim_status(), Some(ClaimStatus::Claiming(PREFERRED)));

            let stronger_name = IsoName::from_raw(1);
            let rival = build_address_claim_frame(stronger_name, PREFERRED);
            host_bus.send(&rival).await.unwrap();

            let cannot_claim = host_bus.recv().await.expect("Cannot Claim expected");
            assert_eq!(cannot_claim.id.source_address(), 254);
            assert_eq!(handle.claim_status(), Some(ClaimStatus::CannotClaim));
            assert_eq!(handle.claimed_address(), None);
        } => {}
    }
}

/// P0: a full application channel must never delay address management.
#[tokio::test]
async fn a_full_frame_channel_does_not_stall_the_claim() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    // Capacity 1, and nothing ever drains it.
    let parts = service(dut_bus, 0, 1).into_parts();
    let _frames = parts.frames.expect("frame channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("supervisor ended unexpectedly: {:?}", result),
        _ = async {
            let claim = host_bus.recv().await.expect("initial claim expected");
            assert_eq!(claim.id.source_address(), PREFERRED);

            // Fill the channel, then keep pushing frames the runner must forward.
            for _ in 0..4 {
                host_bus.send(&data_frame(129025, 7)).await.unwrap();
                sleep(Duration::from_millis(5)).await;
            }

            // A conflict now arrives. The engine loses and must announce the next
            // candidate on the bus, even though the application is not draining.
            let rival = build_address_claim_frame(IsoName::from_raw(0x0000_0000_0000_0001), PREFERRED);
            host_bus.send(&rival).await.unwrap();

            let next = host_bus.recv().await.expect("the next candidate must be emitted");
            assert_eq!(next.id.pgn(), 60928);
            assert_eq!(next.id.source_address(), PREFERRED + 1);
        } => {}
    }
}

/// P0: `len` is a public field of the command; an out-of-range value must be
/// rejected, not sliced into a panic.
#[tokio::test]
async fn an_out_of_range_payload_length_is_rejected() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("a bad command must not stop the runner: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");
            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;

            handle
                .send_command(SupervisorCommand::SendPayload {
                    pgn: 129025,
                    priority: 2,
                    destination: None,
                    len: usize::MAX,
                    payload: [0u8; MAX_FAST_PACKET_PAYLOAD],
                })
                .await
                .expect("queueing must succeed");

            // Still alive, still emitting.
            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            handle.send_pgn(&position, 129025, 2, None).await.unwrap();

            let payload = host_bus.recv().await.expect("the runner must still emit");
            assert_eq!(payload.id.pgn(), 129025);
        } => {}
    }
}

/// `CanFrame.len` is a public `usize`; a classic CAN frame holds 8 bytes, and a
/// driver slicing `data[..len]` would panic on 9.
#[tokio::test]
async fn raw_frame_longer_than_eight_bytes_is_rejected() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = parts.runner.drive();
    tokio::pin!(runner);

    tokio::select! {
        result = &mut runner => panic!("a bad frame must not stop the runner: {:?}", result),
        _ = async {
            host_bus.recv().await.expect("initial claim expected");
            sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;

            // Source address held, so only the length is wrong.
            let oversized = CanFrame {
                id: CanId::builder(129026, PREFERRED).with_priority(2).build().unwrap(),
                data: [0xEE; 8],
                len: 9,
            };
            handle.send_raw_frame(&oversized).await.expect("queueing must succeed");

            // Still alive, and the oversized frame never reached the bus.
            let mut position = Pgn129025::new();
            position.latitude = 47.6;
            handle.send_pgn(&position, 129025, 2, None).await.unwrap();

            let frame = host_bus.recv().await.expect("the runner must still emit");
            assert_eq!(frame.id.pgn(), 129025, "the oversized frame must not be emitted");
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
    // surfaces there, unwrapped, exactly as the driver reported it.
    let result = parts.runner.drive().await;
    assert!(matches!(result, Err(AddressSupervisorRunError::Send(()))));
}

/// P1: the published address must not outlive the runner.
///
/// Nothing defends an address once the loop is gone, so a handle still handing
/// out `Some(142)` would invite the application to emit from an address it no
/// longer holds.
#[tokio::test]
async fn a_dead_bus_clears_the_published_address() {
    let (dut_bus, host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    assert_eq!(handle.claimed_address(), Some(PREFERRED));

    // The driver disappears: the pending `recv` fails and the runner stops.
    drop(host_bus);

    let result = runner.await.expect("the runner task must not panic");
    assert!(matches!(
        result,
        Err(AddressSupervisorRunError::Receive(()))
    ));

    assert_eq!(
        handle.claimed_address(),
        None,
        "a dead runner must not keep publishing an address"
    );
    assert_eq!(handle.claim_status(), None);
}

/// The same cleanup on cancellation: no error is ever returned here, so only
/// `Drop` can clear the address.
#[tokio::test]
async fn a_cancelled_runner_clears_the_published_address() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");

    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    assert_eq!(handle.claimed_address(), Some(PREFERRED));

    runner.abort();
    let _ = runner.await;

    assert_eq!(handle.claimed_address(), None);
    assert_eq!(handle.claim_status(), None);
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

//==================================================================================MULTI_ADDRESS
// One bus, two Controller Applications aiming at the same address.

/// Lower than `AAC_NAME`: wins the arbitration for `PREFERRED`.
const AAC_NAME_LOWER: u64 = AAC_NAME - 1;

fn multi_service(bus: MockCanBus) -> AddressService<'static, MockCanBus, MockTimer, 2> {
    let strategy = AddressClaimStrategy::Arbitrary {
        preferred: PREFERRED,
    };
    let manager = AddressManager::with_controllers(
        bus,
        MockTimer::new(),
        [
            (IsoName::from_raw(AAC_NAME), strategy),
            (IsoName::from_raw(AAC_NAME_LOWER), strategy),
        ],
    )
    .expect("strategies must match the NAMEs");
    AddressService::new(manager, 4, 4)
}

fn claim_request(source: u8, destination: u8) -> CanFrame {
    let mut data = [0xFF; 8];
    data[..3].copy_from_slice(&60928u32.to_le_bytes()[..3]);
    CanFrame {
        id: CanId::builder(59904, source)
            .to_destination(destination)
            .build()
            .unwrap(),
        data,
        len: 3,
    }
}

fn drain(host_bus: &mut MockCanBus) {
    while host_bus.try_recv().is_some() {}
}

/// Bounded: a missing frame must fail the test, not hang it.
async fn next_frame(host_bus: &mut MockCanBus) -> CanFrame {
    tokio::time::timeout(Duration::from_secs(2), host_bus.recv())
        .await
        .expect("frame expected in time")
        .expect("bus open")
}

/// Claim sources seen on the bus until `count` of them arrived.
async fn claim_sources(host_bus: &mut MockCanBus, count: usize) -> Vec<u8> {
    let mut sources = Vec::new();
    while sources.len() < count {
        let frame = next_frame(host_bus).await;
        if frame.id.pgn() == 60928 {
            sources.push(frame.id.source_address());
        }
    }
    sources.sort();
    sources
}

#[tokio::test]
async fn sibling_controllers_settle_on_distinct_addresses() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = multi_service(dut_bus).into_multi_parts();
    let [Some(high), Some(low)] = parts.handles else {
        panic!("command channels requested");
    };
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    // The lower NAME keeps the preferred address; its sibling saw the claim
    // through the loopback and moved on.
    assert_eq!(low.claimed_address(), Some(PREFERRED));
    assert_eq!(high.claimed_address(), Some(PREFERRED + 1));
    drain(&mut host_bus);

    // Each handle emits from its own address.
    let position = Pgn129025::new();
    high.send_pgn(&position, 129025, 2, None).await.unwrap();
    let frame = next_frame(&mut host_bus).await;
    assert_eq!(frame.id.source_address(), PREFERRED + 1);

    // Dropping one handle closes only its channel; the sibling keeps emitting.
    drop(high);
    low.send_pgn(&position, 129025, 2, None).await.unwrap();
    let frame = next_frame(&mut host_bus).await;
    assert_eq!(frame.id.source_address(), PREFERRED);

    runner.abort();
    let _ = runner.await;
    assert_eq!(low.claimed_address(), None);
}

#[tokio::test]
async fn three_sibling_controllers_cascade_to_distinct_addresses() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary {
        preferred: PREFERRED,
    };
    let manager = AddressManager::with_controllers(
        dut_bus,
        MockTimer::new(),
        [
            (IsoName::from_raw(AAC_NAME), strategy),
            (IsoName::from_raw(AAC_NAME - 1), strategy),
            (IsoName::from_raw(AAC_NAME - 2), strategy),
        ],
    )
    .expect("strategies must match the NAMEs");
    let parts = AddressService::new(manager, 2, 0).into_multi_parts();
    let [Some(highest), Some(middle), Some(lowest)] = parts.handles else {
        panic!("command channels requested");
    };
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    assert_eq!(lowest.claimed_address(), Some(PREFERRED));
    assert_eq!(middle.claimed_address(), Some(PREFERRED + 1));
    assert_eq!(highest.claimed_address(), Some(PREFERRED + 2));

    runner.abort();
    let _ = runner.await;
    assert_eq!(lowest.claimed_address(), None);
    assert_eq!(middle.claimed_address(), None);
    assert_eq!(highest.claimed_address(), None);
}

#[tokio::test]
async fn every_controller_answers_a_global_request() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = multi_service(dut_bus).into_multi_parts();
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    drain(&mut host_bus);

    host_bus.send(&claim_request(7, 255)).await.unwrap();
    assert_eq!(
        claim_sources(&mut host_bus, 2).await,
        [PREFERRED, PREFERRED + 1]
    );

    runner.abort();
}

/// A request one Controller Application emits reaches its sibling, which the
/// bus alone would never deliver.
#[tokio::test]
async fn a_sibling_answers_a_local_request() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = multi_service(dut_bus).into_multi_parts();
    let [Some(high), _] = parts.handles else {
        panic!("command channels requested");
    };
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    drain(&mut host_bus);

    high.send_raw_frame(&claim_request(PREFERRED + 1, PREFERRED))
        .await
        .unwrap();

    let request = next_frame(&mut host_bus).await;
    assert_eq!(request.id.pgn(), 59904);
    assert_eq!(claim_sources(&mut host_bus, 1).await, [PREFERRED]);

    runner.abort();
}

#[tokio::test]
async fn a_duplicate_name_is_rejected() {
    let (dut_bus, _host_bus) = MockCanBus::create_pair();
    let strategy = AddressClaimStrategy::Arbitrary {
        preferred: PREFERRED,
    };
    let name = IsoName::from_raw(AAC_NAME);
    let result = AddressManager::with_controllers(dut_bus, MockTimer::new(), [(name, strategy); 2]);
    assert!(matches!(
        result,
        Err(korri_n2k::error::ClaimFault::DuplicateName)
    ));
}

/// J1939-21: a Controller Application holding an address answers its own
/// global Request for Address Claim. True for a plain node too.
#[tokio::test]
async fn a_single_controller_answers_its_own_global_request() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = service(dut_bus, 4, 0).into_parts();
    let handle = parts.handle.expect("command channel requested");
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    drain(&mut host_bus);

    handle
        .send_raw_frame(&claim_request(PREFERRED, 255))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut host_bus).await.id.pgn(), 59904);
    assert_eq!(claim_sources(&mut host_bus, 1).await, [PREFERRED]);

    runner.abort();
}

/// The emitter answers its own global request, and so does its sibling.
#[tokio::test]
async fn a_local_global_request_is_answered_by_every_controller() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = multi_service(dut_bus).into_multi_parts();
    let [Some(high), _] = parts.handles else {
        panic!("command channels requested");
    };
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    drain(&mut host_bus);

    high.send_raw_frame(&claim_request(PREFERRED + 1, 255))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut host_bus).await.id.pgn(), 59904);
    assert_eq!(
        claim_sources(&mut host_bus, 2).await,
        [PREFERRED, PREFERRED + 1]
    );

    runner.abort();
}

/// The applications share `AddressFrames`: each must see what its siblings
/// emit, Fast Packet fragments included, as on a bus of separate devices. Bus
/// traffic still arrives exactly once.
#[tokio::test]
async fn sibling_emissions_reach_the_shared_frames_channel() {
    let (dut_bus, mut host_bus) = MockCanBus::create_pair();
    let parts = multi_service(dut_bus).into_multi_parts();
    let mut frames = parts.frames.expect("frame channel requested");
    let [Some(high), _] = parts.handles else {
        panic!("command channels requested");
    };
    let runner = tokio::spawn(parts.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(20), frames.recv()).await {}

    high.send_pgn(&Pgn129025::new(), 129025, 2, None)
        .await
        .unwrap();
    high.send_command(SupervisorCommand::SendPayload {
        pgn: 129029,
        priority: 3,
        destination: None,
        len: 20,
        payload: [0x55; MAX_FAST_PACKET_PAYLOAD],
    })
    .await
    .unwrap();
    sleep(Duration::from_millis(50)).await;
    host_bus.send(&data_frame(130306, 7)).await.unwrap();

    let mut seen = Vec::new();
    while let Ok(Some(frame)) = tokio::time::timeout(Duration::from_millis(50), frames.recv()).await
    {
        seen.push((frame.id.pgn(), frame.id.source_address()));
    }
    let from_high = PREFERRED + 1;
    assert_eq!(
        seen,
        [
            (129025, from_high),
            (129029, from_high),
            (129029, from_high),
            (129029, from_high),
            (130306, 7),
        ]
    );

    runner.abort();
}

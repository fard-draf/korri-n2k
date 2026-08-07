#![cfg(feature = "tokio")]
//! "First conversation" integration scenario: two nodes claim an address under
//! their own runner, exchange a PGN 59904 request, and answer with PGN 129025.
mod helpers {
    include!("helpers/mod.rs");
}

use helpers::{MockCanBus, MockTimer};
use korri_n2k::{
    infra::codec::traits::PgnData,
    protocol::{
        management::{
            address_claiming::AddressClaimStrategy, address_manager::AddressManager,
            address_supervisor::AddressService, iso_name::IsoName,
        },
        messages::{Pgn129025, Pgn59904},
    },
};
use tokio::time::{sleep, Duration};

const EMITTER_NAME: u64 = 0x9234567890ABCDEF;
const READER_NAME: u64 = 0xA34567890ABCDEF1;
const EMITTER_ADDR: u8 = 42;
const READER_ADDR: u8 = 43;

/// Long enough for a 250 ms claim window to close.
const CLAIM_SETTLED_MS: u64 = 400;

fn service(
    bus: MockCanBus,
    my_name: u64,
    preferred: u8,
) -> AddressService<'static, MockCanBus, MockTimer> {
    let manager = AddressManager::new(
        bus,
        MockTimer::new(),
        IsoName::from_raw(my_name),
        AddressClaimStrategy::Arbitrary { preferred },
    )
    .expect("strategy must match the NAME");
    AddressService::new(manager, 4, 8)
}

#[tokio::test]
async fn test_first_conversation() {
    let (emitter_bus, reader_bus) = MockCanBus::create_pair();

    let emitter = service(emitter_bus, EMITTER_NAME, EMITTER_ADDR).into_parts();
    let reader = service(reader_bus, READER_NAME, READER_ADDR).into_parts();

    let emitter_handle = emitter.handle.expect("command channel requested");
    let mut emitter_frames = emitter.frames.expect("frame channel requested");
    let reader_handle = reader.handle.expect("command channel requested");
    let mut reader_frames = reader.frames.expect("frame channel requested");

    // Each node runs its own event loop, exactly as an application would.
    tokio::spawn(emitter.runner.drive());
    tokio::spawn(reader.runner.drive());

    sleep(Duration::from_millis(CLAIM_SETTLED_MS)).await;

    // 1. The reader asks the emitter for PGN 129025.
    reader_handle
        .send_pgn(&Pgn59904 { pgn: 129025 }, 59904, 6, Some(EMITTER_ADDR))
        .await
        .expect("queueing the request must succeed");

    // 2. The emitter sees it: a Request for another PGN is application traffic,
    //    not something the claim engine answers.
    let request = loop {
        let frame = emitter_frames.recv().await.expect("emitter channel closed");
        if frame.id.pgn() == 59904 {
            break frame;
        }
    };
    assert_eq!(request.id.source_address(), READER_ADDR);
    assert_eq!(request.id.destination().unwrap(), EMITTER_ADDR);

    let asked = Pgn59904::from_payload(&request.data[..request.len])
        .expect("request deserialization must succeed");
    assert_eq!(asked.pgn, 129025);

    // 3. The emitter answers with its position.
    let mut position = Pgn129025::new();
    position.latitude = 47.64425;
    position.longitude = -2.71842;

    emitter_handle
        .send_pgn(&position, 129025, 3, None)
        .await
        .expect("queueing the answer must succeed");

    // 4. The reader gets it, with the emitter's claimed address as source.
    let answer = loop {
        let frame = reader_frames.recv().await.expect("reader channel closed");
        if frame.id.pgn() == 129025 {
            break frame;
        }
    };
    assert_eq!(answer.id.source_address(), EMITTER_ADDR);

    let received = Pgn129025::from_payload(&answer.data[..answer.len])
        .expect("position deserialization must succeed");
    assert!((received.latitude - position.latitude).abs() < 1e-6);
    assert!((received.longitude - position.longitude).abs() < 1e-6);
}

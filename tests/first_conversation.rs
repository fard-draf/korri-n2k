//! “First conversation” integration scenario: two nodes identify themselves,
//! exchange a PGN 59904 request, and respond with PGN 129025.

use korri_n2k::{
    infra::codec::traits::PgnData,
    protocol::{
        managment::address_claiming::claim_address,
        messages::{Pgn129025, Pgn59904},
        transport::{
            can_frame::CanFrame,
            can_id::CanId,
            traits::{can_bus::CanBus, korri_timer::KorriTimer},
        },
    },
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};

#[derive(Clone)]
/// Simulated CAN bus for integration tests.
pub struct MockCanBus {
    tx: mpsc::UnboundedSender<CanFrame>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<CanFrame>>>,
}

impl MockCanBus {
    /// Build two interconnected endpoints (device ↔ host).
    pub fn create_pair() -> (Self, Self) {
        let (dut_tx, host_rx) = mpsc::unbounded_channel();
        let (host_tx, dut_rx) = mpsc::unbounded_channel();

        let dut_bus = Self {
            tx: dut_tx,
            rx: Arc::new(Mutex::new(dut_rx)),
        };

        let host_bus = Self {
            tx: host_tx,
            rx: Arc::new(Mutex::new(host_rx)),
        };

        (dut_bus, host_bus)
    }
}

impl CanBus for MockCanBus {
    type Error = ();

    async fn send<'a>(&'a mut self, frame: &'a CanFrame) -> Result<(), Self::Error> {
        self.tx.send(frame.clone()).map_err(|_| ())?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        let mut rx = self.rx.lock().await;
        rx.recv().await.ok_or(())
    }
}

/// Timer based on `tokio::sleep` to control delays during the test.
pub struct MockTimer;

impl KorriTimer for MockTimer {
    async fn delay_ms(&mut self, millis: u32) {
        sleep(Duration::from_millis(millis as u64)).await;
    }
}

#[tokio::test]
async fn test_premiere_conversation() {
    // Steps: emitter claim → reader claim → request → response → assertions.
    // Create simulated buses for emitter and reader
    let (mut emitter_bus, mut reader_bus) = MockCanBus::create_pair();

    // Create timers
    let mut emitter_timer = MockTimer;
    let mut reader_timer = MockTimer;

    // Device NAME values (64-bit ISO NAME)
    let emitter_name = 0x9234567890ABCDEF;
    let reader_name = 0xA34567890ABCDEF1;

    // Preferred addresses
    let emitter_preferred_address = 42;
    let reader_preferred_address = 43;

    // 1. CLAIM ADDRESS – Emitter
    let emitter_claimed_address = claim_address(
        &mut emitter_bus.clone(),
        &mut emitter_timer,
        emitter_name,
        emitter_preferred_address,
    )
    .await
    .expect("Emitter must successfully claim its address");

    println!("Emitter claimed address: {}", emitter_claimed_address);

    // 2. CLAIM ADDRESS – Reader
    let reader_claimed_address = claim_address(
        &mut reader_bus.clone(),
        &mut reader_timer,
        reader_name,
        reader_preferred_address,
    )
    .await
    .expect("Reader must successfully claim its address");

    println!("Reader claimed address: {}", reader_claimed_address);

    // Ensure the addresses differ
    assert_ne!(
        emitter_claimed_address, reader_claimed_address,
        "Both devices must end up with different addresses"
    );

    // 3. Emitter prepares PGN 129025 (position) to answer requests
    let mut position_pgn = Pgn129025::new();
    position_pgn.latitude = 47.64425; // Example latitude
    position_pgn.longitude = -2.71842; // Example longitude

    // 4. Reader sends PGN 59904 (request) asking for PGN 129025
    let request_pgn = Pgn59904 { pgn: 129025 };

    let mut buffer = [0u8; 8];
    let payload_len = request_pgn
        .to_payload(&mut buffer)
        .expect("Request serialization should succeed");

    // Build the CAN frame for PGN 59904
    let can_id = CanId::builder(59904, reader_claimed_address)
        .with_priority(6)
        .to_destination(emitter_claimed_address) // Direct send to the emitter
        .build()
        .expect("CAN ID construction should succeed");

    let mut frame = CanFrame {
        id: can_id,
        data: [0; 8],
        len: payload_len,
    };

    // Copy payload into the frame
    frame.data[..payload_len].copy_from_slice(&buffer[..payload_len]);

    // Send the request frame
    reader_bus
        .send(&frame)
        .await
        .expect("Sending the request frame should succeed");

    // 5. Emitter receives frames until the request arrives
    let received_request_frame = loop {
        let frame = emitter_bus
            .recv()
            .await
            .expect("Emitter must receive a frame");

        // Ignore and continue when the frame is an Address Claim
        if frame.id.pgn() == 60928 {
            println!("Emitter ignored an Address Claim frame");
            continue;
        }

        // If this is the expected request, process it
        if frame.id.pgn() == 59904 {
            assert_eq!(frame.id.source_address(), reader_claimed_address);
            assert_eq!(frame.id.destination().unwrap(), emitter_claimed_address);
            break frame;
        }
    };

    // Deserialize PGN 59904
    let received_request_pgn =
        Pgn59904::from_payload(&received_request_frame.data[..received_request_frame.len])
            .expect("Request deserialization should succeed");

    // Ensure the request targets PGN 129025
    assert_eq!(received_request_pgn.pgn, 129025);

    // 6. Emitter replies with PGN 129025
    let mut buffer = [0u8; 8];
    let payload_len = position_pgn
        .to_payload(&mut buffer)
        .expect("Position serialization should succeed");

    // Build the CAN frame for PGN 129025
    let can_id = CanId::builder(129025, emitter_claimed_address)
        .with_priority(3)
        .build()
        .expect("CAN ID construction should succeed");

    let mut frame = CanFrame {
        id: can_id,
        data: [0; 8],
        len: payload_len,
    };

    // Copy payload into the frame
    frame.data[..payload_len].copy_from_slice(&buffer[..payload_len]);

    // Send the response frame
    emitter_bus
        .send(&frame)
        .await
        .expect("Sending the response frame should succeed");

    // 7. Reader processes frames until the response arrives
    let received_frame = loop {
        let frame = reader_bus
            .recv()
            .await
            .expect("Reader must receive a frame");

        // Ignore Address Claim frames
        if frame.id.pgn() == 60928 {
            println!("Reader ignored an Address Claim frame");
            continue;
        }

        // If this is the expected response, process it
        if frame.id.pgn() == 129025 {
            assert_eq!(frame.id.source_address(), emitter_claimed_address);
            break frame;
        }
    };

    // 8. Deserialize PGN 129025
    let received_pgn = Pgn129025::from_payload(&received_frame.data[..received_frame.len])
        .expect("Position deserialization should succeed");

    // Validate data
    assert!((received_pgn.latitude - position_pgn.latitude).abs() < 1e-6);
    assert!((received_pgn.longitude - position_pgn.longitude).abs() < 1e-6);

    println!("First conversation test passed!");
    println!(
        "Received position – Latitude: {}, Longitude: {}",
        received_pgn.latitude, received_pgn.longitude
    );
}

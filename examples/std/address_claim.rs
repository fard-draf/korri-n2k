//! # Address claim, end to end
//!
//! Claims a logical address on the bus, then sends a PGN from it.
//!
//! The claim happens under the runner, not in a constructor. A constructor
//! that waited for an address could never return on a saturated bus.
//!
//! ```bash
//! cargo run --example address_claim --features tokio
//! ```

use std::sync::Arc;
use std::time::Duration;

use korri_n2k::protocol::management::address_claiming::AddressClaimStrategy;
use korri_n2k::protocol::management::address_manager::AddressManager;
use korri_n2k::protocol::management::address_supervisor::{AddressHandle, AddressService};
use korri_n2k::protocol::management::iso_name::IsoName;
use korri_n2k::protocol::messages::Pgn129025;
use korri_n2k::protocol::transport::can_frame::CanFrame;
use korri_n2k::protocol::transport::traits::can_bus::CanBus;
use korri_n2k::protocol::transport::traits::korri_timer::TokioTimer;
use tokio::sync::{mpsc, Mutex};

/// Stand-in for a real driver. Replace with SocketCAN, an MCP2515, an STM32
/// bxCAN peripheral: the library only needs these two methods.
#[derive(Clone)]
struct LoopbackBus {
    tx: mpsc::UnboundedSender<CanFrame>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<CanFrame>>>,
}

impl LoopbackBus {
    /// Two ends of one wire: our node, and whatever else is listening.
    fn pair() -> (Self, Self) {
        let (a_tx, b_rx) = mpsc::unbounded_channel();
        let (b_tx, a_rx) = mpsc::unbounded_channel();
        (
            Self {
                tx: a_tx,
                rx: Arc::new(Mutex::new(a_rx)),
            },
            Self {
                tx: b_tx,
                rx: Arc::new(Mutex::new(b_rx)),
            },
        )
    }
}

impl CanBus for LoopbackBus {
    type Error = ();

    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
        self.tx.send(*frame).map_err(|_| ())
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        self.rx.lock().await.recv().await.ok_or(())
    }
}

/// How often to re-ask the handle whether the claim has closed.
const POLL_MS: u64 = 20;

#[tokio::main]
async fn main() {
    let (node_bus, mut bus_watcher) = LoopbackBus::pair();

    // Our identity on the bus. The MSB carries the Arbitrary Address Capable
    // bit, which `Arbitrary` requires: without it, `new` refuses the strategy.
    let my_name = IsoName::from_raw(0xF234_5678_90AB_CDEF);
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };

    // Synchronous, and it never touches the bus. It only checks that the NAME
    // and the strategy agree. That is the single way this call can fail.
    let manager = AddressManager::new(node_bus, TokioTimer::new(), my_name, strategy)
        .expect("the NAME must be Arbitrary Address Capable");

    println!("before the runner starts: {:?}", manager.claimed_address());

    // 4 queued commands, 8 buffered incoming frames. Pass 0 to opt out of either.
    let parts = AddressService::new(manager, 4, 8).into_parts();
    let handle = parts.handle.expect("a command channel was requested");

    // Watch what the node puts on the wire.
    tokio::spawn(async move {
        while let Ok(frame) = bus_watcher.recv().await {
            println!(
                "  bus <- PGN {:>6}  from address {}",
                frame.id.pgn(),
                frame.id.source_address()
            );
        }
    });

    // The runner owns the event loop from here on: it claims, defends, answers
    // ISO Requests, and emits whatever the handle queues.
    tokio::spawn(parts.runner.drive());

    // Ask the handle instead of guessing a delay. Emissions before the address
    // is acquired are refused, never silently swallowed: the node must not speak
    // from an address it does not own.
    let address = wait_for_address(&handle).await;
    println!("claimed address: {address}");

    let mut position = Pgn129025::new();
    position.latitude = 47.7223;
    position.longitude = -4.0022;

    handle
        .send_pgn(&position, 129025, 2, None)
        .await
        .expect("queueing must succeed");

    tokio::time::sleep(Duration::from_millis(50)).await;
    println!("done");
}

/// Poll until the claim closes. `claimed_address` is best effort: it says what
/// the engine holds right now, and a later conflict can still take it away.
async fn wait_for_address(handle: &AddressHandle) -> u8 {
    loop {
        if let Some(address) = handle.claimed_address() {
            return address;
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }
}

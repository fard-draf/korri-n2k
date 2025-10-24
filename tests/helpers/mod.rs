/// Test doubles to simulate the CAN bus and timer during integration tests.
use korri_n2k::protocol::transport::{
    can_frame::CanFrame,
    traits::{can_bus::CanBus, korri_timer::KorriTimer},
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};

#[derive(Clone)]
#[allow(dead_code)]
/// In-memory CAN bus reproducing the `CanBus` trait behavior.
pub struct MockCanBus {
    tx: mpsc::UnboundedSender<CanFrame>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<CanFrame>>>,
}

#[allow(dead_code)]
impl MockCanBus {
    /// Construct a pair of interconnected buses (DUT ↔ host).
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

#[allow(dead_code)]
/// Timer based on `tokio::time::sleep` to drive delays in tests.
pub struct MockTimer;

impl KorriTimer for MockTimer {
    async fn delay_ms(&mut self, millis: u32) {
        sleep(Duration::from_millis(millis as u64)).await;
    }
}

#[allow(dead_code)]
/// Utility loop: drain incoming claims without responding (no conflict).
pub(crate) async fn simulate_no_conflict(mut host_bus: MockCanBus) {
    while let Ok(_frame) = host_bus.recv().await {
        // Receive an Address Claim frame from the DUT and ignore it on purpose.
    }
}

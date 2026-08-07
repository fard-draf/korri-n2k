//! # Driving the claim engine without a runtime
//!
//! `AddressClaimEngine` is synchronous and does no I/O. It takes a millisecond
//! reading and an optional frame, and returns a [`ClaimOutput`]: what to emit,
//! where the state machine stands, and when to come back. No executor, no timer
//! trait, no `CanBus` implementation: a bare-metal main loop is enough.
//!
//! The three fields are independent. That is the whole contract.
//!
//! * Emit `tx` **before** acting on `status`. Otherwise a defence frame handed
//!   back on the acquiring poll is lost.
//! * `wake_at_ms` is an absolute deadline. It is an upper bound, not an order
//!   to sleep. A loop that idles until it without reading the bus misses the
//!   conflicts of that window.
//! * `wake_at_ms: None` means no timer is pending. It never means the engine is
//!   done. A frame must still be able to wake it.
//!
//! ```bash
//! cargo run --example blocking_claim --features std
//! ```

use korri_n2k::protocol::management::address_claiming::engine::{AddressClaimEngine, ClaimStatus};
use korri_n2k::protocol::management::address_claiming::{
    build_address_claim_frame, AddressClaimStrategy,
};
use korri_n2k::protocol::management::iso_name::IsoName;
use korri_n2k::protocol::transport::can_frame::CanFrame;

/// Never idle longer than this. Your value comes from the rest of your loop:
/// a hardware FIFO poll, an interrupt flag, an existing scheduler tick.
const TICK_MS: u64 = 10;

/// Stands in for a driver plus a clock. Frames appear at the millisecond they
/// are scheduled for, which makes the run reproducible.
struct ScriptedBus {
    incoming: Vec<(u64, CanFrame)>,
    sent: Vec<(u64, CanFrame)>,
}

impl ScriptedBus {
    fn new(incoming: Vec<(u64, CanFrame)>) -> Self {
        Self {
            incoming,
            sent: Vec::new(),
        }
    }

    /// Non-blocking: `None` when nothing is waiting. This is the shape the
    /// engine needs. A blocking read with no timeout would hang the loop on a
    /// silent bus, and the claim deadline would never be reached.
    fn try_recv(&mut self, now_ms: u64) -> Option<CanFrame> {
        let index = self.incoming.iter().position(|(at, _)| *at <= now_ms)?;
        Some(self.incoming.remove(index).1)
    }

    fn send(&mut self, now_ms: u64, frame: &CanFrame) {
        println!(
            "  {now_ms:>4} ms  ->  PGN {} from address {}",
            frame.id.pgn(),
            frame.id.source_address()
        );
        self.sent.push((now_ms, *frame));
    }
}

/// The whole driver: poll, emit, act on the status, advance. Nothing else is
/// required.
fn run(engine: &mut AddressClaimEngine, bus: &mut ScriptedBus) -> Option<u8> {
    let mut now_ms: u64 = 0;

    loop {
        let received = bus.try_recv(now_ms);
        let output = engine.poll(now_ms, received.as_ref());

        // First, always. A `Claimed` handed back with a defence frame still owes
        // that frame to the bus, and leaving on the status would drop it.
        if let Some(frame) = output.tx {
            bus.send(now_ms, &frame);
        }

        match output.status {
            ClaimStatus::Claimed(address) => return Some(address),
            // This example gives up here; a long-running node would instead wait
            // for `wake_at_ms` and let the engine retry the whole campaign.
            ClaimStatus::CannotClaim => return None,
            ClaimStatus::Claiming(_) => {}
        }

        // An absolute deadline, and an upper bound rather than a sleep order:
        // never idle past our own tick, otherwise a conflict arriving inside the
        // window is missed. `None` means no timer is pending, so we just tick.
        //
        // The lower bound of 1 is what keeps this loop honest: a deadline that
        // has already passed would otherwise advance the clock by zero forever.
        now_ms += match output.wake_at_ms {
            Some(deadline_ms) => deadline_ms.saturating_sub(now_ms).clamp(1, TICK_MS),
            None => TICK_MS,
        };
    }
}

fn main() {
    // The MSB carries the Arbitrary Address Capable bit, which `Arbitrary` needs.
    let my_name = IsoName::from_raw(0xF234_5678_90AB_CDEF);
    let strategy = AddressClaimStrategy::Arbitrary { preferred: 42 };

    println!("quiet bus:");
    let mut engine = AddressClaimEngine::new(my_name, strategy).expect("NAME must be AAC");
    let mut bus = ScriptedBus::new(Vec::new());
    println!("  claimed {:?}\n", run(&mut engine, &mut bus));

    println!("a lower NAME claims address 42 after 100 ms:");
    let rival = IsoName::from_raw(0x0234_5678_90AB_CDEF);
    assert!(rival < my_name, "a lower NAME wins the arbitration");

    let mut engine = AddressClaimEngine::new(my_name, strategy).expect("NAME must be AAC");
    let mut bus = ScriptedBus::new(vec![(100, build_address_claim_frame(rival, 42))]);
    println!("  claimed {:?}", run(&mut engine, &mut bus));
}

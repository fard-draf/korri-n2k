//! # Driving the claim engine without a runtime
//!
//! `AddressClaimEngine` is synchronous and does no I/O. It takes a millisecond
//! reading and an optional frame, and returns an action. No executor, no timer
//! trait, no `CanBus` implementation: a bare-metal main loop is enough.
//!
//! The one rule is on `Wait(n)`: it is an upper bound, not an order to sleep.
//! A loop that idles the full `n` without looking at the bus would miss the
//! conflicts of that window.
//!
//! ```bash
//! cargo run --example blocking_claim --features std
//! ```

use korri_n2k::protocol::constants::address::NULL_ADDR_254;
use korri_n2k::protocol::management::address_claiming::engine::{AddressClaimEngine, ClaimAction};
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
    /// engine needs — a blocking read with no timeout would hang the loop on a
    /// silent bus and the claim deadline would never be reached.
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

/// The whole driver: poll, act, advance. Nothing else is required.
fn run(engine: &mut AddressClaimEngine, bus: &mut ScriptedBus) -> Option<u8> {
    let mut now_ms: u64 = 0;

    loop {
        let received = bus.try_recv(now_ms);

        match engine.poll(now_ms, received.as_ref()) {
            ClaimAction::Send(frame) | ClaimAction::CannotClaim(frame) => {
                bus.send(now_ms, &frame);
                if frame.id.source_address() == NULL_ADDR_254 {
                    return None;
                }
            }
            ClaimAction::Claimed(address) => return Some(address),
            // Upper bound, not a sleep order: never idle past our own tick,
            // otherwise a conflict arriving inside the window is missed.
            ClaimAction::Wait(delay_ms) => now_ms += (delay_ms as u64).min(TICK_MS),
        }
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

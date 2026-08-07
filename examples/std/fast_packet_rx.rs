//! # Receiving a Fast Packet PGN
//!
//! Reception hands you raw frames. A PGN larger than 8 bytes arrives in
//! fragments and must be reassembled before decoding.
//!
//! Decoding the first fragment of a multi-frame message succeeds and returns
//! wrong values. Branch on `handles()`, never on the decode result.
//!
//! ```bash
//! cargo run --example fast_packet_rx
//! ```

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::{Pgn128267, Pgn129029};
use korri_n2k::protocol::transport::can_frame::CanFrame;
use korri_n2k::protocol::transport::can_id::CanId;
use korri_n2k::protocol::transport::fast_packet::assembler::{FastPacketAssembler, ProcessResult};
use korri_n2k::protocol::transport::fast_packet::builder::FastPacketBuilder;

fn main() {
    // `new()` covers the Fast Packet PGNs of your manifest.
    // A gateway forwarding what it cannot decode wants `with_pgns(FAST_PACKET_PGNS_ALL)`.
    let mut assembler = FastPacketAssembler::new();

    for frame in incoming_frames() {
        let pgn = frame.id.pgn();

        if !assembler.handles(pgn) {
            // Single frame: the payload is the eight bytes, decode directly.
            if pgn == 128267 {
                let depth = Pgn128267::from_payload(&frame.data).expect("valid Water Depth");
                println!("depth      {:.2} m", depth.depth);
            }
            continue;
        }

        // Fast Packet: feed every fragment, decode only what completes.
        let result = assembler.process_frame(now_ms(), pgn, frame.id.source_address(), &frame.data);

        if let ProcessResult::MessageComplete(msg) = result {
            if pgn == 129029 {
                let fix =
                    Pgn129029::from_payload(&msg.payload[..msg.len]).expect("valid GNSS Position");
                println!("position   {:.6} {:.6}", fix.latitude, fix.longitude);
            }
        }
    }
}

/// A millisecond clock. On a real node this is your `Clock` implementation;
/// the assembler only uses it to expire incomplete sessions.
fn now_ms() -> u32 {
    0
}

/// Stands in for the bus: one single-frame PGN, then a fragmented one.
fn incoming_frames() -> Vec<CanFrame> {
    let mut frames = vec![CanFrame {
        id: CanId::builder(128267, 42).build().expect("valid id"),
        data: [0x2A, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
        len: 8,
    }];

    // Serialize a GNSS fix, then fragment it exactly as transmission does.
    let mut fix = Pgn129029::new();
    fix.latitude = 47.7223;
    fix.longitude = -4.0022;

    let mut payload = [0u8; 233];
    let len = fix.to_payload(&mut payload).expect("serialization");

    let builder = FastPacketBuilder::new(129029, 42, None, &payload[..len]);
    frames.extend(builder.build().map(|frame| frame.expect("valid fragment")));

    frames
}

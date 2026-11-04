//! # Passive Bus Monitor
//!
//! Decodes any received PGN without knowing its Rust type at compile time,
//! using runtime descriptors (`descriptor_for`), the field visitor
//! (`for_each_field`), and CANboat lookup labels (`lookup_name`).
//! Never transmits.
//!
//! ```bash
//! cargo run --example monitor --features std
//! ```

use korri_n2k::core::{Availability, PgnValue};
use korri_n2k::infra::codec::engine::{for_each_field, FieldReading};
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::lookups::lookup_name;
use korri_n2k::protocol::messages::{descriptor_for, Pgn129029};
use korri_n2k::protocol::transport::can_frame::CanFrame;
use korri_n2k::protocol::transport::can_id::CanId;
use korri_n2k::protocol::transport::fast_packet::assembler::{FastPacketAssembler, ProcessResult};
use korri_n2k::protocol::transport::fast_packet::builder::FastPacketBuilder;

fn main() {
    let mut assembler = FastPacketAssembler::new();

    for frame in incoming_frames() {
        let pgn = frame.id.pgn();
        let src = frame.id.source_address();

        if assembler.handles(pgn) {
            if let ProcessResult::MessageComplete(msg) =
                assembler.process_frame(0, pgn, src, &frame.data)
            {
                print_pgn(pgn, src, &msg.payload[..msg.len]);
            }
        } else {
            print_pgn(pgn, src, &frame.data[..frame.len]);
        }
    }
}

fn print_pgn(pgn: u32, src: u8, payload: &[u8]) {
    let Some(desc) = descriptor_for(pgn, payload) else {
        println!("PGN {pgn} (src {src}): unknown or unmatched variant\n");
        return;
    };

    println!("PGN {} ({}) — {}", desc.id, desc.name, desc.description);
    for_each_field(desc, payload, |field, reading| {
        let prefix = match reading.repetition {
            Some((group, idx)) => format!("  {group}[{idx}].{}", field.name),
            None => format!("  {}", field.name),
        };
        println!(
            "{prefix} = {}",
            format_reading(field.enum_direct_name, field.physical_unit, &reading)
        );
    })
    .expect("valid payload");
    println!();
}

fn format_reading(
    enum_direct: Option<&'static str>,
    unit: Option<&'static str>,
    reading: &FieldReading,
) -> String {
    match reading.availability {
        Availability::NotAvailable => return "N/A".to_string(),
        Availability::OutOfRange => return "out of range".to_string(),
        Availability::Reserved => return "reserved".to_string(),
        Availability::Available => {}
    }

    if let (Some(enum_name), Some(raw)) = (enum_direct, reading.raw) {
        if let Some(label) = lookup_name(enum_name, raw as u32) {
            return format!("{label} ({raw})");
        }
    }

    let unit_suffix = unit.map(|u| format!(" {u}")).unwrap_or_default();
    match reading.value {
        PgnValue::F32(v) => format!("{v:.4}{unit_suffix}"),
        PgnValue::F64(v) => format!("{v:.6}{unit_suffix}"),
        PgnValue::U8(v) => format!("{v}{unit_suffix}"),
        PgnValue::U16(v) => format!("{v}{unit_suffix}"),
        PgnValue::U32(v) => format!("{v}{unit_suffix}"),
        PgnValue::U64(v) => format!("{v}{unit_suffix}"),
        PgnValue::I8(v) => format!("{v}{unit_suffix}"),
        PgnValue::I16(v) => format!("{v}{unit_suffix}"),
        PgnValue::I32(v) => format!("{v}{unit_suffix}"),
        PgnValue::I64(v) => format!("{v}{unit_suffix}"),
        PgnValue::Bytes(ref b) => format!("{:?}", b.as_slice()),
        PgnValue::Ignored => "ignored".to_string(),
    }
}

/// Synthetic frames: Water Depth (128267, trailing Range = N/A), GNSS Position
/// Data (129029, Fast Packet), and ISO TP Connection Management RTS (60416).
fn incoming_frames() -> Vec<CanFrame> {
    let mut frames = vec![CanFrame {
        id: CanId::builder(128267, 42).build().expect("valid id"),
        // SID = 1, Depth = 5.20 m (520), Offset = -0.30 m (-300), Range = 0xFF (N/A)
        data: [0x01, 0x08, 0x02, 0x00, 0x00, 0xD4, 0xFE, 0xFF],
        len: 8,
    }];

    // Fast Packet 129029 (GNSS Position Data)
    let mut fix = Pgn129029::new();
    fix.sid = 7;
    fix.date = 19800;
    fix.time = 43200.0;
    fix.latitude = 47.7223;
    fix.longitude = -4.0022;
    fix.reference_stations = 0xFF; // N/A -> 0 station repetitions

    let mut payload = [0xFFu8; 64];
    let len = fix.to_payload(&mut payload).expect("serialize 129029");
    let builder = FastPacketBuilder::new(129029, 42, None, &payload[..len]);
    frames.extend(builder.build().map(|f| f.expect("fragment")));

    // Polymorphic 60416: Group Function Code = 16 (RTS), 18 bytes in 3 packets, PGN 126996
    frames.push(CanFrame {
        id: CanId::builder(60416, 10)
            .destination(255)
            .build()
            .expect("valid id"),
        data: [16, 18, 0, 3, 0xFF, 0x14, 0xF0, 0x01],
        len: 8,
    });

    frames
}

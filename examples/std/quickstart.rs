//! # Quickstart Example
//!
//! Minimal example demonstrating the basics of korri-n2k:
//! - Build an ISO Name
//! - Create and serialize PGN messages
//! - Deserialize incoming frames
//!
//! This example uses `std` for a quick trial run.
//! For a full embedded example, see `quickstart_nostd.rs`.
//!
//! ```bash
//! cargo run --example quickstart
//! ```

use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::managment::iso_name::IsoName;
use korri_n2k::protocol::messages::{Pgn128267, Pgn129025, Pgn60928};
use korri_n2k::protocol::transport::can_id::CanId;

fn main() {
    println!("=== korri-n2k Quickstart ===\n");

    // ======================================================================
    // 1. Create an ISO Name identity
    // ======================================================================
    println!("1. Building an ISO Name");

    let iso_name = IsoName::builder()
        .unique_number(12345) // Unique serial number
        .manufacturer_code(229) // Manufacturer code (e.g. Garmin)
        .device_function(145) // Function: GPS
        .device_class(75) // Class: Navigation
        .industry_group(4) // Group: Marine
        .arbitrary_address_capable(true) // Eligible for arbitrary address selection
        .build();

    println!("   ISO Name: {}", iso_name);
    println!("   Manufacturer: {}", iso_name.manufacturer_code());
    println!("   Function: {}", iso_name.device_function());
    println!("   Marine: {}\n", iso_name.is_marine());

    // ======================================================================
    // 2. Create and serialize a GPS position message (PGN 129025)
    // ======================================================================
    println!("2. Building a GPS position message (PGN 129025)");

    let mut position = Pgn129025::new();
    position.latitude = 47.7223; // Latitude in decimal degrees
    position.longitude = -4.0022; // Longitude in decimal degrees

    println!(
        "   Position: {:.4}°N, {:.4}°W",
        position.latitude,
        position.longitude.abs()
    );

    // Serialize into a binary payload
    let mut buffer = [0u8; 64];
    match position.to_payload(&mut buffer) {
        Ok(len) => {
            println!("   Serialized: {} bytes", len);
            print!("   Payload: ");
            for byte in &buffer[..len] {
                print!("{:02X} ", byte);
            }
            println!("\n");
        }
        Err(e) => {
            eprintln!("   Serialization error: {:?}\n", e);
        }
    }

    // ======================================================================
    // 3. Deserialize a message
    // ======================================================================
    println!("3. Deserializing a depth message (PGN 128267)");

    // Example payload: depth 5.2 m, sensor offset 0.5 m (NMEA 2000 format)
    let depth_payload = [
        0x01, // SID
        0x48, 0x14, 0x00, 0x00, // Depth: 5200 mm (little-endian)
        0xF4, 0x01, 0x00, 0x00, // Offset: 500 mm
    ];

    match Pgn128267::from_payload(&depth_payload) {
        Ok(depth) => {
            println!("   SID: {}", depth.sid);
            println!("   Depth: {:.2} m", depth.depth);
            println!("   Transducer offset: {:.2} m\n", depth.offset);
        }
        Err(e) => {
            eprintln!("   Deserialization error: {:?}\n", e);
        }
    }

    // ======================================================================
    // 4. Convert between ISO Name and PGN 60928
    // ======================================================================
    println!("4. ISO Name <-> PGN 60928 (Address Claim)");

    let pgn60928: Pgn60928 = iso_name.into();
    println!("   PGN60928 created from ISO Name");
    println!("   Unique number: {}", pgn60928.unique_number);

    let iso_name_restored: IsoName = pgn60928.into();
    println!("   ISO Name restored from PGN60928");
    println!("   Match: {}\n", iso_name.raw() == iso_name_restored.raw());

    // ======================================================================
    // 5. Build a complete CAN ID
    // ======================================================================
    println!("5. Building a CAN ID");

    let can_id = CanId::builder(129025, 42) // PGN and source address
        .with_priority(2) // Priority 2 (navigation)
        .build()
        .expect("valid CAN ID");

    println!("   CAN ID: 0x{:08X}", can_id.0);
    println!("   Priority: {}", can_id.priority());
    println!("   PGN: {}", can_id.pgn());
    println!("   Source: {}", can_id.source_address());
    println!("   Destination: {:?}\n", can_id.destination());

    // ======================================================================
    println!("Quickstart complete.");
    println!("\nFull documentation:");
    println!("  https://docs.rs/korri-n2k");
}

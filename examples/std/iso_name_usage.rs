//! `IsoName` usage example showing how to manipulate the ISO 11783 NAME field
//! used during the NMEA 2000 address claim procedure.

use korri_n2k::protocol::{
    lookups::{DeviceClass, IndustryCode, ManufacturerCode, YesNo},
    managment::{address_claiming::build_address_claim_frame, iso_name::IsoName},
    messages::Pgn60928,
    transport::can_frame::CanFrame,
};

fn main() {
    println!("=== IsoName Usage Example ===\n");

    // Example 1: build via the fluent builder.
    println!("1. Building an IsoName with the builder:");
    let name = IsoName::builder()
        .unique_number(123456)
        .manufacturer_code(273) // Actisense
        .device_function(130) // Diagnostic Tool
        .device_class(25) // Inter/Intranetwork Device
        .device_instance(1)
        .system_instance(0)
        .industry_group(4) // Marine
        .arbitrary_address_capable(true)
        .build();

    println!("  Created: {}", name);
    println!("  Raw value: 0x{:016X}", name.raw());
    println!("  Is Marine: {}", name.is_marine());
    println!(
        "  Can claim arbitrary address: {}\n",
        name.is_arbitrary_address_capable()
    );

    // Example 2: read individual fields.
    println!("2. Reading fields:");
    println!("  Unique Number: {}", name.unique_number());
    println!("  Manufacturer Code: {}", name.manufacturer_code());
    println!("  Device Function: {}", name.device_function());
    println!("  Device Class: {}", name.device_class());
    println!("  Device Instance: {}", name.device_instance());
    println!("  System Instance: {}", name.system_instance());
    println!("  Industry Group: {}\n", name.industry_group());

    // Example 3: conversion from a raw `u64`.
    println!("3. Building from a raw u64:");
    let raw_value = 0x8000_0000_0000_0000u64;
    let name_from_raw = IsoName::from_raw(raw_value);
    println!("  Raw: 0x{:016X}", raw_value);
    println!("  IsoName: {}", name_from_raw);
    println!(
        "  AAC bit set: {}\n",
        name_from_raw.is_arbitrary_address_capable()
    );

    // Example 4: conversions between IsoName and PGN 60928.
    println!("4. Converting between IsoName and Pgn60928:");
    let mut pgn = Pgn60928::new();
    pgn.unique_number = 999888;
    pgn.manufacturer_code = ManufacturerCode::Actisense;
    pgn.device_function = 255;
    pgn.device_class = DeviceClass::InternetworkDevice;
    pgn.industry_group = IndustryCode::MarineIndustry;
    pgn.arbitrary_address_capable = YesNo::Yes;

    println!(
        "  Created Pgn60928 with unique_number: {}",
        pgn.unique_number
    );

    let iso_from_pgn: IsoName = pgn.into();
    println!("  Converted to IsoName: {}", iso_from_pgn);
    println!(
        "  Unique number matches: {}",
        iso_from_pgn.unique_number() == 999888
    );

    let pgn_from_iso: Pgn60928 = iso_from_pgn.into();
    println!("  Converted back to Pgn60928");
    println!(
        "  Round-trip successful: {}\n",
        pgn_from_iso.unique_number == 999888
    );

    // Example 5: prepare a broadcast-ready Address Claim frame.
    println!("5. Building an Address Claim frame:");
    let claim_frame: CanFrame =
        build_address_claim_frame(name.raw(), /* preferred address */ 37).expect("frame build");
    println!("  CAN ID: 0x{:08X}", claim_frame.id.0);
    println!("  Source address: {}", claim_frame.id.source_address());
    println!(
        "  Destination (broadcast): {:?}",
        claim_frame.id.destination()
    );
    println!(
        "  Payload bytes: {:02X?}\n",
        &claim_frame.data[..claim_frame.len]
    );

    // Example 6: compatibility with the existing address-claim logic.
    println!("6. Compatibility with address management:");
    let my_name_raw = 0x8123_4567_89AB_CDEFu64;
    let iso_name = IsoName::from_raw(my_name_raw);

    // Legacy method (from address_claiming/mod.rs)
    let is_aac_old = (my_name_raw >> 63) & 1 == 1;
    // New method using the typed API.
    let is_aac_new = iso_name.is_arbitrary_address_capable();

    println!("  Raw NAME: 0x{:016X}", my_name_raw);
    println!("  Old method AAC: {}", is_aac_old);
    println!("  New method AAC: {}", is_aac_new);
    println!("  Methods match: {}\n", is_aac_old == is_aac_new);

    println!("=== Example completed successfully ===");
}

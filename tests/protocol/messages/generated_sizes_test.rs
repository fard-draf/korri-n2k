use core::mem::size_of;

use korri_n2k::protocol::{
    lookups::{IndustryCode, IsoCommand, ManufacturerCode},
    messages::{
        Pgn129025, Pgn59904, Pgn60160,
        Pgn60416IsoTransportProtocolConnectionManagementRequestToSend, Pgn60928,
    },
    transport::{can_frame::CanFrame, can_id::CanId},
};

// This test ensures code generation preserves the compactness of critical structures.
// It focuses on five common N2K PGNs plus the lookup enumerations they rely on.
#[test]
fn generated_artifacts_memory_footprint_is_stable() {
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║          MEMORY FOOTPRINT OF GENERATED STRUCTURES (no_std)          ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    // Check each PGN structure: tuple = (label, measured size, expected size).
    let pgn_expectations = [
        ("PGN 59904 (ISO Request)", size_of::<Pgn59904>(), 4usize),
        (
            "PGN 60160 (TP Data Transfer)",
            size_of::<Pgn60160>(),
            8usize,
        ),
        (
            "PGN 60416 (TP Conn. Mgmt - RTS)",
            size_of::<Pgn60416IsoTransportProtocolConnectionManagementRequestToSend>(),
            12usize,
        ),
        (
            "PGN 60928 (ISO Address Claim)",
            size_of::<Pgn60928>(),
            16usize,
        ),
        (
            "PGN 129025 (Position Rapid Update)",
            size_of::<Pgn129025>(),
            8usize,
        ),
    ];

    println!("PGN STRUCTURES (Protocol Data Units):");
    println!("─────────────────────────────────────────────────────────────────────");

    let mut total_pgn_size = 0;
    for (label, measured, expected) in pgn_expectations {
        let status = if measured == expected { "✅" } else { "❌" };
        let delta = if measured > expected {
            format!("+{} B", measured - expected)
        } else if measured < expected {
            format!("-{} B", expected - measured)
        } else {
            "OK".to_string()
        };

        println!(
            "  {} {:45} : {:3} B  [expected: {:3} B] ({})",
            status, label, measured, expected, delta
        );
        total_pgn_size += measured;

        assert_eq!(
            measured, expected,
            "The in-memory size of {label} drifts ({measured} B observed vs {expected} B expected)."
        );
    }

    println!("─────────────────────────────────────────────────────────────────────");
    println!("Total PGNs : {} bytes\n", total_pgn_size);

    // The corresponding lookup tables must remain minimal.
    let lookup_expectations = [
        ("IsoCommand", size_of::<IsoCommand>(), 1usize),
        ("IndustryCode", size_of::<IndustryCode>(), 1usize),
        ("ManufacturerCode", size_of::<ManufacturerCode>(), 2usize),
    ];

    println!("LOOKUP ENUMS (Mapping tables):");
    println!("─────────────────────────────────────────────────────────────────────");

    let mut total_lookup_size = 0;
    for (label, measured, expected) in lookup_expectations {
        let status = if measured == expected { "✅" } else { "❌" };
        println!(
            "  {} {:45} : {:3} B  [expected: {:3} B]",
            status, label, measured, expected
        );
        total_lookup_size += measured;

        assert_eq!(
            measured, expected,
            "Lookup {label} occupies {measured} B instead of the expected {expected} B."
        );
    }

    println!("─────────────────────────────────────────────────────────────────────");
    println!("Total Lookups : {} bytes\n", total_lookup_size);

    // Finally, the classic N2K frame layout should not exceed the strict minimum by
    // more than one machine word (alignment included).
    let frame_size = size_of::<CanFrame>();
    let can_id_size = size_of::<CanId>();
    let baseline = can_id_size + 8 + size_of::<usize>();
    let alignment_budget = size_of::<usize>();
    let max_allowed = baseline + alignment_budget;

    println!("CAN INFRASTRUCTURE (Low-level transport):");
    println!("─────────────────────────────────────────────────────────────────────");
    println!(
        "     CanId (29-bit identifier)              : {:3} B",
        can_id_size
    );
    println!(
        "     CanFrame (complete frame)              : {:3} B",
        frame_size
    );
    println!(
        "     ├─ Baseline theoretical size           : {:3} B",
        baseline
    );
    println!(
        "     ├─ Alignment budget                    : {:3} B",
        alignment_budget
    );
    println!(
        "     └─ Maximum allowed                     : {:3} B",
        max_allowed
    );

    let overhead = if frame_size > baseline {
        frame_size - baseline
    } else {
        0
    };
    let overhead_pct = (overhead as f32 / baseline as f32) * 100.0;

    println!(
        "     Alignment overhead                     : {:3} B ({:.1}%)",
        overhead, overhead_pct
    );
    println!("─────────────────────────────────────────────────────────────────────\n");

    assert!(
        frame_size >= baseline,
        "CanFrame cannot be smaller than its logical content ({frame_size} B vs {baseline} B)."
    );
    assert!(
        frame_size <= max_allowed,
        "CanFrame exceeds the permitted envelope ({frame_size} B for a maximum of {max_allowed} B)."
    );

    // Compute total footprint for an embedded target
    let total_memory = total_pgn_size + total_lookup_size + frame_size;

    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    TOTAL MEMORY FOOTPRINT SUMMARY                    ║");
    println!("╠══════════════════════════════════════════════════════════════════════╣");
    println!(
        "║  PGNs tested (5 structures)              : {:6} bytes ({:5.2} KB)   ║",
        total_pgn_size,
        total_pgn_size as f32 / 1024.0
    );
    println!(
        "║  Lookups tested (3 enums)                : {:6} bytes ({:5.2} KB)   ║",
        total_lookup_size,
        total_lookup_size as f32 / 1024.0
    );
    println!(
        "║  CAN infrastructure (1 frame)            : {:6} bytes ({:5.2} KB)   ║",
        frame_size,
        frame_size as f32 / 1024.0
    );
    println!("╠══════════════════════════════════════════════════════════════════════╣");
    println!(
        "║  TOTAL SAMPLE SIZE                       : {:6} bytes ({:5.2} KB)   ║",
        total_memory,
        total_memory as f32 / 1024.0
    );
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    println!("✅ All structures meet the memory constraints for embedded targets");
    println!("   (typical MCUs: 8–512 KB RAM; these structs are negligible)\n");
}

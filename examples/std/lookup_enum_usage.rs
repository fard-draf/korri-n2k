use korri_n2k::protocol::lookups::*;
use korri_n2k::protocol::messages::*;

fn main() {
    println!("=== Test LOOKUP direct enum assignment ===\n");

    // Test PGN 126993 (Heartbeat) – direct LOOKUP
    let mut heartbeat_pgn = Pgn126993::new();
    println!(
        "Initial equipment_status: {:?}",
        heartbeat_pgn.equipment_status
    );

    // Direct assignment using the enum – previously impossible!
    heartbeat_pgn.equipment_status = EquipmentStatus::Operational;
    heartbeat_pgn.controller1_state = ControllerState::ErrorActive;
    heartbeat_pgn.controller2_state = ControllerState::ErrorPassive;

    println!("After assignment:");
    println!("  equipment_status: {:?}", heartbeat_pgn.equipment_status);
    println!("  controller1_state: {:?}", heartbeat_pgn.controller1_state);
    println!("  controller2_state: {:?}", heartbeat_pgn.controller2_state);

    println!("\n=== Test PGN 60928 (ISO Address Claim) ===\n");
    let mut address_claim = Pgn60928::new();

    // Direct LOOKUP – assign using the enum
    address_claim.manufacturer_code = ManufacturerCode::ArksEnterprisesInc;
    address_claim.device_class = DeviceClass::SteeringAndControlSurfaces;
    address_claim.industry_group = IndustryCode::MarineIndustry;
    address_claim.arbitrary_address_capable = YesNo::Yes;

    println!("manufacturer_code: {:?}", address_claim.manufacturer_code);
    println!("device_class: {:?}", address_claim.device_class);

    // INDIRECT_LOOKUP – use helper accessors
    address_claim.set_device_function(DeviceFunction::Diagnostic);

    if let Some(device_fn) = address_claim.get_device_function() {
        println!("device_function (via helper): {:?}", device_fn);
    }

    println!("\n✅ All enum assignments work correctly!");
}

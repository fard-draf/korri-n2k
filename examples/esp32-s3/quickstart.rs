//! # Quickstart ESP32-S3 Example
//!
//! Complete ESP32-S3 example using Embassy and esp-hal.
//!
//! ## Compilation
//! ```bash
//! # Install espup if needed
//! cargo install espup
//! espup install
//!
//! # Compiler
//! cargo build --example esp32s3_quickstart --target xtensa-esp32s3-none-elf --features embedded-examples
//!
//! # Flash
//! cargo run --example esp32s3_quickstart --target xtensa-esp32s3-none-elf --features embedded-examples
//! ```
//!
//! ## Required hardware
//! - ESP32-S3 DevKit
//! - CAN transceiver (e.g. SN65HVD230)
//! - Connections:
//!   - GPIO17 → CAN TX (green)
//!   - GPIO18 → CAN RX (blue)
//!   - GPIO2 → LED (optional)

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    timer::timg::TimerGroup,
    twai::{BaudRate, TwaiMode},
};
use static_cell::StaticCell;

// ============================================================================
// korri-n2k imports
// ============================================================================

use korri_n2k::{
    infra::codec::traits::PgnData,
    protocol::{
        managment::{address_manager::AddressManager, iso_name::IsoName},
        messages::Pgn129025,
        transport::{can_frame::CanFrame, can_id::CanId, traits::pgn_sender::PgnSender},
    },
};

// ============================================================================
// CanBus implementation for ESP32-S3
// ============================================================================

use embedded_can::{Frame, Id};
use esp_hal::{
    twai::{EspTwaiError, EspTwaiFrame, ExtendedId as EspExtendedId, Twai},
    Async,
};
use korri_n2k::protocol::transport::traits::can_bus::CanBus;

pub struct EspCanBus<'d> {
    can: Twai<'d, Async>,
}

impl<'d> EspCanBus<'d> {
    pub fn new(can: Twai<'d, Async>) -> Self {
        Self { can }
    }
}

impl<'d> CanBus for EspCanBus<'d> {
    type Error = EspTwaiError;

    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
        let ext_id = EspExtendedId::new(frame.id.0).unwrap();
        let twai_frame = EspTwaiFrame::new(ext_id, &frame.data[..frame.len]).unwrap();
        self.can.transmit_async(&twai_frame).await
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        let frame = self.can.receive_async().await?;

        let id = match frame.id() {
            Id::Standard(_) => return Err(EspTwaiError::BusOff),
            Id::Extended(ext) => ext.as_raw(),
        };

        let mut data = [0u8; 8];
        let len = frame.dlc();
        data[..len].copy_from_slice(frame.data());

        Ok(CanFrame {
            id: CanId(id),
            data,
            len,
        })
    }
}

// ============================================================================
// Timer implementation for ESP32-S3
// ============================================================================

use korri_n2k::protocol::transport::traits::korri_timer::KorriTimer;

pub struct EspTimer;

impl KorriTimer for EspTimer {
    async fn delay_ms(&mut self, millis: u32) {
        embassy_time::Timer::after(Duration::from_millis(millis as u64)).await;
    }
}

// ============================================================================
// Main application
// ============================================================================

esp_bootloader_esp_idf::esp_app_desc!();

type AddressManagerType = AddressManager<EspCanBus<'static>, EspTimer>;

static MANAGER_CELL: StaticCell<Mutex<CriticalSectionRawMutex, AddressManagerType>> =
    StaticCell::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::println!("=== ESP32-S3 NMEA2000 Quickstart ===");

    // 1. Initialize ESP32-S3
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // 2. Initialize Embassy with the timer
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    // 3. Configure LED (optional visual indicator)
    let mut led = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    led.set_high();
    embassy_time::Timer::after(Duration::from_millis(500)).await;
    led.set_low();

    // 4. Configure the CAN bus (250 kbps = NMEA2000 standard)
    let can_config = esp_hal::twai::TwaiConfiguration::new(
        peripherals.TWAI0,
        peripherals.GPIO18, // RX (blue)
        peripherals.GPIO17, // TX (green)
        BaudRate::B250K,
        TwaiMode::Normal,
    )
    .into_async();

    let can_peripheral = can_config.start();
    let can_bus = EspCanBus::new(can_peripheral);
    let timer = EspTimer;

    // 5. Build the ISO Name identity for this device
    let iso_name = IsoName::builder()
        .unique_number(12345) // Device serial number
        .manufacturer_code(229) // Manufacturer code (e.g. 229 = Garmin)
        .device_function(145) // Function: GPS
        .device_class(75) // Class: Navigation
        .industry_group(4) // Group: Marine
        .arbitrary_address_capable(true) // Eligible for arbitrary addresses
        .build();

    esp_println::println!("ISO Name: 0x{:016X}", iso_name.raw());

    // 6. Create the AddressManager (automatic address claim)
    let manager = match AddressManager::new(can_bus, timer, iso_name.raw(), 42).await {
        Ok(mgr) => {
            esp_println::println!("✓ Address claimed: {}", mgr.current_address());
            mgr
        }
        Err(_) => {
            esp_println::println!("✗ Failed to claim address!");
            loop {
                embassy_time::Timer::after(Duration::from_secs(1)).await;
            }
        }
    };

    // 7. Share the manager between tasks via a mutex
    let manager_mutex = MANAGER_CELL.init(Mutex::new(manager));

    // 8. Spawn tasks in parallel
    spawner.spawn(task_send_position(manager_mutex)).unwrap();
    spawner.spawn(task_heartbeat(manager_mutex)).unwrap();
    spawner.spawn(task_led_blink(led)).unwrap();

    esp_println::println!("✓ All tasks started");

    // Main loop
    loop {
        embassy_time::Timer::after(Duration::from_secs(10)).await;
        esp_println::println!("Main loop alive");
    }
}

// ============================================================================
// Application tasks
// ============================================================================

/// Periodically send GPS positions every second
#[embassy_executor::task]
async fn task_send_position(manager: &'static Mutex<CriticalSectionRawMutex, AddressManagerType>) {
    let mut ticker = Ticker::every(Duration::from_secs(1));

    loop {
        ticker.next().await;

        // Create a GPS position message (PGN 129025)
        let mut position = Pgn129025::new();
        position.latitude = 47.7223; // Latitude Brest, France
        position.longitude = -4.0022; // Longitude Brest, France

        // Send with priority 2 (navigation data)
        let mut mgr = manager.lock().await;
        match mgr.send_pgn(&position, 2).await {
            Ok(_) => {
                // esp_println::println!("→ Position sent: {:.4}, {:.4}", position.latitude, position.longitude);
            }
            Err(_) => {
                esp_println::println!("✗ Failed to send position");
            }
        }
    }
}

/// Heartbeat task (every 60 seconds)
#[embassy_executor::task]
async fn task_heartbeat(manager: &'static Mutex<CriticalSectionRawMutex, AddressManagerType>) {
    use korri_n2k::protocol::messages::Pgn126993;

    let mut ticker = Ticker::every(Duration::from_secs(60));

    loop {
        ticker.next().await;

        let heartbeat = Pgn126993::new();

        let mut mgr = manager.lock().await;
        match mgr.send_pgn(&heartbeat, 7).await {
            Ok(_) => {
                esp_println::println!("→ Heartbeat sent");
            }
            Err(_) => {
                esp_println::println!("✗ Failed to send heartbeat");
            }
        }
    }
}

/// LED blink task (visual indicator)
#[embassy_executor::task]
async fn task_led_blink(mut led: Output<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(1000));

    loop {
        ticker.next().await;
        led.toggle();
    }
}

// ============================================================================
// Panic handler
// ============================================================================

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {:?}", info);
    loop {
        for _ in 0..10_000_000 {
            core::sync::atomic::spin_loop_hint();
        }
    }
}

//! # Quickstart STM32 Example
//!
//! Complete STM32 (ARM Cortex-M) example using Embassy.
//!
//! ## Compilation
//! ```bash
//! # For STM32F4
//! cargo build --example stm32_quickstart --target thumbv7em-none-eabihf --features embedded-examples
//!
//! # For STM32H7
//! cargo build --example stm32_quickstart --target thumbv7em-none-eabihf --features embedded-examples
//!
//! # Flash with probe-rs
//! cargo run --example stm32_quickstart --target thumbv7em-none-eabihf --features embedded-examples
//! ```
//!
//! ## Required hardware
//! - STM32F4/H7 board with CAN
//! - CAN transceiver (e.g. TJA1050)
//! - Connections (example for STM32F407):
//!   - PA11 → CAN RX
//!   - PA12 → CAN TX
//!   - PC13 → LED (optional)
//!
//! ## Important note
//! This example is a generic template. To use it:
//! 1. Add the STM32-specific dependencies to Cargo.toml:
//!    ```toml
//!    [dev-dependencies]
//!    embassy-stm32 = { version = "0.x", features = ["stm32f407vg", "time-driver-any"] }
//!    embassy-executor = { version = "0.7", features = ["arch-cortex-m", "executor-thread"] }
//!    ```
//! 2. Adapt the pins to your board
//! 3. Configure the appropriate linker script

#![no_std]
#![no_main]

// NOTE: Uncomment these imports once the dependencies are added
// use embassy_executor::Spawner;
// use embassy_stm32::{
//     bind_interrupts,
//     can::{Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler},
//     peripherals,
//     Config as StmConfig,
// };
// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
// use embassy_sync::mutex::Mutex;
// use embassy_time::{Duration, Ticker};
// use static_cell::StaticCell;

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
// CAN configuration for STM32
// ============================================================================

// NOTE: Uncomment and adapt for your STM32
// bind_interrupts!(struct Irqs {
//     CAN1_RX0 => Rx0InterruptHandler<peripherals::CAN1>;
//     CAN1_RX1 => Rx1InterruptHandler<peripherals::CAN1>;
//     CAN1_SCE => SceInterruptHandler<peripherals::CAN1>;
//     CAN1_TX => TxInterruptHandler<peripherals::CAN1>;
// });

// ============================================================================
// CanBus implementation for STM32
// ============================================================================

use korri_n2k::protocol::transport::traits::can_bus::CanBus;

// NOTE: Example implementation (adapt to your HAL)
// pub struct Stm32CanBus<'d> {
//     can: Can<'d>,
// }

// impl<'d> Stm32CanBus<'d> {
//     pub fn new(can: Can<'d>) -> Self {
//         Self { can }
//     }
// }

// impl<'d> CanBus for Stm32CanBus<'d> {
//     type Error = embassy_stm32::can::BusError;
//
//     async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
//         use embassy_stm32::can::{Envelope, ExtendedId, Frame as StmFrame};
//
//         let ext_id = ExtendedId::new(frame.id.0).unwrap();
//         let stm_frame = StmFrame::new_extended(ext_id, &frame.data[..frame.len]).unwrap();
//         let envelope = Envelope { frame: stm_frame };
//
//         self.can.write(&envelope).await
//     }
//
//     async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
//         let envelope = self.can.read().await?;
//         let stm_frame = envelope.frame;
//
//         let id = match stm_frame.id() {
//             embassy_stm32::can::Id::Standard(_) => {
//                 return Err(embassy_stm32::can::BusError::Stuff)
//             }
//             embassy_stm32::can::Id::Extended(ext) => ext.as_raw(),
//         };
//
//         let mut data = [0u8; 8];
//         let len = stm_frame.data().len();
//         data[..len].copy_from_slice(stm_frame.data());
//
//         Ok(CanFrame {
//             id: CanId(id),
//             data,
//             len,
//         })
//     }
// }

// ============================================================================
// Timer implementation for STM32
// ============================================================================

use korri_n2k::protocol::transport::traits::korri_timer::KorriTimer;

pub struct Stm32Timer;

impl KorriTimer for Stm32Timer {
    async fn delay_ms(&mut self, millis: u32) {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(millis as u64)).await;
    }
}

// ============================================================================
// Main application
// ============================================================================

// NOTE: Type alias to uncomment once the implementation is complete
// type AddressManagerType = AddressManager<Stm32CanBus<'static>, Stm32Timer>;
// static MANAGER_CELL: StaticCell<Mutex<CriticalSectionRawMutex, AddressManagerType>> =
//     StaticCell::new();

// NOTE: Uncomment and adapt `main` for your STM32
// #[embassy_executor::main]
// async fn main(spawner: Spawner) {
//     defmt::info!("=== STM32 NMEA2000 Quickstart ===");
//
//     // 1. Initialize STM32
//     let mut config = StmConfig::default();
//     // Configure the clock to match your board
//     let p = embassy_stm32::init(config);
//
//     // 2. Set up the LED (GPIO PC13 on STM32F407 Discovery)
//     let mut led = embassy_stm32::gpio::Output::new(
//         p.PC13,
//         embassy_stm32::gpio::Level::Low,
//         embassy_stm32::gpio::Speed::Low,
//     );
//     led.set_high();
//     embassy_time::Timer::after(Duration::from_millis(500)).await;
//     led.set_low();
//
//     // 3. Configure the CAN bus
//     let can = embassy_stm32::can::Can::new(
//         p.CAN1,
//         p.PA11, // RX
//         p.PA12, // TX
//         Irqs,
//     );
//
//     // CAN configuration: 250 kbps (NMEA2000 standard)
//     // Note: timing parameters depend on your clock
//     // For APB1 = 42 MHz and 250 kbps: prescaler=12, sjw=1, bs1=10, bs2=3
//     can.modify_config()
//         .set_bit_timing(12, 1, 10, 3)
//         .enable();
//
//     let can_bus = Stm32CanBus::new(can);
//     let timer = Stm32Timer;
//
//     // 4. Build the ISO Name identity
//     let iso_name = IsoName::builder()
//         .unique_number(11111)
//         .manufacturer_code(229)
//         .device_function(145)
//         .device_class(75)
//         .industry_group(4)
//         .arbitrary_address_capable(true)
//         .build();
//
//     defmt::info!("ISO Name: 0x{:016X}", iso_name.raw());
//
//     // 5. Create the AddressManager
//     let manager = match AddressManager::new(can_bus, timer, iso_name.raw(), 44).await {
//         Ok(mgr) => {
//             defmt::info!("✓ Address claimed: {}", mgr.current_address());
//             mgr
//         }
//         Err(_) => {
//             defmt::error!("✗ Failed to claim address!");
//             loop {
//                 embassy_time::Timer::after(Duration::from_secs(1)).await;
//             }
//         }
//     };
//
//     // 6. Share the manager
//     let manager_mutex = MANAGER_CELL.init(Mutex::new(manager));
//
//     // 7. Launch tasks
//     spawner.spawn(task_send_position(manager_mutex)).unwrap();
//     spawner.spawn(task_heartbeat(manager_mutex)).unwrap();
//     spawner.spawn(task_led_blink(led)).unwrap();
//
//     defmt::info!("✓ All tasks started");
//
//     // Main loop
//     loop {
//         embassy_time::Timer::after(Duration::from_secs(10)).await;
//         defmt::info!("Main loop alive");
//     }
// }

// ============================================================================
// Application tasks (templates to uncomment)
// ============================================================================

// #[embassy_executor::task]
// async fn task_send_position(manager: &'static Mutex<CriticalSectionRawMutex, AddressManagerType>) {
//     let mut ticker = Ticker::every(Duration::from_secs(1));
//
//     loop {
//         ticker.next().await;
//
//         let mut position = Pgn129025::new();
//         position.latitude = 51.5074;  // Latitude London, UK
//         position.longitude = -0.1278; // Longitude London, UK
//
//         let mut mgr = manager.lock().await;
//         match mgr.send_pgn(&position, 2).await {
//             Ok(_) => {
//                 // defmt::debug!("→ Position sent");
//             }
//             Err(_) => {
//                 defmt::error!("✗ Failed to send position");
//             }
//         }
//     }
// }

// #[embassy_executor::task]
// async fn task_heartbeat(manager: &'static Mutex<CriticalSectionRawMutex, AddressManagerType>) {
//     use korri_n2k::protocol::messages::Pgn126993;
//
//     let mut ticker = Ticker::every(Duration::from_secs(60));
//
//     loop {
//         ticker.next().await;
//
//         let heartbeat = Pgn126993::new();
//
//         let mut mgr = manager.lock().await;
//         match mgr.send_pgn(&heartbeat, 7).await {
//             Ok(_) => {
//                 defmt::info!("→ Heartbeat sent");
//             }
//             Err(_) => {
//                 defmt::error!("✗ Failed to send heartbeat");
//             }
//         }
//     }
// }

// #[embassy_executor::task]
// async fn task_led_blink(mut led: embassy_stm32::gpio::Output<'static>) {
//     let mut ticker = Ticker::every(Duration::from_millis(1000));
//
//     loop {
//         ticker.next().await;
//         led.toggle();
//     }
// }

// ============================================================================
// Panic handler
// ============================================================================

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // NOTE: Use defmt if available
    // defmt::error!("PANIC: {:?}", info);
    loop {
        core::hint::spin_loop();
    }
}

// ============================================================================
// Placeholder main pour compilation sans erreur
// ============================================================================

#[no_mangle]
pub extern "C" fn main() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

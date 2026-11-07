//! Firmware of the korri-n2k hardware-in-the-loop bench: one STM32G431 on `can0`.
//!
//! Each binary claims `DUT_ADDRESS`, runs one scenario, then ends the debug session
//! through a semihosting exit: 0 = scenario passed, 1 = failed. The host
//! (`../host`) reads that code from `probe-rs run` and checks the bus on its own.
//! Logs go through defmt over RTT.
//!
//! Keep the constants below in step with `../host/src/scenario.rs`.
#![no_std]

pub mod conf;
pub mod ports;
pub mod starter;

use conf::{CAN_RX_BUF_DEPTH, CAN_TX_BUF_DEPTH};
use cortex_m_semihosting::debug;
use defmt_rtt as _;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer, with_timeout};
use korri_n2k::protocol::management::{
    address_claiming::AddressClaimStrategy,
    address_manager::AddressManager,
    address_supervisor::{
        AddressFrames, AddressHandle, AddressRunner, AddressService, ClaimedAddress,
        SupervisorCommand,
    },
    iso_name::IsoName,
};
use ports::{Stm32CanBus, Stm32Timer};

/// Source address claimed by the device under test. 80 and 81 belong to the example
/// boards that share the bench bus.
pub const DUT_ADDRESS: u8 = 90;
/// Source address the host injects from (`rx_fast_packet`). Never claimed, never the DUT's.
pub const INJECTOR_ADDRESS: u8 = 91;
/// Engine Parameters, Dynamic: a 26-byte Fast Packet.
pub const PGN: u32 = 127_489;
pub const PAYLOAD_LEN: usize = 26;
/// Messages sent by `tx_fast_packet` and injected by the host for `rx_fast_packet`.
pub const MESSAGE_COUNT: u8 = 5;

const COMMAND_CAPACITY: usize = 4;
const FRAME_CAPACITY: usize = 32;
const CLAIM_TIMEOUT: Duration = Duration::from_secs(5);

pub type Runner = AddressRunner<
    'static,
    Stm32CanBus<'static, CAN_TX_BUF_DEPTH, CAN_RX_BUF_DEPTH>,
    Stm32Timer,
    COMMAND_CAPACITY,
    FRAME_CAPACITY,
>;

static COMMANDS: Channel<CriticalSectionRawMutex, SupervisorCommand, COMMAND_CAPACITY> =
    Channel::new();
static FRAMES: Channel<
    CriticalSectionRawMutex,
    korri_n2k::protocol::transport::can_frame::CanFrame,
    FRAME_CAPACITY,
> = Channel::new();
static CLAIMED: ClaimedAddress = ClaimedAddress::new();

/// Ends the session: 0 when `passed`, 1 otherwise. Without a debugger attached the
/// breakpoint behind the semihosting call faults, so this is a run-once test image.
pub fn exit(passed: bool) -> ! {
    if passed {
        defmt::info!("verdict: PASS");
        debug::exit(debug::EXIT_SUCCESS);
    } else {
        defmt::error!("verdict: FAIL");
        debug::exit(debug::EXIT_FAILURE);
    }
    loop {
        cortex_m::asm::wfi();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    defmt::error!("panic: {}", defmt::Display2Format(info));
    exit(false)
}

/// Engine hours sent by `tx_fast_packet`, starting at this value (offset 11, u32 LE).
/// Other fields keep `Pgn127489::new()` defaults: zero, except the reserved byte (FF).
pub const TX_HOURS_BASE: u32 = 1_000;

/// Payload the host injects as message `index` (`rx_fast_packet`).
pub fn injected_payload(index: u8) -> [u8; PAYLOAD_LEN] {
    core::array::from_fn(|k| (index as usize * 37 + k * 11 + 1) as u8)
}

/// Bring up the board, build the stack and return the pieces. The claim only starts
/// once `runner_task` runs.
pub fn bring_up() -> (
    Runner,
    AddressHandle<'static, COMMAND_CAPACITY>,
    AddressFrames<'static, FRAME_CAPACITY>,
) {
    let p = starter::init_bsp();
    let can = starter::init_can(p);

    // Fixed address: the NAME must not be arbitrary-address-capable.
    let iso_name = IsoName::builder()
        .unique_number(0x1AB0E1)
        .manufacturer_code(229)
        .device_function(145)
        .device_class(75)
        .device_instance(1)
        .system_instance(0)
        .industry_group(4)
        .arbitrary_address_capable(false)
        .build();
    let manager = AddressManager::new(
        Stm32CanBus::new(can),
        Stm32Timer,
        iso_name,
        AddressClaimStrategy::Fixed {
            preferred: DUT_ADDRESS,
        },
    )
    .expect("NAME and strategy agree");

    let parts = AddressService::<_, _, COMMAND_CAPACITY, FRAME_CAPACITY>::new(
        manager,
        Some(&COMMANDS),
        Some(&FRAMES),
        &CLAIMED,
    )
    .into_parts();
    (
        parts.runner,
        parts.handle.expect("command channel given"),
        parts.frames.expect("frame channel given"),
    )
}

#[embassy_executor::task]
pub async fn runner_task(runner: Runner) {
    // `drive` only returns on a bus error, which ends the scenario.
    let result = runner.drive().await;
    defmt::error!("address runner stopped: {}", defmt::Debug2Format(&result));
    exit(false)
}

/// Wait for the claim to succeed on `DUT_ADDRESS`; end the session as failed otherwise.
pub async fn wait_claimed(handle: &AddressHandle<'static, COMMAND_CAPACITY>) {
    let wait = async {
        loop {
            if handle.claimed_address() == Some(DUT_ADDRESS) {
                return;
            }
            Timer::after(Duration::from_millis(10)).await;
        }
    };
    if with_timeout(CLAIM_TIMEOUT, wait).await.is_err() {
        defmt::error!("address {=u8} not claimed in time", DUT_ADDRESS);
        exit(false);
    }
    defmt::info!("claimed address {=u8}", DUT_ADDRESS);
}

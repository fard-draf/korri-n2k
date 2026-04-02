<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="Logo" width="800">
</p>

![CI](https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg)

`korri-n2k` is a `no_std`, `no_alloc` implementation of the NMEA 2000 / ISO 11783 protocol stack for embedded Rust targets.

This crate is designed for industrial and marine applications requiring strict memory determinism (zero heap allocation) and high concurrency. It natively supports asynchronous execution (via `embassy`) and fully isolates the protocol logic from the underlying hardware.

## Core Architecture & Abstractions

The library is built upon four primary pillars, designed to separate concerns between static data structures, hardware drivers, and network state management.

### 1. Static Code Generation (PGNs)
NMEA 2000 Parameter Group Numbers (PGNs) are complex and prone to manual parsing errors. `korri-n2k` eliminates this by generating static Rust structures (`PgnXXXX`) directly from JSON definitions during the build process (`build.rs`).
- **Standard & Proprietary Support:** While it defaults to the [CANboat](https://github.com/canboat/canboat) database, you can easily add **Proprietary PGNs** (e.g., Garmin, Victron) or your own **Custom PGNs** by adding them to your local manifest.
- **Benefit:** Compile-time type safety for all fields, automatic handling of bit offsets, signedness, and physical resolutions without runtime overhead.

### 2. Hardware Isolation (Transport Traits)
The core library has zero knowledge of the host hardware (MCU or Linux). It relies on two traits that the user must implement:
- `CanBus`: For reading and writing raw CAN frames.
- `KorriTimer`: For non-blocking asynchronous delays.

### 3. The Network Manager (`AddressManager`)
Before transmitting data on an NMEA 2000 network, a device must negotiate a logical address. The `AddressManager` handles this ISO 11783 lifecycle autonomously:
- Initial Address Claiming based on the device's unique `IsoName`.
- Automatic defence against address conflicts.
- Transparent segmentation and reassembly of large payloads (Fast Packet protocol).

### 4. The Async Socket (`AddressService`)
To prevent ownership issues in concurrent environments and avoid locking the CAN bus, the library provides an asynchronous abstraction (`AddressService`). It splits the network interaction into three decoupled components using static `Channels`:
- **`AddressHandle` (TX):** A lock-free sender passed to application tasks to queue outgoing PGNs.
- **`AddressFrames` (RX):** A receiver yielding incoming application-level frames (filtered by the manager).
- **`AddressRunner` (Runner):** An asynchronous task that must be spawned. It loops over a `select!` statement, routing messages between the physical CAN bus and the application channels while handling address management internally.

## Footprint & Performance (Indicative)

`korri-n2k` is optimized for deterministic execution and minimal resource consumption. On a typical **ARM Cortex-M4 (STM32G431)** target with a standard PGN manifest (approx. 10 PGNs):

- **Flash (Code):** ~6-10 KiB for the protocol stack (Codec + Manager).
- **RAM (Static):** Near-zero static allocation. Only your application-defined `Channels` consume RAM.
- **Dynamic Memory:** 0 bytes (no `alloc` required).

*Note: These figures are indicative and vary based on your PGN manifest, target architecture, and compiler optimization flags (LTO). Precise, reproducible benchmarks are available in our dedicated examples repository.*

## Implementation Guide

To use `korri-n2k` effectively, it is highly recommended to use **Type Aliases** to mask the generic complexity inherent to `no_alloc` systems.

### 1. Define your Socket types
```rust
// Alias your hardware-specific drivers and channel capacities
type MyCan = stm32_can::Can<'static>; // Example HAL driver
type MyTimer = embassy_time::Timer;
const TX_QUEUE: usize = 16;
const RX_QUEUE: usize = 16;

// Define your clean N2K socket types
pub type N2kSocket<'a> = AddressService<'a, MyCan, MyTimer, TX_QUEUE, RX_QUEUE>;
pub type N2kHandle<'a> = AddressHandle<'a, TX_QUEUE>;
pub type N2kRunner<'a> = AddressRunner<'a, MyCan, MyTimer, TX_QUEUE, RX_QUEUE>;
```

### 2. Initialize and Spawn
```rust
// 1. Statically allocate Embassy Channels
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, SupervisorCommand, TX_QUEUE> = Channel::new();
static FRAME_CHANNEL: Channel<CriticalSectionRawMutex, CanFrame, RX_QUEUE> = Channel::new();

// 2. Claim address and initialize the service
let service = AddressService::claim(
    can_driver,
    timer,
    my_iso_name.into(),
    PREFERRED_ADDRESS,
    Some(&CMD_CHANNEL),
    Some(&FRAME_CHANNEL)
).await?;

// 3. Split the service into concurrent parts
let (handle, mut frames, runner) = service.into_parts();

// 4. Spawn the runner background task
spawner.spawn(n2k_runner_task(runner)).unwrap();
```

### 3. Application Tasks
```rust
// TX Task: Send data lock-free
let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;
handle.send_pgn(&pos, 129025, 2, None).await?;

// RX Task: Wait for incoming data
let frame = frames.recv().await;
if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
    // Process depth data...
}
```

## Hardware Support & Examples

The library is agnostic but was designed with real-world hardware in mind. Hardware-specific examples are maintained in a [dedicated external repository](https://github.com/fard-draf/korri-n2k-examples) to keep this core library lightweight and strictly focused on protocol implementation.

*For a quick software-only test, run: `cargo run --example quickstart`*

## Build & Tooling

To minimize compile times, `korri-n2k` only generates code for the PGNs you require.
1. Define required PGNs in `build_core/var/pgn_manifest.json`.
2. The `build.rs` script fetches the latest CANboat definitions and generates the Rust modules.

## License

MIT OR Apache-2.0 — choose either license. See `LICENSE` for details.

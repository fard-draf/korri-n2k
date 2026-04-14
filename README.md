<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="Logo" width="800">
</p>

![CI](https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg)

`korri-n2k` is a highly optimized NMEA 2000 / ISO 11783 protocol stack for Rust. Built around a strict `no_std`, `no_alloc` core, it natively supports both **bare-metal microcontrollers** and **Linux embedded systems** through interchangeable asynchronous runtimes.

Whether you are building a sensor node on an STM32 or a central chartplotter on a Raspberry Pi using `SocketCAN`, `korri-n2k` provides the same deterministic, zero-cost abstractions.

## Runtimes & Cargo Features

The library is designed to perfectly adapt to your target environment using mutually exclusive features:

- **`embassy` (Default):** For bare-metal `no_std` targets. Uses `embassy-sync` static channels. **Strictly zero-allocation**.
- **`tokio`:** For OS-backed `std` targets (e.g., Linux/SocketCAN). Uses `tokio::sync::mpsc` channels and standard timers. The protocol core remains `no_alloc`, while Tokio manages the background task allocation.

```toml
# For bare-metal microcontrollers (STM32, ESP32, RP2040...)
[dependencies]
korri-n2k = "0.2"

# For Linux Embedded / OS targets (Raspberry Pi, SocketCAN...)
[dependencies]
korri-n2k = { version = "0.2", default-features = false, features = ["tokio"] }
```

## Core Architecture & Abstractions

The library is built upon four primary pillars, designed to separate concerns between static data structures, hardware drivers, and network state management.

### 1. Static Code Generation (PGNs)
NMEA 2000 Parameter Group Numbers (PGNs) are complex and prone to manual parsing errors. `korri-n2k` eliminates this by generating static Rust structures (`PgnXXXX`) directly from JSON definitions during the build process (`build.rs`).
- **Standard & Proprietary Support:** While it defaults to the [CANboat](https://github.com/canboat/canboat) database, you can easily add **Proprietary PGNs** or your own **Custom PGNs**.
- **Benefit:** Compile-time type safety for all fields, automatic handling of bit offsets, signedness, and physical resolutions without runtime overhead.

### 2. Hardware Isolation (Transport Traits)
The core library has zero knowledge of the host hardware. It relies on two traits that the user must implement:
- `CanBus`: For reading and writing raw CAN frames.
- `KorriTimer`: For non-blocking asynchronous delays (Provided out-of-the-box for Tokio).

### 3. The Network Manager (`AddressManager`)
Before transmitting data on an NMEA 2000 network, a device must negotiate a logical address. The `AddressManager` handles this ISO 11783 lifecycle autonomously:
- Initial Address Claiming based on the device's unique `IsoName`.
- Automatic defence against address conflicts.
- Transparent segmentation and reassembly of large payloads (Fast Packet protocol).

### 4. The Async Socket (`AddressService`)
To prevent ownership issues in concurrent environments and avoid locking the CAN bus, the library provides an asynchronous abstraction (`AddressService`). It splits the network interaction into three decoupled components using channels specific to your chosen runtime:
- **`AddressHandle` (TX):** A lock-free sender passed to application tasks to queue outgoing PGNs.
- **`AddressFrames` (RX):** A receiver yielding incoming application-level frames (filtered by the manager).
- **`AddressRunner` (Runner):** An asynchronous task that must be spawned. It routes messages between the physical CAN bus and the application channels while handling address management internally.

## Footprint & Performance

`korri-n2k` is optimized for deterministic execution and minimal resource consumption. On a typical **ARM Cortex-M4** target with a standard PGN manifest:

- **Flash (Code):** ~6-10 KiB for the protocol stack (Codec + Manager).
- **RAM (Static):** Near-zero static allocation. Only your application-defined channels consume RAM.
- **Dynamic Memory:** **0 bytes** (No `alloc` required in `embassy` mode. Minimal standard channel allocation in `tokio` mode).

## Implementation Guide

The `AddressService` API is nearly identical across runtimes, varying only in how channels are allocated.

### Option A: Bare-Metal (Embassy)
In `no_std` environments, channels must be statically allocated to avoid the heap.
```rust
// 1. Statically allocate Embassy Channels
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, SupervisorCommand, 16> = Channel::new();
static FRAME_CHANNEL: Channel<CriticalSectionRawMutex, CanFrame, 16> = Channel::new();

// 2. Claim address and initialize the service
let service = AddressService::claim(
    my_can_driver,
    my_embassy_timer,
    my_iso_name.into(),
    PREFERRED_ADDRESS,
    Some(&CMD_CHANNEL),
    Some(&FRAME_CHANNEL)
).await?;

// 3. Split the service into concurrent parts
let parts = service.into_parts();
let handle = parts.handle.unwrap();
let mut frames = parts.frames.unwrap();

// 4. Spawn the background runner
spawner.spawn(n2k_runner_task(parts.runner)).unwrap();
```

### Option B: Linux / OS (Tokio)
In `std` environments, Tokio handles channel allocation seamlessly.
```rust
// 1. Initialize the service with queue capacities
let service = AddressService::claim(
    my_socketcan_driver,
    TokioTimer, // Provided by korri-n2k when `tokio` feature is active
    my_iso_name.into(),
    PREFERRED_ADDRESS,
    16, // TX Queue capacity
    16  // RX Queue capacity
).await?;

// 2. Split the service into concurrent parts
let parts = service.into_parts();
let handle = parts.handle.unwrap();
let mut frames = parts.frames.unwrap();

// 3. Spawn the background runner
tokio::spawn(parts.runner.drive());
```

### Application Usage (Common to both runtimes)
```rust
// TX Task: Send data lock-free
let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;
handle.send_pgn(&pos, 129025, 2, None).await?;

// RX Task: Wait for incoming data
if let Some(frame) = frames.recv().await {
    if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
        println!("Depth: {}m", depth.depth);
    }
}
```

## Hardware Support & Examples

The library is hardware-agnostic. Hardware-specific implementations (STM32, SocketCAN) are maintained in a [dedicated external repository](https://github.com/fard-draf/korri-n2k-examples) to keep this core library lightweight.

*For a quick software-only test using the `std` feature, run:*
`cargo run --example quickstart --no-default-features --features tokio`

## Build & Tooling

To minimize compile times, `korri-n2k` only generates code for the PGNs you require.
1. Define required PGNs in `build_core/var/pgn_manifest.json`.
2. The `build.rs` script fetches the latest CANboat definitions and generates the Rust modules.

## License

MIT OR Apache-2.0 — choose either license. See `LICENSE` for details.

<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="Logo" width="800">
</p>

![CI](https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg)

`korri-n2k` is a NMEA 2000 / ISO 11783 protocol stack for Rust. It lets you both **send and receive** PGNs on the bus. Its core is `no_std` and zero-allocation, supporting both bare-metal microcontrollers and Linux embedded systems through interchangeable asynchronous runtimes.

## Only the PGNs you need — nothing else

The NMEA 2000 standard defines over 400 Parameter Group Numbers. `korri-n2k` never compiles them all: the build system generates Rust structs **only for the PGN IDs listed in your `pgn_manifest.json`**.

```json
{
  "pgns": [
    { "id": 129025, "name": "Position, Rapid Update" },
    { "id": 128267, "name": "Water Depth" }
  ]
}
```

A depth sensor pulling two PGNs adds ~2 KiB of flash. A full navigation bridge pulling thirty adds proportionally more. Everything else is dead code the linker never sees.

The default manifest already covers the most common marine PGNs (position, heading, AIS, depth, wind…). Swap or extend it to add proprietary PGNs from Garmin, Victron, or your own devices.

## Runtimes & Cargo Features

These two features are mutually exclusive — choose the one that matches your target:

| Feature | Target | `std` | Heap |
|---|---|---|---|
| `embassy` *(default)* | Bare-metal (`no_std`) | No | **0 bytes** — fully static |
| `tokio` | Linux / OS (`std`) | Yes | Channel buffers allocated by Tokio |

```toml
# Bare-metal microcontrollers (STM32, ESP32, RP2040…)
[dependencies]
korri-n2k = "0.3"

# Linux / OS targets (Raspberry Pi, SocketCAN…)
[dependencies]
korri-n2k = { version = "0.3", default-features = false, features = ["tokio"] }
```

> In `tokio` mode the protocol logic (codec, address claiming, fast packet) is still zero-allocation. Only the `mpsc` channel buffers used by `AddressService` are heap-allocated — their capacity is set by the caller.

## Core Architecture & Abstractions

### 1. Hardware Isolation (Transport Traits)
The core library has zero knowledge of the host hardware. It relies on two traits:
- `CanBus`: Read and write raw CAN frames.
- `KorriTimer`: Non-blocking async delays. A `TokioTimer` implementation is provided when the `tokio` feature is active (`use korri_n2k::TokioTimer`).

### 2. The Network Manager (`AddressManager`)
Before transmitting on an NMEA 2000 network, a device must negotiate a logical address. The `AddressManager` handles this autonomously:
- Initial address claiming based on the device's unique `IsoName`.
- Automatic defence against address conflicts.
- Transparent segmentation and reassembly of large payloads (Fast Packet protocol).

### 3. The Async Socket (`AddressService`)
`AddressService` splits network interaction into three decoupled components:
- **`AddressHandle` (TX):** Queue outgoing PGNs from any task. Sends are fire-and-forget — if the runner is gone frames are silently dropped.
- **`AddressFrames` (RX):** Receive incoming application-level frames filtered by the manager.
- **`AddressRunner` (Runner):** Background task that routes messages between the CAN bus and the application channels.

## Footprint & Performance

On a typical **ARM Cortex-M4** target:

- **Flash:** ~6–10 KiB for the protocol stack, plus the structs generated for your PGN manifest.
- **RAM (embassy):** Near-zero static allocation — only your statically-defined channel buffers.
- **RAM (tokio):** Protocol stack same as above; `AddressService` channel buffers heap-allocated at the capacity you specify.

## Implementation Guide

### Option A — Bare-Metal (Embassy)

Channels must be statically allocated to avoid the heap.

```rust
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, SupervisorCommand, 16> = Channel::new();
static FRAME_CHANNEL: Channel<CriticalSectionRawMutex, CanFrame, 16> = Channel::new();

let service = AddressService::claim(
    my_can_driver,
    my_embassy_timer,
    my_iso_name.into(),
    PREFERRED_ADDRESS,
    Some(&CMD_CHANNEL),
    Some(&FRAME_CHANNEL),
).await?;

let parts = service.into_parts();
spawner.spawn(n2k_runner_task(parts.runner)).unwrap();
```

`#[embassy_executor::task]` functions can't be generic, so you must define `n2k_runner_task` yourself, monomorphized over your concrete `CanBus`/`KorriTimer` types:

```rust
#[embassy_executor::task]
async fn n2k_runner_task(runner: AddressRunner<'static, MyCanBus, MyEmbassyTimer, 16, 16>) {
    let _ = runner.drive().await;
}
```

### Option B — Linux / OS (Tokio)

Channel capacities are passed as integers; Tokio allocates the buffers.

```rust
use korri_n2k::TokioTimer;

let service = AddressService::claim(
    my_socketcan_driver,
    TokioTimer,
    my_iso_name.into(),
    PREFERRED_ADDRESS,
    16, // TX queue capacity
    16, // RX queue capacity
).await?;

let parts = service.into_parts();
tokio::spawn(parts.runner.drive());
```

### Sending & Receiving PGNs

`korri-n2k` is bidirectional: any task holding an `AddressHandle` can **send** PGNs, and `AddressFrames` lets you **receive** and decode incoming ones.

```rust
// Send: build a PGN struct and queue it for transmission (identical on both runtimes)
let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;
handle.send_pgn(&pos, 129025, 2, None).await?;
```

Receiving differs slightly between runtimes: Tokio's `mpsc` channel can be closed, so `recv()` returns `Option<CanFrame>`; an Embassy channel never closes, so `recv()` returns `CanFrame` directly.

```rust
// Receive (tokio)
if let Some(frame) = frames.recv().await {
    if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
        println!("Depth: {}m", depth.depth);
    }
}

// Receive (embassy)
let frame = frames.recv().await;
if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
    println!("Depth: {}m", depth.depth);
}
```

## Roadmap

The codec covers **~98% of the CANboat data model** (275 of 280 PGNs). The remaining gap is one meaningful capability plus a few niche proprietary messages:

- **Device configuration & query (PGN 126208).** The NMEA Group Functions meta-protocol — reading and writing parameters on any device on the network (poll an autopilot, configure a sensor, request a value on demand). It needs two runtime-resolved field types (`FIELD_INDEX`, `VARIABLE`) and a second repeating group, which is why it isn't generated yet. This is the next priority.
- **Proprietary B&G / Simnet key-value PGNs (130824, 130833, 130845, 130846).** Niche Navico-family messages using `DYNAMIC_FIELD_*` types; implemented on demand.

None of these are in the default PGN manifest, so a standard build is unaffected.

## Hardware Support & Examples

The library is hardware-agnostic. Hardware-specific implementations (STM32, SocketCAN) are maintained in a [dedicated external repository](https://github.com/fard-draf/korri-n2k-examples).

`std` examples (no hardware required) live in [`examples/std/`](examples/std):
- `cargo run --example quickstart` — ISO Name, PGN (de)serialization, CAN ID basics.
- `cargo run --example lookup_enum_usage` — working with NMEA 2000 lookup enums.
- `cargo run --example iso_name_usage` — ISO Name manipulation and address claiming.

## License

MIT OR Apache-2.0 — choose either license. See `LICENSE` for details.

<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="Logo" width="800">
</p>

![CI](https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg)

An NMEA 2000 / ISO 11783 stack for Rust: send and receive PGNs on a marine CAN
bus. `no_std`, zero-allocation, one runtime for bare metal and one for Linux.

PGN types are generated at build time from [CANboat](https://github.com/canboat/canboat),
the reference catalogue of NMEA 2000 messages — you never hand-write a parser,
and you choose how many get compiled in.

## Quickstart

```toml
[dependencies]
korri-n2k = "0.5"
```

```rust
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn128267;

// An 8-byte Water Depth frame straight off the bus.
let payload = [0x2A, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF];
let depth = Pgn128267::from_payload(&payload)?;

println!("{:.2} m under the transducer", depth.depth);
```

Three runnable examples need no hardware: `cargo run --example quickstart`,
`lookup_enum_usage`, `iso_name_usage`.

## Installation

`embassy` and `tokio` are mutually exclusive; pick the one matching your target.

| Feature | Target | `std` | Heap |
|---|---|---|---|
| `embassy` *(default)* | Bare metal (`no_std`) | No | **none**, fully static |
| `tokio` | Linux / OS | Yes | channel buffers only |

```toml
# Bare metal: STM32, ESP32, RP2040…
korri-n2k = "0.5"

# Linux: Raspberry Pi, SocketCAN…
korri-n2k = { version = "0.5", default-features = false, features = ["tokio"] }
```

Under `tokio` the protocol logic stays zero-allocation; only the `mpsc` buffers
of `AddressService` are heap-allocated. Rust 1.96, edition 2021.

## Choosing your PGNs

The build generates types only for the PGN IDs listed in a manifest.

| Your project | What you compile | How |
|---|---|---|
| A specific device | the PGNs you list | edit `pgn_manifest.json` |
| A proprietary set | your IDs, kept outside the crate | `KORRI_N2K_MANIFEST_PATH=…` |
| A gateway or logger | every supported PGN | `features = ["full-pgns"]` |

```json
{
  "pgns": [
    { "id": 129025, "name": "Position, Rapid Update" },
    { "id": 128267, "name": "Water Depth" }
  ]
}
```

Matching is on `id`; names are documentation. Precedence:
`KORRI_N2K_MANIFEST_PATH` → `full-pgns` → default manifest. The default covers 42
common PGNs — position, heading, AIS, depth, wind; `full-pgns` covers the 313 the
generator supports, out of CANboat's 348.

## Sending and receiving

Any task holding an `AddressHandle` can send. Fragmentation into Fast Packet
frames is automatic.

```rust
let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;
handle.send_pgn(&pos, 129025, 2, None).await?;
```

On receive, `AddressFrames` yields raw `CanFrame`s. A single-frame PGN decodes
directly:

```rust
// tokio: the channel can close, so recv() returns an Option
if let Some(frame) = frames.recv().await {
    if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
        println!("{} m", depth.depth);
    }
}

// embassy: the channel never closes
let frame = frames.recv().await;
```

**A Fast Packet PGN must be reassembled first** — see [Limits](#limits). Feed
every frame to a `FastPacketAssembler` and decode what it completes:

```rust
use korri_n2k::protocol::transport::fast_packet::assembler::{
    FastPacketAssembler, ProcessResult,
};

// new() reassembles the Fast Packet PGNs of your manifest.
let mut assembler = FastPacketAssembler::new();

// for each incoming frame, with a millisecond clock:
let pgn = frame.id.pgn();
if !assembler.handles(pgn) {
    // Single frame: decode the 8 bytes directly, as in the example above.
} else if let ProcessResult::MessageComplete(msg) =
    assembler.process_frame(now_ms, pgn, frame.id.source_address(), &frame.data)
{
    if pgn == 129029 {
        let fix = Pgn129029::from_payload(&msg.payload[..msg.len])?;
        println!("{:.6} {:.6}", fix.latitude, fix.longitude);
    }
}
```

`new()` carries `FAST_PACKET_PGNS`, the Fast Packet PGNs of your manifest. A
gateway that forwards payloads it cannot decode wants every one CANboat knows:

```rust
use korri_n2k::protocol::transport::fast_packet::FAST_PACKET_PGNS_ALL;

let mut assembler = FastPacketAssembler::with_pgns(FAST_PACKET_PGNS_ALL);
```

`full-pgns` is not a substitute: its manifest lists 152 of the 182 Fast Packet
PGNs CANboat declares. Pass your own sorted table for proprietary PGNs CANboat
does not list at all.

## Limits

- **Fast Packet reassembly is not wired into the receive path.** Transmission
  fragments automatically, reception hands you raw frames; run
  `FastPacketAssembler` yourself as shown above. Decoding the first 8 bytes of a
  multi-frame message otherwise succeeds and returns wrong values.
- **The assembler only reassembles the PGNs in its table.** A proprietary PGN
  CANboat does not list is ignored until you hand it to `with_pgns`. Ask
  `handles()` what a given assembler covers.
- **No ISO Transport Protocol.** PGNs 60160 and 60416 decode as messages, but
  the multi-packet TP transport itself is not implemented.
- **35 of CANboat's 348 PGNs are not generated**, listed with their reason in
  `build_core/var/pgn_manifest.full.json`. None are in the default manifest.
- **Runtime-sized fields are unsupported** (`VARIABLE`, `DYNAMIC_FIELD_VALUE`),
  which is what keeps PGN 126208 — the Group Functions meta-protocol for
  querying and configuring devices — out of reach. Next on the roadmap.
- **A repeating group needs an explicit counter field**; the handful of PGNs
  that size their group implicitly are rejected.
- `embassy` and `tokio` cannot be enabled together.

## Architecture

The core knows nothing about your hardware. It sits on two traits: `CanBus` for
raw frame I/O and `KorriTimer` for non-blocking delays (a `TokioTimer` ships with
the `tokio` feature).

`AddressManager` claims a logical address from your `IsoName` and defends it
against conflicts. `AddressService::claim` returns three parts: `AddressHandle`
to queue outgoing PGNs from any task, `AddressFrames` to receive, and
`AddressRunner`, the background task routing between them. Channels are static
under embassy, sized by integer capacities under tokio.

One catch: `#[embassy_executor::task]` cannot be generic, so declare the runner
task yourself over your concrete types.

```rust
#[embassy_executor::task]
async fn n2k_runner_task(runner: AddressRunner<'static, MyCanBus, MyTimer, 16, 16>) {
    let _ = runner.drive().await;
}
```

Hardware implementations for STM32, ESP32 and SocketCAN live in
[korri-n2k-examples](https://github.com/fard-draf/korri-n2k-examples).

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

The bundled [CANboat](https://github.com/canboat/canboat) database
(`build_core/var/canboat.json`) is Apache-2.0, © Kees Verruijt. It stays under
that licence, as does the code generated from it — PGN and field names,
descriptions, lookup variants. See `NOTICE`.

All credit for the NMEA 2000 message catalogue goes to the CANboat authors,
without whom this library could not exist.

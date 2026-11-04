<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="korri-n2k logo" width="800">
</p>

<p align="center">
  <a href="https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml"><img src="https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/korri-n2k"><img src="https://img.shields.io/crates/v/korri-n2k.svg" alt="crates.io"></a>
  <a href="https://docs.rs/korri-n2k"><img src="https://docs.rs/korri-n2k/badge.svg" alt="docs.rs"></a>
  <img src="https://img.shields.io/badge/rustc-1.96%2B-blue.svg" alt="MSRV">
  <img src="https://img.shields.io/crates/l/korri-n2k.svg" alt="License">
</p>

An embedded-first NMEA 2000 / ISO 11783 stack for Rust. It sends and receives
PGNs on a marine CAN bus. The `embassy` feature is `no_std` and uses no heap.
The `tokio` feature provides the same protocol logic on Linux (other OSes are
untested).

PGN types are generated at build time from [CANboat](https://github.com/canboat/canboat),
the reference catalogue of NMEA 2000 messages. You never hand-write a parser.
You choose how many get compiled in.

## Quickstart

```sh
cargo add korri-n2k --features embassy   # bare metal
cargo add korri-n2k --features tokio     # Linux
```

Decoding needs no runtime feature: the snippet below runs as the `quickstart`
example without any.

```rust
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn128267;

// An 8-byte Water Depth frame straight off the bus.
let payload = [0x2A, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF];
let depth = Pgn128267::from_payload(&payload).expect("valid Water Depth");

// Fields are fixed-point on the wire. Compare with a tolerance, never with `==`.
assert!((depth.depth - 1.0).abs() < 1e-6);
```

Claim an address and emit (`embassy`, zero heap; `tokio` uses the same flow with
channel capacities):

```rust,ignore
static COMMANDS: Channel<CriticalSectionRawMutex, SupervisorCommand, 4> = Channel::new();
static FRAMES: Channel<CriticalSectionRawMutex, CanFrame, 8> = Channel::new();
static CLAIMED: ClaimedAddress = ClaimedAddress::new();

let manager = AddressManager::new(bus, timer, my_name, strategy)?;
let parts = AddressService::<_, _, 4, 8>::new(manager, Some(&COMMANDS), Some(&FRAMES), &CLAIMED)
    .into_parts();
let handle = parts.handle.expect("a command channel was provided");

spawner.spawn(n2k_runner_task(parts.runner))?; // claims (250 ms), defends, emits

// Commands queued before the claim completes are dropped by the runner.
if handle.claimed_address().is_some() {
    let mut pos = Pgn129025::new();
    pos.latitude = 47.7223;
    pos.longitude = -4.0022;
    handle.send_pgn(&pos, 129025, 2, None).await?; // Fast Packet is automatic
}
```

## Install

`embassy` and `tokio` are mutually exclusive: pick one, or none to only decode.
Minimum Rust 1.96, edition 2021.

## Choosing your PGNs

Only the PGNs listed in a manifest are compiled in:

| Need | Configuration |
|---|---|
| Common PGNs (position, heading, AIS, depth, wind) | default |
| Custom or minimal set (`{"pgns": [{"id": 128267, "name": "..."}]}`) | `KORRI_N2K_MANIFEST_PATH=/abs/path/manifest.json` |
| Every supported PGN | `features = ["full-pgns"]` |

## Documentation

Guides live in the API docs ([docs.rs](https://docs.rs/korri-n2k)):

- Features, choosing PGNs (manifest / `full-pgns`), limits: crate root
- Claim, send, receive (`AddressService`, `AddressHandle`, `AddressFrames`):
  `protocol::management::address_supervisor`
- Several logical addresses on one node (`AddressManager::with_controllers`,
  `into_multi_parts`): `protocol::management::address_supervisor`
- Claim without a runtime (`AddressClaimEngine`):
  `protocol::management::address_claiming::engine`
- Fast Packet reassembly (`FastPacketAssembler`):
  `protocol::transport::fast_packet`
- Writing a driver (`CanBus`): `protocol::transport::traits::can_bus`

Runnable examples (CI runs them): `quickstart`, `lookup_enum_usage`,
`iso_name_usage`, `fast_packet_rx --features std`,
`blocking_claim --features std`, `address_claim --features tokio`. Firmwares for
STM32, ESP32 and SocketCAN: [korri-n2k-examples](https://github.com/fard-draf/korri-n2k-examples).

Known gaps (Fast Packet reassembly is manual on RX; no ISO Transport Protocol or
Commanded Address; no random pre-claim delay): see *Limits* in the crate root
docs and [ROADMAP.md](ROADMAP.md). History: [CHANGELOG.md](CHANGELOG.md).

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

The bundled [CANboat](https://github.com/canboat/canboat) database
(`build_core/var/canboat.json`) is Apache-2.0, © Kees Verruijt. It stays under
that licence, as does the code generated from it: PGN and field names,
descriptions, lookup variants. See `NOTICE`.

All credit for the NMEA 2000 message catalogue goes to the CANboat authors,
without whom this library could not exist.

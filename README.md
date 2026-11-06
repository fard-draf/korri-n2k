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
PGNs on a marine CAN bus. Decoding alone and the `embassy` runtime are `no_std`
and use no heap. The `tokio` runtime provides the same protocol logic on Linux
(other OSes are untested).

PGN types are generated at build time from [CANboat](https://github.com/canboat/canboat),
the reference catalogue of NMEA 2000 messages. You never hand-write a parser.
You choose how many get compiled in.

## Install

Minimum Rust 1.96, edition 2021. Pick **at most one** runtime: `embassy` and
`tokio` are mutually exclusive and enabling both fails to compile.

```sh
cargo add korri-n2k                      # decode / encode only
cargo add korri-n2k --features embassy   # or: bare metal runtime
cargo add korri-n2k --features tokio     # or: Linux runtime
```

| Feature | Purpose | `no_std` | Heap |
|---|---|---|---|
| *(none)* | decode and encode PGNs | yes | none |
| `embassy` | claim, send, receive on bare metal | yes | none |
| `tokio` | claim, send, receive on Linux (implies `std`) | no | channel buffers only |
| `std` | host without a runtime (blocking claim, examples) | no | none |

Optional: `full-pgns` (see below), `defmt` (logging through `defmt`),
`build-download` (if `build_core/var/canboat.json` is missing, fetch it with
`ureq` instead of `curl`/`wget`).

## Quickstart

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

Claim an address and emit (`embassy`, zero heap):

```rust,ignore
static COMMANDS: Channel<CriticalSectionRawMutex, SupervisorCommand, 4> = Channel::new();
static FRAMES: Channel<CriticalSectionRawMutex, CanFrame, 8> = Channel::new();
static CLAIMED: ClaimedAddress = ClaimedAddress::new();

let manager = AddressManager::new(bus, timer, my_name, strategy)?;
let parts = AddressService::<_, _, 4, 8>::new(manager, Some(&COMMANDS), Some(&FRAMES), &CLAIMED)
    .into_parts();
let handle = parts.handle.expect("a command channel was provided");

spawner.spawn(n2k_runner_task(parts.runner))?; // claims, defends, emits

// The claim takes at least 250 ms, and the runner drops commands queued before it.
while handle.claimed_address().is_none() {
    Timer::after_millis(10).await;
}

let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;
handle.send_pgn(&pos, 129025, 2, None).await?; // one frame; longer PGNs go out as Fast Packet
```

Under `tokio` there are no `static`s: `AddressService::new(manager, 4, 8)` takes
channel capacities (`0` opts out of a channel), and `parts.runner.drive()` is
spawned with `tokio::spawn`. See the `address_claim` example.

## Choosing your PGNs

Only the PGNs listed in a manifest are compiled in:

| Need | Configuration |
|---|---|
| Common PGNs (position, heading, AIS, depth, wind) | default |
| Custom or minimal set (`{"pgns": [{"id": 60928, "name": "..."}, {"id": 128267, "name": "..."}]}`) | `KORRI_N2K_MANIFEST_PATH=/abs/path/manifest.json` |
| Every supported PGN | `features = ["full-pgns"]` |

`KORRI_N2K_MANIFEST_PATH` wins over `full-pgns`. If the file does not exist, the
build fails and names the path. An empty `KORRI_N2K_MANIFEST_PATH` fails the same
way: unset the variable to get the default manifest.

A manifest must list PGN 60928 (ISO Address Claim), which the address
management code needs. Every listed PGN must also be generated: an id unknown
to CANboat, or one the generator cannot handle, fails the build instead of
being left out.

## Passive monitoring

A bus monitor or logger can decode any received PGN without knowing its Rust
type at compile time, and without ever transmitting on the bus:

- `protocol::messages::descriptor(pgn)` and `descriptor_for(pgn, payload)`
  resolve the static `PgnDescriptor` at runtime (including proprietary
  multi-variant PGNs, with the same discriminators and order as `from_payload`).
- `infra::codec::engine::for_each_field(desc, payload, |field, reading| …)`
  visits every non-reserved field in wire order (`no_std`, zero allocation),
  yielding its scaled `PgnValue`, raw wire integer, repeating-group index, and
  `Availability` (`Available`, `NotAvailable`, `OutOfRange`, `Reserved`) matched
  directly against the per-field sentinels in `canboat.json` (`string_fix_trimmed`
  handles fixed-length ASCII padding).
- `protocol::lookups::lookup_name` and `lookup_name_indirect` return the
  original CANboat human-readable labels for the active manifest.

```rust
use korri_n2k::core::Availability;
use korri_n2k::infra::codec::engine::for_each_field;
use korri_n2k::protocol::messages::descriptor_for;

let payload = [0x01, 0x08, 0x02, 0x00, 0x00, 0xD4, 0xFE, 0xFF];
let desc = descriptor_for(128267, &payload).expect("Water Depth in manifest");

let mut range_na = false;
for_each_field(desc, &payload, |field, reading| {
    if field.id == "Range" {
        range_na = reading.availability == Availability::NotAvailable;
    }
})
.expect("valid payload");
assert!(range_na);
```

**Flash footprint (`thumbv7em-none-eabihf`, `opt-level = "s"`, LTO):** zero
overhead when unreferenced (the linker strips unused descriptors and tables).
Calling `descriptor_for` + `for_each_field` + `lookup_name` pulls in every
descriptor of the active manifest, one per PGN variant (~1.1–1.4 KB of `.rodata`
each):

| Manifest | PGNs | Descriptors | `.rodata` | `.text` |
|---|---|---|---|---|
| default | 42 | 49 | ~79 KB | +13 KB |
| `full-pgns` | 313 | 474 | ~573 KB | +48 KB |

On a microcontroller, use a custom `KORRI_N2K_MANIFEST_PATH` listing only the
PGNs you monitor.

## Documentation

Guides live in the API docs ([docs.rs](https://docs.rs/korri-n2k), built with
`embassy`; for the `tokio` API, run `cargo doc --features tokio --open`):

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
`blocking_claim --features std`, `monitor --features std`,
`address_claim --features tokio`. Firmwares for
STM32, ESP32 and SocketCAN: [korri-n2k-examples](https://github.com/fard-draf/korri-n2k-examples).

Known gaps: Fast Packet is split automatically on TX but reassembled manually on
RX (`FastPacketAssembler`); no ISO Transport Protocol or ISO Commanded Address;
no random delay before a claim. See *Limits* in the crate root docs and [ROADMAP.md](ROADMAP.md). History: [CHANGELOG.md](CHANGELOG.md).

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

The bundled [CANboat](https://github.com/canboat/canboat) database
(`build_core/var/canboat.json`) is Apache-2.0, © Kees Verruijt. It stays under
that licence, as does the code generated from it: PGN and field names,
descriptions, lookup variants. See `NOTICE`.

All credit for the NMEA 2000 message catalogue goes to the CANboat authors,
without whom this library could not exist.

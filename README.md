# korri-n2k

`korri-n2k` is a `no_std`, `no_alloc` implementation of the NMEA 2000 / ISO 11783 protocol stack for embedded Rust targets.

The crate focuses on deterministic behaviour (compile-time PGN layout, zero heap usage) and interoperates with async runtimes built on top of `embassy`. It is designed for MCUs with tight RAM/flash budgets and for firmware teams that need explicit control over message scheduling, address claiming, and Fast Packet segmentation.

## Highlights

- **Static PGN types** generated from the official [CANboat](https://github.com/canboat/canboat) manifest
- **Fast Packet** helpers (segment builder + assembler) with zero runtime allocation
- **ISO address management** via `AddressManager` and the new optional `AddressService`
- **Async-first API** (`CanBus`, `KorriTimer`) compatible with `embassy` executors
- **Transport-agnostic**: the crate does not depend on a specific BSP; you supply the CAN + timer drivers

## Getting started

1. Declare the required dependencies (`korri-n2k`, `embassy-time`, `embassy-sync`, `embedded-can`, `static_cell`).
2. Implement the two transport traits (`CanBus`, `KorriTimer`) against your HAL.
3. Pick the integration style that fits: raw `AddressManager` for full control, or `AddressService` to get a supervisor (claim loop + optional command queue).
4. Use the generated `PgnXXXX` structures to serialize, transmit, and decode messages.

The `examples/std/quickstart.rs` sample shows a host-side flow. Hardware-ready showcase projects (ESP32-S3, ESP32-C3, STM32G4 in progress) live under `examples-bsp/`.

## Embedded examples

The repository ships with standalone BSP-oriented crates under `examples-bsp/` (each with its own `Cargo.toml`, toolchain and configuration):

| Board           | Status            | Notes |
|-----------------|-------------------|-------|
| ESP32-S3        | ✅ Supported       | Async TWAI driver, `AddressService` usage |
| ESP32-C3        | ✅ Supported       | Same supervisor integration via TWAI |
| STM32G4 (WIP)   | 🚧 Work in progress | Hardware pending |


## Documentation

- API docs: `cargo doc --no-deps`
- Test suite: `cargo test`
- Custom PGN generation: place a manifest at `build_core/var/pgn_manifest.json` or point `KORRI_N2K_MANIFEST_PATH` to your configuration; the build script takes care of downloading `canboat.json` with `curl`/`wget` (or falls back to `ureq` with the `build-download` feature).

Core modules to explore:

| Module                         | Purpose |
|--------------------------------|---------|
| `protocol::messages::*`        | Generated PGN structures |
| `protocol::transport::fast_packet` | Builder + assembler for segmented PGNs |
| `protocol::managment::address_manager` | ISO address claiming/defence |
| `protocol::managment::address_supervisor` | Optional supervisor wrapping the manager |
| `infra::codec`                 | Bit-level codecs, lookup tables |

## Supplied tooling

- `scripts/download_canboat.sh` — refresh `canboat.json`
- `scripts/verify_docs.sh` — run the documentation examples + unit tests + formatting

## License

MIT OR Apache-2.0 — choose either license. See `LICENSE` for details.

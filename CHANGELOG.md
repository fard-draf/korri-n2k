# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- `AddressClaimStrategy` enum with `Fixed`, `SelfConfigurable` and
  `Arbitrary` variants, letting a device declare how it may claim addresses.
- `ClaimError::InconsistentStrategy`, returned when the NAME's AAC bit
  contradicts the chosen strategy.

### Changed
- **BREAKING**: `claim_address` now takes an `AddressClaimStrategy` instead
  of a raw `preferred_address: u8`.
- AAC capability is no longer inferred from NAME bit 63 alone.
- The arbitrary range is narrowed from `128..=247` to the marine dynamic
  range `128..=207`.


## [0.3.2] - 2026-07-11
### Added
- **`full-pgns` feature**: generate code for every PGN the library currently supports, without hand-writing a manifest. Ideal for bridges/gateways that must handle anything on the bus. Backed by the bundled `build_core/var/pgn_manifest.full.json`. Selection precedence is `KORRI_N2K_MANIFEST_PATH` > `full-pgns` > default manifest.

### Changed
- Refactored `README.md` to introduce the "full-pgns" feature.

### Docs
- **docs**: remove outdated and inaccurate canboat.json analysis

## [0.3.1] - 2026-07-03
### Fixed
- **codec engine**: `FieldKind::IsoName` (the 64-bit device-identity field, e.g. `DataSourceNetworkIdName` in PGN 126985 "Alert Text") had no match arm in `write_field`/`read_field_value`, falling through to `UnsupportedFieldKind`. Any PGN carrying an `IsoName` field failed `to_payload()`/`from_payload()` unconditionally. Now handled like `Number`/`Pgn` (plain unsigned integer, no sign/resolution).

### Changed
- **README**: clarified upfront that the library is bidirectional (send *and* receive). Split the "receive" example per runtime — `AddressFrames::recv()` returns `Option<CanFrame>` under `tokio` but `CanFrame` directly under `embassy` (the channel never closes), so the previous shared snippet didn't compile under the default `embassy` feature.   

## [0.3.0] - 2026-06-20
### Added
- **`tokio` runtime feature**: `AddressService` / `AddressRunner` / `AddressHandle` / `AddressFrames` built on `tokio::sync::mpsc`, for `std` targets such as Linux/SocketCAN.
- **`std` feature**: enables building on hosted targets (`#![no_std]` is now conditional on `not(feature = "std")`).
- **`TokioTimer`**: a ready-to-use `KorriTimer` implementation backed by `tokio::time`, re-exported at the crate root (`korri_n2k::TokioTimer`).
- Integration tests covering the Tokio supervisor (PGN queueing, CAN-bus error, command/frame channel close).

### Changed
- **BREAKING**: `embassy` is now an explicit feature, enabled by default (`default = ["embassy"]`). `embassy-time` and `embassy-sync` became optional dependencies. Users who build with `default-features = false` must now opt into `embassy` (or `tokio`) explicitly.
- `embassy` and `tokio` are mutually exclusive; enabling both triggers a `compile_error!`.
- Refactored `README.md`: runtimes table, honest allocation notes for the `tokio` mode, PGN-manifest selection highlighted, and a roadmap section.

## [0.2.1] - 2026-06-12
### Fixed
- **address_claiming**: A non-AAC node that loses arbitration now returns `Err(ClaimError::NoAddressAvailable)` instead of `Ok(254)`. Previously, the J1939 null address (0xFE) was silently treated as a valid claimed address, causing the node to keep sending frames with an illegal source address.
- **address_manager**: `reclaim()` no longer sets `current_address` to 255 before the async call. If reclaim fails with a bus error, the error is now returned to the caller. If no address is available, the node is marked with the null address (0xFE) instead of quietly continuing with the global broadcast address (0xFF) as its source.

## [0.2.0] - 2026-04-02
### Added
- GitHub Actions CI/CD pipeline for automated testing on Linux, ARM Cortex-M targets, Risc-V targets (Xtensa WIP).

### Changed
- **BREAKING**: Migrated from `async-trait` to native Rust **Async Functions in Traits (AFIT)**. The library is now 100% allocation-free (yay!).
- Refactored `README.md` to highlight the "Socket" architecture and provide a clearer implementation guide.
- Streamlined `Cargo.toml` and project structure.
- Moved `annexes/` to `docs/` for better standard compliance.

### Removed
- **BREAKING**: Moved hardware-specific examples to a dedicated external repository.
- Obsolete bash scripts and redundant build-time tools.

## [0.1.1] - 2025-10-29
### Added
- `AddressService` supervisor wrapping `AddressManager` with optional command/frame channels.
- Integration test `supervisor_queues_and_sends_pgn` to validate the supervisor flow.
- Lightweight README highlighting key features and pointing to BSP examples.

### Changed
- `AddressManager` now exposes `send_payload` for pre-serialized frames (used by the supervisor).

## [0.1.0] - 2025-10-24
### Added
- Initial public release (PGN generation, Fast Packet, AddressManager)

## [TEMPLATE]
### Added
### Changed
### Removed
### Fixed
### Security

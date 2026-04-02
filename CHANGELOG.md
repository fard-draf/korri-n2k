# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

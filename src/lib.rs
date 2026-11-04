//! `korri-n2k` library: primitives and protocols required to handle
//! NMEA 2000 frames in a `no_std` environment. The crate exposes the
//! infrastructure modules (codec, CAN bus), protocol logic (address management,
//! transport, messages), and a few prototypes.
//!
//! # Features
//!
//! `embassy` and `tokio` are mutually exclusive; enable none to only decode.
//!
//! | Feature | Target | `std` | Heap |
//! |---|---|---|---|
//! | `embassy` | bare metal | no | none |
//! | `tokio` | Linux | yes | channel buffers only |
//! | `std` | host, no runtime (implied by `tokio`) | yes | none |
//!
//! `full-pgns` compiles every supported PGN; `defmt` and `build-download` are optional.
//!
//! # Choosing PGNs
//!
//! Types are generated at build time from [CANboat](https://github.com/canboat/canboat),
//! only for the ids in a manifest (`{"pgns": [{"id": 128267, "name": "Water Depth"}]}`;
//! names are documentation). Precedence: `KORRI_N2K_MANIFEST_PATH` (absolute
//! path) → `full-pgns` → default manifest (common PGNs). Counts and rejected
//! PGNs: `build_core/var/pgn_manifest*.json`.
//!
//! # Guides
//!
//! - Claim, send, receive: `protocol::management::address_supervisor`
//! - Claim without a runtime: [`protocol::management::address_claiming::engine`]
//! - Fast Packet: [`protocol::transport::fast_packet`]
//! - Writing a driver: [`protocol::transport::traits::can_bus::CanBus`]
//! - Decoding: `cargo run --example quickstart`, `lookup_enum_usage`, `iso_name_usage`
//!
//! # Limits
//!
//! - Fast Packet reassembly is not wired into the receive path.
//! - No ISO Transport Protocol (60160, 60416) and no ISO Commanded Address (65240).
//! - No pseudo-random delay before a claim: nodes starting together collide.
//! - PGNs with runtime-sized fields or several repeating groups are not
//!   generated (126208); repeating groups without a counter (126464, 129796,
//!   129805) are rejected by `from_payload()`.
//! - A maximum Fast Packet send blocks the runner for at least 62 ms.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(async_fn_in_trait)]
// The README's self-contained snippets are compiled and run as doctests, so a
// signature cannot change without the documentation failing with it.
#![cfg_attr(doctest, doc = include_str!("../README.md"))]
//==================================================================================
// use pgn::Pgn;
//==================================================================================
/// Core data types shared by the build script and the codec engine.
pub mod core;
/// Domain and low-level errors (CAN identifier construction, serialization,
/// deserialization, and related issues).
pub mod error;
/// Representation of a raw NMEA 2000 frame as it is read from the CAN bus.
pub mod infra;
/// NMEA 2000 protocol implementation: CAN transport, fast packets,
/// address management, and lookup tables.
pub mod protocol;

#[cfg(feature = "tokio")]
pub use protocol::transport::traits::korri_timer::TokioTimer;
//==================================================================================

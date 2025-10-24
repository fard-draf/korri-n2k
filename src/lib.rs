//! `korri-n2k` library: primitives and protocols required to handle
//! NMEA 2000 frames in a `no_std` environment. The crate exposes the
//! infrastructure modules (codec, CAN bus), protocol logic (address management,
//! transport, messages), and a few prototypes.
#![no_std]
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
//==================================================================================

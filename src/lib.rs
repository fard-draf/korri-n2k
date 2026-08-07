//! `korri-n2k` library: primitives and protocols required to handle
//! NMEA 2000 frames in a `no_std` environment. The crate exposes the
//! infrastructure modules (codec, CAN bus), protocol logic (address management,
//! transport, messages), and a few prototypes.
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

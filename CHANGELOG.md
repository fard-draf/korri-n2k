# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Fixed
- The cross-compilation CI jobs. `rust-toolchain.toml` pinned `stable`, and a
  rustup directory override wins over the `rustup default` the workflow sets: the
  targets were installed for 1.96.0 while cargo built on stable, which had none.
  The file is now the single source of truth and declares the targets itself.

### Docs
- Corrected the README's figures for the generated code, and dropped the build
  cost of `full-pgns` as a decision criterion.

## [0.5.0] - 2026-07-23
### Added
- `PgnValue::kind()`, returning the variant name as a `&'static str`. It lets an
  error report which type was found without copying the value itself.
- `BitReader::seek()`, moving the cursor to an absolute bit position. The
  generated polymorphic dispatch reads discriminator fields out of order.
- `tests/robustness_fuzz.rs` and `tests/codec_round_trip.rs`. The first asserts
  that no payload — truncated, oversized or random — can abort the decoder, on
  every PGN of the manifest, plus the bit cursor and the Fast Packet assembler.
  The second asserts that decode/encode/decode is a fixed point, that latitude
  survives its full range bit-exact, and that CAN identifiers round-trip over the
  whole PGN space.
- `tests/replay_real_capture.rs`, replaying a real NMEA 2000 backbone through the
  whole stack: CAN identifier, Fast Packet reassembly, then decoding. It runs on
  `tests/fixtures/backbone_sample.bin`, 43 KB covering 63 PGNs, taken from a live
  bus and anonymised — positions, MMSIs and names replaced, every structural byte
  preserved. `KORRI_N2K_CAPTURE` points it at a full recording instead.
- `BitReader::bit_cursor()` and `bits_remaining()`, so the engine can size a field
  against what the frame actually holds.

### Changed
- **BREAKING** — generated lookup enums carry an `Unrecognized(repr)` variant and
  no longer use explicit discriminants. `From<repr>` replaces the fallible
  conversion; `try_from` still resolves through the blanket impl in `core`, with
  `Error = Infallible`. The per-enum `Invalid<Name>` error structs are gone, and
  a new `const fn raw()` returns the wire value. BITLOOKUP enums are unaffected:
  they name bit positions, not field values.
- **BREAKING** — `CodecError::DataTypeMismatch` no longer carries the offending
  value. Its `value: PgnValue` field is replaced by `value_kind: &'static str`,
  holding the variant name.

  The old field embedded a whole `PgnValue`, whose `Bytes` variant holds an
  inline payload buffer. That made `CodecError`, `SerializationError` and
  `DeserializationError` 264 bytes each, so every `Result` in the codec moved
  264 bytes through every `?`, on targets with as little as 20 KB of RAM. The
  value was never read: it only fed the `Display` output.

  The three types now measure 32, 40 and 40 bytes on a 64-bit host, and about
  half that on a 32-bit target. Error messages keep naming the type that was
  found, so the diagnostic value is unchanged.
- `FieldKind` now derives `Copy`. It is a field-less enum, and the missing
  derive was forcing a pointless clone on the deserialization error path.

### Licensing
- Now dual `MIT OR Apache-2.0`, at your option — the Rust ecosystem convention.
  MIT alone kept GPLv2 compatibility but offered no patent grant; Apache-2.0
  alone would have lost that compatibility. `LICENSE` becomes `LICENSE-MIT`,
  joined by `LICENSE-APACHE` and a `NOTICE`.
- The `NOTICE` settles a compliance gap: `canboat.json` is Apache-2.0 and ships
  inside the crate, but no copy of that licence did. It and the code generated
  from it — PGN names, descriptions, lookup variants — stay under Apache-2.0,
  © Kees Verruijt.

### Removed
- `embedded-can`, unused anywhere in the crate.
- `embassy-executor` from the dev-dependencies, unused.
- The `macros` feature of the optional `tokio` dependency; no macro of it is used
  outside the dev-dependency, which pulls `full` on its own.
- `.cargo-ignore` removed because unused.

### Fixed
- `STRING_LAU` fields are off by one no more. The declared length counts itself
  and the encoding byte, so the text is two bytes shorter — the reader took one
  byte too many and the writer emitted one more than it announced. Reader and
  writer being wrong in the same direction, round trips passed while every frame
  on the wire was malformed. 129285 never decoded a real message.
- A repeating group whose counter reads all ones is empty, not full. That value
  is the N2K "not available" sentinel: a GNSS receiver with no DGNSS station
  sends 0xFF, which was read as 255 reference stations. Every real 129029 on the
  bus was rejected. The count is now also clamped to what the frame carries, so a
  short group yields the repetitions present instead of discarding the message.
- A `STRING_FIX` reads what the frame holds rather than its declared capacity.
  Navico 130821 reserves 230 bytes for its text and sends about 155, the real
  length being carried by the Fast Packet header; the field is a maximum, not a
  fixed size. Same for Simnet 130856.
- A truncated CAN frame no longer aborts the decoder. `BitReader::seek` accepts
  any position, so the cursor could sit past the end of the buffer; computing the
  remaining room then underflowed. Any short frame addressed to a polymorphic PGN
  was enough to bring a node down. All cursor arithmetic is now saturating.
- Scaled values round instead of truncating on encode. Dividing by the resolution
  and casting shifted every field one LSB towards zero: 23.767 at 0.001 became
  23.766, and 29 of the 42 default PGNs failed to survive a decode/encode cycle.
- A `Duration` wider than 24 bits and carrying a resolution is held in an `f64`.
  That kind takes its own branch in the generator, which kept the old 32-bit
  threshold: 21 fields across 12 PGNs stayed `f32` while the engine had moved to
  `F64`, so their setter rejected every payload and the PGN decoded nothing.
- A field wider than 24 bits and carrying a resolution is held in an `f64`. `f32`
  represents every integer only up to 2^24, so a 32-bit latitude scaled by 1e-7
  drifted by up to 67 raw units — 0.75 m on the ground. 25 fields across 15 PGNs
  of the default manifest are affected, and those PGNs grow by 4 bytes per field:
  129025 goes from 8 to 16 bytes.
- A lookup field narrower than its enum's repr decodes. The repr follows the
  CANboat `MaxValue`, a declared range rather than a width: `ENTERTAINMENT_PLAY_STATUS`
  declares 65535 but rides in 8 bits, so the engine yielded `U8` against a setter
  expecting `U16`. The generated accessors now take the field's bit length and
  convert through the repr.
- `DYNAMIC_FIELD_KEY`, `DYNAMIC_FIELD_LENGTH` and `FIELD_INDEX` map to `Number`.
  They are fixed-width integers despite the naming — they carry a key, an index
  or a length — but fell through to `Unimplemented` and were rejected.
  `DYNAMIC_FIELD_VALUE` and `VARIABLE` stay unsupported: they have no `BitLength`.
- `FLOAT` fields decode. CANboat defines them as raw IEEE-754 singles, but the
  engine handled the kind in neither direction and rejected them with
  `UnsupportedFieldKind`, while the generator typed them as signed integers —
  their resolution of 1 put them on the scaled-integer path. 10 fields across
  129045, 130321 and four Garmin autopilot variants of 126720, all `full-pgns`.
- AIS Class A and B position reports (129038, 129039) decode. Both defects below
  hit them, and both PGNs ship in the default manifest, so no frame could be read.
- `BINARY` fields that do not fill whole bytes are read and written as scalars.
  The generator already typed them that way; the engine rejected them with
  `InvalidFieldBits`. 9 fields, including the 19-bit `CommunicationState` of
  129038, 129039, 129793 and 129798.
- A lookup value CANboat does not name no longer sinks the whole message. Every
  lookup enum gains an `Unrecognized(repr)` variant holding the raw value, and
  `From` replaces the fallible conversion. `TIME_STAMP` names only 60..=63 while
  0..=59 are ordinary seconds, so an AIS frame died on its timestamp; 85 lookup
  fields of the default manifest had enums narrower than their bit range.

  Cost: a lookup field now occupies twice its repr — tag plus payload. PGN 60928
  goes from 16 to 20 bytes, `ManufacturerCode` from 2 to 4. `generated_sizes_test`
  records the new figures.
- `BITLOOKUP` fields wider than 8 bits now deserialize. The generated
  `field_mut` demanded a `PgnValue::U8` for every bitmask regardless of width,
  while the engine and the struct field agree on U8/U16/U32/U64 by bit length.
  Any wider bitmask therefore failed with `FieldAssignmentFailed`, taking the
  whole message down with it.

  A bitmask is a plain unsigned integer, not an exclusive enum, so it no longer
  takes the `Lookup` path in the setter — the same generic path the getter
  already used. 7 fields across 3 PGNs were affected, including the two
  `Discrete Status` fields of 127489 (Engine Parameters Dynamic), which ships in
  the default manifest.
- Polymorphic PGNs are now selected by every field carrying a CANboat `Match`,
  not by field #1 alone. That first field is `Manufacturer Code` on every
  proprietary PGN, so all variants of one manufacturer collided: `from_payload`
  returned the first one and deserialized the payload with the wrong descriptor,
  silently and without an error. A Simnet 65305 "Pilot Mode" frame decoded as
  "Device Status".

  6 PGNs and 64 variants were affected — 126720, 130850, 65305, 130842, 130851
  and 130825, all of them `full-pgns` only. The default manifest never contained
  one. Variants are now emitted most-constrained-first, so a specific variant is
  tried before the generic one it would otherwise be swallowed by.
- Removed the crate-wide `allow(clippy::large_enum_variant, clippy::result_large_err)`.
  It was silencing the very lint that reported the oversized errors above.
  The three enums that legitimately carry an inline buffer — `PgnValue`,
  `SupervisorCommand` and `ProcessResult` — now carry a local `allow` explaining
  why: the lint suggests `Box`, which needs `alloc`, and this crate has none.

### Internal
- `cargo clippy` now fails the CI on any warning, and the toolchain is pinned to
  1.96.0. Clippy ran without teeth before: warnings printed, the job stayed
  green, and the lint that reported the oversized errors above went unheeded for
  releases.
- `build.rs` watches `build_core/`. Editing the generator otherwise left stale
  code in `OUT_DIR` and produced compiler errors matching no source.

## [0.4.0] - 2026-07-22
### Added
- `AddressClaimStrategy` enum with `Fixed`, `SelfConfigurable` and
  `Arbitrary` variants, letting a device declare how it may claim addresses.
- `ClaimError::InconsistentStrategy`, returned when the NAME's AAC bit
  contradicts the chosen strategy.
- Session timeout: an incomplete session whose slot goes 500 ms without a new
  fragment is released and its slot reused. Late fragments are ignored rather
  than appended to stale data.
- Four cumulative diagnostic counters on `FastPacketAssembler`, read through
  `expired_sessions()`, `pool_exhausted()`, `lost_fragments()` and
  `rejected_frames()`. The first three report dropped data and should stay at
  zero on a healthy bus:
  - `expired_sessions` — an incomplete session was reclaimed after timing out.
  - `pool_exhausted` — a new message was refused, every slot being in use.
  - `lost_fragments` — a continuation frame arrived out of sequence, or with no
    live session behind it.

  The fourth is not a loss and is expected to be non-zero on a real bus:
  - `rejected_frames` — a first frame announced a size outside the Fast Packet
    range, so the frame is not a message this assembler handles.

### Changed
- **BREAKING**: `claim_address` now takes an `AddressClaimStrategy` instead
  of a raw `preferred_address: u8`.
- AAC capability is no longer inferred from NAME bit 63 alone.
- **BREAKING** — a node may now claim any address in `0..=251`, up from
  `0..=207`, and an Arbitrary Address Capable node walks upwards from its
  preferred address over that whole range instead of jumping into `128..=207`.

  J1939-81 reserves `0..=127` for SAE-assigned preferred addresses and keeps
  `128..=247` for self-configurable nodes, but NMEA 2000 does not inherit that
  split. The reference C++ stack caps addresses at `251`, its documentation
  states that "each device will get device source address (0-251)", and a
  28-minute capture of an 18-device bus shows every node — all Arbitrary
  Address Capable — between 0 and 43. The previous range parked a node in an
  empty part of the bus, where arbitration never happens.

  `MARINE_DYNAMIC_START`, `MARINE_DYNAMIC_END` and `MARINE_DYNAMIC_COUNT` are
  replaced by `MIN_CLAIMABLE`, `MAX_CLAIMABLE` and `CLAIMABLE_COUNT`.
- **BREAKING** — `FastPacketAssembler::process_frame` now takes `now_ms: u32`
  and `pgn: u32` in addition to the source address and payload. Sessions are
  keyed by (source, PGN, sequence id), so a device interleaving two Fast Packet
  PGNs no longer corrupts either message.

### Internal
- `cargo clippy` is clean again. The `allow` covering the code generated by
  `build.rs` was an outer attribute, so it only applied to the single `use`
  that followed it and left every generated item linted; it now sits on the
  including modules. Also removes a no-op `& 0xFF` on a `u8` in `IsoName` and
  two `drop()` calls on futures that do not implement `Drop`. No behaviour or
  API change.

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

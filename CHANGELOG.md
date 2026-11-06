# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

This cycle moves the bundled CANboat database from v7.1.0 (schema 2.5.0) to
v9.1.0 (schema 2.8.0). Most of what follows is the generated code tracking
upstream: renamed lookups, corrected layouts, new variants. The engine gains
`MinLength`, so a device built against an older layout of a PGN is decoded
instead of rejected.

### Added
- PGN 59392, ISO Acknowledgement, in the default and full manifests. Nothing
  sends it yet: answering a Request with a NAK is not part of this change.
- `MinLength` support. A payload shorter than `Length` but at least
  `MinLength` long now decodes, its missing tail read as 0xFF, the N2K "not
  available" value. NMEA 2000 keeps appending fields to existing PGNs, devices
  built against the earlier layout keep sending it, and CANboat 9.1.0 now
  publishes that shorter form. The default manifest has six such PGNs: 126996,
  129038, 129039, 129794, 129809 and 129810. `full-pgns` adds 127258, 127506
  and 129793. Below `MinLength`, an absent field is still missing data and
  still an error.
- `PgnDescriptor::min_length`, carrying that bound.
- Several logical addresses on one node. `AddressManager::with_controllers`
  takes one `(IsoName, AddressClaimStrategy)` per Controller Application; a
  single runner drives them all on one bus. `into_multi_parts()` returns one
  `AddressHandle` per Controller Application and a shared `AddressFrames`.
  Under embassy, `AddressService::with_controllers` takes one command channel
  and one `ClaimedAddress` per Controller Application, and panics if two share
  one. Local emissions are looped back, so two local NAMEs aiming at the same
  address arbitrate as two devices would, siblings see each other's traffic on
  `AddressFrames` (Fast Packet included), and a Controller Application answers
  its own global Request for Address Claim, as J1939-21 requires.
  Single-address code is unchanged.
- The `ADDRESS` field type, new in CANboat 9.1.0 for a one-byte bus address.
  It maps to `Number`. Its 20 fields were plain `NUMBER` in 7.1.0 and would
  otherwise have fallen through to `Unimplemented`. All are `full-pgns` only:
  65240, 65332, 130840, 130845, 130846, 130850 and 130851.
- INDIRECT_LOOKUP fields wider than 8 bits. `FUSION_SETTING_VALUE`, used by
  126720 and 130820, is the first: both halves of its key ride in 32-bit
  fields. Such a field is now typed by its bit length instead of always `u8`,
  and the encoder writes its raw value instead of the low byte alone. The
  generated `get_` helper returns `None` when either half does not fit a byte.
- 23 lookup types, among them `YesNo1bit`, `GnssSystem` and
  `FusionSettingValue`. CANboat 9.1.0 also defines 48 new PGN variants, mostly
  Fusion, Furuno, Garmin, Simnet, Sleipner and Navico proprietary messages,
  generated under `full-pgns` wherever the manifest enables their PGN.
- Passive monitoring APIs (`no_std`, zero allocation, zero transmission):
  - `protocol::messages::descriptor(pgn)` and `descriptor_for(pgn, payload)` to
    resolve a static `PgnDescriptor` at runtime, including multi-variant
    proprietary PGNs with the same discriminators and order as `from_payload`.
  - `Availability` (`Available`, `NotAvailable`, `OutOfRange`, `Reserved`),
    `FieldDescriptor::availability(raw)` comparing against the exact per-field
    CANboat sentinels (`UnknownValue`, `OutOfRangeValue`, `ReservedValue`, plus
    `MMSI == 0`), and `infra::codec::engine::string_fix_trimmed` for fixed-width
    ASCII fields.
  - `infra::codec::engine::for_each_field` and `FieldReading`, visiting every
    non-reserved field of a payload (repeating groups and `MinLength` padding
    included) and exposing both the scaled `PgnValue` and the raw wire integer.
  - `protocol::lookups::lookup_name` and `lookup_name_indirect`, returning the
    original CANboat labels for the lookups referenced by the active manifest.
  - Runnable example `monitor` (`cargo run --example monitor --features std`).
- PGN 127233, Man Overboard Notification, under `full-pgns`: `Pgn127233`. It was
  listed as unsupported only because of its multi-line explanation.
- `FastPacketAssembler` takes the size of its session pool as a const
  parameter, `FastPacketAssembler<const SESSIONS: usize = 4>`, built with
  `FastPacketAssembler::<8>::with_pool(table)`. `new()`, `with_pgns()` and
  `Default` keep the pool of 4, so existing code compiles unchanged. A full
  pool still refuses the next message and evicts nothing. Each session holds a
  223-byte buffer: the assembler is 1064 bytes at 4 and 2088 at 8.

### Changed
- Every Fast Packet frame `FastPacketBuilder` emits now has DLC 8, its unused
  bytes 0xFF. The last frame, and the single frame of a short Fast Packet PGN,
  used to carry only the bytes in use (DLC 7 for the last frame of a 26-byte
  127489). Other senders observed on a bus pad to 8, and a receiver may drop
  a shorter frame. Single-frame PGNs outside Fast Packet keep their length.
- **BREAKING**: `FieldDescriptor` gains `unknown_value: Option<i64>`,
  `out_of_range_value: Option<i64>` and `reserved_value: Option<i64>`. A
  descriptor built by hand must set them (`None` disables sentinel matching).
- **BREAKING**: a payload announcing more repetitions than the generated array
  holds is now rejected with the new
  `DeserializationError::RepetitionsExceedCapacity { array_id, count, max }`,
  by decoding and by `for_each_field`. It used to decode successfully with the
  surplus dropped: 33 `TestId`/`TestResult` pairs of 130944 came back as 32,
  and re-encoding wrote a counter of 32. The generator still caps an array at
  32 elements (`RepeatingFieldSet::max_repetitions`).
- **BREAKING**: `FastPacketAssembler::process_frame` now takes `now_ms: u64`
  instead of `u32`, matching `Clock::now_ms` and `AddressClaimEngine::poll`.
- **BREAKING**: `canboat.json` is CANboat v9.1.0. Every generated type follows
  it; the entries below are the ones a caller is likely to hit.
- **BREAKING**: one-bit yes/no fields use `YesNo1bit` instead of `YesNo`. In
  the default manifest: `Pgn60928::arbitrary_address_capable`, and
  `integrated_display`, `dsc` and `can_handle_msg22` of `Pgn129039`.
  `full-pgns` adds fields of 126983, 128520, 129041, 129801 and 65314. The
  variants are the same (`No`, `Yes`, `Unrecognized`), so replacing the type
  name is enough. `YesNo` remains for the wider fields.
- **BREAKING**: `ManufacturerCode` drops legal suffixes and fixes upstream
  typos. 84 variants are renamed or removed: `ArksEnterprisesInc` becomes
  `ArksEnterprises`, `HondaMotorCompanyLtd` becomes `HondaMotor`, `Nobletec`
  becomes `Nobeltec`, `BavariaYacts` becomes `BavariaYachts`.
- **Behaviour change.** Two `ManufacturerCode` variants keep their name but not
  their value, so code naming them still compiles and now emits another code.
  `BepMarine` goes from 116 to 295 and `HumminbirdMarineElectronics` from 467
  to 476: CANboat folded the duplicate entries `BepMarine2` and
  `HumminbirdMarineElectronics2` into them. Codes 116, 175, 200, 225, 250,
  311, 467 and 645 are no longer named and decode as `Unrecognized`.
- **BREAKING**: in PGN 129540, `PrnInfo::reserved11` becomes
  `gnss_system: GnssSystem`. The four bits CANboat called reserved now name the
  constellation the satellite belongs to.
- **BREAKING**: PGN 129039 and 129794 gain a trailing `sequence_id: u8`. 129039
  keeps its 27 bytes: its 15-bit trailing reserved field was 7 reserved bits
  followed by that byte. 129794 grows from 75 to 76 bytes. Its 75-byte form
  still decodes through `MinLength`, with `sequence_id` reading 0xFF.
- **BREAKING**: `ClaimFault` gains `DuplicateName`, returned when two
  Controller Applications of one node share a NAME. An exhaustive `match` must
  handle it.
- **BREAKING**: `PgnDescriptor` gains `min_length: Option<u16>`. A descriptor
  built by hand must set it; `None` keeps the previous behaviour.
- **BREAKING**: lookup variants renamed after upstream typo fixes.
  `CzoneAlarmType::ShuntMistmatch` becomes `ShuntMismatch`,
  `NavicoDataType::GeoidalSeperation` becomes `GeoidalSeparation`, and
  `Seatalk1Command::DeviceIndentification` becomes `DeviceIdentification`.
  `FusionMessageId` fixes `DabUpdtateCommand`, `SetLowPassFiler` and
  `ScanMultirooomDevices`, and drops the `Fapi` prefix of four variants.
  `BandgKeyValue::PitchAngle` gives way to `YawRate` and `PitchRate`.
  `SimnetBacklightLevel` loses `DayMode` and `NightMode`.
- **Behaviour change.** Three `FusionSetting` variants change value:
  `MaxVolumeStart` goes from 75 to 76, `BtAutoConnect` from 76 to 77 and
  `NullSetting` from 77 to 1000.
- `EntertainmentPlayStatus` is `repr(u8)`, down from `u16`, and its
  `Unrecognized` variant holds a `u8`. CANboat lowered its `MaxValue` from
  65535 to 255, the width of the field carrying it.
- **Behaviour change.** PGN 127751 (DC Voltage/Current) is single frame, no
  longer Fast Packet. It leaves `FAST_PACKET_PGNS_ALL`, which drops from 182 to
  181 entries, so an assembler built on that table no longer reassembles it.
  The default manifest does not contain it.
- `full-pgns` layouts corrected upstream. 129793 grows from 24 to 27 bytes.
  The Simnet 130850 command variants all declare 12 bytes, instead of 11 or 14.
  Airmar 130944 becomes a repeating group of test results. Furuno 130842 names
  its roll, pitch and yaw rates. The reactive power of 65006 and 65009 becomes
  signed. Garmin 126720 variants type their message identifiers as lookups.
- The `build-download` feature fetches `canboat.json` from the `v9.1.0` tag
  instead of `master`, so a build no longer depends on the day it runs.
- `NOTICE` names the bundled database: schema 2.8.0, CANboat v9.1.0.
- **BREAKING (build)**: a manifest that cannot be honoured now fails the build
  instead of compiling with fewer PGNs. A manifest without 60928 stopped on an
  `unresolved import` of `Pgn60928` that never mentioned the manifest, and the
  `README` example was one of them. A manifest id that is unknown to CANboat or
  refused by the generator (e.g. 129792) was only a `cargo:warning`. Polymorphic
  PGNs with some variants refused (126208, 130820) still stop in rustc, not in
  the build script. A non-UTF-8 `KORRI_N2K_MANIFEST_PATH` is no longer ignored.
- **BREAKING (build)**: a `KORRI_N2K_MANIFEST_PATH` that points to a missing
  file now fails the build, naming the path, instead of falling back to the
  default manifest with a warning. The variable set but empty fails too: unset
  it to get the default manifest.

### Fixed
- The generator treated CANboat's `Fallback` entries as real messages. A
  fallback is a catch-all for a whole PGN range and takes the number of the
  first PGN of that range, so 59392 (ISO Acknowledgement) was generated twice
  and a manifest listing 59392 did not compile. A fallback is now skipped when
  a real entry has the same number, and kept when it is alone (61440, 126976).
- Changing `KORRI_N2K_MANIFEST_PATH` did not invalidate the generated code: the
  build kept the code of the previous manifest. The build script now reruns
  when the variable changes.
- Five PGNs of the default manifest rejected the shorter payload of a device
  built against their earlier layout: 126996, 129038, 129039, 129809 and
  129810. The message failed with `InvalidDataLength` and was lost whole. They
  now decode through `MinLength`.
- Simnet 130850 decodes every message of the replay capture. Four of its ten
  failed against the 14-byte length CANboat 7.1.0 declared, while the autopilot
  sends 12.
- A signed `DURATION` field was read as unsigned, so a negative value came back
  huge: UTC-5 in `Pgn129033.local_offset` (raw `D4 FE`) decoded as 3914160 s
  instead of -18000 s. Encoding was already right. The 24 signed `DURATION`
  fields are all outside the default manifest (`full-pgns` or a custom one).
- A Fast Packet PGN whose payload fits in 8 bytes was sent as a bare CAN frame,
  without the sequence and length header, so every receiver misread it: PGN
  129540 (GNSS Satellites in View) with no satellite is 3 bytes. Framing now
  follows the PGN: any Fast Packet PGN, whatever its length, goes out as Fast
  Packet frames, and any other payload over 8 bytes still does. The inter-frame
  delay follows the frame count instead of the payload length. No receiver has
  seen these frames yet, so interoperability is not proven.
- `FrameIterator` returned the same identifier error on every call: a
  destination-specific PGN built without a destination yielded `Err` forever,
  so a loop that skips errors never ended. It now yields the error once, then
  `None`.
- Network discovery listed addresses 254 and 255: a node in Cannot Claim and
  the invalid source. Neither can be sent to, and they used up the caller's
  buffer. A claim from an address above 251 is now ignored.
- `SendRawFrame` accepted a `CanFrame` whose `len` exceeded its 8 data bytes,
  and a driver slicing `data[..len]` would panic and end the runner. The
  supervisor now drops such a frame, as it already does for a `SendPayload`
  with a bad length.
- The generator copied CANboat's multi-line `Explanation` into a single `///`
  line, so every following line was bare text and the generated code did not
  compile: 127233 (Man Overboard Notification) failed that way with a custom
  manifest. Each line now gets its own `///`, and the descriptor `description`
  and field `name` strings are escaped. The generated code of the default,
  `full-pgns` and `{60928, 129033}` builds differs only by the two blank lines
  that followed the doc of 65332.
- A Fast Packet first frame repeated by its sender (same source, PGN and
  sequence ID) opened a second session slot instead of restarting the first.
  The continuations then matched the stale slot, were dropped as out of
  sequence, and the message was lost. The first frame now restarts the session.
  Messages of one PGN from one source with different sequence IDs still get
  one slot each. The destination is not part of the key, and messages a source
  aborts under different sequence IDs each hold a slot for 500 ms.
- The dispatch enum of a multi-variant PGN (e.g. `Pgn130944`) did not forward
  the four repeating-group methods of `FieldAccess`, so `repetitive_count` and
  its siblings answered `None` on the enum while the variant held the data.
  Every enum now delegates them, as it already did `field` and `field_mut`.
- A payload that stopped inside a fixed-size string was accepted, whatever the
  string: 129044 (Datum) decoded from 16 to 19 bytes with `ReferenceDatum`
  half missing. Only the text of 130821 and 130856, which reserve about 230
  bytes and are sent shorter, may stop early now; any `STRING_FIX` of 32 bytes
  or less must be complete. Under `full-pgns` this also applies to the trailing
  strings of 129045, 130817, 130833 and 130842.
- A field declared after a repeating group was read, written and visited
  before it, so the codec disagreed with the wire layout. 130571 (Library Data
  Group, `full-pgns`) is the only generated PGN with one: its `Artist` string
  follows the `IdType`/`Id`/`Name` group. A group is now handled where it
  starts in the descriptor, by decoding, encoding and `for_each_field`.

### Removed
- **BREAKING**: the `AirmarFilter`, `FusionSiriusAlert` and `Seatalk1Att`
  lookup types, dropped by CANboat.
- **BREAKING**: `Pgn126720MaretronSlaveResponse` and `Pgn130817NavicoUnknown`,
  and the matching variants of their dispatch enums. Both were `full-pgns`
  only.
- **BREAKING**: `Pgn1267200x1Ef00ManufacturerProprietaryFastPacketAddressed`,
  the struct generated from CANboat's 126720 fallback. No dispatch enum
  referenced it. `full-pgns` only. A custom manifest listing 65280, 61184,
  126208 or 130816 loses the struct of that PGN's fallback too, e.g.
  `Pgn652800xff000XffffManufacturerProprietarySingleFrameNonAddressed`.

### Internal
- Dead generator code removed: `derive_struct_name_from_counter` and its test,
  `set_lookup_bit_map` with the unused `lookup_bit_map` parameters it fed,
  `_FORBIDEN_PGN`, and a commented-out copy of `map_type`. The generated code is
  unchanged. The `reason` of eleven `unsupported` entries of
  `pgn_manifest.full.json` was wrong: they compile alone (127233 since the
  multi-line fix) and collide on the name of a repeating-group struct, e.g.
  `WpIdInfo` for 130067, 130068 and 130074.
- CI runs `cargo test --features full-pgns` and checks the build with a minimal
  custom manifest (60928 only). Neither was built before.
- `tests/replay_real_capture.rs` requires zero decode failures. The exception
  it kept for 130850 has no reason left.
- `tests/test_min_length.rs` pins the three cases of 129794: 76 bytes read the
  trailing field, 75 bytes read it as 0xFF, 74 bytes are rejected.
- `tests/test_indirect_lookup_32bit.rs` round-trips a 32-bit indirect value
  above 255 through 126720, which the low-byte encoder used to truncate.
- The generator reads the `MaxValue` of an indirect lookup as `u32`.
  `FUSION_SETTING_VALUE` declares 4294967295, and the `u8` it was read into
  skipped the whole lookup with a mere build warning.

## [0.7.1] - 2026-09-23

### Added
- `AddressHandle::claim_status()`, exposing `Claiming(address)`,
  `Claimed(address)`, and `CannotClaim` without changing the existing
  `claimed_address()` contract.

## [0.7.0] - 2026-08-06

This cycle turns address management into an I/O-free engine driven by a runner.
The engine takes a clock reading and an optional frame, and returns an action.

### Added
- `AddressClaimEngine`, the synchronous claim engine. It handles the initial
  claim, NAME arbitration, defence, loss and reclaim, and Cannot Claim.
- `AddressClaimEngine::poll(now_ms, rx)`, the single entry point. It never
  sleeps and never reads the bus, so it can be tested and replayed without a
  runtime.
- `ClaimOutput`, what one `poll` decided along three independent axes. `tx` is
  the frame to emit, `status` the state reached, `wake_at_ms` the absolute
  deadline. None of them folds into another. A defence frame handed back on the
  very poll that acquires the address is no longer lost.
- `ClaimStatus`, the state the caller sees: `Claiming(addr)`, `Claimed(addr)`
  or `CannotClaim`. `ClaimStatus::claimed_address()` returns the address only
  once it is held. `Claiming` is not yours yet.
- `AddressClaimEngine::claimed_address() -> Option<u8>`. `None` replaces the
  address 254 that used to stand for "no address".
- Incoming ISO Request (PGN 59904 asking for 60928) is now answered. A node
  holding an address answers with its claim. A node that exhausted its strategy
  answers with a Cannot Claim, as J1939-81 requires. A node that has not started
  yet opens its campaign, and the claim it emits is the answer.
- `AddressHandle::send_command` under `embassy`. Only `tokio` had it.
- `classify_claim` and `ClaimRelation`, turning a received frame into one of
  five relations. A claim with source address 254 never becomes a local
  conflict.
- `AddressRequester`, the same I/O-free shape for network discovery, now
  reachable: its module was private.
- `ClaimedAddress` and `AddressHandle::claimed_address()`. Best effort: it
  reports what the engine holds now, and a later conflict can still take the
  address away.
- `SendPgnError::NotClaimed`. Emitting before an address is acquired now
  returns this instead of a silent `Ok(())`.
- `ClaimFault`, for claim failures decided from frame content alone.
- `AddressHandleError::RunnerGone`, returned when the runner is no longer there
  to execute a queued command.
- `FastPacketBuilder::with_priority`, setting the priority of a built frame.
- Three runnable examples, `address_claim`, `fast_packet_rx` and
  `blocking_claim`, all run by CI. The last one drives the engine from a plain
  loop, with no runtime, no timer trait and no `CanBus` implementation.

### Changed
- **BREAKING**: the `managment` module is spelled `management`. Replace
  `protocol::managment::` with `protocol::management::` in your imports; nothing
  else moves.
- **BREAKING**: `ClaimError<E>` splits in two, the bus-free faults moving to
  `ClaimFault`. Migration: `ClaimError::InconsistentStrategy` becomes
  `ClaimError::Fault(ClaimFault::InconsistentStrategy)`, and likewise for
  `RequestAddressClaimErr` and `Extraction`.
- **BREAKING**: `AddressManager::new` is no longer `async` and returns
  `Result<Self, ClaimFault>`. It only validates that the NAME and the strategy
  agree; the claim now happens under the runner.
- **BREAKING**: `AddressManager::current_address` becomes `claimed_address` and
  returns `Option<u8>`.
- **BREAKING**: the runner owns the single `select`. Each round polls the engine
  synchronously, publishes the status, emits `tx` if there is one, then waits on
  the bus, the command channel and `wake_at_ms` at once. No state transition can
  be cancelled halfway, and an emission is never skipped because the status said
  the machine had moved on.
- `Clock::now_ms` states its contract: monotonic, free of wall-clock steps, and
  wrap-free for the life of the engine. Deadlines are built with
  `saturating_add`, so a reading near `u64::MAX` pins them instead of
  overflowing.
- **BREAKING**: `AddressService::claim` becomes `AddressService::with_name` and
  is no longer `async`. It has nothing left to wait for.
- **BREAKING**: `SupervisorCommand::SendFrame` becomes `SendRawFrame` and is now
  guarded. A frame whose source address is not the claimed one is refused.
- **BREAKING**: a refused command no longer stops the runner. Only a bus failure
  does. A rejected command is dropped, and says so only under `defmt`.
- **BREAKING**: `AddressSupervisorRunError` keeps `Receive(E)` and `Send(E)`.
  Bus errors reach the caller unwrapped.
- **BREAKING**: `default = []`. Pick `embassy` or `tokio` explicitly; neither is
  enabled by default. docs.rs builds with `embassy`.
- **BREAKING**: `IsoName` replaces `u64` wherever a NAME is stored or compared.
  `u64` remains the wire format only.
- Incoming frames now reach the application unfiltered, PGN 60928 included, so
  network discovery can see claim traffic too.
- `TokioTimer::now_ms` returns `u64` instead of a `tokio::time::Instant`, and
  `TokioTimer::new` is public.
- Claim delays moved to `constants::iso_delay`. `CANNOT_CLAIM_RETRY_DELAY_MS` is
  a tuning knob, not a protocol constant.
- The README is rebuilt around use cases. Its self-contained snippets run as
  doctests, and the examples are built and run by CI.

### Fixed
- The application channel could stall address management. The runner forwarded
  each frame with an awaiting send, so a full channel delayed the action the
  engine had just decided. A lost conflict could move the internal state without
  the new claim ever reaching the bus. Forwarding is now non-blocking and drops
  instead.
- `AddressHandle::claimed_address` kept reporting an address after the runner
  had stopped. Nothing defends that address any more, so the reading invited the
  application to emit from an address it no longer held. `AddressRunner` now
  clears it on `Drop`, which also covers a cancelled task.
- `SupervisorCommand::SendPayload` panicked on an out-of-range `len`. The field
  is public and the caller owns the channel under embassy; it is now validated.
- Network discovery sent its ISO Request with source address 255. That is the
  broadcast destination, never a valid source; `AddressRequester::new` now takes
  the address the node holds.
- A claim campaign could stall forever. In `Claiming`, any harmless frame
  arriving on the deadline returned `Wait(0)` and the deadline was never
  reached again.
- `Cannot Claim` is no longer terminal. A single-address node that lost its only
  address stayed dead until reboot, although the conflict is usually transient.
- `AddressRequester` used a relative deadline, which never expired.

### Removed
- **BREAKING**: `claim_address()`. It returned `Ok(254)` on Cannot Claim, which
  no longer means anything now that the state retries. Drive the runner instead.
- **BREAKING**: `AddressManager::handle_frame`, `recv`, `defend` and `reclaim`.
  The manager no longer duplicates the claim rules; the engine owns them.
- **BREAKING**: the `ToPayload` and `FromPayload` traits. Nothing referenced
  them; the codec goes through `PgnData`.
- **BREAKING**: `ClaimFault::NoAddressAvailable`, `UnvailableName` and
  `BuildErr`. An exhausted iterator yields Cannot Claim, and nothing constructed
  the other two.
- **BREAKING**: the `CanBusBlocking` trait. It had no implementor and no caller,
  and its unbounded `recv()` would hang a claim on a quiet bus. See the
  `blocking_claim` example for the shape a blocking driver actually needs.
- `ClaimError::NetworkConflict`, `CanBusError`, `InvalidIncomingFrame` and
  `InvalidDataLen`. None was ever constructed, and the last two duplicated
  `ExtractionError`.

## [0.6.0] - 2026-07-26
### Added
- `FAST_PACKET_PGNS` and `FAST_PACKET_PGNS_ALL`, generated from canboat by the
  build script and re-exported from `protocol::transport::fast_packet`. The first
  holds the Fast Packet PGNs of the active manifest, the second every one canboat
  declares (182), regardless of the manifest. Both are `const`, so the linker drops
  the one a binary does not reference.
- `FastPacketAssembler::with_pgns`, building an assembler over a caller-supplied
  table, and `handles`, asking whether it treats a PGN as Fast Packet.
- `FastPacketAssembler::unknown_pgn`, counting frames whose PGN is absent from the
  assembler's table. Kept apart from `rejected_frames`, which stays reserved for a
  first frame announcing a size no Fast Packet can carry.

### Changed
- **Behaviour change.** `FastPacketAssembler::process_frame` now ignores any PGN
  outside its table instead of opening a session for it. On a live bus, ordinary
  single-frame PGNs such as 127250 at 10 Hz match the first-fragment pattern and
  used to starve the four-slot session pool.

  A proprietary PGN absent from canboat, or a Fast Packet PGN absent from the
  manifest, is therefore no longer reassembled by `new()`. Migrate with
  `FastPacketAssembler::with_pgns(FAST_PACKET_PGNS_ALL)`, or your own sorted table
  for proprietary PGNs. Note that `full-pgns` is not a substitute: its manifest
  lists 152 of the 182 Fast Packet PGNs canboat knows, so a gateway needs
  `with_pgns` whatever its features. On a 420k-frame backbone capture the full
  table reassembles 24738 messages against 20631 for the manifest one, the
  difference being 126208, 130822, 130824, 130845 and 130846.

  Callers that also decode single-frame PGNs must branch on `handles` before the
  call. `ProcessResult::Ignored` cannot distinguish a single-frame message from a
  lost fragment, and never could.

  Released as 0.6.0 rather than a patch: no signature breaks, but frames 0.5
  accepted are now dropped.

### Fixed
- `FastPacketAssembler` refused Fast Packet messages shorter than eight bytes.
  The lower bound was ours: neither `ttlappalainen/NMEA2000` nor
  `akeamc/nmea2000` sets one, and canboat declares such messages (130817 is seven
  bytes, 130824 announces two). Two changes:
  - the accepted range is now `1..=MAX_FAST_PACKET_PAYLOAD` instead of `8..=`;
  - a payload of six bytes or less rides entirely in the first frame, so
    `process_frame` now returns `MessageComplete` on that frame instead of waiting
    for a continuation that never comes. It takes no session slot.

  `rejected_frames` therefore only counts a size of zero or one above the buffer.
  On the same 420k-frame capture, Fast Packet reassembly goes from 23090 messages
  out of 24738 announced to 24738 out of 24738, with every diagnostic counter at
  zero.
- The cross-compilation CI jobs. `rust-toolchain.toml` pinned `stable`, and a
  rustup directory override wins over the `rustup default` the workflow sets: the
  targets were installed for 1.96.0 while cargo built on stable, which had none.
  The file is now the single source of truth and declares the targets itself.

### Docs
- The README's Fast Packet section now branches on `handles` before calling
  `process_frame`, so the single-frame path is visible, and it shows the gateway
  setup with `with_pgns(FAST_PACKET_PGNS_ALL)`. A new entry under Limits states
  that the assembler only covers the PGNs in its table.
- Corrected the README's figures for the generated code, and dropped the build
  cost of `full-pgns` as a decision criterion.
  
### Removed
- `embassy-time`: removed unused dep. 

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

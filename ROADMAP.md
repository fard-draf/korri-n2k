# Roadmap

Work is ordered by priority across the numbered sections and within each
section. Correctness of existing behaviour comes before new features.
Validation accompanies each change; it is not deferred until the end.
`[ ]` to do, `[~]` in progress, `[x]` done.

## Completed foundations

- [x] Several addresses on one node (`feat/multi_address`).
- [x] Shared local and GitHub software checks through the `justfile`, including
      offline tests for the HIL host oracle (`just ci` locally).
- [x] Local STM32G431 nominal Fast Packet TX/RX bench (`just hil`, PGN 127489),
      with independent host byte/framing checks and firmware exit-code handling.
      Both scenarios exercised with J-Link and WCH-Link. Real non-zero firmware
      exit and fault scenarios remain to be validated. See `hil/README.md`.
- [x] Track CANboat releases through its release notifications.

## 1. Correct the existing transport and network management

Keep this implementation order, adding regression tests and HIL scenarios as
fixes land. These corrections do not depend on completing ISO TP or obtaining
certification documents.

- [ ] Fast Packet RX: use the actual DLC, include destination in the session
      key, and handle reordered fragments, duplicates and session restarts.
      Verify timeouts, exhausted pools and complete payloads, not only counters.
- [ ] Address Claim: handle duplicate NAMEs, ignore non-global claims and add
      the NAME-seeded pseudo-random delay before the first claim and Cannot
      Claim, with non-blocking deadlines and timing tests.
- [ ] ISO Commanded Address (65240): bounded BAM reception, disabled by default,
      exact NAME matching and normal address arbitration. Leave persistence to
      the application; do not wait for general ISO TP support.
- [ ] Runtime: make Fast Packet TX progress without blocking RX. The current
      maximum-size send includes 62 ms of inter-frame waits before RX resumes,
      excluding driver time. Bound address-defence and command progress under
      tested RX/TX loads; stop submitting frames once address loss is detected.
      Document the limits for frames already queued in the driver.
- [ ] Keep network discovery under address supervision: reconcile devices by
      NAME, handle Cannot Claim and full buffers, and report duplicate NAMEs.
- [ ] Expose useful queue-drop and session-exhaustion diagnostics, reusing the
      existing assembler counters. Document capacities and overflow policies;
      hardware losses are observable only when the driver reports them.
- [ ] Finish build validation, generated-name collision fixes and documentation
      corrections, then update the STM32, ESP32 and SocketCAN examples to the
      revised APIs without losing their error handling.

## 2. Close safety and data-integrity gaps before expanding scope

- [ ] Make Fast Packet sequence allocation safe on supported targets without
      8-bit atomics. A safe API must not rely on callers to synchronise access
      to a shared `static mut`. Review the remaining unsafe/concurrency paths.
- [ ] Make `new()` on a proprietary variant initialise its `Match` fields;
      test expected wire bytes and dispatch back to the intended variant.
      Treat overlapping variant definitions as a separate limitation.
- [ ] Reject generated `MinLength` layouts whose padded tail cannot safely
      represent absence. Add accepted and rejected build fixtures rather than
      rejecting every signed, string or floating-point tail indiscriminately.
- [ ] Document numeric precision and sentinel-preservation limits, especially
      for scaled 64-bit fields. Test expected wire values and availability,
      not only symmetric encode/decode round trips.

## 3. Define the supported profiles and complete their network services

Start requirements research early, without blocking section 1. Before adding
services, select finite use cases and configurations for 1.0.

- [ ] Establish a requirements and tested-capabilities matrix for the selected
      profiles: source/edition, mandatory/conditional/optional status, TX/RX
      support, tests, peer versions, hardware configuration and known limits.
      Keep unverified normative requirements explicit. Generated types alone
      do not establish runtime support; library support does not certify a
      finished product.
- [ ] Integrate Fast Packet reassembly into the receive path, with explicit
      session capacities, complete-message metadata and access to raw frames.
- [ ] Support counterless repeating groups for 126464, 129796 and 129805,
      including empty, incomplete and over-capacity payload tests.
- [ ] Provide profile-appropriate PGN List (126464), Product Information
      (126996) and Heartbeat (126993) services. Use application-supplied identity
      and actual TX/RX capabilities; verify request and periodic behaviour
      against the requirements matrix.
- [ ] Support bounded runtime-sized fields (`DYNAMIC_FIELD_VALUE`, `VARIABLE`)
      needed by the selected services, starting with PGN 126208.
- [ ] Implement the selected Group Function (126208) Request, Command and
      Acknowledge operations and their required error responses. Resolve
      partially supported polymorphic generation; assess multiple repeating
      groups and conditional layouts separately for Read/Write Fields.
- [ ] Triage `unsupported` entries in `pgn_manifest.full.json` and enable the
      subset required by supported use cases, with wire-level regression tests.
      Keep the remaining entries explicitly unsupported.
- [ ] Settle typed access to unavailable values before freezing the API: avoid
      raw-sentinel comparisons while preserving raw values and the distinct
      NotAvailable, OutOfRange and Reserved states. Document the migration.

## 4. Prove interoperability and operational limits

Begin with existing PGNs while sections 1–3 progress. Use a finite test matrix
for the selected profiles, not an assertion that every device or PGN works.

- [ ] Compare both directions with Timo Lappalainen's NMEA2000 C++ library:
      known values, unavailable values and boundaries for representative
      single-frame and Fast Packet PGNs. Record the peer version.
- [ ] Use CANboat `analyzer` as a differential decoder: encode with korri-n2k,
      decode with a pinned analyzer version, and compare fields and availability
      with explicit units and tolerances. Shared CANboat definitions mean this
      is not an independent check of the specification.
- [ ] Test representative claim, arbitration, discovery and data workflows
      against identified certified devices. Record model, firmware,
      certification provenance, scenarios and limits; interoperability does
      not confer product certification.
- [ ] Validate a bounded load/fault matrix on an isolated, authorised bench:
      congestion, missing ACKs, bus-off, node restarts and endurance. Define
      deadlines and failure/recovery outcomes, including a real non-zero
      firmware exit and host-process cleanup.
- [ ] Measure RAM, peak stack and latency for named STM32 and ESP32 targets,
      runtimes, drivers and representative manifests. Publish tested limits.
- [ ] Add authorised, anonymised captures that close identified coverage gaps.
      Preserve wire structure and inspect proprietary/text fields for sensitive
      data. Continue expanding the regression corpus after 1.0.

## 5. Make integration reproducible and stabilise 1.0

- [ ] Validate the supported STM32, ESP32 and Linux/SocketCAN driver contracts:
      RX cancellation safety, TX completion semantics, transient versus terminal
      errors, bounded waits and bus-off recovery responsibilities.
- [ ] Have an external user follow a supported setup guide to build, connect,
      claim, send and receive without maintainer-specific configuration.
- [ ] Document failure reproduction, regression-test contributions and software
      versus optional hardware checks. Reference the `justfile` and HIL guide
      rather than duplicating their command lists.
- [ ] Document releases and CANboat updates, including generated API changes,
      wire-format compatibility checks and database provenance/licensing.
- [ ] Publish supported use cases, platforms, protocol scope and explicit limits,
      without unsupported certification claims.
- [ ] Resolve known correctness and soundness defects in that scope with
      regression tests. Reject or disable excluded paths where necessary and
      test those exclusions; a prose warning alone does not make an unsafe
      supported path acceptable.
- [ ] Verify representative end-to-end paths on hardware and against an
      independent implementation; link the evidence in the capabilities matrix.
- [ ] Stabilise the public API, including generated types, for 1.0. Allow
      breaking changes only in later major releases and provide a migration
      guide from 0.x.

## Conditional extensions, not general 1.0 prerequisites

Promote an item into the 1.0 scope only when a selected profile requires it.

- [ ] Extend the bounded BAM receiver to ISO TP BAM and RTS/CTS TX/RX, with
      explicit resource limits, timeouts, aborts and address-loss handling.
- [ ] Support additional multi-group, conditional and dynamic layouts for
      selected operations; do not require the entire unsupported catalogue.
- [ ] Add standard-trait adapters where a supported driver demonstrates a
      concrete simplification, preserving runtime contracts without coupling
      protocol engines to a HAL.

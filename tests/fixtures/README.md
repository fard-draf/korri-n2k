# Test fixtures

## `backbone_sample.bin`

A slice of a real NMEA 2000 backbone, replayed by `tests/replay_real_capture.rs`
through the whole stack: CAN identifier, Fast Packet reassembly, then decoding.

Synthetic payloads only exercise the shapes we already thought of. This capture
is what found the three defects fixed in 0.5.0 — a repeating-group counter read
literally instead of as the "not available" sentinel, a `STRING_LAU` length off
by one, and a `STRING_FIX` read at its declared capacity rather than at the
length the frame carries. The `STRING_LAU` one is the telling case: reader and
writer were wrong in the same direction, so every round-trip test passed while
every frame on the wire was malformed. Only a real frame could show it.

### Contents

63 PGNs, up to 10 complete messages each: 1821 frames, 43 KB. Same record format
as the sniffer output, so the same parser reads both this file and a full
recording.

### Anonymised

Taken from a live bus, so identifying data was neutralised before committing:

| replaced | with |
|---|---|
| latitude, longitude | `0` |
| MMSI / `UserId` | `111111111` |
| vessel names, call signs, destinations | `X` |
| product serial numbers, model and vendor strings | `X` |

The substitution preserves every structural byte — lengths, counters,
discriminators — so a decoder sees exactly the same message shapes. Verified:
replaying the file before and after gives an identical result, down to the
per-PGN counts.

Autopilot telemetry (`130821`) and alarm text (`130856`) are kept as they are:
rudder angles and "Steering compass missing" identify nobody, and they make
realistic string payloads.


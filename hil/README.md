# Hardware-in-the-loop bench

A local bench, not run by GitHub CI: two STM32G431 boards, each connected to its own debug
probe, share the host's `can0` bus. Both boards have passed the TX and RX scenarios.
Each `just hil` invocation selects one board as the device under test (DUT), flashes it and
checks the result twice: the firmware's own verdict and an oracle on the frames captured on
`can0`. The two boards are tested in separate runs, not as a simultaneous transmitter/receiver pair.

> **It transmits on the real bus.** The DUT claims address 90 and sends traffic, and the host
> injects frames from address 91. Every other node on the bus sees them. Do not run it on a bus
> that carries a vessel's equipment.

## Prerequisites

- `just` >= 1.44.0, `probe-rs` (0.32 tested), `ip`, `timeout`, and the toolchain of `hil/firmware/rust-toolchain.toml`
  (Rust 1.96.0 with `thumbv7em-none-eabihf`).
- Two STM32G431CB boards with their CAN transceivers on `can0`'s bus, each connected to a
  debug probe supported by `probe-rs`. A single board and its probe suffice for an individual run.
- `can0` brought up by you, at **250000 bit/s**, UP and ERROR-ACTIVE. The bench never changes it
  and never uses `sudo`:

  ```sh
  ip -details link show can0    # must say: UP, can state ERROR-ACTIVE, bitrate 250000
  ```

## Variables

| Variable | Default | Meaning |
|---|---|---|
| `KORRI_HIL_PROBE` | none, required | Probe selector as printed by `probe-rs list` (`VID:PID:SERIAL`). It picks the DUT among the probes plugged in. |
| `KORRI_HIL_CAN` | `can0` | SocketCAN interface. |
| `KORRI_HIL_OUT` | `$TMPDIR/korri-hil` | Where RTT logs (`<scenario>.rtt.log`), captures (`<scenario>.can.log`) and, unless `CARGO_TARGET_DIR` is set, the firmware build go. |
| `KORRI_HIL_TIMEOUT_S` | 90 | Positive limit for the firmware process, flashing included; a forced stop may take 5 s more. SocketCAN writes have a separate 100 ms timeout per frame. |
| `DEFMT_LOG` | `error` | Firmware log level at build time; set `info` to include readiness, received messages and PASS traces. |
| `KORRI_HIL_EXPECTED_MESSAGES` | 5 | Positive number of messages the `tx_fast_packet` oracle expects. A wrong value makes the bench fail, which shows the verdict is not rubber-stamped. |

## Use

```sh
export KORRI_HIL_PROBE=<VID:PID:SERIAL>
export DEFMT_LOG=info        # include informative firmware RTT logs
just hil                    # both scenarios
just hil tx_fast_packet     # one scenario
just check                  # `just ci`, then `just hil`, before a PR
```

To test both boards, run them sequentially with their respective probe selectors from
`probe-rs list`. Use separate output directories to preserve both sets of results:

```sh
KORRI_HIL_PROBE=<BOARD_A_PROBE_SELECTOR> KORRI_HIL_OUT=./hil-results/board-a just hil
KORRI_HIL_PROBE=<BOARD_B_PROBE_SELECTOR> KORRI_HIL_OUT=./hil-results/board-b just hil
```

Do not run both DUTs simultaneously: the test firmware uses the same fixed address 90.

Exit codes of `just hil`: **0** every scenario passed; **1** a scenario failed (firmware
verdict, timeout or oracle), configuration is invalid, or the firmware did not build;
**77** the bench is absent (probe variable
unset, probe not listed, `can0` missing, down, not ERROR-ACTIVE or not at 250 kbit/s). In the
last case nothing was flashed and nothing was sent. Invalid numbers or empty optional variables
are rejected before preflight, even when the probe is not configured.

## Scenarios

Each firmware claims the fixed address 90, runs, then ends the debug session with a
semihosting exit (0 pass, 1 fail).

| Scenario | DUT | Host oracle |
|---|---|---|
| `tx_fast_packet` | Sends five Engine Parameters, Dynamic (127489, 26 bytes, Fast Packet) every 100 ms. | Reassembles the frames of address 90 byte by byte, independently of the library: DLC 8, `FF` padding, frame indices, sequence counter (+1 per message), announced length, all 26 payload bytes (zero defaults except the reserved `FF` byte and engine hours 1000…1004), plus the address claim before the first emission. |
| `rx_fast_packet` | Reassembles with the library the five messages the host injects. Exit 0 if all arrive intact and in order within 20 s. | The DUT's claim is seen, then the host injects five 127489 messages from address 91 (never claimed) once the DUT is ready. |

The oracle filters other traffic by source address. Before the HIL tests, the two boards ran
continuous publishers at addresses 80 and 81; flashing the test firmware replaces those images.
Each scenario ends after its checks, so a silent bus after both boards have finished is normal.
The current scenarios use the host as the DUT's peer, not the other STM32.

The firmware ends with a semihosting call that faults when no debugger is attached: flashed
boards are test images, to be reflashed with their normal firmware afterwards.

`hil/firmware` and `hil/host` are independent cargo projects (no workspace, not part of the
published crate). `just ci` runs fmt and clippy on both, plus `cargo test --locked` on the host,
without accessing the bench.

A forced kill after the timeout's 5 s grace can be reported as "firmware killed by a signal"
rather than "timeout"; both fail the verdict. Cleanup after interrupting or killing the host
itself (Ctrl-C/SIGKILL) has not been validated.

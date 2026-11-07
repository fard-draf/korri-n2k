# Hardware-in-the-loop bench

A local bench, not run by GitHub CI: one STM32G431 board (the device under test, DUT) and the
host's `can0`. `just hil` flashes the DUT, runs the scenarios and checks the result twice: the
firmware's own verdict and an oracle on the frames captured on `can0`.

> **It transmits on the real bus.** The DUT claims address 90 and sends traffic, and the host
> injects frames from address 91. Every other node on the bus sees them. Do not run it on a bus
> that carries a vessel's equipment.

## Prerequisites

- `just`, `probe-rs` (0.32 tested), `ip`, `timeout`, and the toolchain of `hil/firmware/rust-toolchain.toml`
  (Rust 1.96.0 with `thumbv7em-none-eabihf`).
- A debug probe wired to an STM32G431CB board with its CAN transceiver on `can0`'s bus.
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
| `KORRI_HIL_TIMEOUT_S` | 90 | Limit of one scenario, flashing included. |
| `KORRI_HIL_EXPECTED_MESSAGES` | 5 | Messages the `tx_fast_packet` oracle expects. A wrong value makes the bench fail, which shows the verdict is not rubber-stamped. |

## Use

```sh
export KORRI_HIL_PROBE=<VID:PID:SERIAL>
just hil                    # both scenarios
just hil tx_fast_packet     # one scenario
just check                  # `just ci`, then `just hil`, before a PR
```

Exit codes of `just hil`: **0** every scenario passed; **1** a scenario failed (firmware
verdict, timeout or oracle) or the firmware did not build; **77** the bench is absent (variable
unset, probe not listed, `can0` missing, down, not ERROR-ACTIVE or not at 250 kbit/s). In the
last case nothing was flashed and nothing was sent.

## Scenarios

Each firmware claims the fixed address 90, runs, then ends the debug session with a
semihosting exit (0 pass, 1 fail).

| Scenario | DUT | Host oracle |
|---|---|---|
| `tx_fast_packet` | Sends five Engine Parameters, Dynamic (127489, 26 bytes, Fast Packet) every 100 ms. | Reassembles the frames of address 90 byte by byte, independently of the library: DLC 8, `FF` padding, frame indices, sequence counter (+1 per message), announced length, engine hours 1000…1004, plus the address claim. |
| `rx_fast_packet` | Reassembles with the library the five messages the host injects. Exit 0 if all arrive intact and in order within 20 s. | The DUT's claim is seen, then the host injects five 127489 messages from address 91 (never claimed) once the DUT is ready. |

The bus carries other nodes (addresses 80 and 81 on the author's bench): the oracle filters by
source address. Scenarios that need two DUTs are out of scope.

The firmware ends with a semihosting call that faults when no debugger is attached: flashed
boards are test images, to be reflashed with their normal firmware afterwards.

`hil/firmware` and `hil/host` are independent cargo projects (no workspace, not part of the
published crate). `just ci` only runs `cargo check` on them.

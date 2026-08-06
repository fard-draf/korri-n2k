<p align="center">
  <img src="https://github.com/user-attachments/assets/c383dba3-0408-4b2c-bae6-cd1b935ff10e" alt="Logo" width="800">
</p>

![CI](https://github.com/fard-draf/korri-n2k/actions/workflows/ci.yml/badge.svg)

An NMEA 2000 / ISO 11783 stack for Rust: send and receive PGNs on a marine CAN
bus. `no_std`, zero-allocation, one runtime for bare metal and one for Linux.

PGN types are generated at build time from [CANboat](https://github.com/canboat/canboat),
the reference catalogue of NMEA 2000 messages. You never hand-write a parser.
You choose how many get compiled in.

## Quickstart

```sh
cargo add korri-n2k --features tokio     # Linux
cargo add korri-n2k --features embassy   # bare metal
```

```rust
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::Pgn128267;

// An 8-byte Water Depth frame straight off the bus.
let payload = [0x2A, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF];
let depth = Pgn128267::from_payload(&payload).expect("valid Water Depth");

// Fields are fixed-point on the wire. Compare with a tolerance, never with `==`.
assert!((depth.depth - 1.0).abs() < 1e-6);
```

## Install

`embassy` and `tokio` are mutually exclusive. Neither is on by default: pick one.

| Feature | Target | `std` | Heap |
|---|---|---|---|
| `embassy` | Bare metal (`no_std`) | No | **none**, fully static |
| `tokio` | Linux / OS | Yes | channel buffers only |

Under `tokio` the protocol logic stays zero-allocation. Only the `mpsc` buffers
of `AddressService` are heap-allocated. Rust 1.96, edition 2021.

## Choosing your PGNs

The build generates types only for the PGN IDs listed in a manifest.

| Your project | What you compile | How |
|---|---|---|
| A specific device | the PGNs you list | edit `pgn_manifest.json` |
| A proprietary set | your IDs, kept outside the crate | `KORRI_N2K_MANIFEST_PATH=…` |
| A gateway or logger | every supported PGN | `features = ["full-pgns"]` |

```json
{
  "pgns": [
    { "id": 129025, "name": "Position, Rapid Update" },
    { "id": 128267, "name": "Water Depth" }
  ]
}
```

Matching is on `id`. Names are documentation. Precedence:
`KORRI_N2K_MANIFEST_PATH` → `full-pgns` → default manifest. The default covers 42
common PGNs: position, heading, AIS, depth, wind. `full-pgns` covers the 313 the
generator supports, out of CANboat's 348.

## Use cases

Every example below is compiled and run by CI.

### Claim an address, then talk

```sh
cargo run --example address_claim --features tokio
```

A node must own an address before it may emit. The claim happens under the
runner, never in a constructor. A constructor that waited for an address could
never return on a saturated bus.

```rust,ignore
// Synchronous. Never touches the bus. Fails only if the NAME and the
// strategy disagree.
let manager = AddressManager::new(bus, TokioTimer::new(), my_name, strategy)?;

// 4 queued commands, 8 buffered incoming frames. 0 opts out of either.
let parts = AddressService::new(manager, 4, 8).into_parts();
let handle = parts.handle.expect("a command channel was requested");

// The runner owns the event loop: it claims, defends, answers ISO Requests,
// and emits whatever the handle queues.
tokio::spawn(parts.runner.drive());
```

Two `send_pgn` exist, with different contracts:

| Method | Guarantees |
|---|---|
| `AddressManager::send_pgn` | refuses with `NotClaimed`, never a silent `Ok(())` |
| `AddressHandle::send_pgn` | confirms queueing only |

The runner may still refuse a queued command, and drops the refusal rather than
returning it. Ask the handle first:

```rust,ignore
match handle.claimed_address() {
    Some(address) => println!("emitting from {address}"),
    None => println!("still claiming"),
}
```

Best effort, not a lock. A conflict can take the address away a microsecond
later. The library refuses that emission anyway; this only spares you from
asking.

The accessor hangs off the handle, not the service. A node holding several NAMEs
gets one handle per Controller Application, and reads each address through its
own.

Under `embassy` the cell is a `static` you own, like the channels:

```rust,ignore
static CLAIMED: ClaimedAddress = ClaimedAddress::new();
```

### Send a PGN

Any task holding an `AddressHandle` can send. Fast Packet fragmentation is
automatic. The source address is filled in at emission time.

```rust,ignore
let mut pos = Pgn129025::new();
pos.latitude = 47.7223;
pos.longitude = -4.0022;

handle.send_pgn(&pos, 129025, 2, None).await?;
```

### Receive a single-frame PGN

`AddressFrames` yields raw `CanFrame`s, unfiltered. Address-claim traffic is
included, so network discovery can see it too.

```rust,ignore
// tokio: the channel can close, so recv() returns an Option
if let Some(frame) = frames.recv().await {
    if let Ok(depth) = Pgn128267::from_payload(&frame.data) {
        println!("{} m", depth.depth);
    }
}

// embassy: the channel never closes
let frame = frames.recv().await;
```

The application is never allowed to slow the engine down. If you stop draining
this channel, frames are dropped, not queued.

### Reassemble a Fast Packet PGN

```sh
cargo run --example fast_packet_rx
```

A PGN over 8 bytes arrives in fragments. Decoding the first one succeeds and
returns wrong values, so branch on `handles()`, never on the decode result.

```rust,ignore
let mut assembler = FastPacketAssembler::new();

if !assembler.handles(pgn) {
    // Single frame: decode the eight bytes directly.
} else if let ProcessResult::MessageComplete(msg) =
    assembler.process_frame(now_ms, pgn, frame.id.source_address(), &frame.data)
{
    let fix = Pgn129029::from_payload(&msg.payload[..msg.len])?;
}
```

`new()` carries the Fast Packet PGNs of your manifest. A gateway that forwards
payloads it cannot decode wants every one CANboat knows:

```rust,ignore
let mut assembler = FastPacketAssembler::with_pgns(FAST_PACKET_PGNS_ALL);
```

`full-pgns` is not a substitute: its manifest lists 152 of the 182 Fast Packet
PGNs CANboat declares. Pass your own sorted table for proprietary PGNs.

### Claim without a runtime

```sh
cargo run --example blocking_claim --features std
```

`AddressClaimEngine` is synchronous and does no I/O. It takes a millisecond
reading and an optional frame, and returns an action. A bare-metal main loop
drives it with no executor, no timer trait, no `CanBus` implementation.

```rust,ignore
loop {
    let received = bus.try_recv(now_ms);

    match engine.poll(now_ms, received.as_ref()) {
        ClaimAction::Send(frame) | ClaimAction::CannotClaim(frame) => bus.send(&frame),
        ClaimAction::Claimed(address) => return address,
        // Upper bound, not a sleep order.
        ClaimAction::Wait(delay_ms) => now_ms += (delay_ms as u64).min(TICK_MS),
    }
}
```

The `min` is the whole contract. `Wait(n)` says "nothing is due for n ms", not
"sleep for n ms". A loop that idles the full window misses the conflicts inside
it, and a blocking read with no timeout hangs forever on a quiet bus.

The library ships no blocking facade. Your read may be a `try_recv`, an interrupt
flag, a hardware FIFO or a scheduler tick. No trait covers all four honestly.

### Decode without a bus

```sh
cargo run --example quickstart
cargo run --example lookup_enum_usage
cargo run --example iso_name_usage
```

## Writing a driver

Implement `CanBus`, two methods. One rule is not obvious.

**`recv()` must be safe to drop.** The supervisor races it against a deadline and
against queued commands, so it is cancelled often: once per expired `Wait`, once
per command. An application publishing at 10 Hz cancels a pending `recv` ten
times a second.

A driver that pulls a frame off its hardware queue before the future resolves
loses that frame every time. If the lost frame is a competing Address Claim, the
node keeps an address it no longer owns. Buffer inside the driver and return from
the buffer.

## Limits

- **Fast Packet reassembly is not wired into the receive path.** Run
  `FastPacketAssembler` yourself, as above.
- **The assembler only reassembles the PGNs in its table.** Ask `handles()`.
- **No ISO Transport Protocol.** PGNs 60160 and 60416 decode as messages. The
  multi-packet transport itself is not implemented.
- **35 of CANboat's 348 PGNs are not generated.** Listed with their reason in
  `build_core/var/pgn_manifest.full.json`. None are in the default manifest.
- **Runtime-sized fields are unsupported** (`VARIABLE`, `DYNAMIC_FIELD_VALUE`).
  That keeps PGN 126208, the Group Functions meta-protocol, out of reach. Next on
  the roadmap.
- **A repeating group needs an explicit counter field.** The few PGNs that size
  their group implicitly are rejected.
- **A full Fast Packet send holds the loop for about 62 ms**, 32 frames with a
  2 ms gap. Single-frame PGNs are unaffected.
- `embassy` and `tokio` cannot be enabled together.

## Architecture

The core knows nothing about your hardware. It sits on two traits: `CanBus` for
raw frame I/O, `KorriTimer` for non-blocking delays.

Address management is a synchronous, I/O-free engine. It is handed a clock
reading and an optional frame, and returns an action. It never sleeps and never
touches the bus, which makes it testable without a runtime and replayable against
recorded captures.

The runner owns the single `select` above it: wait, poll the engine, execute,
repeat. No state transition can be cancelled halfway.

`AddressService::into_parts` returns three pieces:

| Piece | Role |
|---|---|
| `AddressHandle` | queue outgoing PGNs, from any task |
| `AddressFrames` | receive incoming frames |
| `AddressRunner` | the background task, owner of the event loop |

Channels are static under embassy, sized by integer capacities under tokio. A
command entry costs 240 bytes whatever the payload. The buffer is inline, so
nothing is allocated. Size `CMD_CAP` with that in mind.

One catch: `#[embassy_executor::task]` cannot be generic, so declare the runner
task yourself over your concrete types.

```rust,ignore
#[embassy_executor::task]
async fn n2k_runner_task(runner: AddressRunner<'static, MyCanBus, MyTimer, 16, 16>) {
    let _ = runner.drive().await;
}
```

Hardware implementations for STM32, ESP32 and SocketCAN live in
[korri-n2k-examples](https://github.com/fard-draf/korri-n2k-examples).

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

The bundled [CANboat](https://github.com/canboat/canboat) database
(`build_core/var/canboat.json`) is Apache-2.0, © Kees Verruijt. It stays under
that licence, as does the code generated from it: PGN and field names,
descriptions, lookup variants. See `NOTICE`.

All credit for the NMEA 2000 message catalogue goes to the CANboat authors,
without whom this library could not exist.

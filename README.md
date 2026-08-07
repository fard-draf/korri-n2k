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

Two `send_pgn` exist. Which one you use is decided by who owns the manager, not
by preference:

| You drive | Emission API | Guarantees |
|---|---|---|
| the manager yourself, no runner | `AddressManager::send_pgn` / `send_payload` | refuses with `NotClaimed`, never a silent `Ok(())` |
| a runner | `AddressHandle::send_pgn` / `send_raw_frame` | confirms queueing only |

`AddressService::into_parts` moves the manager into the `AddressRunner`, so its
methods become unreachable from then on. The runner does not hand out a
`&mut AddressManager`: the engine would then be mutated from outside its own
loop, which is exactly what this design removes.

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
reading and an optional frame, and returns a `ClaimOutput`. A bare-metal main
loop drives it with no executor, no timer trait, no `CanBus` implementation.

```rust,ignore
loop {
    let received = bus.try_recv(now_ms);
    let output = engine.poll(now_ms, received.as_ref());

    // First, always: leaving on the status would drop this frame.
    if let Some(frame) = output.tx {
        bus.send(&frame);
    }

    if let ClaimStatus::Claimed(address) = output.status {
        return address;
    }

    // Upper bound, not a sleep order.
    now_ms += match output.wake_at_ms {
        Some(deadline_ms) => deadline_ms.saturating_sub(now_ms).clamp(1, TICK_MS),
        None => TICK_MS,
    };
}
```

The output answers three questions that do not fold into one another.

| Field | Meaning |
|---|---|
| `tx` | at most one frame to emit, **before** you act on `status` |
| `status` | `Unclaimed`, `Claiming(addr)`, `Claimed(addr)` or `CannotClaim` |
| `wake_at_ms` | absolute deadline in the `now_ms` domain, `None` if no timer is pending |

Emitting first is not a style preference. A request arriving on the exact
millisecond the claim window closes returns a defence frame *and*
`Claimed(addr)`. Return on the status and that frame never reaches the bus.

The `clamp` is the other half of the contract. `wake_at_ms` says "nothing is due
before this instant", not "sleep until it". A loop that idles the full window
misses the conflicts inside it. A blocking read with no timeout hangs forever on
a quiet bus. And `None` means no timer is pending, never that the engine is
finished: a conflict must still be able to wake it.

The library ships no blocking facade. Your read may be a `try_recv`, an interrupt
flag, a hardware FIFO or a scheduler tick. No trait covers all four honestly.

### Decode without a bus

```sh
cargo run --example quickstart
cargo run --example lookup_enum_usage
cargo run --example iso_name_usage
```

## Writing a driver

Implement `CanBus`, two methods. Two rules are not obvious.

**`recv()` must be safe to drop.** The supervisor races it against a deadline and
against queued commands, so it is cancelled often: once per expired deadline,
once per command. An application publishing at 10 Hz cancels a pending `recv` ten
times a second.

A driver that pulls a frame off its hardware queue before the future resolves
loses that frame every time. If the lost frame is a competing Address Claim, the
node keeps an address it no longer owns. Buffer inside the driver and return from
the buffer.

**Any error you return is terminal.** An `Err` from `send` or `recv` stops
`AddressRunner` for good, and the node keeps no address. Absorb what you can
recover from: arbitration loss, a full TX mailbox, a bus-off recovery cycle.
Return an error only for a condition the caller has to act on, such as a closed
socket or a dead peripheral.

The library does not retry. It cannot tell a transient failure from a permanent
one through an opaque `Error`, and a blind retry loop on a dead bus is worse than
stopping.

## Limits

- **Fast Packet reassembly is not wired into the receive path.** Run
  `FastPacketAssembler` yourself, as above.
- **The assembler only reassembles the PGNs in its table.** Ask `handles()`.
- **No ISO Transport Protocol.** PGNs 60160 and 60416 decode as messages. The
  multi-packet transport itself is not implemented.
- **No ISO Commanded Address.** PGN 65240 is treated as ordinary traffic, so a
  tool cannot force this node onto a given address. The PGN carries nine bytes
  and arrives by BAM, so implementing it needs the transport above first.
- **No pseudo-random delay before a claim.** J1939-81 asks a node to jitter its
  first Address Claim. This one emits immediately. Nodes powering up together on
  the same preferred address therefore collide, and resolve by NAME arbitration
  instead of by spreading out.
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

The two runtimes expose the same methods with different signatures. An embassy
channel never closes and never refuses, so the failures tokio has to report
simply do not exist there:

| Method | `tokio` | `embassy` |
|---|---|---|
| `AddressHandle::send_pgn` | `Result<(), AddressHandleError>` | `Result<(), AddressHandleError>` |
| `AddressHandle::send_raw_frame` | `Result<(), AddressHandleError>` | `()` |
| `AddressHandle::send_command` | `Result<(), AddressHandleError>` | `()` |
| `AddressFrames::recv` | `Option<CanFrame>` | `CanFrame` |

Only `send_pgn` matches, because serialization can fail under both. Code that
must build for the two runtimes needs a `cfg` at these three call sites.

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

# korri-n2k

`no_std`, `no_alloc` NMEA 2000 stack for embedded Rust targets.

## Key advantages

- **Compile-time PGN types** generated from `canboat.json` (sourced from the [CANboat](https://github.com/canboat/canboat) project)
- **Zero heap allocation**, ideal for MCUs without a global allocator
- **Full Fast Packet support** (segmentation and reassembly helpers)
- **Automatic ISO 11783 address management** via `AddressManager`
- **Async-first API** built on `embassy-time`, `embedded-can`, and `futures-util`

## Quick install

```bash
cargo add korri-n2k embedded-can embassy-time
cargo build
```

- The build script defaults to `build_core/var/pgn_manifest.json` to decide which PGNs are generated.
- Provide a custom manifest by exporting `KORRI_N2K_MANIFEST_PATH` (absolute path recommended) before `cargo build`.
- When `curl` or `wget` are unavailable on the build host, enable `--features build-download` to fetch `canboat.json` through `ureq`.

## Minimal host example (`std`)

```rust
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::{
    managment::iso_name::IsoName,
    messages::{Pgn128267, Pgn129025, Pgn60928},
};

fn main() {
    let base_name = IsoName::builder()
        .unique_number(12345)
        .manufacturer_code(229)
        .device_function(145)
        .device_class(75)
        .industry_group(4)
        .arbitrary_address_capable(true)
        .build();

    let mut position = Pgn129025::new();
    position.latitude = 47.7223;
    position.longitude = -4.0022;

    let mut buffer = [0u8; 64];
    let len = position.to_payload(&mut buffer).unwrap();
    let decoded = Pgn129025::from_payload(&buffer[..len]).unwrap();

    let depth_payload = [0x01, 0x48, 0x14, 0x00, 0x00, 0xF4, 0x01, 0x00, 0x00];
    let depth = Pgn128267::from_payload(&depth_payload).unwrap();

    let claim: Pgn60928 = base_name.into();
    let restored_name: IsoName = claim.into();

    println!(
        "Position {:?}, depth {:.2} m, identical name: {}",
        (decoded.latitude, decoded.longitude),
        depth.depth,
        restored_name.raw() == base_name.raw()
    );
}
```

Run the full sample with `cargo run --example quickstart`; the code above matches that binary and is ready to compile.

## Async network integration

```rust
use korri_n2k::{
    error::SendPgnError,
    protocol::{
        managment::address_manager::AddressManager,
        messages::Pgn127503,
        transport::traits::{can_bus::CanBus, korri_timer::KorriTimer},
    },
};

async fn send_status<C, T>(
    manager: &mut AddressManager<C, T>,
) -> Result<(), SendPgnError<C::Error>>
where
    C: CanBus,
    T: KorriTimer,
    C::Error: core::fmt::Debug,
{
    let mut status = Pgn127503::new();
    status.instance = 0;
    status.number_of_lines = 1;

    manager.send_pgn(&status, 127503, None).await
}
```

Platform traits to implement:

```rust
use korri_n2k::protocol::transport::{
    can_frame::CanFrame,
    traits::{can_bus::CanBus, korri_timer::KorriTimer},
};

struct MyCan;
struct MyTimer;
#[derive(Debug)]
struct MyError;

impl CanBus for MyCan {
    type Error = MyError;

    fn send<'a>(
        &'a mut self,
        _frame: &'a CanFrame,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>> + 'a {
        async move { /* Push the frame onto your CAN controller */ Ok(()) }
    }

    fn recv<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = Result<CanFrame, Self::Error>> + 'a {
        async move { Err(MyError) }
    }
}

impl KorriTimer for MyTimer {
    fn delay_ms<'a>(
        &'a mut self,
        millis: u32,
    ) -> impl core::future::Future<Output = ()> + 'a {
        async move {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(millis.into())).await
        }
    }
}
```

- `AddressManager` claims and defends the node address (PGN 60928) and exposes `send_pgn` for both single-frame and Fast Packet messages.
- `FastPacketBuilder` / `FastPacketAssembler` are available when you need full manual control.

## Tests & docs

```bash
cargo test            # full suite (unit + integration)
cargo test --doc      # documentation snippets
cargo run --example quickstart
cargo build --examples
cargo doc --no-deps
```

## Useful scripts

- `./scripts/download_canboat.sh` – download/validate `canboat.json` from CANboat (current upstream version: 6.1.3)
- `./scripts/verify_docs.sh` – convenience wrapper that runs every command above

## Resources

- `examples/std/` – host-side examples (`quickstart`, `lookup_enum_usage`, `iso_name_usage`)
- `examples/esp32-s3/`, `examples/esp32-c3/`, `examples/stm32/` – embedded templates (enable `--features embedded-examples`)
- `build_core/` – code generator and manifests
- `src/` – `core`, `infra`, and `protocol` modules (lookup tables, transport, address management, messages)

## License

Dual-licensed under MIT or Apache 2.0 — see `LICENSE`.

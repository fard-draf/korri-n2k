# Examples

All examples live under `examples/std/` and are compiled and run by CI:

| Example | Command | Purpose |
|---|---|---|
| `quickstart` | `cargo run --example quickstart` | Decode a single-frame PGN |
| `lookup_enum_usage` | `cargo run --example lookup_enum_usage` | Lookup enums |
| `iso_name_usage` | `cargo run --example iso_name_usage` | ISO NAME fields and strategy |
| `fast_packet_rx` | `cargo run --example fast_packet_rx --features std` | Reassemble a Fast Packet PGN |
| `blocking_claim` | `cargo run --example blocking_claim --features std` | Drive `AddressClaimEngine` without a runtime |
| `address_claim` | `cargo run --example address_claim --features tokio` | Claim an address and send a PGN over a loopback bus |

Hardware firmwares for STM32, ESP32 and SocketCAN live in
[korri-n2k-examples](https://github.com/fard-draf/korri-n2k-examples).

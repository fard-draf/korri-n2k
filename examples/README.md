# korri-n2k Examples

This directory contains examples organized by platform, following the [Embassy](https://github.com/embassy-rs/embassy/tree/main/examples) structure.

## Structure

```
examples/
├── std/              # Standard Rust examples (compile on any platform)
├── esp32-s3/         # ESP32-S3 specific examples
├── esp32-c3/         # ESP32-C3 specific examples
└── stm32/            # STM32 specific examples
```

## Running Examples

### Standard Examples (std/)

These examples compile and run on any platform with `std`:

```bash
# Run the quickstart example
cargo run --example quickstart

# Run all std examples
cargo test --examples
```

Available examples:
- `quickstart` - Basic introduction to korri-n2k
- `lookup_enum_usage` - Working with NMEA 2000 lookup enums
- `iso_name_usage` - ISO Name manipulation and address claiming

### Embedded Examples

Embedded examples require the `embedded-examples` feature and the appropriate target.

#### ESP32-S3

```bash
# Setup (first time only)
cargo install espup
espup install

# Build
cargo build --example esp32s3_quickstart \
  --target xtensa-esp32s3-none-elf \
  --features embedded-examples

# Flash
cargo run --example esp32s3_quickstart \
  --target xtensa-esp32s3-none-elf \
  --features embedded-examples
```

#### ESP32-C3 (RISC-V)

```bash
# Setup (first time only)
cargo install espup
espup install

# Build
cargo build --example esp32c3_quickstart \
  --target riscv32imc-unknown-none-elf \
  --features embedded-examples

# Flash
cargo run --example esp32c3_quickstart \
  --target riscv32imc-unknown-none-elf \
  --features embedded-examples
```

#### STM32

```bash
# Build (example for STM32F4)
cargo build --example stm32_quickstart \
  --target thumbv7em-none-eabihf \
  --features embedded-examples

# Flash with probe-rs
cargo run --example stm32_quickstart \
  --target thumbv7em-none-eabihf \
  --features embedded-examples
```

**Note**: STM32 examples are templates and require additional setup. See the example source code for details.

## Adding New Examples

- **std examples**: Add to `examples/std/` - they will automatically compile with `cargo test`
- **Embedded examples**: Add to the platform-specific directory and update `Cargo.toml` with:
  ```toml
  [[example]]
  name = "platform_example_name"
  path = "examples/platform/example_name.rs"
  required-features = ["embedded-examples"]
  ```

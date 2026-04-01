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


## Adding New Examples
- **std examples**: Add to `examples/std/` - they will automatically compile with `cargo test`

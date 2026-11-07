# Single list of the CI commands: `.github/workflows/ci.yml` only calls these recipes.
# `just ci` replays CI locally. Needs just >= 1.44.0 and the pinned Rust toolchain.

set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Everything CI runs.
ci: ci-test ci-embedded ci-lint

# Job `test`. `default = []`, so the first run compiles no runner at all: the I/O-free engine and
# the codec alone. `full-pgns` is built by no other step: replay, float, lookup-width and
# round-trip tests need PGNs the default manifest does not carry.
ci-test:
    cargo test --verbose --no-default-features
    cargo test --verbose --features tokio
    cargo test --verbose --features embassy
    cargo test --verbose --features full-pgns
    just ci-minimal-manifest
    cargo run --example quickstart
    cargo run --example lookup_enum_usage
    cargo run --example iso_name_usage
    cargo run --example fast_packet_rx --features std
    cargo run --example blocking_claim --features std
    cargo run --example address_claim --features tokio
    cargo run --example monitor --features std

# The smallest manifest the build accepts (60928 is mandatory).
[private]
[script("bash")]
ci-minimal-manifest:
    set -euo pipefail
    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
    printf '{"pgns":[{"id":60928,"name":"ISO Address Claim"}]}' > "$dir/minimal.json"
    KORRI_N2K_MANIFEST_PATH="$dir/minimal.json" cargo check --lib

# Jobs `check-embedded-cortex-m4` and `check-embedded-esp32c3`.
ci-embedded: ci-cortex-m4 ci-esp32c3

# Job `check-embedded-cortex-m4`. The bare-metal runner only exists under `embassy`, so the first
# check never compiles it.
ci-cortex-m4:
    cargo check --target thumbv7em-none-eabihf --verbose
    cargo check --lib --target thumbv7em-none-eabihf --features embassy --verbose
    just ci-hil-crates

# Offline bench checks, from each crate's directory for its toolchain and target.
# Clippy also checks compilation; host tests never access hardware.
[private]
ci-hil-crates:
    cd hil/firmware && cargo fmt -- --check
    cd hil/firmware && cargo clippy --locked --release -- -D warnings
    cd hil/host && cargo fmt -- --check
    cd hil/host && cargo clippy --locked --all-targets -- -D warnings
    cd hil/host && cargo test --locked

# Job `check-embedded-esp32c3`.
ci-esp32c3:
    cargo check --lib --target riscv32imc-unknown-none-elf --verbose
    cargo check --lib --target riscv32imc-unknown-none-elf --features embassy --verbose

# Job `lint`.
ci-lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo clippy --no-default-features --features tokio --all-targets -- -D warnings
    cargo clippy --no-default-features --features embassy --all-targets -- -D warnings

# The hardware bench: one STM32G431 and `can0`, see hil/README.md. Exits 77 when the bench is
# absent. Writes frames on the real bus. Optional arguments select scenarios.
hil *scenarios:
    cd hil/host && cargo run --locked --quiet -- {{ scenarios }}

# Before a PR: CI, then the bench.
check: ci hil

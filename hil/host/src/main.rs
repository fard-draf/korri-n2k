//! `just hil`: flash the DUT, run each scenario against `can0`, verdict by exit code.
//!
//! Exit codes: 0 every scenario passed, 1 a scenario failed, 77 the bench is absent
//! (no probe, `can0` down): nothing was flashed. Scenario names on the command line
//! select a subset. Writes frames on the real bus: see `hil/README.md`.

mod bus;
mod hil_env;
mod oracle;
mod preflight;
mod scenario;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const ABSENT: u8 = 77;
const TARGET: &str = "thumbv7em-none-eabihf";

fn main() -> ExitCode {
    let env = match hil_env::Env::from_env() {
        Ok(env) => env,
        Err(message) => return absent(&message),
    };
    let requested: Vec<String> = std::env::args().skip(1).collect();
    if let Some(unknown) = requested
        .iter()
        .find(|n| !scenario::SCENARIOS.contains(&n.as_str()))
    {
        eprintln!(
            "unknown scenario {unknown}; known: {}",
            scenario::SCENARIOS.join(", ")
        );
        return ExitCode::from(1);
    }
    let names: Vec<&str> = scenario::SCENARIOS
        .into_iter()
        .filter(|n| requested.is_empty() || requested.iter().any(|r| r == n))
        .collect();

    if let Err(message) = preflight::check(&env.probe, &env.can) {
        return absent(&message);
    }
    if let Err(e) = std::fs::create_dir_all(&env.out) {
        eprintln!("cannot create {}: {e}", env.out.display());
        return ExitCode::from(1);
    }
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.out.join("firmware-target"));
    if let Err(message) = build_firmware(&target_dir) {
        eprintln!("firmware build failed: {message}");
        return ExitCode::from(1);
    }

    let mut failed = 0;
    for name in names {
        let elf = target_dir.join(TARGET).join("release").join(name);
        println!("== {name}");
        match scenario::run(name, &elf, &env) {
            Ok(verdict) => {
                println!("   firmware: {}", describe(&verdict.firmware));
                println!("   oracle:   {}", describe(&verdict.oracle));
                println!("   => {}", if verdict.passed() { "PASS" } else { "FAIL" });
                failed += !verdict.passed() as u32;
            }
            Err(message) => {
                println!("   => FAIL: {message}");
                failed += 1;
            }
        }
    }
    println!("logs and captures: {}", env.out.display());
    ExitCode::from(if failed == 0 { 0 } else { 1 })
}

fn absent(message: &str) -> ExitCode {
    eprintln!("HIL bench absent: {message}");
    ExitCode::from(ABSENT)
}

fn describe(result: &Result<(), String>) -> String {
    match result {
        Ok(()) => "ok".to_string(),
        Err(message) => format!("FAILED: {message}"),
    }
}

fn build_firmware(target_dir: &PathBuf) -> Result<(), String> {
    // Run from the firmware directory: its `.cargo/config.toml` (target, linker flags) and
    // `rust-toolchain.toml` are found from the working directory, not from `--manifest-path`.
    let status = Command::new("cargo")
        .args(["build", "--release", "--locked", "--bins", "--target-dir"])
        .arg(target_dir)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../firmware"))
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo exited with {status}"))
    }
}

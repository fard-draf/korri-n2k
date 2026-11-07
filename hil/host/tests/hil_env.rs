use std::process::{Command, Output};

fn run(timeout: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_korri-hil-host"))
        .env_clear()
        // No external command can run, even if validation regresses.
        .env("PATH", "")
        .env("KORRI_HIL_PROBE", "offline")
        .env("KORRI_HIL_TIMEOUT_S", timeout)
        .output()
        .unwrap()
}

#[test]
fn zero_timeout_is_rejected_before_preflight() {
    let output = run("0");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("KORRI_HIL_TIMEOUT_S must be greater than zero"),
        "{error}"
    );
}

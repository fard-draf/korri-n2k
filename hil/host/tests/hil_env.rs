use std::process::{Command, Output};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri-hil-host"));
    command
        .env_clear()
        // No external command can run, even if validation regresses.
        .env("PATH", "")
        .env("KORRI_HIL_PROBE", "offline");
    command
}

fn run(timeout: &str) -> Output {
    command()
        .env("KORRI_HIL_TIMEOUT_S", timeout)
        .output()
        .unwrap()
}

#[test]
fn zero_timeout_is_rejected_before_preflight() {
    let output = run("0");
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("KORRI_HIL_TIMEOUT_S must be greater than zero"),
        "{error}"
    );
}

#[test]
fn invalid_values_are_errors_even_without_a_probe() {
    for (name, value) in [
        ("KORRI_HIL_TIMEOUT_S", "banana"),
        ("KORRI_HIL_TIMEOUT_S", ""),
        ("KORRI_HIL_TIMEOUT_S", "18446744073709551616"),
        ("KORRI_HIL_EXPECTED_MESSAGES", "-1"),
        ("KORRI_HIL_CAN", ""),
        ("KORRI_HIL_OUT", ""),
    ] {
        let output = command()
            .env_remove("KORRI_HIL_PROBE")
            .env(name, value)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{name}={value}: {output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(name), "{error}");
        assert!(!error.contains("cannot run"), "preflight reached: {error}");
    }
}

#[test]
fn unconfigured_or_absent_bench_is_skipped() {
    for probe in [None, Some(""), Some("offline")] {
        let mut command = command();
        command.env_remove("KORRI_HIL_PROBE");
        if let Some(probe) = probe {
            command.env("KORRI_HIL_PROBE", probe);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(77), "{output:?}");
    }
}

#[test]
fn non_unicode_values_are_invalid_not_missing() {
    use std::os::unix::ffi::OsStringExt;
    for name in [
        "KORRI_HIL_PROBE",
        "KORRI_HIL_TIMEOUT_S",
        "KORRI_HIL_CAN",
        "KORRI_HIL_OUT",
    ] {
        let output = command()
            .env(name, std::ffi::OsString::from_vec(vec![0xFF]))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
    }
}

//! The scenarios and what the host knows about them. Keep the constants in step with
//! `../firmware/src/lib.rs`.

use crate::oracle::{self, Frame};
use crate::{bus, hil_env::Env};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const DUT_ADDRESS: u8 = 90;
/// Free on the bench (80 and 81 are the example boards). The host injects from it without
/// claiming it: the DUT is the only node that looks at it.
pub const INJECTOR_ADDRESS: u8 = 91;
pub const PGN: u32 = 127_489;
pub const PAYLOAD_LEN: usize = 26;
pub const MESSAGE_COUNT: usize = 5;
pub const TX_HOURS_BASE: u32 = 1_000;

/// The payload of injected message `index`, as `injected_payload` in the firmware.
pub fn injected_payload(index: u8) -> Vec<u8> {
    (0..PAYLOAD_LEN)
        .map(|k| (index as usize * 37 + k * 11 + 1) as u8)
        .collect()
}

pub const SCENARIOS: [&str; 2] = ["tx_fast_packet", "rx_fast_packet"];

pub struct Verdict {
    pub firmware: Result<(), String>,
    pub oracle: Result<(), String>,
}

impl Verdict {
    pub fn passed(&self) -> bool {
        self.firmware.is_ok() && self.oracle.is_ok()
    }
}

/// Runs one scenario: capture first, then the firmware, injecting when it asks for it.
pub fn run(name: &str, elf: &Path, env: &Env) -> Result<Verdict, String> {
    let log_path = env.out.join(format!("{name}.rtt.log"));
    let capture = bus::Capture::start(&env.can)?;
    let mut firmware = spawn_firmware(elf, env, &log_path)?;

    let injection = if name == "rx_fast_packet" {
        inject_when_ready(&capture, &mut firmware, env)
    } else {
        Ok(())
    };
    let status = wait_firmware(&mut firmware);
    std::thread::sleep(Duration::from_millis(300));
    let (frames, capture_result) = capture.stop();
    bus::write_log(&env.out.join(format!("{name}.can.log")), &frames)
        .map_err(|e| format!("cannot write the capture: {e}"))?;

    let firmware_result = match status {
        Some(0) => Ok(()),
        Some(124) | Some(137) => Err(format!(
            "timeout after {} s (see {})",
            env.timeout_s,
            log_path.display()
        )),
        Some(code) => Err(format!("exit code {code} (see {})", log_path.display())),
        None => Err("firmware killed by a signal".to_string()),
    };
    let oracle = match name {
        "tx_fast_packet" => {
            oracle::check_tx(&frames, DUT_ADDRESS, env.expected_messages, TX_HOURS_BASE)
        }
        _ => injection.and_then(|()| check_claim(&frames)),
    };
    Ok(Verdict {
        firmware: firmware_result,
        oracle: capture_result.and(oracle),
    })
}

fn check_claim(frames: &[Frame]) -> Result<(), String> {
    let claimed = frames.iter().any(|f| {
        oracle::source_address(f.id) == DUT_ADDRESS
            && oracle::pgn(f.id) == 60928
            && f.data.len() == 8
    });
    if claimed {
        Ok(())
    } else {
        Err(format!("no address claim from {DUT_ADDRESS}"))
    }
}

fn spawn_firmware(elf: &Path, env: &Env, log_path: &PathBuf) -> Result<Child, String> {
    let log = File::create(log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    Command::new("timeout")
        .args(["-k", "5", &env.timeout_s.to_string()])
        .args([
            "probe-rs",
            "run",
            "--non-interactive",
            "--disable-progressbars",
        ])
        .args(["--probe", &env.probe, "--chip", "STM32G431CBUx"])
        .arg(elf)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("cannot start probe-rs: {e}"))
}

fn wait_firmware(child: &mut Child) -> Option<i32> {
    child.wait().ok().and_then(|status| status.code())
}

/// Waits for the DUT to claim its address on the bus, then injects the messages.
fn inject_when_ready(
    capture: &bus::Capture,
    firmware: &mut Child,
    env: &Env,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if matches!(firmware.try_wait(), Ok(Some(_))) {
            return Err("firmware ended before claiming its address".to_string());
        }
        if check_claim(&capture.snapshot()).is_ok() {
            break;
        }
        if started.elapsed() > Duration::from_secs(env.timeout_s) {
            return Err("the DUT never claimed its address".to_string());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // The initial claim starts the 250 ms contention window; allow it to finish.
    std::thread::sleep(Duration::from_millis(300));

    let socket = bus::Injector::open(&env.can)?;
    let id = (2u32 << 26) | (PGN << 8) | INJECTOR_ADDRESS as u32;
    for index in 0..MESSAGE_COUNT as u8 {
        for frame in oracle::fast_packet_frames(id, index % 8, &injected_payload(index)) {
            socket.send(&frame)?;
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_payload_pattern_is_pinned() {
        // Same literals as the firmware pattern: index 1, bytes 0 and 1.
        assert_eq!(&injected_payload(1)[..3], &[38, 49, 60]);
        assert_eq!(injected_payload(0).len(), PAYLOAD_LEN);
    }
}

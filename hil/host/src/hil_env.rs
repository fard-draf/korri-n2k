//! Configuration, all from the environment. Nothing about the bench is written in the code.

use std::path::PathBuf;

pub struct Env {
    /// `probe-rs` selector of the DUT's probe (`VID:PID:SERIAL`), as printed by `probe-rs list`.
    pub probe: String,
    pub can: String,
    /// Logs, captures and the firmware build directory.
    pub out: PathBuf,
    /// Wall-clock limit of one scenario, flashing included.
    pub timeout_s: u64,
    /// Messages the `tx_fast_packet` oracle expects. Setting it wrong is how to see the bench fail.
    pub expected_messages: usize,
}

impl Env {
    pub fn from_env() -> Result<Self, String> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let number = |name: &str, default: u64| match var(name) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| format!("{name}={v} is not a number")),
        };
        Ok(Self {
            probe: var("KORRI_HIL_PROBE")
                .ok_or("KORRI_HIL_PROBE is not set (see hil/README.md)")?,
            can: var("KORRI_HIL_CAN").unwrap_or_else(|| "can0".to_string()),
            out: var("KORRI_HIL_OUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("korri-hil")),
            timeout_s: number("KORRI_HIL_TIMEOUT_S", 90)?,
            expected_messages: number(
                "KORRI_HIL_EXPECTED_MESSAGES",
                crate::scenario::MESSAGE_COUNT as u64,
            )? as usize,
        })
    }
}

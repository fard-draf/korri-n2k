//! Configuration, all from the environment. Nothing about the bench is written in the code.

use std::path::PathBuf;

pub struct Env {
    /// `probe-rs` selector of the DUT's probe (`VID:PID:SERIAL`), as printed by `probe-rs list`.
    pub probe: String,
    pub can: String,
    /// Logs, captures and the firmware build directory.
    pub out: PathBuf,
    /// Positive limit of the firmware process, flashing included (plus 5 s kill grace).
    pub timeout_s: u64,
    /// Messages the `tx_fast_packet` oracle expects. Setting it wrong is how to see the bench fail.
    pub expected_messages: usize,
}

pub enum ConfigError {
    MissingProbe,
    Invalid(String),
}

impl Env {
    pub fn from_env() -> Result<Self, ConfigError> {
        use ConfigError::Invalid;
        let var = |name: &str| match std::env::var(name) {
            Ok(value) if value.is_empty() => Err(Invalid(format!("{name} is empty"))),
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(Invalid(format!("{name} is not valid UTF-8"))),
        };
        let number = |name: &str, default: u64| match var(name)? {
            None => Ok(default),
            Some(v) => v
                .parse()
                .map_err(|_| Invalid(format!("{name}={v} is not a number"))),
        };
        // Validate all values before deciding whether the bench is configured.
        let timeout_s = number("KORRI_HIL_TIMEOUT_S", 90)?;
        if timeout_s == 0 {
            return Err(Invalid(
                "KORRI_HIL_TIMEOUT_S must be greater than zero".to_string(),
            ));
        }
        let expected_messages = usize::try_from(number(
            "KORRI_HIL_EXPECTED_MESSAGES",
            crate::scenario::MESSAGE_COUNT as u64,
        )?)
        .map_err(|_| Invalid("KORRI_HIL_EXPECTED_MESSAGES is too large".to_string()))?;
        let can = var("KORRI_HIL_CAN")?.unwrap_or_else(|| "can0".to_string());
        let out = var("KORRI_HIL_OUT")?
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("korri-hil"));
        let probe = match std::env::var("KORRI_HIL_PROBE") {
            Ok(value) if !value.is_empty() => value,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(Invalid("KORRI_HIL_PROBE is not valid UTF-8".to_string()))
            }
            _ => return Err(ConfigError::MissingProbe),
        };
        Ok(Self {
            probe,
            can,
            out,
            timeout_s,
            expected_messages,
        })
    }
}

//! Is the bench there? Every failure here is "hardware absent", never a test failure.

use std::process::Command;

/// The probe must be listed by `probe-rs list` and `can` UP, ERROR-ACTIVE at 250 kbit/s.
pub fn check(probe: &str, can: &str) -> Result<(), String> {
    let listed = output("probe-rs", &["list"])?;
    if !listed.contains(probe) {
        return Err(format!(
            "probe {probe} not listed by `probe-rs list`:\n{listed}"
        ));
    }
    let link = output("ip", &["-details", "link", "show", can])?;
    check_link(&link, can)
}

fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run `{program}`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reads the output of `ip -details link show <can>`.
fn check_link(link: &str, can: &str) -> Result<(), String> {
    let flags = link
        .split_once('<')
        .and_then(|(_, rest)| rest.split_once('>'))
        .map(|(flags, _)| flags)
        .unwrap_or("");
    if !flags.split(',').any(|flag| flag == "UP") {
        return Err(format!(
            "{can} is not UP (flags <{flags}>); bring it up at 250000 bit/s first"
        ));
    }
    if !link.contains("can state ERROR-ACTIVE") {
        return Err(format!("{can} is not ERROR-ACTIVE:\n{link}"));
    }
    if !link.contains("bitrate 250000 ") {
        return Err(format!("{can} is not at 250000 bit/s:\n{link}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: &str = "4: can0: <NOARP,UP,LOWER_UP,ECHO> mtu 16 qdisc pfifo_fast state UP\n    can state ERROR-ACTIVE restart-ms 0 \n\t  bitrate 250000 sample-point 0.875\n";

    #[test]
    fn a_working_interface_passes() {
        check_link(UP, "can0").unwrap();
    }

    #[test]
    fn down_passive_and_wrong_rate_are_refused() {
        assert!(check_link(&UP.replace("NOARP,UP,", "NOARP,"), "can0")
            .unwrap_err()
            .contains("not UP"));
        assert!(check_link(&UP.replace("ERROR-ACTIVE", "ERROR-PASSIVE"), "can0").is_err());
        assert!(check_link(&UP.replace("250000", "500000"), "can0")
            .unwrap_err()
            .contains("250000"));
    }
}

use log::{info, warn};

/// Doorbell chime mode: on or off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    On,
    Off,
}

impl Mode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "on" => Mode::On,
            "off" => Mode::Off,
            _ => {
                warn!("[config] Unknown mode '{}', defaulting to On", s);
                Mode::On
            }
        }
    }

    pub fn should_ring_chime(&self) -> bool {
        matches!(self, Mode::On)
    }
}

/// Parse the mode from a retained config message.
/// Expected format: {"mode":"on"} or {"mode":"off"}
pub fn parse_mode_from_config(payload: &[u8]) -> Mode {
    let payload_str = core::str::from_utf8(payload).unwrap_or("");

    if let Some(mode_idx) = payload_str.find("\"mode\"") {
        let after_mode = &payload_str[mode_idx + 6..];
        if let Some(colon_quote) = after_mode.find('"') {
            let value_start = colon_quote + 1;
            if let Some(value_end) = after_mode[value_start..].find('"') {
                let mode_str = &after_mode[value_start..value_start + value_end];
                info!("[config] Parsed mode: '{}'", mode_str);
                return Mode::from_str(mode_str);
            }
        }
    }

    warn!("[config] Could not parse mode, defaulting to On");
    Mode::On
}

use heapless::String;
use log::{info, warn};

/// Doorbell operating mode, controlled via AWS IoT Device Shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Sms,
    Chime,
    Both,
    Silent,
}

impl Mode {
    /// Parse mode from a JSON string value.
    pub fn from_str(s: &str) -> Self {
        match s {
            "sms" => Mode::Sms,
            "chime" => Mode::Chime,
            "both" => Mode::Both,
            "silent" => Mode::Silent,
            _ => {
                warn!("[shadow] Unknown mode '{}', defaulting to Sms", s);
                Mode::Sms
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Sms => "sms",
            Mode::Chime => "chime",
            Mode::Both => "both",
            Mode::Silent => "silent",
        }
    }

    pub fn should_send_sms(&self) -> bool {
        matches!(self, Mode::Sms | Mode::Both)
    }

    pub fn should_ring_chime(&self) -> bool {
        matches!(self, Mode::Chime | Mode::Both)
    }
}

/// Parse the desired mode from a Device Shadow JSON response.
///
/// Expected format (from $aws/things/doorbell/shadow/get/accepted):
/// ```json
/// {"state":{"desired":{"mode":"sms"},"reported":{...}},"metadata":{...}}
/// ```
///
/// This is a minimal parser that looks for `"mode":"<value>"` in the desired state.
/// We avoid pulling in a full JSON parser to save flash space.
pub fn parse_mode_from_shadow(payload: &[u8]) -> Mode {
    let payload_str = core::str::from_utf8(payload).unwrap_or("");

    // Look for "desired" section and extract mode value
    if let Some(desired_idx) = payload_str.find("\"desired\"") {
        let after_desired = &payload_str[desired_idx..];
        if let Some(mode_idx) = after_desired.find("\"mode\"") {
            let after_mode = &after_desired[mode_idx + 6..]; // skip past "mode"
            // Find the value between quotes: :"<value>"
            if let Some(colon_quote) = after_mode.find('"') {
                let value_start = colon_quote + 1;
                if let Some(value_end) = after_mode[value_start..].find('"') {
                    let mode_str = &after_mode[value_start..value_start + value_end];
                    info!("[shadow] Parsed mode: '{}'", mode_str);
                    return Mode::from_str(mode_str);
                }
            }
        }
    }

    warn!("[shadow] Could not parse mode from shadow, defaulting to Sms");
    Mode::Sms
}

/// Build the reported state JSON for shadow update.
pub fn build_reported_state(mode: Mode, timestamp: u64) -> String<128> {
    let mut json: String<128> = String::new();
    // Manual JSON construction to avoid allocator
    let _ = core::fmt::Write::write_fmt(
        &mut json,
        format_args!(
            r#"{{"state":{{"reported":{{"mode":"{}","last_ring":{}}}}}}}"#,
            mode.as_str(),
            timestamp
        ),
    );
    json
}

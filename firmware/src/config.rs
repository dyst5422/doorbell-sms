use core::ffi::CStr;

/// WiFi credentials (set via environment variables in .cargo/config.toml)
pub const WIFI_SSID: &str = env!("WIFI_SSID");
pub const WIFI_PASS: &str = env!("WIFI_PASS");

/// AWS IoT Core endpoint
pub const AWS_IOT_ENDPOINT: &str = "a3a6f24s9h7ycl-ats.iot.us-west-2.amazonaws.com";
pub const AWS_IOT_ENDPOINT_CSTR: &CStr = match CStr::from_bytes_with_nul(
    b"a3a6f24s9h7ycl-ats.iot.us-west-2.amazonaws.com\0",
) {
    Ok(c) => c,
    Err(_) => panic!("Invalid endpoint CStr"),
};
pub const AWS_IOT_PORT: u16 = 8883;
pub const MQTT_CLIENT_ID: &str = "doorbell";

/// MQTT Topics
pub const TOPIC_RING: &str = "doorbell/ring";
pub const TOPIC_SHADOW_GET: &str = "$aws/things/doorbell/shadow/get";
pub const TOPIC_SHADOW_GET_ACCEPTED: &str = "$aws/things/doorbell/shadow/get/accepted";
pub const TOPIC_SHADOW_UPDATE: &str = "$aws/things/doorbell/shadow/update";

/// GPIO Pin assignments
pub const PIN_DOORBELL_WAKE: u8 = 2; // Optocoupler output, wakes from deep sleep
pub const PIN_RELAY: u8 = 3; // Relay control for chime

/// Timing
pub const WIFI_CONNECT_TIMEOUT_MS: u64 = 10_000;
pub const RELAY_PULSE_MS: u64 = 500;
pub const MQTT_TIMEOUT_MS: u64 = 10_000;

/// TLS Certificates (embedded at compile time as PEM with null terminator for mbedtls)
pub const CA_CERT: &CStr = match CStr::from_bytes_with_nul(
    concat!(include_str!("../certs/AmazonRootCA1.pem"), "\0").as_bytes(),
) {
    Ok(c) => c,
    Err(_) => panic!("Invalid CA cert"),
};

pub const DEVICE_CERT: &CStr = match CStr::from_bytes_with_nul(
    concat!(include_str!("../certs/device.cert.pem"), "\0").as_bytes(),
) {
    Ok(c) => c,
    Err(_) => panic!("Invalid device cert"),
};

pub const DEVICE_KEY: &CStr = match CStr::from_bytes_with_nul(
    concat!(include_str!("../certs/device.key.pem"), "\0").as_bytes(),
) {
    Ok(c) => c,
    Err(_) => panic!("Invalid device key"),
};

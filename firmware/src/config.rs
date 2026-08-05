/// WiFi credentials (set via environment variables in .cargo/config.toml)
pub const WIFI_SSID: &str = env!("SSID");
pub const WIFI_PASSWORD: &str = env!("PASSWORD");

/// AWS IoT Core endpoint (get via: aws iot describe-endpoint --endpoint-type iot:Data-ATS --region us-west-2)
pub const AWS_IOT_ENDPOINT: &str = "a3a6f24s9h7ycl-ats.iot.us-west-2.amazonaws.com";
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

/// TLS Certificates (embedded at compile time)
/// Place your certificate files in the certs/ directory
pub const DEVICE_CERT: &[u8] = include_bytes!("../certs/device.cert.pem");
pub const DEVICE_KEY: &[u8] = include_bytes!("../certs/device.key.pem");
pub const CA_CERT: &[u8] = include_bytes!("../certs/AmazonRootCA1.pem");

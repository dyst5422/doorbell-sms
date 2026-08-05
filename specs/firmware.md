# Firmware Specification — doorbell-sms

## 1. Overview

| Property       | Value                            |
|----------------|----------------------------------|
| Language       | Rust (`no_std`, no heap allocation) |
| Target MCU     | ESP32-C3 (RISC-V)               |
| Async Runtime  | Embassy                          |
| Build Target   | `riscv32imc-unknown-none-elf`    |

The firmware operates in a wake-do-sleep cycle. The ESP32-C3 remains in deep sleep until the doorbell button (GPIO 2) triggers a rising edge interrupt. Upon wake, the device connects to WiFi, communicates with AWS IoT Core over MQTT/TLS, executes the configured mode logic, then returns to deep sleep.

---

## 2. Dependencies

| Crate | Version / Source | Purpose |
|-------|-----------------|---------|
| `esp-hal` | crates.io (latest compatible) | Hardware Abstraction Layer for ESP32-C3 (GPIO, timers, RTC) |
| `esp-wifi` | crates.io (latest compatible) | WiFi driver (`no_std`, Embassy-compatible) |
| `embassy-executor` | crates.io | Async task executor |
| `embassy-net` | crates.io | TCP/IP stack (DHCP, DNS, TCP sockets) |
| `embassy-time` | crates.io | Async timers and delays |
| `embedded-tls` | crates.io | TLS 1.3 for mutual authentication with AWS IoT Core |
| `rust-mqtt` | crates.io | `no_std` async MQTT 3.1.1 client |
| `embedded-hal` | crates.io | HAL traits for portable peripheral access |
| `defmt` | crates.io | Structured logging framework for embedded |
| `defmt-rtt` | crates.io | RTT transport for defmt (logging via debug probe) |
| `esp-backtrace` | crates.io | Panic handler with backtrace support for ESP32-C3 |

---

## 3. Boot Sequence

1. **Wake** from deep sleep via GPIO 2 rising edge interrupt.
2. **Initialize peripherals** — configure GPIO pins, bring up WiFi radio.
3. **Connect to WiFi** — SSID and password are compiled into the firmware as constants.
4. **Obtain IP address** via DHCP.
5. **Establish TLS connection** to AWS IoT Core endpoint on port 8883 (mutual TLS with device certificate).
6. **MQTT CONNECT** with client ID `doorbell`.
7. **Read Device Shadow** — publish to `$aws/things/doorbell/shadow/get` and subscribe to `$aws/things/doorbell/shadow/get/accepted`.
8. **Parse shadow** response for the current desired mode.
9. **Execute mode logic** (see Section 4).
10. **Publish reported state** to Device Shadow via `$aws/things/doorbell/shadow/update`.
11. **MQTT DISCONNECT**.
12. **Configure GPIO 2** as deep sleep wake source (rising edge trigger).
13. **Enter deep sleep**.

---

## 4. Mode Logic

### Local Override Check (Future Enhancement)

> **Note:** The local toggle switch on GPIO 4 is deferred to a future iteration. For the initial build, mode is controlled exclusively via the Device Shadow.

When implemented:
- If the switch is in the **override position**: use the locally configured mode regardless of the shadow's desired state.
- If the switch is in the **normal position**: use the shadow's `desired.mode` value.

### Modes

| Mode | Behavior |
|------|----------|
| `sms` | Publish `{"event":"ring","timestamp":<epoch>,"device":"doorbell"}` to topic `doorbell/ring` |
| `chime` | Assert **GPIO 3 HIGH** for 500 ms (energize relay to sound physical chime) |
| `both` | Execute both `sms` and `chime` actions |
| `silent` | Do nothing; proceed directly to sleep |

---

## 5. Configuration

| Item | Strategy |
|------|----------|
| WiFi SSID | Compiled in as `const &str` |
| WiFi Password | Compiled in as `const &str` |
| AWS IoT Endpoint | Compiled in as `const &str` |
| Device Certificate | Embedded via `include_bytes!("../certs/device.cert.pem")` |
| Private Key | Embedded via `include_bytes!("../certs/device.key.pem")` |
| Root CA | Embedded via `include_bytes!("../certs/AmazonRootCA1.pem")` |
| Device Name / Client ID | `doorbell` |

### MQTT Topics

| Purpose | Topic |
|---------|-------|
| Ring event publish | `doorbell/ring` |
| Shadow get (publish) | `$aws/things/doorbell/shadow/get` |
| Shadow get accepted (subscribe) | `$aws/things/doorbell/shadow/get/accepted` |
| Shadow update (publish) | `$aws/things/doorbell/shadow/update` |

---

## 6. Error Handling

| Condition | Behavior |
|-----------|----------|
| WiFi connection timeout (10 s) | Abort; go back to deep sleep. Retry on next wake. |
| MQTT connection failure | Abort; go back to deep sleep. |
| Shadow parse failure | Default to mode `sms`. |
| Any panic | `esp-backtrace` logs via defmt-rtt; device resets automatically. |

---

## 7. Project Structure

```
doorbell-sms/
├── .cargo/
│   └── config.toml          # target, runner (espflash)
├── src/
│   ├── main.rs              # entry point, boot sequence
│   ├── wifi.rs              # WiFi connection logic
│   ├── mqtt.rs              # MQTT + TLS connection, publish
│   ├── shadow.rs            # Device Shadow get/parse/update
│   ├── doorbell.rs          # Mode logic (relay, SMS trigger)
│   └── config.rs            # Constants (SSID, endpoint, pins)
├── certs/
│   ├── device.cert.pem      # X.509 device certificate
│   ├── device.key.pem       # Private key
│   └── AmazonRootCA1.pem    # AWS root CA
├── specs/
│   └── firmware.md          # This specification
├── Cargo.toml
├── rust-toolchain.toml      # nightly + riscv32imc target
└── README.md
```

---

## 8. Build & Flash

```bash
# Install the RISC-V target
rustup target add riscv32imc-unknown-none-elf

# Install the flash tool
cargo install espflash

# Build the firmware (release profile for size optimization)
cargo build --release

# Flash to a connected ESP32-C3
espflash flash target/riscv32imc-unknown-none-elf/release/doorbell-sms

# Monitor serial output (defmt logs)
espflash monitor
```

---

## 9. Testing Strategy

| Level | Approach |
|-------|----------|
| Unit tests | Mode logic in `doorbell.rs` is testable with `cargo test` on the host (guarded by `#[cfg(test)]`). |
| Hardware integration | Connect a pushbutton to GPIO 2 to simulate a doorbell press; verify wake and full cycle. |
| MQTT verification | Use the AWS IoT Core MQTT test client to subscribe to `doorbell/ring` and confirm messages arrive. |
| End-to-end | Press button → verify SMS is received on the configured phone number. |

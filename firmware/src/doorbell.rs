use embassy_time::{Duration, Timer};
use esp_hal::gpio::Output;
use log::info;

use crate::config::RELAY_PULSE_MS;
use crate::shadow::Mode;

/// Execute doorbell mode logic.
///
/// Returns true if an MQTT message should be published (mode is Sms or Both).
pub async fn execute_mode(mode: Mode, relay_pin: &mut Output<'_>) -> bool {
    info!("[doorbell] Executing mode: {:?}", mode);

    let should_publish = mode.should_send_sms();
    let should_chime = mode.should_ring_chime();

    if should_chime {
        info!("[doorbell] Energizing relay for chime");
        relay_pin.set_high();
        Timer::after(Duration::from_millis(RELAY_PULSE_MS)).await;
        relay_pin.set_low();
        info!("[doorbell] Relay released");
    }

    should_publish
}

/// Build the MQTT payload for a doorbell ring event.
pub fn build_ring_payload(timestamp: u64) -> heapless::String<128> {
    let mut json: heapless::String<128> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut json,
        format_args!(
            r#"{{"event":"ring","timestamp":{},"device":"doorbell"}}"#,
            timestamp
        ),
    );
    json
}

use embassy_time::{Duration, Timer};
use esp_hal::gpio::Output;
use log::info;

use crate::config::RELAY_PULSE_MS;

/// Fire the relay to ring the chime.
pub async fn ring_chime(relay_pin: &mut Output<'_>) {
    info!("[doorbell] Energizing relay for chime");
    relay_pin.set_high();
    Timer::after(Duration::from_millis(RELAY_PULSE_MS)).await;
    relay_pin.set_low();
    info!("[doorbell] Relay released");
}

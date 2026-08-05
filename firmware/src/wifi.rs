use embassy_time::{Duration, Timer};
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState,
};
use embassy_net::Runner;
use log::{debug, error, info};

use crate::config::{WIFI_SSID, WIFI_PASSWORD};

/// Background task: maintains WiFi connection, reconnects on disconnect.
#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    info!("[wifi] Starting connection task");
    debug!("[wifi] Device capabilities: {:?}", controller.capabilities());

    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                info!("[wifi] Disconnected, will reconnect...");
                Timer::after(Duration::from_millis(1000)).await;
            }
            _ => {}
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: WIFI_SSID.try_into().unwrap(),
                password: WIFI_PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            info!("[wifi] Starting WiFi...");
            controller.start_async().await.unwrap();
            info!("[wifi] WiFi started");
        }

        info!("[wifi] Connecting...");
        match controller.connect_async().await {
            Ok(_) => info!("[wifi] Connected!"),
            Err(e) => {
                error!("[wifi] Connection failed: {:?}", e);
                Timer::after(Duration::from_millis(2000)).await;
            }
        }
    }
}

/// Background task: processes network packets for embassy-net.
#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

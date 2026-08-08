use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{Interface, WifiController};
use log::{error, info};

/// Background task: maintains WiFi connection, reconnects on disconnect.
#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    info!("[wifi] Starting connection task");

    loop {
        info!("[wifi] About to connect...");
        match controller.connect_async().await {
            Ok(info) => {
                info!("[wifi] Connected: {:?}", info);
                // Wait until we're no longer connected
                let disconnect_info = controller.wait_for_disconnect_async().await.ok();
                error!("[wifi] Disconnected: {:?}", disconnect_info);
            }
            Err(e) => {
                error!("[wifi] Connection failed: {:?}", e);
            }
        }
        Timer::after(Duration::from_millis(5000)).await;
    }
}

/// Background task: processes network packets for embassy-net.
#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await;
}

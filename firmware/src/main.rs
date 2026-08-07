#![no_std]
#![no_main]

extern crate alloc;

use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_wifi::{init, wifi::WifiDevice, EspWifiController};

use embassy_executor::Spawner;
use embassy_net::{Config as EmbassyNetConfig, StackResources};
use embassy_time::{Duration, Timer};

use log::info;
use static_cell::StaticCell;

mod config;
mod doorbell;
mod mqtt;
mod shadow;
mod wifi;

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    info!("[main] Doorbell firmware starting...");

    // Initialize hardware
    let hal_config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(hal_config);

    // Heap allocator (needed for esp-wifi)
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // GPIO setup
    let mut relay_pin = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());
    // GPIO2 is the wake pin — handled by deep sleep config before sleeping

    // Timer groups
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timg1 = TimerGroup::new(peripherals.TIMG1);

    // RNG (needed for WiFi and network stack)
    let mut rng = Rng::new(peripherals.RNG);

    // Initialize WiFi
    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timg0.timer0, rng.clone()).unwrap()
    );

    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, peripherals.WIFI).unwrap();
    let wifi_interface = interfaces.sta;

    // Initialize Embassy timer
    esp_hal_embassy::init(timg1.timer0);

    // Network stack config (DHCP)
    let net_config = EmbassyNetConfig::dhcpv4(Default::default());
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    // Create network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    // Spawn background tasks
    spawner.spawn(wifi::connection_task(controller)).ok();
    spawner.spawn(wifi::net_task(runner)).ok();

    // Wait for WiFi link
    info!("[main] Waiting for WiFi link...");
    let wifi_timeout = Duration::from_millis(config::WIFI_CONNECT_TIMEOUT_MS);
    let start = embassy_time::Instant::now();
    loop {
        if stack.is_link_up() {
            break;
        }
        if embassy_time::Instant::now() - start > wifi_timeout {
            info!("[main] WiFi timeout, going to sleep");
            enter_deep_sleep();
        }
        Timer::after(Duration::from_millis(100)).await;
    }

    // Wait for IP address
    info!("[main] Waiting for IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("[main] Got IP: {}", config.address);
            break;
        }
        if embassy_time::Instant::now() - start > wifi_timeout {
            info!("[main] DHCP timeout, going to sleep");
            enter_deep_sleep();
        }
        Timer::after(Duration::from_millis(100)).await;
    }

    // Run MQTT workflow (shadow check → mode logic → publish)
    info!("[main] Starting MQTT workflow...");
    let rng_seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let _ = mqtt::mqtt_workflow(stack, &mut relay_pin, rng_seed).await;

    // Done — enter deep sleep until next doorbell press
    info!("[main] Work complete, entering deep sleep...");
    enter_deep_sleep();
}

/// Configure GPIO2 as wake source and enter deep sleep.
///
/// TODO: Implement actual deep sleep using esp-hal RTC/sleep APIs.
/// For now, this is a placeholder that loops forever (low power idle).
/// Real implementation will use:
///   - rtc_cntl.sleep_deep() with GPIO2 as ext0 wake source
fn enter_deep_sleep() -> ! {
    info!("[main] Entering deep sleep (GPIO2 wake)...");
    // TODO: Configure RTC GPIO2 as wake source (rising edge)
    // TODO: Call deep sleep
    // For now, just halt
    loop {
        // In production: this will be replaced with actual deep sleep entry
        unsafe { core::arch::asm!("wfi") };
    }
}

#![no_std]
#![no_main]

extern crate alloc;

use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    ram,
    rng::{Trng, TrngSource},
    rtc_cntl::Rtc,
    rtc_cntl::{reset_reason, wakeup_cause, SocResetReason},
    system::Cpu,
    timer::timg::TimerGroup,
};
use esp_metadata_generated::memory_range;
use esp_radio as _;
use esp_radio::wifi::{Config, ControllerConfig};
use esp_radio::wifi::sta::StationConfig;
use tinyrlibc as _;

use embassy_executor::Spawner;
use embassy_net::{Config as NetConfig, StackResources};
use embassy_time::{Duration, Timer};

use mbedtls_rs::sys::hook::backend::embassy::timer::EmbassyTimer;
use mbedtls_rs::sys::hook::backend::esp::wall_clock::EspRtcWallClock;
use mbedtls_rs::sys::hook::backend::esp::EspAccel;
use mbedtls_rs::Tls;

use log::info;
use static_cell::StaticCell;

mod config;
mod doorbell;
mod mqtt;
mod shadow;
mod wifi;

/// Reclaimed RAM size from linker metadata.
pub const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// Heap size: 140 KiB to accommodate TLS buffers.
const HEAP_SIZE: usize = 140 * 1024;

const CURRENT_TIME_MS: &str = env!("CURRENT_TIME_MS");

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        STATIC_CELL.uninit()
    }};
    ($t:ty,$val:expr) => {{
        mk_static!($t).write($val)
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    info!("[main] Doorbell firmware starting...");

    // Startup delay: gives USB time to enumerate for flashing/monitoring
    for _ in 0..5_000_000 {
        unsafe { core::arch::asm!("nop") };
    }

    // Log reset and wakeup reason
    let reason = reset_reason(Cpu::ProCpu);
    let wake = wakeup_cause();
    info!("[main] Reset reason: {:?}, Wake cause: {:?}", reason, wake);

    // Heap allocator (with reclaimed RAM)
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);
    heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);

    // Initialize hardware
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // GPIO setup
    let mut relay_pin = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());

    // Timer groups - TIMG0 used for esp-rtos
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    // Start esp-rtos (replaces esp_hal_embassy::init)
    esp_rtos::start(
        timg0.timer0,
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT)
            .software_interrupt0,
    );

    // Hook Embassy timer for mbedtls
    let timer = mk_static!(EmbassyTimer, EmbassyTimer);
    unsafe {
        mbedtls_rs::sys::hook::timer::hook_timer(Some(timer));
    }

    // Setup RTC for EspRtcWallClock
    let rtc = &*mk_static!(Rtc, Rtc::new(peripherals.LPWR));
    rtc.set_current_time_us(
        CURRENT_TIME_MS
            .parse::<u64>()
            .expect("Failed to parse CURRENT_TIME_MS")
            * 1000,
    );

    // Hook wall clock for mbedtls
    let clock = mk_static!(EspRtcWallClock<&Rtc>, EspRtcWallClock::new(rtc));
    unsafe {
        mbedtls_rs::sys::hook::wall_clock::hook_wall_clock(Some(clock));
    }

    // Configure hardware accelerators
    let mut accel = EspAccel::new()
        .with_sha(peripherals.SHA)
        .with_rsa(peripherals.RSA)
        .with_aes(peripherals.AES);

    let accel_queue = accel.start();
    let _hooked = unsafe { accel_queue.hook() };

    // Create TRNG for random number generation
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let trng = mk_static!(Trng, Trng::try_new().unwrap());

    // Get seed before handing trng to Tls (which borrows it mutably)
    let seed = (trng.random() as u64) << 32 | trng.random() as u64;

    // Create TLS context
    let mut tls = Tls::new(trng).unwrap();

    // Configure WiFi
    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(config::WIFI_SSID)
            .with_password(config::WIFI_PASS.into()),
    );

    info!("[main] Starting WiFi...");
    let (controller, wifi_interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();

    // Network stack config (DHCP)
    let net_config = NetConfig::dhcpv4(Default::default());

    // Create network stack
    let stack_resources = mk_static!(StackResources<3>, StackResources::new());
    let (stack, runner) = embassy_net::new(wifi_interfaces.station, net_config, stack_resources, seed);

    // Spawn background tasks
    spawner.spawn(wifi::connection_task(controller).unwrap());
    spawner.spawn(wifi::net_task(runner).unwrap());

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
        if let Some(cfg) = stack.config_v4() {
            info!("[main] Got IP: {}", cfg.address);
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
    let _ = mqtt::mqtt_workflow(&mut tls, stack, &mut relay_pin).await;

    // Done — enter deep sleep until next doorbell press
    info!("[main] Work complete, entering deep sleep...");
    enter_deep_sleep();
}

/// Configure GPIO5 as wake source and enter deep sleep.
fn enter_deep_sleep() -> ! {
    info!("[main] Entering deep sleep (GPIO5 wake)...");

    // Small delay to let the log flush
    for _ in 0..100_000 {
        unsafe { core::arch::asm!("nop") };
    }

    unsafe {
        use esp_hal::rtc_cntl::Rtc;
        use esp_hal::rtc_cntl::sleep::{RtcioWakeupSource, WakeupLevel};
        use esp_hal::gpio::RtcPinWithResistors;

        let peripherals = esp_hal::peripherals::Peripherals::steal();
        let mut rtc = Rtc::new(peripherals.LPWR);
        let mut gpio5 = peripherals.GPIO5;

        let wakeup_pins: &mut [(&mut dyn RtcPinWithResistors, WakeupLevel)] = &mut [
            (&mut gpio5, WakeupLevel::Low),
        ];
        let rtcio = RtcioWakeupSource::new(wakeup_pins);

        rtc.sleep_deep(&[&rtcio]);
    }
}

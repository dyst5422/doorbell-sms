use esp_idf_hal::gpio::{Gpio1, Gpio3, Gpio14, Gpio21, Gpio22, PinDriver, Input, Output, Pull};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_sys as _;
use log::*;
use std::thread;
use std::time::Duration;

// Zigbee FFI bindings
use esp_idf_sys::{
    esp_zb_cfg_t, esp_zb_init, esp_zb_set_primary_network_channel_set,
    esp_zb_start, esp_zb_main_loop_iteration,
    esp_zb_on_off_light_cfg_t, esp_zb_on_off_light_ep_create,
    esp_zb_ep_list_add_ep, esp_zb_ep_list_create,
    esp_zb_device_register, esp_zb_core_action_handler_register,
    ESP_ZB_ZED_CONFIG, ESP_ZB_TRANSCEIVER_ALL_CHANNELS_MASK,
};

/// GPIO pins
const GPIO_OPTOCOUPLER: i32 = 1;   // D1 - wake from deep sleep
const GPIO_RELAY_SET: i32 = 21;     // D3 - latching relay SET (coil pin 1)
const GPIO_RELAY_RESET: i32 = 22;   // D4 - latching relay RESET (coil pin 10)
const GPIO_ANT_ENABLE: i32 = 3;     // RF switch enable
const GPIO_ANT_SELECT: i32 = 14;    // RF switch select (high = external)

/// Relay pulse duration in ms
const RELAY_PULSE_MS: u64 = 15;

/// Current chime state (persisted in Zigbee NVS via on/off attribute)
static mut CHIME_ENABLED: bool = true;

fn main() {
    // Initialize ESP-IDF
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Doorbell Zigbee firmware starting...");

    let peripherals = Peripherals::take().unwrap();

    // Configure external antenna
    let mut ant_enable = PinDriver::output(peripherals.pins.gpio3).unwrap();
    let mut ant_select = PinDriver::output(peripherals.pins.gpio14).unwrap();
    ant_enable.set_low().unwrap();  // Enable RF switch
    thread::sleep(Duration::from_millis(100));
    ant_select.set_high().unwrap(); // Select external antenna
    info!("External antenna enabled");

    // Configure relay pins
    let mut relay_set = PinDriver::output(peripherals.pins.gpio21).unwrap();
    let mut relay_reset = PinDriver::output(peripherals.pins.gpio22).unwrap();
    relay_set.set_low().unwrap();
    relay_reset.set_low().unwrap();

    // Check wake reason
    let wake_reason = unsafe { esp_idf_sys::esp_sleep_get_wakeup_cause() };
    info!("Wake reason: {}", wake_reason);

    // If woken by GPIO (doorbell press), pulse the relay based on current state
    if wake_reason == esp_idf_sys::esp_sleep_wakeup_cause_t_ESP_SLEEP_WAKEUP_EXT1 {
        info!("Woken by doorbell press!");
        // State is managed by Zigbee attribute - read from NVS on next Zigbee init
        // For now, ring the chime (safe default on wake)
        // The actual state will be set by Zigbee after initialization
    }

    // Initialize Zigbee
    info!("Initializing Zigbee...");
    unsafe {
        let zb_cfg = esp_zb_cfg_t {
            esp_zb_role: esp_idf_sys::esp_zb_nwk_device_type_t_ESP_ZB_DEVICE_TYPE_ED,
            install_code_policy: false,
            nwk_cfg: esp_idf_sys::esp_zb_cfg_t__bindgen_ty_1 {
                zed_cfg: esp_idf_sys::esp_zb_cfg_t__bindgen_ty_1__bindgen_ty_2 {
                    ed_timeout: 10, // ESP_ZB_ED_AGING_TIMEOUT_64MIN
                    keep_alive: 60000, // 60 second keep-alive (poll interval)
                },
            },
        };

        esp_zb_init(&zb_cfg);
        esp_zb_set_primary_network_channel_set(ESP_ZB_TRANSCEIVER_ALL_CHANNELS_MASK);

        // Create On/Off light endpoint (endpoint 1)
        // This makes it appear as a switchable device in Alexa
        let ep_list = esp_zb_ep_list_create();

        let on_off_cfg = esp_zb_on_off_light_cfg_t::default();
        let ep = esp_zb_on_off_light_ep_create(1, &on_off_cfg);
        esp_zb_ep_list_add_ep(ep_list, ep);

        esp_zb_device_register(ep_list);

        // Register action handler for on/off commands
        esp_zb_core_action_handler_register(Some(zb_action_handler));

        // Start Zigbee stack
        esp_zb_start(false);
    }

    info!("Zigbee started, entering main loop...");

    // Main loop - process Zigbee events
    loop {
        unsafe {
            esp_zb_main_loop_iteration();
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Zigbee action handler - called when we receive on/off commands
unsafe extern "C" fn zb_action_handler(
    callback_type: esp_idf_sys::esp_zb_core_action_callback_id_t,
    message: *const core::ffi::c_void,
) -> esp_idf_sys::esp_err_t {
    info!("Zigbee action: callback_type={}", callback_type);

    // Handle SET_ATTRIBUTE action (on/off command from Alexa)
    if callback_type == esp_idf_sys::ESP_ZB_CORE_SET_ATTR_VALUE_CB_ID {
        let msg = message as *const esp_idf_sys::esp_zb_zcl_set_attr_value_message_t;
        if !msg.is_null() {
            let cluster_id = (*msg).info.cluster;
            let attr_id = (*msg).attribute.id;

            // On/Off cluster (0x0006), OnOff attribute (0x0000)
            if cluster_id == 0x0006 && attr_id == 0x0000 {
                let value_ptr = (*msg).attribute.data.value as *const u8;
                let on_off = *value_ptr != 0;

                info!("Received on/off command: {}", if on_off { "ON" } else { "OFF" });

                if on_off {
                    // Chime ON: relay opens (NO opens = chime circuit intact)
                    set_relay_state(true);
                    CHIME_ENABLED = true;
                } else {
                    // Chime OFF: relay closes (NO closes = shorts chime = silent)
                    set_relay_state(false);
                    CHIME_ENABLED = false;
                }
            }
        }
    }

    esp_idf_sys::ESP_OK as esp_idf_sys::esp_err_t
}

/// Set the latching relay state
/// true = chime enabled (relay reset/open), false = chime disabled (relay set/closed)
fn set_relay_state(chime_on: bool) {
    // Safety: we're the only ones accessing these pins
    unsafe {
        let peripherals = Peripherals::take().unwrap_or_else(|_| {
            // Peripherals already taken, use raw GPIO
            set_relay_raw(chime_on);
            return;
        });
    }
}

/// Raw GPIO relay control (when peripherals already taken)
fn set_relay_raw(chime_on: bool) {
    unsafe {
        if chime_on {
            // Pulse RESET coil: current flows pin 1 → pin 10, NO opens
            esp_idf_sys::gpio_set_level(GPIO_RELAY_RESET as u32, 1);
            thread::sleep(Duration::from_millis(RELAY_PULSE_MS));
            esp_idf_sys::gpio_set_level(GPIO_RELAY_RESET as u32, 0);
            info!("Relay RESET: chime enabled");
        } else {
            // Pulse SET coil: current flows pin 10 → pin 1, NO closes (shorts chime)
            esp_idf_sys::gpio_set_level(GPIO_RELAY_SET as u32, 1);
            thread::sleep(Duration::from_millis(RELAY_PULSE_MS));
            esp_idf_sys::gpio_set_level(GPIO_RELAY_SET as u32, 0);
            info!("Relay SET: chime disabled (silenced)");
        }
    }
}

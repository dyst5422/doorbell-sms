use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_sys as _;
use esp_idf_sys::zigbee::*;
use log::*;
use std::thread;
use std::time::Duration;

/// GPIO pins (XIAO ESP32-C6 mapping)
const GPIO_RELAY_SET: i32 = 21;    // D3 - latching relay SET coil
const GPIO_RELAY_RESET: i32 = 22;  // D4 - latching relay RESET coil
const RELAY_PULSE_MS: u64 = 15;

fn main() {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Doorbell Zigbee firmware starting...");

    let peripherals = Peripherals::take().unwrap();

    // Configure external antenna
    let mut ant_enable = PinDriver::output(peripherals.pins.gpio3).unwrap();
    let mut ant_select = PinDriver::output(peripherals.pins.gpio14).unwrap();
    ant_enable.set_low().unwrap();
    thread::sleep(Duration::from_millis(100));
    ant_select.set_high().unwrap();
    info!("External antenna enabled");

    // Configure relay pins as outputs
    unsafe {
        esp_idf_sys::gpio_set_direction(GPIO_RELAY_SET as esp_idf_sys::gpio_num_t, esp_idf_sys::gpio_mode_t_GPIO_MODE_OUTPUT);
        esp_idf_sys::gpio_set_direction(GPIO_RELAY_RESET as esp_idf_sys::gpio_num_t, esp_idf_sys::gpio_mode_t_GPIO_MODE_OUTPUT);
        esp_idf_sys::gpio_set_level(GPIO_RELAY_SET as esp_idf_sys::gpio_num_t, 0);
        esp_idf_sys::gpio_set_level(GPIO_RELAY_RESET as esp_idf_sys::gpio_num_t, 0);
    }

    // Initialize NVS (required for PHY calibration and Zigbee storage)
    unsafe {
        esp_idf_sys::nvs_flash_init();
    }

    // Initialize Zigbee
    info!("Initializing Zigbee...");
    unsafe {
        let mut zb_cfg: esp_zb_cfg_s = core::mem::zeroed();
        zb_cfg.esp_zb_role = esp_zb_nwk_device_type_t_ESP_ZB_DEVICE_TYPE_ED;
        zb_cfg.install_code_policy = false;
        zb_cfg.nwk_cfg.zed_cfg.ed_timeout = 10; // ESP_ZB_ED_AGING_TIMEOUT_64MIN
        zb_cfg.nwk_cfg.zed_cfg.keep_alive = 60000; // 60 second poll interval

        esp_zb_init(&mut zb_cfg);
        esp_zb_set_primary_network_channel_set(ESP_ZB_TRANSCEIVER_ALL_CHANNELS_MASK);

        // Create endpoint list
        let ep_list = esp_zb_ep_list_create();

        // Create On/Off light cluster list
        let mut on_off_cfg: esp_zb_on_off_light_cfg_s = core::mem::zeroed();
        let cluster_list = esp_zb_on_off_light_clusters_create(&mut on_off_cfg);

        // Configure endpoint
        let ep_config = esp_zb_endpoint_config_s {
            endpoint: 1,
            app_profile_id: 0x0104, // HA profile
            app_device_id: 0x0100,  // On/Off Light
            _bitfield_align_1: [0; 0],
            _bitfield_1: Default::default(),
        };

        // Add endpoint to list
        esp_zb_ep_list_add_ep(ep_list, cluster_list, ep_config);

        // Register device
        esp_zb_device_register(ep_list);

        // Register action handler
        esp_zb_core_action_handler_register(Some(zb_action_handler));

        // Start Zigbee
        info!("Starting Zigbee stack...");
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
    callback_type: esp_zb_core_action_callback_id_t,
    message: *const core::ffi::c_void,
) -> esp_idf_sys::esp_err_t {
    info!("Zigbee action: callback_type={}", callback_type);

    if callback_type == esp_zb_core_action_callback_id_s_ESP_ZB_CORE_SET_ATTR_VALUE_CB_ID as esp_zb_core_action_callback_id_t {
        let msg = message as *const esp_zb_zcl_set_attr_value_message_s;
        if !msg.is_null() {
            let cluster_id = (*msg).info.cluster;
            let attr_id = (*msg).attribute.id;

            // On/Off cluster (0x0006), OnOff attribute (0x0000)
            if cluster_id == 0x0006 && attr_id == 0x0000 {
                let value_ptr = (*msg).attribute.data.value as *const u8;
                let on_off = *value_ptr != 0;

                info!("On/Off command: {}", if on_off { "ON (chime enabled)" } else { "OFF (chime silenced)" });

                if on_off {
                    // Enable chime: pulse RESET coil (NO opens)
                    esp_idf_sys::gpio_set_level(GPIO_RELAY_RESET as esp_idf_sys::gpio_num_t, 1);
                    thread::sleep(Duration::from_millis(RELAY_PULSE_MS));
                    esp_idf_sys::gpio_set_level(GPIO_RELAY_RESET as esp_idf_sys::gpio_num_t, 0);
                    info!("Relay RESET: chime enabled");
                } else {
                    // Silence chime: pulse SET coil (NO closes, shorts chime)
                    esp_idf_sys::gpio_set_level(GPIO_RELAY_SET as esp_idf_sys::gpio_num_t, 1);
                    thread::sleep(Duration::from_millis(RELAY_PULSE_MS));
                    esp_idf_sys::gpio_set_level(GPIO_RELAY_SET as esp_idf_sys::gpio_num_t, 0);
                    info!("Relay SET: chime silenced");
                }
            }
        }
    }

    esp_idf_sys::ESP_OK as esp_idf_sys::esp_err_t
}

/// Required signal handler for the Zigbee stack
#[no_mangle]
pub unsafe extern "C" fn esp_zb_app_signal_handler(signal_s: *mut esp_zb_app_signal_s) {
    let sig_type = *((*signal_s).p_app_signal);
    info!("Zigbee signal: type={}", sig_type);
}

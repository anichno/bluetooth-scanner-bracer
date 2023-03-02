//! Runtime tasks:
//! - BLE scan
//! - LED control
//! - BLE decay
//! - Input monitor
//! - TODO: interpolate device signal strength
//!
//! TODO: Remove floating point math

#![warn(clippy::disallowed_macros)]

use std::sync::Arc;

use esp_idf_sys as _;
use log::*;
use std::sync::Mutex;

mod ble_device_mgr;
mod light_mgr;
mod messages;
mod tasks;
mod utils;

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Debug);

    #[allow(clippy::needless_update)]
    {
        esp_idf_sys::esp!(unsafe {
            esp_idf_sys::esp_vfs_eventfd_register(&esp_idf_sys::esp_vfs_eventfd_config_t {
                max_fds: 5,
                ..Default::default()
            })
        })
        .unwrap();
    }

    let device_mgr = Arc::new(Mutex::new(ble_device_mgr::DeviceTracker::new()));

    let (light_controls_chan_tx, light_controls_chan_rx) = smol::channel::bounded(1);

    smol::block_on(async {
        info!("Starting BLE scanner task");
        let ble_scan_task = smol::spawn(tasks::ble_scanner(device_mgr.clone()));
        let led_animate_task = smol::spawn(tasks::led_animator(
            device_mgr.clone(),
            light_controls_chan_rx,
        ));
        let ble_decayer_task = smol::spawn(tasks::ble_device_decayer(device_mgr.clone()));
        let button_monitor_task = smol::spawn(tasks::button_monitor(light_controls_chan_tx));

        futures::future::select_all([
            ble_scan_task,
            led_animate_task,
            ble_decayer_task,
            button_monitor_task,
        ])
        .await;
        error!("One of the tasks has exited unexpectedly. Exiting...")
    });

    info!("END");
}

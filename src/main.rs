use esp32_nimble::BLEDevice;
use esp_idf_sys as _;
use log::*;

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

    let ble_device = BLEDevice::take();
    let ble_scan = ble_device.get_scan();

    smol::block_on(async {
        info!("Starting BLE scan");
        ble_scan
            .active_scan(true)
            .interval(100)
            .window(100)
            .on_result(|scan_result| {
                info!("Scan result: {:?}", scan_result);
            })
            .start(5000)
            .await
            .unwrap();
        info!("Scan end");
    });

    info!("END");
}

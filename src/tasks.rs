use std::sync::Arc;

use esp_idf_hal::{
    gpio::{PinDriver, Pull},
    prelude::Peripherals,
};
use log::*;
use smol::stream::StreamExt;
use std::sync::Mutex;

const DEBOUNCE_TIME_MS: u64 = 5;

#[cfg(not(feature = "simulator"))]
pub async fn ble_scanner(device_mgr: Arc<Mutex<crate::ble_device_mgr::DeviceTracker>>) {
    use esp32_nimble::BLEDevice;

    let ble_device = BLEDevice::take();

    // Set up advertising
    ble_device
        .get_advertising()
        .name(const_format::concatcp!(
            crate::ble_device_mgr::FAVORITE_DEVICE_ID,
            ':',
            env!("FAVORITE_COLOR")
        ))
        .scan_response(true)
        .start()
        .unwrap();

    // Set up scanning
    let ble_scan = ble_device.get_scan();
    let scanner = ble_scan
        .active_scan(true)
        .interval(40)
        .window(30)
        .filter_duplicates(true);

    loop {
        let device_mgr = device_mgr.clone();

        scanner
            .on_result(move |scan_result| {
                #[cfg(feature = "debug")]
                info!(
                    "Scan result: {{name: {}, addr: {}, rssi: {}}}",
                    scan_result.name(),
                    scan_result.addr(),
                    scan_result.rssi()
                );
                device_mgr.lock().unwrap().update(
                    *scan_result.addr(),
                    scan_result.name(),
                    scan_result.rssi(),
                );
            })
            .start(100)
            .await
            .unwrap();
        scanner.clear_results();
    }
}

#[cfg(feature = "simulator")]
pub async fn ble_scanner(device_mgr: Arc<Mutex<crate::ble_device_mgr::DeviceTracker>>) {
    use const_format::concatcp;

    let mut first_update = true;

    loop {
        let do_update = device_mgr.lock().unwrap().devices.is_empty();
        if do_update {
            if !first_update {
                smol::Timer::after(std::time::Duration::from_millis(5000)).await;
            }
            first_update = false;
            let mut device_mgr = device_mgr.lock().unwrap();
            // add fake devices
            for i in 0..10 {
                device_mgr.update(
                    esp32_nimble::BLEAddress::new_from_addr([i, 0x22, 0x33, 0x44, 0x55, 0x66]),
                    "",
                    -50 - i as i32 * 2,
                );
            }

            // add fake favorite device
            device_mgr.update(
                esp32_nimble::BLEAddress::new_from_addr([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
                concatcp!(crate::ble_device_mgr::FAVORITE_DEVICE_ID, ":0"),
                -50,
            );
        }

        smol::Timer::after(std::time::Duration::from_millis(100)).await;
    }
}

pub fn led_animator(
    device_mgr: Arc<Mutex<crate::ble_device_mgr::DeviceTracker>>,
    light_controls_chan: smol::channel::Receiver<crate::messages::LightControls>,
) {
    let mut light_manager =
        crate::light_mgr::LightMgr::new(device_mgr, crate::messages::DisplaySortMode::Ordered);
    let interval = light_manager.get_tick_interval();

    loop {
        let start = std::time::Instant::now();
        match light_controls_chan.try_recv() {
            Ok(msg) => {
                info!("Received control message: {:?}", msg);
                match msg {
                    crate::messages::LightControls::BrightnessIncrease => {
                        light_manager.increase_brightness()
                    }
                    crate::messages::LightControls::BrightnessDecrease => {
                        light_manager.decrease_brightness()
                    }
                    crate::messages::LightControls::ModeChange(new_mode) => {
                        light_manager.switch_mode(new_mode)
                    }
                }
            }
            Err(err) => match err {
                smol::channel::TryRecvError::Empty => {} // no message, do nothing
                smol::channel::TryRecvError::Closed => {
                    info!("Light controls channel closed, exiting");
                    return;
                }
            },
        }
        light_manager.tick();
        let wait_time = interval.saturating_sub(start.elapsed());
        std::thread::sleep(wait_time);
    }
}

pub async fn ble_device_decayer(device_mgr: Arc<Mutex<crate::ble_device_mgr::DeviceTracker>>) {
    let mut update_timer = smol::Timer::interval(std::time::Duration::from_secs(1));
    loop {
        info!("Decaying devices");
        device_mgr.lock().unwrap().decay_tick();
        update_timer.next().await;
    }
}

/// Monitor for button presses and the toggle switches
/// Using polling for now, but could be changed to interrupt based
pub async fn button_monitor(
    light_controls_chan: smol::channel::Sender<crate::messages::LightControls>,
) {
    async fn wait_stable_low<T: esp_idf_hal::gpio::Pin, MODE: esp_idf_hal::gpio::InputMode>(
        pin: PinDriver<'_, T, MODE>,
    ) -> PinDriver<'_, T, MODE> {
        smol::Timer::after(std::time::Duration::from_millis(DEBOUNCE_TIME_MS)).await;
        while pin.is_high() {
            smol::Timer::after(std::time::Duration::from_millis(10)).await;
        }
        let mut last_state = pin.is_high();
        let mut last_state_change =
            smol::Timer::after(std::time::Duration::from_millis(DEBOUNCE_TIME_MS)).await;
        loop {
            let current_state = pin.is_high();
            if current_state != last_state {
                last_state = current_state;
                last_state_change =
                    smol::Timer::after(std::time::Duration::from_millis(DEBOUNCE_TIME_MS)).await;
            }
            if last_state_change.elapsed() > std::time::Duration::from_millis(DEBOUNCE_TIME_MS) {
                return pin;
            }
        }
    }
    enum SwitchPosition {
        /// LOW
        Left,

        /// HIGH
        Right,
    }

    let peripherals = Peripherals::take().unwrap();
    let gpio_pins = peripherals.pins;

    let mut btn_brightness_increase = PinDriver::input(gpio_pins.gpio13).unwrap();
    btn_brightness_increase.set_pull(Pull::Down).unwrap();

    let mut btn_brightness_decrease = PinDriver::input(gpio_pins.gpio12).unwrap();
    btn_brightness_decrease.set_pull(Pull::Down).unwrap();

    // let mut switch_display_mode = PinDriver::input(gpio_pins.gpio36).unwrap();
    let mut switch_display_mode = PinDriver::input(gpio_pins.gpio15).unwrap();
    switch_display_mode.set_pull(Pull::Down).unwrap();

    // get switch initial state
    let mut switch_display_last_position = if switch_display_mode.is_high() {
        SwitchPosition::Right
    } else {
        SwitchPosition::Left
    };

    loop {
        // Read one button per loop
        if btn_brightness_increase.is_high() {
            light_controls_chan
                .send(crate::messages::LightControls::BrightnessIncrease)
                .await
                .unwrap();
            btn_brightness_increase = wait_stable_low(btn_brightness_increase).await;
        } else if btn_brightness_decrease.is_high() {
            light_controls_chan
                .send(crate::messages::LightControls::BrightnessDecrease)
                .await
                .unwrap();
            btn_brightness_decrease = wait_stable_low(btn_brightness_decrease).await;
        } else if switch_display_mode.is_high()
            && matches!(switch_display_last_position, SwitchPosition::Left)
        {
            switch_display_last_position = SwitchPosition::Right;
            light_controls_chan
                .send(crate::messages::LightControls::ModeChange(
                    crate::messages::DisplaySortMode::Ordered,
                ))
                .await
                .unwrap();
        } else if switch_display_mode.is_low()
            && matches!(switch_display_last_position, SwitchPosition::Right)
        {
            switch_display_last_position = SwitchPosition::Left;
            light_controls_chan
                .send(crate::messages::LightControls::ModeChange(
                    crate::messages::DisplaySortMode::Sticky,
                ))
                .await
                .unwrap();
        }

        smol::Timer::after(std::time::Duration::from_millis(20)).await;
    }
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use const_format::assertcp;
use esp32_nimble::BLEAddress;
use log::info;
use palette::{FromColor, Hsv, Mix, RgbHue, Srgb};
use rand::Rng;

use crate::{
    ble_device_mgr::{DeviceTracker, SIGNAL_IGNORE_ABOVE_THRESHOLD, SIGNAL_IGNORE_BELOW_THRESHOLD},
    messages::DisplaySortMode,
    utils,
};

const NUM_LIGHTS: usize = 60;
pub const MAX_DEVICES_SHOWN: usize = 10;

// /// How many seconds to fade in a new device, keeps a bright light from popping in
// const FADE_IN_DURATION: u32 = 1;

/// How many lights to reserve for the favorite device at the top
const FAVORITE_RESERVE_LIGHTS: usize = 10;

/// How many lights to show per device, must be odd for looks
const SLOT_WIDTH: usize = (NUM_LIGHTS - FAVORITE_RESERVE_LIGHTS) / MAX_DEVICES_SHOWN;
assertcp!(SLOT_WIDTH % 2 == 1);

/// As you move away from the center light, what is the brightness of each subsequent light
const FALL_OFF_RATE: f32 = 0.5;

const BRIGHTNESS_LEVELS: f32 = 10.0;

const TRANSITION_SECONDS: u64 = 3;
#[cfg(esp32)]
const STEPS_PER_SECOND: u64 = 30; // slow for simulation

#[cfg(esp32s3)]
const STEPS_PER_SECOND: u64 = 120;

const NUM_TRANSITIONAL_STEPS: u64 = TRANSITION_SECONDS * STEPS_PER_SECOND;

#[derive(Debug, Copy, Clone, Default)]
struct AvgPixel {
    // colors: tinyvec::TinyVec<[Hsv; MAX_DEVICES_SHOWN * 2]>,
    color: Option<Hsv>,
    // num_samples: usize,
}

impl AvgPixel {
    fn add(&mut self, pixel: Hsv) {
        // self.color += pixel; // TODO: check that this is correct
        // self.colors.push(pixel);
        // self.num_samples += 1;
        if let Some(color) = self.color {
            self.color = Some(color.mix(&pixel, 0.5));
        } else {
            self.color = Some(pixel);
        }
    }

    fn avg(&self) -> Hsv {
        // // TODO this probably needs improvement
        // if self.colors.len() == 0 {
        //     Hsv::default()
        // } else {
        //     self.colors
        //         .iter()
        //         .fold(Hsv::default(), |sum, color| sum.mix(color, 0.5))
        // }
        self.color.unwrap_or_default()
    }
}

struct DeviceLightState {
    rssi: i32,
    color: RgbHue,
    current_rank_slot: usize,
    target_rank_slot: usize,
    steps_remaining: u64,
}

impl DeviceLightState {
    fn get_target_pixel(&self) -> f32 {
        let slot_progress = (self.steps_remaining as f32 / NUM_TRANSITIONAL_STEPS as f32)
            * (self.current_rank_slot as f32 - self.target_rank_slot as f32)
            + self.target_rank_slot as f32;
        utils::num_linear_conversion(
            slot_progress,
            0.0,
            MAX_DEVICES_SHOWN as f32,
            FAVORITE_RESERVE_LIGHTS as f32 + SLOT_WIDTH as f32 / 2.0,
            NUM_LIGHTS as f32 - SLOT_WIDTH as f32 / 2.0,
        )
    }

    /// Update pixel positions based on current position, target position, and steps remaining.
    /// Write pixel data to light strip
    /// TODO: might make sense to memoize this if steps_remaining is 0
    fn tick(&mut self, brightness: f32, light_strip: &mut [AvgPixel; NUM_LIGHTS]) {
        let target_pixel = self.get_target_pixel();
        // determine the brightness of each pixel in slot, mapped back to physical lights

        // center
        let left_pixel = target_pixel.floor() as usize;
        let right_pixel = target_pixel.ceil() as usize;

        let left_brightness = (1.0 - (target_pixel - target_pixel.floor())) * brightness;
        let right_brightness = (target_pixel.ceil() - target_pixel) * brightness;

        light_strip[left_pixel].add(Hsv::new(self.color, 1.0, left_brightness));
        light_strip[right_pixel].add(Hsv::new(self.color, 1.0, right_brightness));

        // sides
        let mut cur_falloff = FALL_OFF_RATE;
        let mut left_virt_pixel = target_pixel - 1.0;
        let mut right_virt_pixel = target_pixel + 1.0;
        for _ in 0..SLOT_WIDTH / 2 {
            // left side
            let left_pixel = left_virt_pixel.floor() as usize;
            let right_pixel = left_virt_pixel.ceil() as usize;
            let left_brightness =
                (1.0 - (left_virt_pixel - left_virt_pixel.floor())) * brightness * cur_falloff;
            let right_brightness =
                (left_virt_pixel.ceil() - left_virt_pixel) * brightness * cur_falloff;

            light_strip[left_pixel].add(Hsv::new(self.color, 1.0, left_brightness));

            light_strip[right_pixel].add(Hsv::new(self.color, 1.0, right_brightness));

            // right side
            let left_pixel = right_virt_pixel.floor() as usize;
            let right_pixel = right_virt_pixel.ceil() as usize;
            let left_brightness =
                (1.0 - (right_virt_pixel - right_virt_pixel.floor())) * brightness * cur_falloff;
            let right_brightness =
                (right_virt_pixel.ceil() - right_virt_pixel) * brightness * cur_falloff;

            light_strip[left_pixel].add(Hsv::new(self.color, 1.0, left_brightness));

            if right_pixel < NUM_LIGHTS {
                light_strip[right_pixel].add(Hsv::new(self.color, 1.0, right_brightness));
            }

            // inc vals
            left_virt_pixel -= 1.0;
            right_virt_pixel += 1.0;
            cur_falloff *= FALL_OFF_RATE;
        }

        // update state
        self.steps_remaining -= 1;
        if self.steps_remaining == 0 {
            self.current_rank_slot = self.target_rank_slot;
        }
    }
}

struct FavoriteLightState {
    rssi: i32,
    color: RgbHue,
}

impl FavoriteLightState {
    fn tick(&self, brightness: f32, light_strip: &mut [AvgPixel; NUM_LIGHTS]) {
        let signal_strength = utils::num_linear_conversion(
            self.rssi as f32,
            SIGNAL_IGNORE_BELOW_THRESHOLD as f32,
            SIGNAL_IGNORE_ABOVE_THRESHOLD as f32,
            0.0,
            1.0,
        );
        let middle_idx = (FAVORITE_RESERVE_LIGHTS - 1) as f32 / 2.0;

        for (i, pixel) in light_strip
            .iter_mut()
            .enumerate()
            .take(FAVORITE_RESERVE_LIGHTS)
        {
            let signal_bias = 1.0 - (i as f32 - middle_idx).abs() / middle_idx;

            pixel.add(Hsv::new(
                self.color,
                1.0,
                brightness * signal_bias * signal_strength,
            ));
        }
    }
}

pub struct LightMgr {
    device_manager: Arc<Mutex<DeviceTracker>>,
    led_strip: crate::led_strip::LedStrip<NUM_LIGHTS>,
    mode: DisplaySortMode,
    brightness: f32, // maybe custom type to enforce 0.0-1.0

    displayed_devices: HashMap<BLEAddress, DeviceLightState>,
    favorite_device: Option<FavoriteLightState>,
}

impl LightMgr {
    pub fn new(device_manager: Arc<Mutex<DeviceTracker>>, initial_mode: DisplaySortMode) -> Self {
        let led_strip = crate::led_strip::LedStrip::<NUM_LIGHTS>::new(
            esp_idf_sys::rmt_channel_t_RMT_CHANNEL_0,
            esp_idf_sys::gpio_num_t_GPIO_NUM_14,
        )
        .unwrap();

        // TODO load last brightness from flash
        Self {
            device_manager,
            led_strip,
            mode: initial_mode,
            brightness: 0.5,
            displayed_devices: HashMap::new(),
            favorite_device: None,
        }
    }

    pub fn get_tick_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(1000 / STEPS_PER_SECOND)
    }

    /// Animate the lights
    pub fn tick(&mut self) {
        // determine which devices to show
        let mut device_rankings = tinyvec::tiny_vec!([(i32, BLEAddress); MAX_DEVICES_SHOWN * 2]);

        {
            let mut found_favorite = false;
            let device_manager = self.device_manager.lock().unwrap();
            for device in device_manager.devices.iter() {
                if device.is_favorite {
                    found_favorite = true;
                    if let Some(fav_device) = &mut self.favorite_device {
                        fav_device.rssi = device.signal_strength.get_avg();
                    } else {
                        self.favorite_device = Some(FavoriteLightState {
                            rssi: device.signal_strength.get_avg(),
                            color: device.favorite_color.unwrap_or_default(),
                        });
                    }
                } else {
                    device_rankings.push((device.signal_strength.get_avg(), device.address));
                }
            }

            if !found_favorite {
                self.favorite_device = None;
            }
        }

        device_rankings.sort_by_key(|(rssi, _)| *rssi);

        // for each device that is no longer tracked by the device manager, mark its target position as off the strip
        for (dev_addr, device) in self.displayed_devices.iter_mut() {
            if !device_rankings.iter().any(|(_, addr)| addr == dev_addr)
                && device.target_rank_slot != MAX_DEVICES_SHOWN + 1
            {
                device.target_rank_slot = MAX_DEVICES_SHOWN + 1;
                device.steps_remaining = NUM_TRANSITIONAL_STEPS;
            }
        }

        // if any new devices, create a new light state for them, otherwise update
        for (i, (rssi, address)) in device_rankings.into_iter().rev().enumerate() {
            if let Some(device) = self.displayed_devices.get_mut(&address) {
                device.rssi = rssi;
                device.target_rank_slot = i;

                if device.steps_remaining == 0
                    && device.current_rank_slot != device.target_rank_slot
                {
                    info!("resetting steps_remaining");
                    device.steps_remaining = NUM_TRANSITIONAL_STEPS;
                }
            } else if i < MAX_DEVICES_SHOWN {
                info!("new device: addr: {}, signal: {}", address, rssi);
                // only insert new devices if there is room
                let rand_color = RgbHue::from_degrees(rand::thread_rng().gen_range(0..360) as f32);

                let new_device = DeviceLightState {
                    rssi,
                    color: rand_color,
                    current_rank_slot: MAX_DEVICES_SHOWN + 1,
                    target_rank_slot: i,
                    steps_remaining: NUM_TRANSITIONAL_STEPS,
                };

                self.displayed_devices.insert(address, new_device);
            }
        }

        // create new light strip update
        let mut next_light_update = [AvgPixel::default(); NUM_LIGHTS];

        // update device positions
        for (_, device) in self.displayed_devices.iter_mut() {
            device.tick(self.brightness, &mut next_light_update);
        }

        // update favorite device signal strength indicator
        if let Some(fav) = &self.favorite_device {
            fav.tick(self.brightness, &mut next_light_update);
        }

        // write light strip update
        // TODO: gamma correct
        for (i, avg_pixel) in next_light_update.iter().enumerate() {
            let rgb = Srgb::from_color(avg_pixel.avg());
            let r = utils::num_linear_conversion(rgb.red, 0.0, 1.0, 0.0, 255.0) as u8;
            let g = utils::num_linear_conversion(rgb.green, 0.0, 1.0, 0.0, 255.0) as u8;
            let b = utils::num_linear_conversion(rgb.blue, 0.0, 1.0, 0.0, 255.0) as u8;
            self.led_strip.colors[i] = crate::led_strip::Color::new(r, g, b);
        }
        self.led_strip.update().unwrap();

        // remove devices that are off the strip
        self.displayed_devices.retain(|_, device| {
            device.current_rank_slot <= MAX_DEVICES_SHOWN
                || device.target_rank_slot <= MAX_DEVICES_SHOWN
        });
    }

    pub fn increase_brightness(&mut self) {
        self.brightness = (self.brightness + (1.0 / BRIGHTNESS_LEVELS)).min(1.0);
    }

    pub fn decrease_brightness(&mut self) {
        self.brightness = (self.brightness - (1.0 / BRIGHTNESS_LEVELS)).max(0.0);
    }

    pub fn switch_mode(&mut self, new_mode: DisplaySortMode) {
        self.mode = new_mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_light_state_tick() {
        esp_idf_svc::log::EspLogger::initialize_default();

        let mut device = DeviceLightState {
            rssi: 0,
            color: super::RgbHue::from_degrees(0.0),
            current_rank_slot: 10,
            target_rank_slot: 0,
            steps_remaining: NUM_TRANSITIONAL_STEPS / 3,
        };

        info!("{:?}", device.get_target_pixel());
        assert!(device.get_target_pixel() - 27.5 < 0.1);

        device.current_rank_slot = 0;
        device.target_rank_slot = 10;

        info!("{:?}", device.get_target_pixel());
        assert!(device.get_target_pixel() - 42.5 < 0.1);
    }
}

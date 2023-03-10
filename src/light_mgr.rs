use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use const_format::assertcp;
use esp32_nimble::BLEAddress;
use log::info;
use palette::{Blend, FromColor, Hsv, RgbHue, Srgb};
use rand::seq::SliceRandom;

use crate::{ble_device_mgr::DeviceTracker, messages::DisplaySortMode, utils};

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

const BRIGHTNESS_LEVELS: u8 = 10;
const DEFAULT_BRIGHTNESS: u8 = 3;

const TRANSITION_SECONDS: u64 = 3;
#[cfg(esp32)]
const STEPS_PER_SECOND: u64 = 30; // slow for simulation

#[cfg(esp32s3)]
const STEPS_PER_SECOND: u64 = 120;

const NUM_TRANSITIONAL_STEPS: u64 = TRANSITION_SECONDS * STEPS_PER_SECOND;

#[derive(Debug, Copy, Clone, Default)]
struct AvgPixel {
    color: Option<Hsv>,
}

impl AvgPixel {
    fn add(&mut self, pixel: Hsv) {
        if let Some(color) = self.color {
            // apply the screen blend mode to the new pixel
            let srgb1 = Srgb::from_color(color).into_linear();
            let srgb2 = Srgb::from_color(pixel).into_linear();
            let blended = Srgb::from_linear(srgb1.screen(srgb2));
            self.color = Some(Hsv::from_color(blended));
        } else {
            self.color = Some(pixel);
        }
    }

    fn avg(&self) -> Hsv {
        self.color.unwrap_or_default()
    }
}

struct ColorAllocator {
    colors: [RgbHue; MAX_DEVICES_SHOWN * 2],
    colors_in_use: [u8; MAX_DEVICES_SHOWN * 2],
}

impl ColorAllocator {
    fn new() -> Self {
        let mut colors = [RgbHue::default(); MAX_DEVICES_SHOWN * 2];
        for (i, color) in colors.iter_mut().enumerate() {
            *color = RgbHue::from_degrees(360.0 / (MAX_DEVICES_SHOWN * 2) as f32 * i as f32);
        }
        Self {
            colors,
            colors_in_use: [0; MAX_DEVICES_SHOWN * 2],
        }
    }

    fn allocate_color(&mut self) -> RgbHue {
        let mut available_colors = tinyvec::tiny_vec!([usize; MAX_DEVICES_SHOWN * 2]);

        let mut in_use = 0;
        loop {
            for (i, color_in_use) in self.colors_in_use.iter().enumerate() {
                if *color_in_use == in_use {
                    available_colors.push(i);
                }
            }

            if !available_colors.is_empty() {
                break;
            } else {
                in_use += 1;
            }
        }

        let color_idx = *available_colors.choose(&mut rand::thread_rng()).unwrap();
        self.colors_in_use[color_idx] += 1;
        self.colors[color_idx]
    }

    fn release_color(&mut self, color: RgbHue) {
        let color_idx = self
            .colors
            .iter()
            .position(|c| *c == color)
            .expect("color not found");
        self.colors_in_use[color_idx] = self.colors_in_use[color_idx].saturating_sub(1);
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
            MAX_DEVICES_SHOWN as f32 - 1.0,
            (FAVORITE_RESERVE_LIGHTS as f32 + SLOT_WIDTH as f32 / 2.0).floor(),
            NUM_LIGHTS as f32 - (SLOT_WIDTH as f32 / 2.0).ceil(),
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
        self.steps_remaining = self.steps_remaining.saturating_sub(1);
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
            -70.0, // less tolerant, so that as the other device hits the noise floor, we don't flicker in and out
            -55.0, // A bit more tolerant since our transmit power is low
            0.0,
            1.0,
        );
        let middle_idx = FAVORITE_RESERVE_LIGHTS / 2;
        let signal_width =
            ((FAVORITE_RESERVE_LIGHTS - 1) as f32 * signal_strength).round() as usize;
        if signal_width > 0 {
            light_strip[middle_idx].add(Hsv::new(self.color, 1.0, brightness));

            let mut last_low_idx = middle_idx;
            let mut last_high_idx = middle_idx;
            let mut last_low = false;
            for _ in 1..=signal_width {
                if !last_low {
                    last_low_idx = last_low_idx.saturating_sub(1);
                    light_strip[last_low_idx].add(Hsv::new(self.color, 1.0, brightness));
                    last_low = true;
                } else {
                    last_high_idx = last_high_idx.saturating_add(1);
                    light_strip[last_high_idx].add(Hsv::new(self.color, 1.0, brightness));
                    last_low = false;
                }
            }
        }
    }
}

pub struct LightMgr {
    device_manager: Arc<Mutex<DeviceTracker>>,
    led_strip: crate::led_strip::LedStrip<NUM_LIGHTS>,
    mode: DisplaySortMode,
    brightness: f32,
    brightness_level: u8,
    color_allocator: ColorAllocator,

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

        Self {
            device_manager,
            led_strip,
            mode: initial_mode,
            brightness: Self::get_brightness(DEFAULT_BRIGHTNESS),
            brightness_level: DEFAULT_BRIGHTNESS,
            color_allocator: ColorAllocator::new(),
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
                let new_device = DeviceLightState {
                    rssi,
                    color: self.color_allocator.allocate_color(),
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
            if device.current_rank_slot > MAX_DEVICES_SHOWN
                && device.target_rank_slot > MAX_DEVICES_SHOWN
            {
                self.color_allocator.release_color(device.color);
                false
            } else {
                true
            }
        });
    }

    fn get_brightness(level: u8) -> f32 {
        0.714_f32.powf((BRIGHTNESS_LEVELS - level) as f32)
    }

    pub fn increase_brightness(&mut self) {
        self.brightness_level = (self.brightness_level + 1).min(BRIGHTNESS_LEVELS);
        info!("Brightness increased to level: {}", self.brightness_level);
        self.brightness = Self::get_brightness(self.brightness_level);
    }

    pub fn decrease_brightness(&mut self) {
        self.brightness_level = (self.brightness_level - 1).max(1);
        info!("Brightness decreased to level: {}", self.brightness_level);
        self.brightness = Self::get_brightness(self.brightness_level);
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

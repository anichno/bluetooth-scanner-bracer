use esp32_nimble::BLEAddress;
use log::info;
use palette::RgbHue;

/// How many seconds until device signal strength begins to decay
const DECAY_DELAY: u64 = 5;

/// Once device begins to decay, how much signal strength to lose per second (from last seen value)
const DECAY_RATE: f32 = 0.1;

/// Cutoff signal strength for devices likely to be physically on us
pub const SIGNAL_IGNORE_ABOVE_THRESHOLD: i32 = -45;

/// Cutoff signal strength for devices too far to be relevant
pub const SIGNAL_IGNORE_BELOW_THRESHOLD: i32 = -80;

const SIGNAL_ALLOW_RANGE: std::ops::RangeInclusive<i32> =
    SIGNAL_IGNORE_BELOW_THRESHOLD..=SIGNAL_IGNORE_ABOVE_THRESHOLD;

const SIGNAL_MOVING_AVG_WINDOW: usize = 5;

/// Pairing ID for favorite device
pub const FAVORITE_DEVICE_ID: &str = env!("FAVORITE_DEVICE_ID");

// type BLEAddressStr = String;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Device {
    pub address: BLEAddress,
    pub is_favorite: bool,
    pub favorite_color: Option<RgbHue>,
    pub signal_strength: crate::utils::MovingAvg<SIGNAL_MOVING_AVG_WINDOW>,
    last_seen: std::time::Instant,

    /// Once decay triggers, how much to lose per second
    last_decay_time: Option<std::time::Instant>,
    decay_rate: i32,
    decaying: bool,
}

pub struct DeviceTracker {
    pub devices: Vec<Device>,
}

impl DeviceTracker {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    pub fn update(&mut self, addr: BLEAddress, name: &str, signal_strength: i32) {
        let now = std::time::Instant::now();

        // check if device already exists
        if let Some(device) = self.devices.iter_mut().find(|d| d.address == addr) {
            device.last_seen = now;
            device.signal_strength.push(signal_strength);
            device.decaying = false;
            device.decay_rate = (signal_strength as f32 * DECAY_RATE) as i32;
        } else {
            // add new device if in range or favorite
            let is_favorite = name.contains(FAVORITE_DEVICE_ID);

            if !is_favorite && !SIGNAL_ALLOW_RANGE.contains(&signal_strength) {
                return;
            }

            // lookup color for device
            let favorite_color = if is_favorite {
                // parse color from name (e.g. "FAVORITE_DEVICE_ID:255")
                let color = name
                    .split(':')
                    .nth(1)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    .into();
                Some(color)
            } else {
                None
            };

            let mut signal_strengths = crate::utils::MovingAvg::new();
            signal_strengths.push(signal_strength);

            self.devices.push(Device {
                address: addr,
                is_favorite,
                favorite_color,
                signal_strength: signal_strengths,
                last_seen: now,
                decay_rate: (signal_strength as f32 * DECAY_RATE) as i32,
                decaying: false,
                last_decay_time: None,
            });
        }
    }

    pub fn decay_tick(&mut self) {
        // update decaying devices and remove devices that are too far away
        let now = std::time::Instant::now();
        info!("Devices: {:?}", self.devices);

        self.devices.retain_mut(|device| {
            // check how long since last seen and start decaying if necessary
            if now.duration_since(device.last_seen).as_secs() > DECAY_DELAY {
                device.decaying = true;
            }

            if device.decaying {
                device.last_decay_time = Some(now);
                device
                    .signal_strength
                    .push(device.signal_strength.peek_last() + device.decay_rate);
            }

            // retain devices above SIGNAL_IGNORE_BELOW_THRESHOLD
            device.signal_strength.get_avg() > SIGNAL_IGNORE_BELOW_THRESHOLD
        });
    }

    // /// TODO: Devices only advertise periodically, but we want to show them moving smoothly
    // pub fn interpolate_tick(&mut self) {
    //     todo!()
    // }
}

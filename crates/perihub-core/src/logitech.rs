//! A Logitech HID++ 2.0 device behind a receiver that PeriHub knows only by the features it reports
//! (see [`UnsupportedDevice`]), on its own thread: its battery and DPI as they are now, and the DPI
//! the user chose.
//!
//! The DPI goes to the device's RAM only (0x2201 function 3): nothing is written to its memory. Such
//! devices forget it when they are turned off or fall asleep, so PeriHub sets it again every time
//! the device connects (the thread ends when the device stops answering, and the manager opens it
//! again when it is back). Changing it is an experimental feature: it has not been tried on such a
//! device yet.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hidpp::receiver::UnsupportedDevice;
use crate::hidpp::{Access, Battery, DpiCapabilities, HidppDevice, feature};
use crate::worker::DeviceTask;

/// How often the thread checks that the device still answers.
const CHECK_EVERY: Duration = Duration::from_secs(10);

/// What changes while the device is in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogitechLive {
    pub battery: Option<Battery>,
    /// The DPI it uses now, if it has 0x2201.
    pub dpi: Option<u16>,
    /// The DPI PeriHub sets every time the device connects, if the user chose one.
    pub saved_dpi: Option<u16>,
}

pub struct LogitechTask {
    dev: HidppDevice,
    pub device: UnsupportedDevice,
    saved_dpi: Option<u16>,
    last_check: Instant,
}

/// Whether `dpi` is one the device accepts, from what it reports.
fn check_dpi(caps: &DpiCapabilities, dpi: u16) -> Result<()> {
    let accepted = if caps.list.is_empty() {
        (caps.min..=caps.max).contains(&dpi) && (caps.step == 0 || (dpi - caps.min).is_multiple_of(caps.step))
    } else {
        caps.list.contains(&dpi)
    };
    if accepted { Ok(()) } else { Err(Error::Invalid(format!("this device does not accept {dpi} DPI"))) }
}

impl LogitechTask {
    /// `dev` comes read-only; it gets RAM changes only if the device has 0x2201. `saved_dpi` is set
    /// at once: the device has just connected or woken up, and has forgotten it.
    pub fn open(mut dev: HidppDevice, device: UnsupportedDevice, saved_dpi: Option<u16>) -> Result<Self> {
        if device.dpi.is_some() {
            dev.grant(Access::Ram);
        }
        let mut task = Self { dev, device, saved_dpi, last_check: Instant::now() };
        if let Some(dpi) = saved_dpi {
            match task.apply_dpi(dpi) {
                Ok(()) => log::info!("{}: DPI set to {dpi} again", task.device.label()),
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                Err(e) => log::warn!("{}: setting the DPI again failed: {e}", task.device.label()),
            }
        }
        Ok(task)
    }

    pub fn live(&mut self) -> Result<LogitechLive> {
        let battery = match self.dev.battery() {
            Ok(battery) => battery,
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
            Err(e) => {
                log::debug!("{} battery: {e}", self.device.label());
                None
            }
        };
        let dpi = if self.device.dpi.is_some() { Some(self.dev.sensor_dpi(0)?.0) } else { None };
        Ok(LogitechLive { battery, dpi, saved_dpi: self.saved_dpi })
    }

    fn apply_dpi(&mut self, dpi: u16) -> Result<()> {
        let caps = self.device.dpi.as_ref().ok_or(Error::FeatureMissing(feature::ADJUSTABLE_DPI))?;
        check_dpi(caps, dpi)?;
        self.dev.set_sensor_dpi(0, dpi)
    }

    /// Sets the DPI and keeps it, to set it again every time the device connects. Returns the key
    /// to save it under (see [`UnsupportedDevice::settings_key`]).
    pub fn set_dpi(&mut self, dpi: u16) -> Result<String> {
        self.apply_dpi(dpi)?;
        self.saved_dpi = Some(dpi);
        log::info!("{}: DPI set to {dpi}", self.device.label());
        Ok(self.device.settings_key())
    }

    /// Goes back to the device's own default DPI and stops setting one. Returns the settings key.
    pub fn reset_dpi(&mut self) -> Result<String> {
        let (_, default) = self.dev.sensor_dpi(0)?;
        self.apply_dpi(default)?;
        self.saved_dpi = None;
        log::info!("{}: DPI back to its default, {default}", self.device.label());
        Ok(self.device.settings_key())
    }
}

impl DeviceTask for LogitechTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.last_check.elapsed() >= CHECK_EVERY {
            self.last_check = Instant::now();
            // A device that fell asleep or was turned off ends the thread: it has forgotten its
            // DPI, which is set again when it is opened next.
            match self.dev.protocol_version() {
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                Err(e) => log::debug!("{}: {e}", self.device.label()),
                Ok(_) => {}
            }
        }
        Ok(CHECK_EVERY.saturating_sub(self.last_check.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::hidpp::tests::{FakeDevice, receiver_error, reply};

    #[test]
    fn only_dpi_values_the_device_reports_are_accepted() {
        let range = DpiCapabilities { min: 200, max: 8000, step: 50, list: Vec::new() };
        assert!(check_dpi(&range, 200).is_ok() && check_dpi(&range, 1650).is_ok() && check_dpi(&range, 8000).is_ok());
        assert!(
            check_dpi(&range, 100).is_err() && check_dpi(&range, 8050).is_err() && check_dpi(&range, 1625).is_err()
        );
        let list = DpiCapabilities { min: 400, max: 1600, step: 0, list: vec![400, 800, 1600] };
        assert!(check_dpi(&list, 800).is_ok() && check_dpi(&list, 1000).is_err());
    }

    /// A mouse in slot 1 with battery (0x1004) and DPI (0x2201): 200 to 8000 in steps of 50, 1000 now,
    /// 1000 by default. Records every DPI it is set to; `asleep` makes it stop answering.
    struct Mouse {
        dpi: u16,
        set: Vec<u16>,
        asleep: bool,
    }

    const FEATURES: &[u16] = &[feature::ROOT, feature::UNIFIED_BATTERY, feature::ADJUSTABLE_DPI];

    fn open(saved: Option<u16>, dpi_feature: bool) -> (Result<LogitechTask>, Arc<Mutex<Mouse>>) {
        let mouse = Arc::new(Mutex::new(Mouse { dpi: 1000, set: Vec::new(), asleep: false }));
        let state = mouse.clone();
        let fake = FakeDevice::new(move |request| {
            let mut m = state.lock().unwrap();
            if m.asleep {
                return vec![receiver_error(request, 0x09)];
            }
            let (index, function) = (request[2] as usize, request[3] >> 4);
            let payload = match (FEATURES.get(index).copied(), function) {
                (Some(feature::ROOT), 1) => vec![4, 5, request[6]],
                (Some(feature::ROOT), 0) => {
                    let id = u16::from_be_bytes([request[4], request[5]]);
                    vec![FEATURES.iter().position(|&f| f == id).unwrap_or(0) as u8]
                }
                (Some(feature::UNIFIED_BATTERY), 1) => vec![80, 0x04, 0, 0],
                (Some(feature::ADJUSTABLE_DPI), 2) => {
                    let [h, l] = m.dpi.to_be_bytes();
                    vec![0, h, l, 0x03, 0xE8]
                }
                (Some(feature::ADJUSTABLE_DPI), 3) => {
                    m.dpi = u16::from_be_bytes([request[5], request[6]]);
                    let dpi = m.dpi;
                    m.set.push(dpi);
                    vec![0]
                }
                _ => vec![],
            };
            vec![reply(request, &payload)]
        });
        let dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        let device = UnsupportedDevice {
            slot: 1,
            name: Some("MX Master 3S".into()),
            kind: Some(3),
            protocol: "4.5".into(),
            wireless_id: None,
            unit_id: Some([0x12, 0x34, 0x56, 0x78]),
            device_info: None,
            firmware: Vec::new(),
            features: Vec::new(),
            dpi: dpi_feature.then(|| DpiCapabilities { min: 200, max: 8000, step: 50, list: Vec::new() }),
            report_rates_ms: Vec::new(),
            onboard: None,
            read_errors: Vec::new(),
        };
        (LogitechTask::open(dev, device, saved), mouse)
    }

    #[test]
    fn the_saved_dpi_is_set_again_when_the_device_connects() {
        let (task, mouse) = open(Some(1600), true);
        let mut task = task.unwrap();
        assert_eq!(mouse.lock().unwrap().set, [1600]);
        let live = task.live().unwrap();
        assert_eq!((live.dpi, live.saved_dpi), (Some(1600), Some(1600)));
        assert_eq!(live.battery.unwrap().percent, 80);
    }

    #[test]
    fn a_dpi_change_is_kept_and_a_reset_goes_back_to_the_default() {
        let (task, mouse) = open(None, true);
        let mut task = task.unwrap();
        assert!(mouse.lock().unwrap().set.is_empty(), "nothing to set again");
        assert_eq!(task.set_dpi(2400).unwrap(), "unit-12345678");
        assert!(task.set_dpi(2410).is_err(), "not a multiple of the step");
        assert_eq!(task.live().unwrap().saved_dpi, Some(2400));
        task.reset_dpi().unwrap();
        assert_eq!(mouse.lock().unwrap().set, [2400, 1000]);
        assert_eq!(task.live().unwrap().saved_dpi, None);
    }

    #[test]
    fn a_device_without_adjustable_dpi_is_only_read() {
        let (task, mouse) = open(Some(1600), false);
        let mut task = task.unwrap();
        assert!(task.set_dpi(1600).is_err());
        assert!(task.dev.set_sensor_dpi(0, 1600).is_err(), "read-only at the HID++ layer");
        assert!(mouse.lock().unwrap().set.is_empty());
        assert_eq!(task.live().unwrap().dpi, None);
    }

    #[test]
    fn a_device_that_falls_asleep_ends_its_thread() {
        let (task, mouse) = open(None, true);
        let mut task = task.unwrap();
        task.last_check = Instant::now() - CHECK_EVERY;
        mouse.lock().unwrap().asleep = true;
        assert!(matches!(task.tick(), Err(Error::Unreachable)));
    }
}

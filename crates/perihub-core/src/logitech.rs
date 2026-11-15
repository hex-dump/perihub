//! A Logitech HID++ 2.0 device behind a receiver that PeriHub knows only by the features it reports
//! (see [`UnsupportedDevice`]), on its own thread: its battery and DPI as they are now, the DPI the
//! user chose, and the lights of a keyboard PeriHub drives in host mode (see [`crate::hidpp::host`]).
//!
//! Host mode lasts for the session: the manager keeps the look and shows it again every time the
//! keyboard connects. PeriHub gives the lights back when the user asks, when G HUB or Solaar starts,
//! when a command fails, when the thread stops and when PeriHub quits.
//!
//! The DPI goes to the device's RAM only (0x2201 function 3): nothing is written to its memory. Such
//! devices forget it when they are turned off or fall asleep, so PeriHub sets it again every time
//! the device connects (the thread ends when the device stops answering, and the manager opens it
//! again when it is back). Changing it is an experimental feature: it has not been tried on such a
//! device yet.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hidpp::host::{self, Host, HostStatus, Look};
use crate::hidpp::lighting::LightingNow;
use crate::hidpp::receiver::UnsupportedDevice;
use crate::hidpp::{Access, Battery, DpiCapabilities, HidppDevice, feature};
use crate::worker::DeviceTask;

/// How often the thread checks that the device still answers.
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// While PeriHub drives the lights: how often the keyboard's notices are read, and how often PeriHub
/// checks that it still has the lights and that no other Logitech program started.
const EVENTS_EVERY: Duration = Duration::from_millis(200);
const HOST_CHECK_EVERY: Duration = Duration::from_secs(5);

/// What changes while the device is in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogitechLive {
    pub battery: Option<Battery>,
    /// The DPI it uses now, if it has 0x2201.
    pub dpi: Option<u16>,
    /// The DPI PeriHub sets every time the device connects, if the user chose one.
    pub saved_dpi: Option<u16>,
    /// Its lighting as it is now, if it has a lighting feature PeriHub reads.
    pub lighting: Option<LightingNow>,
    /// While PeriHub drives the lights: the look and how host mode went.
    pub host: Option<HostLive>,
    /// Why PeriHub gave the lights back on its own, if it did.
    pub gave_back: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostLive {
    pub look: Look,
    #[serde(flatten)]
    pub status: HostStatus,
}

pub struct LogitechTask {
    dev: HidppDevice,
    pub device: UnsupportedDevice,
    saved_dpi: Option<u16>,
    last_check: Instant,
    /// Set while PeriHub drives the keyboard's lights.
    host: Option<Host>,
    last_host_check: Instant,
    gave_back: Option<String>,
    /// Finds the Logitech programs running now (see [`other_programs`]); tests replace it.
    programs: fn() -> Vec<&'static str>,
}

/// PeriHub can drive the device's lights in host mode (see [`host::can_drive`]).
/// Not on a device whose DPI PeriHub changes: that one gets RAM access, which has no host mode.
pub fn lights_drivable(device: &UnsupportedDevice) -> bool {
    let features: Vec<u16> = device.features.iter().map(|f| f.id).collect();
    !dpi_changeable(device) && host::can_drive(&features, device.lighting.as_ref())
}

/// Logitech programs that would fight PeriHub over the lights, running now.
fn other_programs() -> Vec<&'static str> {
    let mut found: Vec<&'static str> = crate::conflicts::running()
        .iter()
        .filter(|c| **c == crate::conflicts::Conflict::LogitechGHub)
        .map(|c| c.name())
        .collect();
    found.extend(crate::conflicts::logitech_programs_running());
    found
}

/// The DPI can be changed: the device has adjustable DPI (0x2201) and no onboard profiles. With
/// onboard profiles the DPI comes from the active profile, and a 0x2201 change is only a preview that
/// the next profile or DPI level change overrides.
pub fn dpi_changeable(device: &UnsupportedDevice) -> bool {
    device.dpi.is_some() && device.onboard.is_none()
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
    /// `dev` comes read-only; it gets RAM changes only if the DPI can be changed (see
    /// [`dpi_changeable`]). `saved_dpi` is set at once: the device has just connected or woken up, and
    /// has forgotten it.
    ///
    /// `device` was read earlier, maybe from another device in the same receiver slot: the unit id is
    /// checked first, and a different device gives [`Error::OtherDevice`] with nothing sent to it but
    /// reads.
    ///
    /// `look`: what PeriHub showed on the keyboard earlier in the session, shown again now (see
    /// [`LogitechTask::drive_lights`]).
    pub fn open(
        mut dev: HidppDevice,
        device: UnsupportedDevice,
        saved_dpi: Option<u16>,
        look: Option<Look>,
    ) -> Result<Self> {
        if let Some(unit) = device.unit_id {
            let info = dev.device_info()?;
            if info[1..5] != unit {
                return Err(Error::OtherDevice);
            }
        }
        if dpi_changeable(&device) {
            dev.grant(Access::Ram);
        } else if lights_drivable(&device) {
            dev.grant(Access::Host);
        }
        let mut task = Self {
            dev,
            device,
            saved_dpi,
            last_check: Instant::now(),
            host: None,
            last_host_check: Instant::now(),
            gave_back: None,
            programs: other_programs,
        };
        if let Some(dpi) = saved_dpi {
            match task.apply_dpi(dpi) {
                Ok(()) => log::info!("{}: DPI set to {dpi} again", task.device.label()),
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                Err(e) => log::warn!("{}: setting the DPI again failed: {e}", task.device.label()),
            }
        }
        if let Some(look) = look {
            match task.drive_lights(look) {
                Ok(_) => log::info!("{}: PeriHub drives the lights again", task.device.label()),
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                Err(e) => log::warn!("{}: driving the lights again failed: {e}", task.device.label()),
            }
        }
        Ok(task)
    }

    /// Takes the keyboard's lights over (host mode) and shows `look`, or shows it if PeriHub drives
    /// them already. Refused while G HUB or Solaar runs, or while the keyboard says a program drives
    /// its lights. Returns the key to keep the look under for the session.
    pub fn drive_lights(&mut self, look: Look) -> Result<String> {
        let lighting = self.device.lighting.clone().filter(|_| lights_drivable(&self.device));
        let lighting =
            lighting.ok_or_else(|| Error::Invalid("PeriHub cannot drive the lights of this device".into()))?;
        host::check(&look, &lighting)?;
        let result = match self.host.as_mut() {
            Some(host) => host.show(&mut self.dev, &lighting, look),
            None => {
                let others = (self.programs)();
                if !others.is_empty() {
                    return Err(Error::Invalid(format!(
                        "quit {} first: it drives the lights of Logitech keyboards too",
                        others.join(" and ")
                    )));
                }
                let control = host::who_drives(&mut self.dev)?;
                if control != 0 {
                    return Err(Error::Invalid(format!(
                        "another program drives the lights (mode {control}): quit it first, such as G HUB"
                    )));
                }
                Host::take(&mut self.dev, &lighting, look).map(|host| {
                    log::info!("{}: PeriHub drives the lights (host mode)", self.device.label());
                    self.host = Some(host);
                    self.last_host_check = Instant::now();
                    self.gave_back = None;
                })
            }
        };
        match result {
            Ok(()) => Ok(self.device.settings_key()),
            Err(error) => {
                self.release(&format!("a command failed: {error}"));
                Err(Error::Protocol(format!("{error}. The keyboard has its own lights back.")))
            }
        }
    }

    /// Gives the lights back to the keyboard. Returns the settings key, to forget the look.
    pub fn give_back_lights(&mut self) -> Result<String> {
        self.release("");
        self.gave_back = None;
        Ok(self.device.settings_key())
    }

    /// Back to the keyboard's own lights, best effort: it may be gone already. `why` is shown on
    /// the page when PeriHub did it on its own.
    fn release(&mut self, why: &str) {
        if self.host.take().is_some() {
            match host::give_back(&mut self.dev) {
                Ok(()) => log::info!("{}: lights given back to the keyboard", self.device.label()),
                Err(e) => log::warn!("{}: giving the lights back failed: {e}", self.device.label()),
            }
            if !why.is_empty() {
                log::info!("{}: lights given back: {why}", self.device.label());
                self.gave_back = Some(why.to_string());
            }
        }
    }

    /// Sets the brightness of all the lights; the keyboard keeps it, as with its brightness key.
    pub fn set_brightness(&mut self, value: u16) -> Result<()> {
        let lighting = self.device.lighting.clone().filter(|_| lights_drivable(&self.device));
        let lighting = lighting.ok_or_else(|| Error::Invalid("PeriHub cannot change this device's lights".into()))?;
        host::set_brightness(&mut self.dev, &lighting, value)?;
        log::info!("{}: brightness set to {value}", self.device.label());
        Ok(())
    }

    /// While PeriHub drives the lights: the keyboard's notices, the off timer, and every few seconds
    /// whether PeriHub still has the lights and no other program started.
    fn drive_tick(&mut self) -> Result<()> {
        let Some(lighting) = self.device.lighting.clone() else { return Ok(()) };
        let events = self.dev.events(0)?;
        let check = self.last_host_check.elapsed() >= HOST_CHECK_EVERY;
        let Some(host) = self.host.as_mut() else { return Ok(()) };
        let mut result = host.on_events(&mut self.dev, &lighting, &events).and_then(|()| host.on_timer(&mut self.dev));
        if result.is_ok() && check {
            self.last_host_check = Instant::now();
            self.last_check = Instant::now();
            let others = (self.programs)();
            if !others.is_empty() {
                self.release(&format!("{} started", others.join(" and ")));
                return Ok(());
            }
            result = match host::who_drives(&mut self.dev) {
                // The keyboard is on its own lights again: it restarted without PeriHub noticing.
                Ok(0) => {
                    log::info!("{}: the keyboard took its lights back: taking them again", self.device.label());
                    host.retake(&mut self.dev, &lighting)
                }
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            };
        }
        match result {
            Ok(()) => Ok(()),
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => {
                // Asleep or turned off: it has its own lights when it is back, and the manager
                // shows the look again then.
                self.host = None;
                Err(e)
            }
            Err(error) => {
                self.release(&format!("a command failed: {error}"));
                Ok(())
            }
        }
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
        let lighting = match &self.device.lighting {
            Some(lighting) => {
                let effects = lighting.effects.is_some();
                match LightingNow::read(&mut self.dev, lighting, effects) {
                    Ok(now) => Some(now),
                    Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                    Err(e) => {
                        log::debug!("{} lighting: {e}", self.device.label());
                        None
                    }
                }
            }
            None => None,
        };
        let host = self.host.as_ref().map(|h| HostLive { look: h.look.clone(), status: h.status() });
        Ok(LogitechLive { battery, dpi, saved_dpi: self.saved_dpi, lighting, host, gave_back: self.gave_back.clone() })
    }

    fn apply_dpi(&mut self, dpi: u16) -> Result<()> {
        if !dpi_changeable(&self.device) {
            return Err(Error::Invalid("PeriHub does not change the DPI of this device".into()));
        }
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

impl Drop for LogitechTask {
    fn drop(&mut self) {
        self.release("");
    }
}

impl DeviceTask for LogitechTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.host.is_some() {
            self.drive_tick()?;
            return Ok(EVENTS_EVERY);
        }
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

    const FEATURES: &[u16] = &[feature::ROOT, feature::UNIFIED_BATTERY, feature::ADJUSTABLE_DPI, feature::DEVICE_INFO];

    /// The unit id the simulated mouse reports.
    const UNIT: [u8; 4] = [0x12, 0x34, 0x56, 0x78];

    fn open(saved: Option<u16>, dpi_feature: bool) -> (Result<LogitechTask>, Arc<Mutex<Mouse>>) {
        open_with(saved, dpi_feature, None, UNIT)
    }

    /// `known_unit`: the unit id read earlier from the device in that slot.
    fn open_with(
        saved: Option<u16>,
        dpi_feature: bool,
        onboard: Option<crate::hidpp::OnboardInfo>,
        known_unit: [u8; 4],
    ) -> (Result<LogitechTask>, Arc<Mutex<Mouse>>) {
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
                (Some(feature::DEVICE_INFO), 0) => [&[1][..], &UNIT, &[0, 0x02, 0xB0, 0x34]].concat(),
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
            unit_id: Some(known_unit),
            device_info: None,
            firmware: Vec::new(),
            features: Vec::new(),
            dpi: dpi_feature.then(|| DpiCapabilities { min: 200, max: 8000, step: 50, list: Vec::new() }),
            report_rates_ms: Vec::new(),
            onboard,
            lighting: None,
            read_errors: Vec::new(),
        };
        (LogitechTask::open(dev, device, saved, None), mouse)
    }

    #[test]
    fn the_saved_dpi_of_a_device_is_never_set_on_another_one_in_its_slot() {
        let (task, mouse) = open_with(Some(1600), true, None, [0xAA, 0xBB, 0xCC, 0xDD]);
        assert!(matches!(task, Err(Error::OtherDevice)));
        assert!(mouse.lock().unwrap().set.is_empty());
    }

    #[test]
    fn the_dpi_of_a_device_with_onboard_profiles_is_left_to_its_profiles() {
        let onboard = crate::hidpp::OnboardInfo {
            memory_model: 1,
            profile_format: 5,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 5,
            button_count: 13,
            sector_count: 16,
            sector_size: 255,
            has_gshift: false,
            has_dpi_shift: true,
        };
        let (task, mouse) = open_with(Some(1600), true, Some(onboard), UNIT);
        let mut task = task.unwrap();
        assert!(task.set_dpi(1600).is_err());
        assert!(task.dev.set_sensor_dpi(0, 1600).is_err(), "read-only at the HID++ layer");
        assert!(mouse.lock().unwrap().set.is_empty());
        assert_eq!(task.live().unwrap().dpi, Some(1000), "still read");
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

    mod keyboard {
        use super::*;
        use crate::hidpp::FeatureEntry;
        use crate::hidpp::host::tests::{RED, calls, g915x, keyboard_read_only, look, zone};
        use crate::hidpp::host::{Power, effect};

        fn device() -> UnsupportedDevice {
            let ids = [
                feature::BRIGHTNESS_CONTROL,
                feature::RGB_EFFECTS,
                feature::PER_KEY_LIGHTING_V2,
                feature::PROFILE_MANAGEMENT,
            ];
            UnsupportedDevice {
                slot: 1,
                name: Some("G915 X LS".into()),
                kind: Some(0),
                protocol: "4.2".into(),
                wireless_id: None,
                unit_id: None,
                device_info: None,
                firmware: Vec::new(),
                features: ids.iter().map(|&id| FeatureEntry { id, kind: 0, version: 0 }).collect(),
                dpi: None,
                report_rates_ms: Vec::new(),
                onboard: None,
                lighting: Some(g915x()),
                read_errors: Vec::new(),
            }
        }

        type Kb = std::sync::Arc<std::sync::Mutex<crate::hidpp::host::tests::Keyboard>>;

        fn open(look: Option<Look>) -> (LogitechTask, Kb) {
            let (dev, state, _) = keyboard_read_only();
            let mut task = LogitechTask::open(dev, device(), None, None).unwrap();
            task.programs = Vec::new;
            if let Some(look) = look {
                task.drive_lights(look).unwrap();
            }
            (task, state)
        }

        #[test]
        fn the_lights_are_not_taken_while_another_program_drives_them() {
            let (mut task, state) = open(None);
            state.lock().unwrap().control = 3;
            assert!(matches!(task.drive_lights(look()), Err(Error::Invalid(_))));
            assert_eq!(calls(&state), [(feature::RGB_EFFECTS, 5, vec![])], "only asked");
            task.programs = || vec!["Logitech G HUB"];
            state.lock().unwrap().control = 0;
            assert!(matches!(task.drive_lights(look()), Err(Error::Invalid(m)) if m.contains("G HUB")));
            assert!(calls(&state).is_empty());
        }

        #[test]
        fn the_lights_are_given_back_on_request_and_when_the_task_ends() {
            let (mut task, state) = open(Some(look()));
            assert_eq!(task.live().unwrap().host.unwrap().status.power, Power::Full);
            calls(&state);
            assert_eq!(task.give_back_lights().unwrap(), "name-G915 X LS-0000");
            assert_eq!(calls(&state), [(feature::RGB_EFFECTS, 5, vec![1]), (feature::PROFILE_MANAGEMENT, 6, vec![3])]);
            assert!(task.live().unwrap().host.is_none());

            task.drive_lights(look()).unwrap();
            calls(&state);
            drop(task);
            assert_eq!(calls(&state), [(feature::RGB_EFFECTS, 5, vec![1]), (feature::PROFILE_MANAGEMENT, 6, vec![3])]);
        }

        #[test]
        fn a_keyboard_back_on_its_own_lights_is_taken_again() {
            let (mut task, state) = open(Some(look()));
            {
                let mut k = state.lock().unwrap();
                k.control = 0;
                k.host_mode = false;
                k.calls.clear();
            }
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            assert_eq!(task.tick().unwrap(), EVENTS_EVERY);
            let sent = calls(&state);
            assert_eq!(sent[1], (feature::PROFILE_MANAGEMENT, 6, vec![5]));
            assert_eq!(sent[2], (feature::RGB_EFFECTS, 5, vec![1, 3, 6]));
            assert_eq!(task.live().unwrap().host.unwrap().status.retaken, 1);
            assert!(state.lock().unwrap().host_mode);
        }

        #[test]
        fn another_program_starting_gets_the_lights_back() {
            let (mut task, state) = open(Some(look()));
            calls(&state);
            task.programs = || vec!["Solaar"];
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            task.tick().unwrap();
            assert_eq!(calls(&state), [(feature::RGB_EFFECTS, 5, vec![1]), (feature::PROFILE_MANAGEMENT, 6, vec![3])]);
            let live = task.live().unwrap();
            assert!(live.host.is_none());
            assert_eq!(live.gave_back.as_deref(), Some("Solaar started"));
        }

        #[test]
        fn a_failed_command_gives_the_lights_back() {
            let (mut task, state) = open(Some(look()));
            state.lock().unwrap().busy = 10;
            let error = task.drive_lights(look()).unwrap_err().to_string();
            assert!(error.contains("own lights back"), "{error}");
            assert!(!state.lock().unwrap().host_mode);
            assert!(task.live().unwrap().gave_back.is_some());
        }

        #[test]
        fn a_look_is_checked_before_anything_is_sent() {
            let (mut task, state) = open(None);
            let mut wrong = look();
            wrong.zones[0] = zone(0, effect::CYCLE, Some(RED));
            assert!(matches!(task.drive_lights(wrong), Err(Error::Invalid(_))));
            assert!(calls(&state).is_empty());
        }

        #[test]
        fn a_mouse_whose_dpi_peri_hub_changes_gets_no_host_mode() {
            let mut device = device();
            device.dpi = Some(DpiCapabilities { min: 200, max: 8000, step: 50, list: Vec::new() });
            assert!(!lights_drivable(&device));
            assert!(lights_drivable(&super::keyboard::device()));
        }
    }
}

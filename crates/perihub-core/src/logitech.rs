//! A Logitech HID++ 2.0 device behind a receiver that PeriHub knows only by the features it reports
//! (see [`UnsupportedDevice`]), on its own thread: its battery and DPI as they are now, the DPI the
//! user chose, and the lights of a keyboard PeriHub drives in host mode (see [`crate::hidpp::host`]).
//!
//! Host mode lasts for the session: the manager keeps the look and shows it again every time the
//! keyboard connects (see [`LightsMemory`]); the look itself is saved in PeriHub's settings, but the
//! lights are only taken when the user asks. PeriHub gives the lights back when the user asks, when
//! G HUB or Solaar starts, when a command is refused and taking the lights again fails too, and when
//! PeriHub quits. A keyboard that does not answer for a while ends the thread without giving them
//! back: the manager opens it again and takes them again.
//!
//! The DPI goes to the device's RAM only (0x2201 function 3): nothing is written to its memory. Such
//! devices forget it when they are turned off or fall asleep, so PeriHub sets it again every time
//! the device connects (the thread ends when the device stops answering, and the manager opens it
//! again when it is back). Changing it is an experimental feature: it has not been tried on such a
//! device yet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hidpp::host::{self, Host, HostStatus, Look};
use crate::hidpp::lighting::{Lighting, LightingNow};
use crate::hidpp::receiver::UnsupportedDevice;
use crate::hidpp::{Access, Battery, DpiCapabilities, HidppDevice, feature};
use crate::worker::DeviceTask;

/// How often the thread checks that the device still answers.
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// While PeriHub drives the lights: how often the keyboard's notices are read, and how often PeriHub
/// checks that it still has the lights and that no other Logitech program started.
const EVENTS_EVERY: Duration = Duration::from_millis(200);
const HOST_CHECK_EVERY: Duration = Duration::from_secs(5);
/// Calls in a row the keyboard does not answer before the thread ends, to be opened again. One
/// missed reply is common over the radio: the G915 X LS took up to 1.4 s to answer in the G HUB
/// captures of issue #30.
const TIMEOUTS_IN_A_ROW: u32 = 3;

/// How host mode went this session, across give-backs and reconnections, for the test report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HostLog {
    /// Times PeriHub took the lights, and of them when the keyboard connected again.
    pub taken: u32,
    pub on_connect: u32,
    /// Times PeriHub gave them back, and why the last time ("asked" when the user did).
    pub given_back: u32,
    pub last_reason: Option<String>,
    /// Calls the keyboard did not answer in time while PeriHub drove it.
    pub timeouts: u32,
    pub idle_events: u32,
    pub active_events: u32,
    pub retaken: u32,
    pub shown: u32,
    pub resets: u32,
}

/// What the manager keeps about a keyboard's lights while its thread comes and goes.
#[derive(Debug, Clone, Default)]
pub struct LightsMemory {
    /// The user wants PeriHub to drive these lights (saved in the settings): set by "Drive the
    /// lights", cleared only by "Give the lights back" and the tray's "Turn off experimental
    /// lighting". While it is set PeriHub takes the lights every time the keyboard connects, at
    /// every start too, and again once G HUB or Solaar quit.
    pub wanted: Arc<AtomicBool>,
    /// PeriHub took the lights this session and has not given them back: a keyboard that still says
    /// a program drives them is driven by PeriHub (it went away and came back).
    pub ours: Arc<AtomicBool>,
    /// The last look the user showed, saved in PeriHub's settings: the editor starts from it.
    pub saved: Option<Look>,
    pub log: Arc<Mutex<HostLog>>,
}

/// What to do when a call fails while PeriHub drives the lights.
#[derive(Debug, PartialEq, Eq)]
enum AfterError {
    /// The keyboard is gone (asleep, turned off, out of reach): end the thread without giving the
    /// lights back; the manager opens it again and takes them again.
    EndThread,
    /// No answer in time: paint again at the next tick.
    TryAgain,
    /// Refused (such as error 5, "not allowed", once the keyboard left host mode): take the lights
    /// again, and give them back only if that fails too.
    TakeAgain,
}

fn after_error(error: &Error, timeouts_in_a_row: u32) -> AfterError {
    match error {
        Error::Hid(_) | Error::Unreachable => AfterError::EndThread,
        Error::Timeout if timeouts_in_a_row >= TIMEOUTS_IN_A_ROW => AfterError::EndThread,
        Error::Timeout => AfterError::TryAgain,
        _ => AfterError::TakeAgain,
    }
}

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
    /// The last look shown on the keyboard, kept in PeriHub's settings.
    pub saved_look: Option<Look>,
    /// How host mode went this session, once PeriHub took the lights.
    pub host_log: Option<HostLog>,
    /// PeriHub takes the lights every time the keyboard connects, at every start too.
    pub drive_at_start: bool,
    /// Why PeriHub does not drive them now although it should, such as G HUB running.
    pub waiting: Option<String>,
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
    lights: LightsMemory,
    timeouts_in_a_row: u32,
    /// Why the lights were not taken automatically (see [`LightsMemory::wanted`]).
    waiting: Option<String>,
    /// No automatic take-over until the keyboard connects again: one failed, or PeriHub is quitting.
    auto_paused: bool,
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
    /// `lights`: what the manager keeps about the keyboard's lights. If the user wants PeriHub to
    /// drive them, it takes them and shows the saved look, with the checks of "Drive the lights"
    /// (see [`LogitechTask::auto_take`]).
    pub fn open(
        mut dev: HidppDevice,
        device: UnsupportedDevice,
        saved_dpi: Option<u16>,
        lights: LightsMemory,
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
            lights: lights.clone(),
            timeouts_in_a_row: 0,
            waiting: None,
            auto_paused: false,
            // Tests never look at the programs running on the machine that runs them.
            programs: if cfg!(test) { Vec::new } else { other_programs },
        };
        if let Some(dpi) = saved_dpi {
            match task.apply_dpi(dpi) {
                Ok(()) => log::info!("{}: DPI set to {dpi} again", task.device.label()),
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                Err(e) => log::warn!("{}: setting the DPI again failed: {e}", task.device.label()),
            }
        }
        task.auto_take()?;
        Ok(task)
    }

    /// Takes the lights and shows the saved look if the user wants PeriHub to drive them and it
    /// does not yet, with the checks of "Drive the lights": G HUB and Solaar are not running, and
    /// the keyboard drives its own lights (or PeriHub did, before it went away). The "No warranty"
    /// box was accepted for the keyboard when the user first pressed "Drive the lights", which is
    /// what makes [`LightsMemory::wanted`] true. Otherwise it says why on the page and tries again
    /// at the next check. Only a keyboard that is gone is an error.
    fn auto_take(&mut self) -> Result<()> {
        let wanted = self.lights.wanted.load(Ordering::SeqCst);
        let Some(look) = self.lights.saved.clone().filter(|_| wanted && self.host.is_none() && !self.auto_paused)
        else {
            return Ok(());
        };
        let again = self.lights.ours.load(Ordering::SeqCst);
        match self.drive(look, again) {
            Ok(_) => {
                log::info!("{}: PeriHub drives the lights, as the user asked", self.device.label());
                self.lights.log.lock().unwrap().on_connect += 1;
                Ok(())
            }
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => Err(e),
            // Refused before anything was sent (G HUB runs, another program drives the lights),
            // or no answer: tried again at the next check.
            Err(e @ (Error::Invalid(_) | Error::Timeout)) => {
                if self.waiting.is_none() {
                    log::info!("{}: not driving the lights yet: {e}", self.device.label());
                }
                self.waiting = Some(e.to_string().trim_start_matches("invalid value: ").to_string());
                Ok(())
            }
            // The take-over failed and the keyboard got its lights back: not again until it
            // connects again.
            Err(e) => {
                log::warn!("{}: taking the lights failed: {e}", self.device.label());
                self.auto_paused = true;
                self.waiting =
                    Some(format!("taking them failed ({e}): PeriHub tries again when the keyboard connects"));
                Ok(())
            }
        }
    }

    /// Takes the keyboard's lights over (host mode) and shows `look`, or shows it if PeriHub drives
    /// them already. Refused while G HUB or Solaar runs, or while the keyboard says a program drives
    /// its lights. Returns the key to keep the look under for the session.
    pub fn drive_lights(&mut self, look: Look) -> Result<String> {
        let key = self.drive(look, false)?;
        self.lights.wanted.store(true, Ordering::SeqCst);
        self.auto_paused = false;
        Ok(key)
    }

    /// `again`: PeriHub drove the lights before the keyboard went away, so a program driving them
    /// (mode 3) is PeriHub itself, unless G HUB or Solaar runs.
    fn drive(&mut self, look: Look, again: bool) -> Result<String> {
        let lighting = self.device.lighting.clone().filter(|_| lights_drivable(&self.device));
        let lighting =
            lighting.ok_or_else(|| Error::Invalid("PeriHub cannot drive the lights of this device".into()))?;
        host::check(&look, &lighting)?;
        let kept = look.clone();
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
                if control != 0 && !again {
                    return Err(Error::Invalid(format!(
                        "another program drives the lights (mode {control}): quit it first, such as G HUB"
                    )));
                }
                if control != 0 {
                    log::info!(
                        "{}: the keyboard is still in host mode from before: taking it again",
                        self.device.label()
                    );
                }
                let label = self.device.label();
                Host::take(&mut self.dev, &lighting, look).map(|host| {
                    log::info!("{label}: PeriHub drives the lights (host mode)");
                    self.host = Some(host);
                    self.last_host_check = Instant::now();
                    self.timeouts_in_a_row = 0;
                    self.gave_back = None;
                    self.waiting = None;
                    self.lights.ours.store(true, Ordering::SeqCst);
                    self.lights.log.lock().unwrap().taken += 1;
                })
            }
        };
        match result {
            Ok(()) => {
                self.lights.saved = Some(kept);
                Ok(self.device.settings_key())
            }
            // No answer in time while PeriHub drives the lights: the next tick paints again.
            Err(Error::Timeout) if self.host.is_some() => {
                self.lights.saved = Some(kept);
                self.lights.log.lock().unwrap().timeouts += 1;
                if let Some(host) = self.host.as_mut() {
                    host.paint_later();
                }
                Err(Error::Protocol("the keyboard did not answer in time: PeriHub shows it again in a moment".into()))
            }
            Err(error) => {
                self.release(&format!("a command failed: {error}"), true);
                self.auto_paused = true;
                Err(Error::Protocol(format!("{error}. The keyboard has its own lights back.")))
            }
        }
    }

    /// Gives the lights back to the keyboard because the user asked: PeriHub no longer takes them
    /// when the keyboard connects (the look stays in the editor). Returns the settings key.
    pub fn give_back_lights(&mut self) -> Result<String> {
        self.lights.wanted.store(false, Ordering::SeqCst);
        self.release("asked", false);
        self.gave_back = None;
        self.waiting = None;
        Ok(self.device.settings_key())
    }

    /// Gives the lights back because PeriHub quits or installs an update: they are taken again at
    /// the next start, not before.
    pub fn give_back_for_now(&mut self) -> Result<()> {
        self.auto_paused = true;
        self.release("PeriHub quits", false);
        Ok(())
    }

    /// Ends host mode without a word to the keyboard (it is gone), keeping the counts.
    fn end_host(&mut self) -> Option<Host> {
        let host = self.host.take()?;
        let status = host.status();
        let mut log = self.lights.log.lock().unwrap();
        log.idle_events += status.idle_events;
        log.active_events += status.active_events;
        log.retaken += status.retaken;
        log.shown += status.shown;
        log.resets += status.resets;
        Some(host)
    }

    /// How host mode went this session, the lights PeriHub drives now included.
    fn host_log(&self) -> HostLog {
        let mut log = self.lights.log.lock().unwrap().clone();
        if let Some(status) = self.host.as_ref().map(|h| h.status()) {
            log.idle_events += status.idle_events;
            log.active_events += status.active_events;
            log.retaken += status.retaken;
            log.shown += status.shown;
            log.resets += status.resets;
        }
        log
    }

    /// Back to the keyboard's own lights, best effort: it may be gone already. `notice`: `why` is
    /// shown on the page, PeriHub did it on its own.
    fn release(&mut self, why: &str, notice: bool) {
        if self.end_host().is_some() {
            self.lights.ours.store(false, Ordering::SeqCst);
            match host::give_back(&mut self.dev) {
                Ok(()) => log::info!("{}: lights given back to the keyboard ({why})", self.device.label()),
                Err(e) => log::warn!("{}: giving the lights back failed: {e}", self.device.label()),
            }
            let mut log = self.lights.log.lock().unwrap();
            log.given_back += 1;
            log.last_reason = Some(why.to_string());
            if notice {
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
    /// whether PeriHub still has the lights and no other program started. A failed call never gives
    /// the lights back at once (see [`after_error`]): before, one missed reply or one refused call
    /// gave them back, and a keyboard that went away and came back still in host mode was not taken
    /// again (issue #30: "the Drive the lights button is there again").
    fn drive_tick(&mut self) -> Result<()> {
        let Some(lighting) = self.device.lighting.clone() else { return Ok(()) };
        let check = self.last_host_check.elapsed() >= HOST_CHECK_EVERY;
        if check {
            self.last_host_check = Instant::now();
            self.last_check = Instant::now();
            let others = (self.programs)();
            if !others.is_empty() {
                self.release(&format!("{} started", others.join(" and ")), true);
                return Ok(());
            }
        }
        let error = match self.drive_step(&lighting, check) {
            Ok(()) => {
                self.timeouts_in_a_row = 0;
                return Ok(());
            }
            Err(error) => error,
        };
        if matches!(error, Error::Timeout) {
            self.timeouts_in_a_row += 1;
            self.lights.log.lock().unwrap().timeouts += 1;
        }
        match after_error(&error, self.timeouts_in_a_row) {
            AfterError::EndThread => {
                log::info!("{}: the keyboard does not answer ({error}): opened again later", self.device.label());
                self.end_host();
                Err(error)
            }
            AfterError::TryAgain => {
                if let Some(host) = self.host.as_mut() {
                    host.paint_later();
                }
                Ok(())
            }
            AfterError::TakeAgain => {
                log::info!("{}: {error}: taking the lights again", self.device.label());
                let Some(host) = self.host.as_mut() else { return Ok(()) };
                match host.retake(&mut self.dev, &lighting) {
                    Ok(()) => Ok(()),
                    Err(e @ (Error::Hid(_) | Error::Unreachable | Error::Timeout)) => {
                        self.end_host();
                        Err(e)
                    }
                    Err(e) => {
                        self.release(&format!("a command failed: {error}, then {e}"), true);
                        self.auto_paused = true;
                        Ok(())
                    }
                }
            }
        }
    }

    fn drive_step(&mut self, lighting: &Lighting, check: bool) -> Result<()> {
        let events = self.dev.events(0)?;
        let Some(host) = self.host.as_mut() else { return Ok(()) };
        host.catch_up(&mut self.dev, lighting)?;
        host.on_events(&mut self.dev, lighting, &events)?;
        host.on_timer(&mut self.dev)?;
        // The keyboard is on its own lights again: it restarted without PeriHub noticing.
        if check && host::who_drives(&mut self.dev)? == 0 {
            log::info!("{}: the keyboard took its lights back: taking them again", self.device.label());
            host.retake(&mut self.dev, lighting)?;
        }
        Ok(())
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
        let log = self.host_log();
        Ok(LogitechLive {
            battery,
            dpi,
            saved_dpi: self.saved_dpi,
            lighting,
            host,
            gave_back: self.gave_back.clone(),
            saved_look: self.lights.saved.clone(),
            host_log: (log.taken > 0).then_some(log),
            drive_at_start: self.wants_lights(),
            waiting: self.waiting.clone().filter(|_| self.host.is_none() && self.wants_lights()),
        })
    }

    /// The user wants PeriHub to drive the lights, and there is a look to show.
    fn wants_lights(&self) -> bool {
        self.lights.wanted.load(Ordering::SeqCst) && self.lights.saved.is_some()
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
        self.release("PeriHub stopped driving them", false);
    }
}

impl DeviceTask for LogitechTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.host.is_some() {
            self.drive_tick()?;
            return Ok(EVENTS_EVERY);
        }
        // The user wants PeriHub to drive the lights and it does not: tried again every few
        // seconds, such as once G HUB or Solaar quit.
        let auto = self.wants_lights() && !self.auto_paused;
        if auto && self.last_host_check.elapsed() >= HOST_CHECK_EVERY {
            self.last_host_check = Instant::now();
            self.auto_take()?;
            if self.host.is_some() {
                return Ok(EVENTS_EVERY);
            }
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
        let next = CHECK_EVERY.saturating_sub(self.last_check.elapsed());
        Ok(if auto { next.min(HOST_CHECK_EVERY.saturating_sub(self.last_host_check.elapsed())) } else { next })
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
        (LogitechTask::open(dev, device, saved, LightsMemory::default()), mouse)
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
        use crate::hidpp::host::tests::{Pending, RED, calls, g915x, idle_event, keyboard_read_only, look, zone};
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
            let (task, state, _) = open_with(look, LightsMemory::default());
            (task, state)
        }

        fn open_with(look: Option<Look>, lights: LightsMemory) -> (LogitechTask, Kb, Pending) {
            let (dev, state, pending) = keyboard_read_only();
            let mut task = LogitechTask::open(dev, device(), None, lights).unwrap();
            task.programs = Vec::new;
            if let Some(look) = look {
                task.drive_lights(look).unwrap();
            }
            (task, state, pending)
        }

        #[test]
        fn a_keyboard_that_went_away_in_host_mode_is_taken_again_when_it_answers() {
            let lights = LightsMemory::default();
            let (mut task, state, _) = open_with(Some(look()), lights.clone());
            // Out of reach for a moment: the thread ends without giving the lights back.
            state.lock().unwrap().asleep = true;
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            assert!(matches!(task.tick(), Err(Error::Unreachable)));
            drop(task);
            let k = state.lock().unwrap();
            assert!(k.host_mode && k.control == 3, "not given back");
            assert!(!k.calls.iter().any(|c| c.0 == feature::PROFILE_MANAGEMENT && c.2 == [3]));
            drop(k);
            // Back, still in host mode (mode 3): the manager opens it with the look it drove.
            let (dev, state, _) = keyboard_read_only();
            {
                let mut k = state.lock().unwrap();
                k.control = 3;
                k.host_mode = true;
            }
            let memory = LightsMemory { saved: Some(look()), ..lights };
            let mut again = LogitechTask::open(dev, device(), None, memory).unwrap();
            again.programs = Vec::new;
            let live = again.live().unwrap();
            assert!(live.host.is_some(), "taken again");
            let log = live.host_log.unwrap();
            assert_eq!((log.taken, log.on_connect, log.given_back), (2, 1, 0));
            assert_eq!(live.saved_look, Some(look()));
            // Asked by the user, mode 3 is still refused.
            again.give_back_lights().unwrap();
            state.lock().unwrap().control = 3;
            assert!(matches!(again.drive_lights(look()), Err(Error::Invalid(_))));
        }

        /// A keyboard whose lights the user drove with `look()` (as saved in the settings).
        fn wanted() -> LightsMemory {
            let memory = LightsMemory { saved: Some(look()), ..LightsMemory::default() };
            memory.wanted.store(true, Ordering::SeqCst);
            memory
        }

        fn took(state: &Kb) -> bool {
            calls(state).iter().any(|c| *c == (feature::PROFILE_MANAGEMENT, 6, vec![5]))
        }

        #[test]
        fn the_saved_look_is_taken_when_the_keyboard_connects() {
            let (mut task, state, _) = open_with(None, wanted());
            assert!(took(&state));
            let live = task.live().unwrap();
            assert_eq!(live.host.unwrap().look, look());
            assert!(live.drive_at_start && live.waiting.is_none());
            assert_eq!(live.host_log.unwrap().on_connect, 1);
        }

        #[test]
        fn the_lights_are_not_taken_after_the_user_gave_them_back() {
            let memory = wanted();
            let (mut task, state, _) = open_with(None, memory.clone());
            task.give_back_lights().unwrap();
            assert!(!memory.wanted.load(Ordering::SeqCst));
            calls(&state);
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            task.tick().unwrap();
            assert!(!took(&state) && task.live().unwrap().host.is_none());
            assert!(!task.live().unwrap().drive_at_start);
            // Nor when it connects again (at the next start).
            drop(task);
            let (dev, state, _) = keyboard_read_only();
            let mut task = LogitechTask::open(dev, device(), None, memory).unwrap();
            assert!(!took(&state) && task.live().unwrap().host.is_none());
        }

        #[test]
        fn the_lights_are_not_taken_while_g_hub_runs_or_another_program_has_them() {
            // At the start, the keyboard says a program drives its lights, and it was not PeriHub.
            let (dev, state, _) = keyboard_read_only();
            state.lock().unwrap().control = 3;
            let memory = wanted();
            let mut task = LogitechTask::open(dev, device(), None, memory.clone()).unwrap();
            assert!(!took(&state));
            let live = task.live().unwrap();
            assert!(live.host.is_none() && live.drive_at_start);
            assert!(live.waiting.unwrap().contains("mode 3"));

            // G HUB runs: nothing is asked of the keyboard.
            state.lock().unwrap().control = 0;
            task.programs = || vec!["Logitech G HUB"];
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            calls(&state);
            task.tick().unwrap();
            assert!(calls(&state).is_empty());
            assert!(task.live().unwrap().waiting.unwrap().contains("G HUB"));
        }

        #[test]
        fn the_lights_are_taken_again_once_g_hub_quits() {
            let (mut task, state, _) = open_with(None, wanted());
            task.programs = || vec!["Logitech G HUB"];
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            task.tick().unwrap();
            let live = task.live().unwrap();
            assert!(live.host.is_none() && live.drive_at_start);
            assert_eq!(live.gave_back.as_deref(), Some("Logitech G HUB started"));
            calls(&state);
            // Still running at the next check; then it quits and gives the keyboard its lights.
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            task.tick().unwrap();
            assert!(!took(&state));
            task.programs = Vec::new;
            task.last_host_check = Instant::now() - HOST_CHECK_EVERY;
            assert_eq!(task.tick().unwrap(), EVENTS_EVERY);
            assert!(took(&state));
            let live = task.live().unwrap();
            assert!(live.host.is_some() && live.gave_back.is_none() && live.waiting.is_none());
            assert_eq!(live.host_log.unwrap().taken, 2);
        }

        #[test]
        fn a_refused_command_takes_the_lights_again_instead_of_giving_them_back() {
            let (mut task, state, pending) = open_with(Some(look()), LightsMemory::default());
            pending.lock().unwrap().push_back(idle_event(false));
            task.tick().unwrap();
            assert_eq!(task.live().unwrap().host.unwrap().status.power, Power::Dimmed);
            // The keyboard restarted on its own lights: turning them off is refused (error 5).
            {
                let mut k = state.lock().unwrap();
                k.control = 0;
                k.host_mode = false;
                k.calls.clear();
            }
            task.host.as_mut().unwrap().idle_for(Duration::from_secs(270));
            task.tick().unwrap();
            let live = task.live().unwrap();
            assert!(live.host.is_some() && live.gave_back.is_none(), "{live:?}");
            assert_eq!(live.host_log.unwrap().retaken, 1);
            let k = state.lock().unwrap();
            assert!(k.host_mode && k.control == 3);
        }

        #[test]
        fn what_to_do_when_a_call_fails() {
            assert_eq!(after_error(&Error::Unreachable, 0), AfterError::EndThread);
            assert_eq!(after_error(&Error::Timeout, 1), AfterError::TryAgain);
            assert_eq!(after_error(&Error::Timeout, TIMEOUTS_IN_A_ROW), AfterError::EndThread);
            assert_eq!(after_error(&Error::Hidpp(crate::error::HidppErrorCode(5)), 0), AfterError::TakeAgain);
        }

        #[test]
        fn the_last_look_and_the_session_counts_stay_after_giving_back() {
            let (mut task, _state) = open(Some(look()));
            task.give_back_lights().unwrap();
            let live = task.live().unwrap();
            assert!(live.host.is_none() && live.gave_back.is_none());
            assert_eq!(live.saved_look, Some(look()));
            let log = live.host_log.unwrap();
            assert_eq!((log.taken, log.given_back, log.last_reason.as_deref()), (1, 1, Some("asked")));
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

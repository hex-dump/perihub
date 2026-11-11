//! The thread of one device of another brand: reads it, and makes the experimental changes the
//! user asks for, the same way for every driver.
//!
//! A change: the settings are read again and checked against what the device accepts, a backup of
//! them is saved, the driver sends only what differs, and the settings are read back. A change
//! that does not read back is undone at once. One the user does not keep within the countdown is
//! undone too, also the next time the device is opened if PeriHub was closed in between.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{
    BlindSetting, BlindValue, Command, Hid, Io, Model, PeripheralInfo, Protocol, Reading, Settings, asus,
    check_settings, steelseries,
};
use crate::config;
use crate::error::{Error, Result};
use crate::worker::DeviceTask;

/// How a protocol reads and changes a device. A driver reaches the device only through [`Io`].
pub trait Driver: Send {
    /// The commands the driver may send: [`Io`] refuses every other one.
    fn commands(&self) -> Vec<Command>;
    /// The device can report its settings. If not, only [`Driver::send_blind`] changes it.
    fn readable(&self) -> bool {
        true
    }
    /// The settings of a device that cannot report them.
    fn blind_settings(&self) -> Vec<BlindSetting> {
        Vec::new()
    }
    /// Sends one setting to a device that cannot report it, without saving it to its memory.
    fn send_blind(&mut self, _io: &mut Io, id: &str, _value: &BlindValue) -> Result<()> {
        Err(Error::Invalid(format!("{id}: this device's settings are changed together")))
    }
    /// Reads what the page shows, with read commands only.
    fn read(&mut self, io: &mut Io) -> Result<Reading>;
    /// Sends what turns `from` into `to`, only what differs. `to` was checked against the limits.
    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()>;
    /// Stores the settings in the device's memory, on devices that can.
    fn save(&mut self, io: &mut Io) -> Result<()>;
    fn can_save(&self) -> bool;
    /// Where the protocol is documented, for the page and the test report.
    fn source(&self) -> &'static str;
    fn profiles(&self) -> u8;
}

pub fn driver(model: &'static Model) -> Box<dyn Driver> {
    match model.protocol {
        Protocol::Asus(params) => Box::new(asus::Asus::new(params)),
        Protocol::SteelSeries(params) => Box::new(steelseries::SteelSeries::new(params)),
    }
}

/// The interface counts down 15 seconds: the core undoes a change that was not kept a bit later.
const UNDO_AFTER: Duration = Duration::from_secs(20);
/// A change that was not kept is undone when the device is opened again, but not a day later.
const PENDING_UNDO_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// The device's buttons change its active DPI level and profile: read again this often.
const REFRESH: Duration = Duration::from_secs(5);
const SETTINGS_FILE: &str = "settings.json";
const RAW_FILE: &str = "answers.txt";
const PENDING_FILE: &str = "pending-undo";

pub struct PeripheralTask {
    io: Io,
    driver: Box<dyn Driver>,
    pub info: PeripheralInfo,
    backups: PathBuf,
    /// Backup taken before the last change, to undo it.
    last_backup: Option<String>,
    /// The last change was also saved to the device's memory: undoing it saves too.
    last_saved: bool,
    undo_at: Option<Instant>,
    last_refresh: Instant,
    /// What the device's memory is taken to hold: the settings read when PeriHub connected, until
    /// a save is kept. The undo of a save puts this back, not what a kept Apply left in RAM.
    in_memory: Option<Settings>,
}

impl PeripheralTask {
    /// `changes`: changes may be tried (PeriHub's app; never the command line).
    pub fn open(hid: Box<dyn Hid>, model: &'static Model, product_id: u16, changes: bool) -> Result<Self> {
        let backups =
            config::config_dir().join("backups").join(format!("{}-{:04x}", model.brand.to_lowercase(), product_id));
        Self::start(hid, driver(model), model, product_id, changes, backups)
    }

    fn start(
        hid: Box<dyn Hid>,
        mut driver: Box<dyn Driver>,
        model: &'static Model,
        product_id: u16,
        changes: bool,
        backups: PathBuf,
    ) -> Result<Self> {
        let mut io = Io::new(hid, driver.commands(), changes);
        let reading = driver.read(&mut io)?;
        let info = PeripheralInfo {
            key: format!("{:04x}:{product_id:04x}", model.vendor_id),
            vendor_id: model.vendor_id,
            product_id,
            brand: model.brand,
            model: model.name,
            kind: model.kind,
            source: driver.source(),
            profiles: driver.profiles(),
            firmware: None,
            battery: None,
            active_profile: None,
            active_dpi: None,
            readable: driver.readable(),
            settings: Settings::default(),
            limits: Default::default(),
            buttons: Vec::new(),
            blind: driver.blind_settings(),
            editable: false,
            can_save: driver.can_save(),
            problem: None,
            pending_undo: false,
            raw: Vec::new(),
        };
        let mut task = Self {
            io,
            driver,
            info,
            backups,
            last_backup: None,
            last_saved: false,
            undo_at: None,
            last_refresh: Instant::now(),
            in_memory: None,
        };
        task.take(reading);
        log::info!(
            "{} {} connected (experimental, from {}): {}",
            model.brand,
            model.name,
            task.info.source,
            match (&task.info.problem, task.info.editable) {
                (Some(problem), _) => format!("read-only, {problem}"),
                (None, true) => "may be changed".into(),
                (None, false) => "read-only".into(),
            }
        );
        task.finish_pending_undo();
        task.in_memory = task.info.readable.then(|| task.info.settings.clone());
        Ok(task)
    }

    /// Keeps what was read. Something that did not look as documented ends the changes for good.
    fn take(&mut self, reading: Reading) {
        if let Some(problem) = &reading.problem {
            if self.info.problem.is_none() {
                log::warn!("{} {}: {problem}. Nothing will be changed on it.", self.info.brand, self.info.model);
            }
            self.io.forbid_changes();
        }
        self.info.firmware = reading.firmware;
        self.info.battery = reading.battery.map(|(level, charging)| super::Battery { level, charging });
        self.info.active_profile = reading.active_profile;
        self.info.active_dpi = reading.active_dpi;
        self.info.settings = reading.settings;
        self.info.limits = reading.limits;
        self.info.buttons = reading.buttons;
        self.info.problem = reading.problem.or(self.info.problem.take());
        self.info.editable = self.io.changes_allowed();
        self.info.raw = self.io.take_raw();
        self.last_refresh = Instant::now();
    }

    fn read(&mut self) -> Result<()> {
        let reading = self.driver.read(&mut self.io)?;
        self.take(reading);
        Ok(())
    }

    /// What the device shows now.
    pub fn info(&mut self) -> Result<PeripheralInfo> {
        self.read()?;
        Ok(self.info.clone())
    }

    /// Sends one setting to a device that cannot report its settings. Nothing is saved to its
    /// memory, and nothing can be read back or undone: unplugging the device brings back what it
    /// has saved.
    pub fn set_blind(&mut self, id: &str, value: &BlindValue) -> Result<PeripheralInfo> {
        if !self.info.editable {
            return Err(Error::Invalid("this connection may not change anything".into()));
        }
        let index = self
            .info
            .blind
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| Error::Invalid(format!("{id} is not a setting of this device")))?;
        self.driver.send_blind(&mut self.io, id, value)?;
        self.info.blind[index].last = Some(value.clone());
        self.info.raw = self.io.take_raw();
        Ok(self.info.clone())
    }

    /// Changes the settings, and also saves them to the device's memory if `save`. Undone unless
    /// kept (see [`Self::keep_changes`]). `profile`: the active profile the settings were edited on;
    /// if the device switched profile since, nothing is changed.
    pub fn change(&mut self, wanted: &Settings, save: bool, profile: Option<u8>) -> Result<PeripheralInfo> {
        if !self.info.editable {
            return Err(Error::Invalid(match &self.info.problem {
                Some(problem) => format!("nothing is changed on this device: {problem}"),
                None => "this connection may not change anything".into(),
            }));
        }
        if !self.info.readable {
            return Err(Error::Invalid("this device cannot report its settings: they are sent one at a time".into()));
        }
        if save && !self.info.can_save {
            return Err(Error::Invalid("this device has no memory to save to".into()));
        }
        self.read()?;
        if !self.info.editable {
            return Err(Error::Invalid(format!("nothing is changed on this device: {:?}", self.info.problem)));
        }
        if profile.is_some() && profile != self.info.active_profile {
            return Err(Error::Invalid(format!(
                "the device switched to profile {} since the page was edited: nothing was changed, check the page and try again",
                self.info.active_profile.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
            )));
        }
        let before = self.info.settings.clone();
        check_settings(&self.info.limits, &before, wanted)?;
        if *wanted == before && !save {
            return Ok(self.info.clone());
        }
        // Undoing a save puts back what the memory held, not what a kept Apply left in RAM.
        let restore = if save { self.in_memory.clone().unwrap_or_else(|| before.clone()) } else { before.clone() };
        let id = self.backup(&restore)?;
        std::fs::write(self.backups.join(PENDING_FILE), format!("{id}\n{}", if save { "save" } else { "change" }))?;
        self.last_backup = Some(id);
        self.last_saved = save;
        self.undo_at = Some(Instant::now() + UNDO_AFTER);
        log::info!(
            "{} {}: changing {before:?} to {wanted:?}{}",
            self.info.brand,
            self.info.model,
            if save { ", and saving" } else { "" }
        );

        let result = self
            .driver
            .apply(&mut self.io, &before, wanted)
            .and_then(|()| if save { self.driver.save(&mut self.io) } else { Ok(()) });
        let read_back = result.and_then(|()| self.read()).and_then(|()| {
            if self.info.settings == *wanted {
                Ok(())
            } else {
                Err(Error::Protocol(format!("the device reads back {:?} instead of what was sent", self.info.settings)))
            }
        });
        if let Err(error) = read_back {
            return Err(match self.undo_last_change() {
                Ok(_) => Error::Protocol(format!("{error}. The settings from before were put back.")),
                Err(undo) => Error::Protocol(format!(
                    "{error}. Putting the settings from before back failed too ({undo}): unplug the device and plug it in again."
                )),
            });
        }
        Ok(self.info.clone())
    }

    fn backup(&self, settings: &Settings) -> Result<String> {
        let json = serde_json::to_vec_pretty(settings)?;
        let raw = self.info.raw.join("\n");
        let dir =
            config::write_backup_in(&self.backups, "settings", &[(SETTINGS_FILE, &json), (RAW_FILE, raw.as_bytes())])?;
        // The folder name only: the full path would put the user's name in the log file.
        let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        log::info!("{} {} backup saved: {id}", self.info.brand, self.info.model);
        Ok(id)
    }

    /// The user checked the device after a change and keeps it.
    pub fn keep_changes(&mut self) {
        if self.last_saved && self.undo_at.is_some() {
            self.in_memory = Some(self.info.settings.clone());
        }
        self.undo_at = None;
        let _ = std::fs::remove_file(self.backups.join(PENDING_FILE));
    }

    /// Puts back the settings from before the last change, and saves them if that change was saved.
    pub fn undo_last_change(&mut self) -> Result<PeripheralInfo> {
        self.undo_at = None;
        let id = self.last_backup.clone().ok_or_else(|| Error::Invalid("nothing to undo".into()))?;
        let saved: Settings = serde_json::from_slice(&config::read_backup_file_in(&self.backups, &id, SETTINGS_FILE)?)?;
        self.read()?;
        let now = self.info.settings.clone();
        // The undo goes through even on a connection where something since looked wrong: it only
        // sends back values the device itself gave.
        let allowed = self.io.changes_allowed();
        self.io.set_changes(true);
        let sent = (|| {
            if now != saved {
                self.driver.apply(&mut self.io, &now, &saved)?;
            }
            if self.last_saved {
                self.driver.save(&mut self.io)?;
            }
            Ok::<(), Error>(())
        })();
        self.io.set_changes(allowed);
        sent?;
        self.read()?;
        self.last_backup = None;
        let _ = std::fs::remove_file(self.backups.join(PENDING_FILE));
        if self.info.settings != saved {
            return Err(Error::Protocol(format!("after the undo the device reads {:?}", self.info.settings)));
        }
        log::info!("{} {}: last change undone, backup {id} put back", self.info.brand, self.info.model);
        Ok(self.info.clone())
    }

    /// When the device is opened: a change that was neither kept nor undone is undone now. A day
    /// later the backup is too old to put back blindly.
    fn finish_pending_undo(&mut self) {
        let path = self.backups.join(PENDING_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let mut lines = text.lines();
        let id = lines.next().unwrap_or_default().trim().to_string();
        let saved = lines.next() == Some("save");
        let stamp = id.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if now.saturating_sub(stamp) > PENDING_UNDO_MAX_AGE.as_secs() {
            log::warn!(
                "a change to the {} that was not kept is too old to undo now: backup {id} left alone",
                self.info.model
            );
            let _ = std::fs::remove_file(&path);
            return;
        }
        if !self.io.granted() {
            self.info.pending_undo = true;
            return;
        }
        log::warn!(
            "a change to the {} was not kept before PeriHub closed or it was unplugged: undoing it",
            self.info.model
        );
        self.last_backup = Some(id);
        self.last_saved = saved;
        if let Err(e) = self.undo_last_change() {
            log::warn!("{} undo failed: {e}", self.info.model);
        }
    }
}

impl DeviceTask for PeripheralTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.undo_at.is_some_and(|at| Instant::now() >= at) {
            log::warn!("a change to the {} was not kept: undoing it", self.info.model);
            if let Err(e) = self.undo_last_change() {
                log::warn!("{} undo failed: {e}", self.info.model);
            }
        }
        if self.last_refresh.elapsed() >= REFRESH {
            // A device that stops answering is gone: the manager opens it again when it is back.
            self.read()?;
        }
        Ok(match self.undo_at {
            Some(at) => REFRESH.min(at.saturating_duration_since(Instant::now())),
            None => REFRESH,
        })
    }
}

#[cfg(test)]
pub(crate) fn start_for_test(
    hid: Box<dyn Hid>,
    model: &'static Model,
    product_id: u16,
    changes: bool,
    backups: PathBuf,
) -> Result<PeripheralTask> {
    PeripheralTask::start(hid, driver(model), model, product_id, changes, backups)
}

#[cfg(test)]
impl PeripheralTask {
    pub(crate) fn backups(&self) -> &std::path::Path {
        &self.backups
    }
    pub(crate) fn expire_undo(&mut self) {
        self.undo_at = Some(Instant::now());
    }
}

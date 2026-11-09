//! Logitech G300 (`046d:c246`). The G300 does not speak HID++: its three profiles are feature
//! reports of 35 bytes (0xF3, 0xF4, 0xF5) and the active profile is report 0xF0. The format is in
//! `docs/protocols/logitech-g300.md`, checked byte by byte against a real G300 and Logitech Gaming
//! Software in issue #16.
//!
//! PeriHub reads the G300 and saves its profiles: saving was confirmed on a real G300 in issue #21
//! (the changes took effect, stayed after the mouse was unplugged, and a save that was not kept
//! was undone). A save is backed up, read back, and undone unless the user keeps it. Only a task
//! given a [`FeatureDevice`] can send a report; with a [`FeatureSource`] it only reads.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::color::Rgb;
use crate::config;
use crate::error::{Error, Result};
use crate::worker::DeviceTask;

pub const PRODUCT_ID: u16 = 0xC246;
/// The interface with the vendor collection (usage page 0xFF80) that holds the reports.
pub const INTERFACE: i32 = 1;
pub const USAGE_PAGE: u16 = 0xFF80;

const REPORT_ACTIVE: u8 = 0xF0;
const REPORT_PROFILES: [u8; 3] = [0xF3, 0xF4, 0xF5];
const ACTIVE_LEN: usize = 4;
const PROFILE_LEN: usize = 35;
const BUTTONS: usize = 9;
const BUTTONS_AT: usize = 8;
const DPI_AT: usize = 3;
const DPI_LEVELS: usize = 4;
const SHIFT_AT: usize = 7;
/// DPI values are stored in steps of 250, from 1 (250 DPI) to 10 (2500 DPI).
const DPI_STEP: u16 = 250;
const DPI_MAX: u16 = 2500;
/// Byte 7 with this bit set: no shift DPI.
const SHIFT_OFF: u8 = 0x40;
const DEFAULT_LEVEL: u8 = 0x80;
/// Bits of a DPI byte whose meaning is unknown: 0 on every G300 seen.
const DPI_UNKNOWN_BITS: u8 = 0x70;
const COLOR_BITS: u8 = 0x07;
/// Report rate codes, in the order of their values.
const RATES_HZ: [u16; 4] = [1000, 125, 250, 500];
/// The mouse's buttons change the active profile and DPI level: read again this often.
const REFRESH: Duration = Duration::from_secs(2);
/// Writes are slow: the profile is read back only after this.
const WRITE_SETTLE: Duration = Duration::from_millis(500);
/// A profile that still reads the old bytes is read again this many times, this far apart.
const READ_BACK_TRIES: usize = 4;
const READ_BACK_WAIT: Duration = Duration::from_millis(250);
/// The interface counts down 15 seconds: the core undoes a save that was not kept a bit later.
const UNDO_AFTER: Duration = Duration::from_secs(20);
/// A save that was not kept is undone when the mouse is opened again, but not a day later.
const PENDING_UNDO_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Reads feature reports, nothing else.
pub trait FeatureSource: Send {
    /// Fills `buf` (report id in byte 0) and returns how many bytes came back.
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize>;
}

/// Reads and sends feature reports: what a task that saves profiles is given.
pub trait FeatureDevice: FeatureSource {
    /// Sends a feature report, report id in byte 0.
    fn send_feature(&self, data: &[u8]) -> Result<()>;
}

impl FeatureSource for hidapi::HidDevice {
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(self.get_feature_report(buf)?)
    }
}

impl FeatureDevice for hidapi::HidDevice {
    fn send_feature(&self, data: &[u8]) -> Result<()> {
        Ok(self.send_feature_report(data)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum G300Button {
    /// Nothing assigned.
    None,
    /// Mouse button 1 to 9 (1 left, 2 right, 3 middle, 4 back, 5 forward).
    Mouse {
        button: u8,
    },
    DpiUp,
    DpiDown,
    DpiCycle,
    ProfileCycle,
    /// Hold for the shift DPI.
    DpiShift,
    DpiDefault,
    /// A key with modifiers: bit 0 left Ctrl, 1 left Shift, 2 left Alt, 3 left Win, 4–7 the right
    /// ones. `key` is a USB keyboard usage, 0 for modifiers alone.
    Key {
        modifiers: u8,
        key: u8,
    },
    /// A code PeriHub does not know.
    Unknown {
        code: u8,
        modifiers: u8,
        key: u8,
    },
}

fn button(bytes: &[u8]) -> G300Button {
    let (code, modifiers, key) = (bytes[0], bytes[1], bytes[2]);
    match code {
        0x00 if modifiers == 0 && key == 0 => G300Button::None,
        0x00 => G300Button::Key { modifiers, key },
        0x01..=0x09 => G300Button::Mouse { button: code },
        0x0A => G300Button::DpiUp,
        0x0B => G300Button::DpiDown,
        0x0C => G300Button::DpiCycle,
        0x0D => G300Button::ProfileCycle,
        0x0E => G300Button::DpiShift,
        0x0F => G300Button::DpiDefault,
        _ => G300Button::Unknown { code, modifiers, key },
    }
}

/// The bytes of a button PeriHub may assign. Mouse buttons beyond 5 and unknown codes are only
/// kept as they are.
fn encode_button(b: &G300Button) -> Result<[u8; 3]> {
    let code = |c: u8| Ok([c, 0, 0]);
    match *b {
        G300Button::None => code(0x00),
        G300Button::Mouse { button: b @ 1..=5 } => code(b),
        G300Button::DpiUp => code(0x0A),
        G300Button::DpiDown => code(0x0B),
        G300Button::DpiCycle => code(0x0C),
        G300Button::ProfileCycle => code(0x0D),
        G300Button::DpiShift => code(0x0E),
        G300Button::DpiDefault => code(0x0F),
        G300Button::Key { modifiers, key } if (key == 0 && modifiers != 0) || (0x04..=0x65).contains(&key) => {
            Ok([0x00, modifiers, key])
        }
        _ => Err(Error::Invalid(format!("the G300 cannot do this: {b:?}"))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct G300Profile {
    /// 1 to 3.
    pub number: u8,
    /// The G300's light is on or off per channel: white, red, green, blue, yellow, cyan, magenta or off.
    pub color: Rgb,
    pub report_rate_hz: Option<u16>,
    pub dpi_levels: Vec<u16>,
    /// Index of the default DPI level.
    pub default_level: Option<u8>,
    /// DPI while the DPI shift button is held; `None` when the shift is off.
    pub shift_dpi: Option<u16>,
    /// Byte 7 as read: the shift DPI, and bit 0x40 when it is off.
    pub shift_byte: u8,
    pub buttons: Vec<G300Button>,
    /// The bytes as read, for test reports.
    pub raw: String,
}

/// What can be changed in a G300 profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G300Settings {
    /// Each channel fully on (255) or off (0).
    pub color: Rgb,
    pub report_rate_hz: u16,
    /// Four levels, 250 to 2500 DPI in steps of 250.
    pub dpi_levels: Vec<u16>,
    pub default_level: u8,
    pub shift_dpi: Option<u16>,
    /// Nine buttons. The left and right buttons keep what they have.
    pub buttons: Vec<G300Button>,
}

impl G300Profile {
    pub fn settings(&self) -> G300Settings {
        G300Settings {
            color: self.color,
            report_rate_hz: self.report_rate_hz.unwrap_or(1000),
            dpi_levels: self.dpi_levels.clone(),
            default_level: self.default_level.unwrap_or(0),
            shift_dpi: self.shift_dpi,
            buttons: self.buttons.clone(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn dpi_value(byte: u8) -> u16 {
    (byte & 0x7F) as u16 * DPI_STEP
}

fn check_profile_report(number: u8, r: &[u8]) -> Result<()> {
    let id = REPORT_PROFILES.get((number as usize).wrapping_sub(1));
    if r.len() < PROFILE_LEN || Some(&r[0]) != id {
        return Err(Error::Protocol(format!("unexpected profile report: {}", hex(r))));
    }
    Ok(())
}

pub fn parse_profile(number: u8, r: &[u8]) -> Result<G300Profile> {
    check_profile_report(number, r)?;
    let on = |bit: u8| if r[1] & bit != 0 { 255 } else { 0 };
    let levels = &r[DPI_AT..DPI_AT + DPI_LEVELS];
    let shift = r[SHIFT_AT];
    Ok(G300Profile {
        number,
        color: Rgb { r: on(0x01), g: on(0x02), b: on(0x04) },
        report_rate_hz: RATES_HZ.get(r[2] as usize).copied(),
        dpi_levels: levels.iter().map(|&l| dpi_value(l)).collect(),
        default_level: levels.iter().position(|l| l & DEFAULT_LEVEL != 0).map(|i| i as u8),
        shift_dpi: (shift & SHIFT_OFF == 0 && shift & 0x0F != 0).then(|| (shift & 0x0F) as u16 * DPI_STEP),
        shift_byte: shift,
        buttons: (0..BUTTONS).map(|i| button(&r[BUTTONS_AT + 3 * i..BUTTONS_AT + 3 * i + 3])).collect(),
        raw: hex(&r[..PROFILE_LEN]),
    })
}

fn dpi_code(dpi: u16) -> Result<u8> {
    if !(DPI_STEP..=DPI_MAX).contains(&dpi) || !dpi.is_multiple_of(DPI_STEP) {
        return Err(Error::Invalid(format!("{dpi} DPI: the G300 takes 250 to 2500 DPI in steps of 250")));
    }
    Ok((dpi / DPI_STEP) as u8)
}

/// The profile report with `settings` written over `original`. Only the edited fields change: the
/// unknown bits, and the buttons left as they are, keep their bytes.
pub fn encode_profile(number: u8, original: &[u8], settings: &G300Settings) -> Result<Vec<u8>> {
    check_profile_report(number, original)?;
    let current = parse_profile(number, original)?;
    let mut r = original[..PROFILE_LEN].to_vec();

    let bit = |channel: u8, mask: u8| match channel {
        0 => Ok(0),
        255 => Ok(mask),
        _ => Err(Error::Invalid("the G300's light is on or off per colour".into())),
    };
    let Rgb { r: red, g: green, b: blue } = settings.color;
    r[1] = (r[1] & !COLOR_BITS) | bit(red, 0x01)? | bit(green, 0x02)? | bit(blue, 0x04)?;

    let rate = RATES_HZ.iter().position(|&hz| hz == settings.report_rate_hz);
    r[2] = rate.ok_or_else(|| Error::Invalid(format!("report rate {} Hz", settings.report_rate_hz)))? as u8;

    if settings.dpi_levels.len() != DPI_LEVELS {
        return Err(Error::Invalid("the G300 has four DPI levels".into()));
    }
    if settings.default_level as usize >= DPI_LEVELS {
        return Err(Error::Invalid("default DPI level does not exist".into()));
    }
    for (i, &dpi) in settings.dpi_levels.iter().enumerate() {
        let byte = &mut r[DPI_AT + i];
        if *byte & DPI_UNKNOWN_BITS != 0 {
            return Err(Error::Invalid(format!("DPI level {} holds bits PeriHub does not know", i + 1)));
        }
        let default = if i == settings.default_level as usize { DEFAULT_LEVEL } else { 0 };
        *byte = default | dpi_code(dpi)?;
    }

    // Turning the shift off keeps its DPI, as Logitech Gaming Software does.
    r[SHIFT_AT] = match settings.shift_dpi {
        _ if settings.shift_dpi == current.shift_dpi => r[SHIFT_AT],
        None => r[SHIFT_AT] | SHIFT_OFF,
        Some(dpi) => (r[SHIFT_AT] & !(SHIFT_OFF | 0x0F)) | dpi_code(dpi)?,
    };

    if settings.buttons.len() != BUTTONS {
        return Err(Error::Invalid("the G300 has nine buttons".into()));
    }
    for (i, (new, old)) in settings.buttons.iter().zip(&current.buttons).enumerate() {
        if new == old {
            continue;
        }
        if i < 2 {
            return Err(Error::Invalid("the left and right buttons cannot be changed".into()));
        }
        r[BUTTONS_AT + 3 * i..BUTTONS_AT + 3 * i + 3].copy_from_slice(&encode_button(new)?);
    }
    Ok(r)
}

/// Active profile (1 to 3) and DPI level (index) from report 0xF0.
pub fn parse_active(r: &[u8]) -> Result<(u8, u8)> {
    if r.len() < ACTIVE_LEN || r[0] != REPORT_ACTIVE {
        return Err(Error::Protocol(format!("unexpected active profile report: {}", hex(r))));
    }
    Ok(((r[1] >> 4) + 1, (r[1] >> 1) & 0x07))
}

#[derive(Debug, Clone, Serialize)]
pub struct G300Info {
    pub product_id: u16,
    pub model: String,
    pub active_profile: u8,
    pub active_level: u8,
    pub profiles: Vec<G300Profile>,
    /// Report 0xF0 as read, for test reports.
    pub active_raw: String,
    /// The profiles can be saved (always, unless the task was opened read-only).
    pub editable: bool,
    /// A save was neither kept nor undone (PeriHub closed or the mouse unplugged during the
    /// countdown), and this connection is read-only, so it could not be undone.
    pub pending_undo: bool,
}

/// The connection to the mouse: only the second kind can send reports.
enum Connection {
    Read(Box<dyn FeatureSource>),
    Write(Box<dyn FeatureDevice>),
}

impl Connection {
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        match self {
            Connection::Read(dev) => dev.get_feature(buf),
            Connection::Write(dev) => dev.get_feature(buf),
        }
    }
}

pub struct G300Task {
    dev: Connection,
    pub info: G300Info,
    last_refresh: Instant,
    /// The G300's backup folder.
    backups: PathBuf,
    /// Backup taken before the last save, to undo it.
    last_backup: Option<String>,
    /// When the last save is undone, unless it is kept.
    undo_at: Option<Instant>,
    settle: Duration,
    read_back_wait: Duration,
}

fn profile_file(number: u8) -> String {
    format!("profile_{number}.bin")
}

impl G300Task {
    /// The profiles can be read and saved.
    pub fn open(dev: impl FeatureDevice + 'static) -> Result<Self> {
        Self::start(Connection::Write(Box::new(dev)), default_backups())
    }

    /// Read only: nothing can be sent to the mouse.
    pub fn open_read_only(dev: impl FeatureSource + 'static) -> Result<Self> {
        Self::start(Connection::Read(Box::new(dev)), default_backups())
    }

    fn start(dev: Connection, backups: PathBuf) -> Result<Self> {
        let editable = matches!(dev, Connection::Write(_));
        let mut task = Self {
            dev,
            info: G300Info {
                product_id: PRODUCT_ID,
                model: "G300".into(),
                active_profile: 1,
                active_level: 0,
                profiles: Vec::new(),
                active_raw: String::new(),
                editable,
                pending_undo: false,
            },
            last_refresh: Instant::now(),
            backups,
            last_backup: None,
            undo_at: None,
            settle: WRITE_SETTLE,
            read_back_wait: READ_BACK_WAIT,
        };
        for number in 1..=3 {
            let bytes = task.read_profile(number)?;
            task.info.profiles.push(parse_profile(number, &bytes)?);
        }
        task.read_active()?;
        log::info!(
            "Logitech G300: profile {} active, {}, profiles {}",
            task.info.active_profile,
            if editable { "can be saved to" } else { "read-only" },
            task.info.profiles.iter().map(|p| p.raw.clone()).collect::<Vec<_>>().join(" | ")
        );
        task.finish_pending_undo();
        Ok(task)
    }

    fn read_profile(&self, number: u8) -> Result<Vec<u8>> {
        let mut buf = [0u8; PROFILE_LEN];
        buf[0] = REPORT_PROFILES[number as usize - 1];
        let n = self.dev.get_feature(&mut buf)?;
        check_profile_report(number, &buf[..n])?;
        Ok(buf[..PROFILE_LEN].to_vec())
    }

    fn read_active(&mut self) -> Result<()> {
        let mut buf = [0u8; ACTIVE_LEN];
        buf[0] = REPORT_ACTIVE;
        let n = self.dev.get_feature(&mut buf)?;
        let (profile, level) = parse_active(&buf[..n])?;
        self.info.active_profile = profile;
        self.info.active_level = level;
        self.info.active_raw = hex(&buf[..n]);
        Ok(())
    }

    /// What the mouse shows now: the active profile and level change with its buttons.
    pub fn info(&mut self) -> Result<G300Info> {
        self.read_active()?;
        Ok(self.info.clone())
    }

    fn check_number(number: u8) -> Result<()> {
        if (1..=3).contains(&number) { Ok(()) } else { Err(Error::Invalid(format!("profile {number}"))) }
    }

    /// Saves a profile: backup first, then the report, read back and compared. Undone unless kept
    /// (see [`Self::keep_changes`]).
    pub fn save(&mut self, number: u8, settings: &G300Settings) -> Result<G300Info> {
        Self::check_number(number)?;
        if !self.info.editable {
            return Err(Error::Invalid("this connection to the G300 is read-only".into()));
        }
        let current = self.read_profile(number)?;
        let new = encode_profile(number, &current, settings)?;
        if new == current {
            return self.info();
        }
        let id = self.backup(&format!("profile{number}"))?;
        std::fs::write(self.pending_undo_path(), &id)?;
        self.last_backup = Some(id);
        self.undo_at = Some(Instant::now() + UNDO_AFTER);
        log::info!("G300 profile {number}: writing {}", hex(&new));
        if let Err(error) = self.write_verified(number, &new) {
            return Err(match self.undo_last_write() {
                Ok(_) => Error::Protocol(format!("{error}. The backup was put back.")),
                Err(undo) => Error::Protocol(format!(
                    "{error}. Putting the backup back failed too ({undo}): unplug the mouse and plug it in again."
                )),
            });
        }
        self.info.profiles[number as usize - 1] = parse_profile(number, &new)?;
        self.info()
    }

    /// Sends a profile, then reads it back until it matches. The mouse needs time to store it.
    fn write_verified(&mut self, number: u8, data: &[u8]) -> Result<()> {
        let Connection::Write(dev) = &self.dev else {
            return Err(Error::Invalid("this connection to the G300 is read-only".into()));
        };
        dev.send_feature(data)?;
        std::thread::sleep(self.settle);
        let mut back = Vec::new();
        for attempt in 0..READ_BACK_TRIES {
            if attempt > 0 {
                std::thread::sleep(self.read_back_wait);
            }
            back = self.read_profile(number)?;
            if back == data {
                return Ok(());
            }
        }
        Err(Error::Protocol(format!("profile {number} reads back {} instead of what was written", hex(&back))))
    }

    /// Saves the three profiles as they are now, and returns the backup's id.
    fn backup(&self, label: &str) -> Result<String> {
        let profiles = (1..=3).map(|n| self.read_profile(n)).collect::<Result<Vec<_>>>()?;
        let names: Vec<String> = (1..=3).map(profile_file).collect();
        let files: Vec<(&str, &[u8])> =
            names.iter().map(String::as_str).zip(profiles.iter().map(Vec::as_slice)).collect();
        let dir = config::write_backup_in(&self.backups, label, &files)?;
        // The folder name only: the full path would put the user's name in the log file.
        let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        log::info!("G300 backup saved: {id}");
        Ok(id)
    }

    fn pending_undo_path(&self) -> PathBuf {
        self.backups.join("pending-undo")
    }

    /// The user checked the mouse after a save and keeps the changes.
    pub fn keep_changes(&mut self) {
        self.undo_at = None;
        let _ = std::fs::remove_file(self.pending_undo_path());
    }

    /// Puts back the profiles saved before the last save. Only profiles that differ are written,
    /// each one read back.
    pub fn undo_last_write(&mut self) -> Result<G300Info> {
        self.undo_at = None;
        let id = self.last_backup.clone().ok_or_else(|| Error::Invalid("nothing to undo".into()))?;
        let mut backup = Vec::new();
        for number in 1..=3 {
            let data = config::read_backup_file_in(&self.backups, &id, &profile_file(number))?;
            check_profile_report(number, &data)?;
            if data.len() != PROFILE_LEN {
                return Err(Error::Invalid(format!("backup {id}: profile {number} has {} bytes", data.len())));
            }
            backup.push(data);
        }
        for number in 1..=3u8 {
            let saved = &backup[number as usize - 1];
            if self.read_profile(number)? != *saved {
                log::info!("G300 profile {number}: putting back {}", hex(saved));
                self.write_verified(number, saved)?;
            }
            self.info.profiles[number as usize - 1] = parse_profile(number, saved)?;
        }
        self.last_backup = None;
        let _ = std::fs::remove_file(self.pending_undo_path());
        log::info!("G300: last save undone, backup {id} put back");
        self.info()
    }

    /// When the mouse is opened: a save that was neither kept nor undone is undone now. A day
    /// later the backup is too old to put back blindly.
    fn finish_pending_undo(&mut self) {
        let path = self.pending_undo_path();
        let Ok(id) = std::fs::read_to_string(&path) else { return };
        let id = id.trim().to_string();
        let stamp = id.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if now.saturating_sub(stamp) > PENDING_UNDO_MAX_AGE.as_secs() {
            log::warn!("a G300 save that was not kept is too old to undo now: backup {id} left alone");
            let _ = std::fs::remove_file(&path);
            return;
        }
        if !self.info.editable {
            self.info.pending_undo = true;
            return;
        }
        log::warn!("a G300 save was not kept before PeriHub closed or the mouse was unplugged: undoing it");
        self.last_backup = Some(id);
        if let Err(e) = self.undo_last_write() {
            log::warn!("G300 undo failed: {e}");
        }
    }
}

fn default_backups() -> PathBuf {
    config::config_dir().join("backups").join("g300")
}

impl DeviceTask for G300Task {
    fn tick(&mut self) -> Result<Duration> {
        if self.undo_at.is_some_and(|at| Instant::now() >= at) {
            log::warn!("a G300 save was not kept: undoing it");
            if let Err(e) = self.undo_last_write() {
                log::warn!("G300 undo failed: {e}");
            }
        }
        if self.last_refresh.elapsed() >= REFRESH {
            self.last_refresh = Instant::now();
            // A mouse that stops answering is gone: the manager opens it again when it is back.
            self.read_active()?;
        }
        Ok(match self.undo_at {
            Some(at) => REFRESH.min(at.saturating_duration_since(Instant::now())),
            None => REFRESH,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// The reports of the G300 in issue #16, read with hidapitester on 2026-11-07.
    const ACTIVE: [u8; 4] = [0xF0, 0x02, 0x00, 0x00];
    const P1: &str =
        "F3 07 00 02 84 06 0A 40 01 00 00 02 00 00 03 00 00 00 03 2B 00 01 2B 00 01 19 00 01 06 0D 00 00 0F 00 00";
    const P2: &str =
        "F4 01 00 01 84 08 0A 04 01 00 00 02 00 00 03 00 00 00 00 0E 00 00 08 00 00 13 00 00 12 0D 00 00 0E 00 00";
    const P3: &str =
        "F5 02 03 84 04 04 04 40 01 00 00 02 00 00 03 00 00 04 00 00 05 00 00 00 00 2C 00 01 00 0D 00 00 0C 00 00";

    fn bytes(s: &str) -> Vec<u8> {
        s.split(' ').map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn key(modifiers: u8, key: u8) -> G300Button {
        G300Button::Key { modifiers, key }
    }

    #[test]
    fn the_g300_of_issue_16_reads_as_logitech_gaming_software_shows_it() {
        let p1 = parse_profile(1, &bytes(P1)).unwrap();
        assert_eq!(p1.color, Rgb { r: 255, g: 255, b: 255 });
        assert_eq!(p1.report_rate_hz, Some(1000));
        assert_eq!(p1.dpi_levels, [500, 1000, 1500, 2500]);
        assert_eq!(p1.default_level, Some(1));
        assert_eq!(p1.shift_dpi, None);
        // G4 Ctrl+Shift+Tab, G5 Ctrl+Tab, G6 Ctrl+V, G7 Ctrl+C, G8 next profile, G9 default DPI.
        assert_eq!(
            p1.buttons[3..],
            [
                key(0x03, 0x2B),
                key(0x01, 0x2B),
                key(0x01, 0x19),
                key(0x01, 0x06),
                G300Button::ProfileCycle,
                G300Button::DpiDefault
            ]
        );

        // The screenshot of profile 2 in the issue: red, 250/1000/2000/2500 with 1000 the default,
        // 1000 Hz, G4 K, G5 E, G6 P, G7 O, G9 DPI shift.
        let p2 = parse_profile(2, &bytes(P2)).unwrap();
        assert_eq!(p2.color, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(p2.report_rate_hz, Some(1000));
        assert_eq!(p2.dpi_levels, [250, 1000, 2000, 2500]);
        assert_eq!(p2.default_level, Some(1));
        assert_eq!(p2.buttons[3..7], [key(0, 0x0E), key(0, 0x08), key(0, 0x13), key(0, 0x12)]);
        assert_eq!(p2.buttons[8], G300Button::DpiShift);
        // Byte 7 is 0x04: shift DPI 1000, the profile whose G9 is DPI shift.
        assert_eq!(p2.shift_dpi, Some(1000));

        // Profile 3: green, 500 Hz, G4 back, G5 forward, G6 space, G7 Ctrl alone, G9 DPI cycle.
        let p3 = parse_profile(3, &bytes(P3)).unwrap();
        assert_eq!(p3.color, Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(p3.report_rate_hz, Some(500));
        assert_eq!(p3.buttons[3..5], [G300Button::Mouse { button: 4 }, G300Button::Mouse { button: 5 }]);
        assert_eq!(p3.buttons[5], key(0, 0x2C));
        assert_eq!(p3.buttons[6], key(0x01, 0));
        assert_eq!(p3.buttons[8], G300Button::DpiCycle);
        assert_eq!(p3.shift_dpi, None);

        assert_eq!(parse_active(&ACTIVE).unwrap(), (1, 1));
    }

    #[test]
    fn reports_of_another_kind_are_refused() {
        assert!(parse_profile(1, &bytes(P2)).is_err(), "report F4 is profile 2");
        assert!(parse_profile(1, &bytes(P1)[..20]).is_err(), "too short");
        assert!(parse_active(&[0xF1, 0, 0, 0]).is_err());
    }

    #[test]
    fn opening_reads_the_three_profiles_and_the_active_one() {
        let mut task = G300Task::start(Connection::Read(Box::new(Fake::new())), temp_backups("open")).unwrap();
        let info = task.info().unwrap();
        assert_eq!(info.profiles.len(), 3);
        assert_eq!(info.active_profile, 1);
        assert_eq!(info.active_level, 1);
    }

    /// A G300 that answers with the reports of issue #16, keeps what is written to it, and
    /// records every report sent.
    #[derive(Clone)]
    struct Fake(Arc<Mutex<FakeState>>);

    struct FakeState {
        profiles: [Vec<u8>; 3],
        sent: Vec<Vec<u8>>,
        /// Reads that still return the old bytes after a write.
        slow_reads: usize,
        /// Writes are accepted but not stored.
        forgetful: bool,
    }

    impl Fake {
        fn new() -> Self {
            Fake(Arc::new(Mutex::new(FakeState {
                profiles: [bytes(P1), bytes(P2), bytes(P3)],
                sent: Vec::new(),
                slow_reads: 0,
                forgetful: false,
            })))
        }

        fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
            self.0.lock().unwrap()
        }
    }

    impl FeatureSource for Fake {
        fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
            let mut state = self.state();
            let data = match buf[0] {
                0xF0 => ACTIVE.to_vec(),
                id @ 0xF3..=0xF5 => {
                    let i = (id - 0xF3) as usize;
                    match state.sent.iter().rev().find(|r| r[0] == id) {
                        Some(_) if state.slow_reads > 0 => {
                            state.slow_reads -= 1;
                            [bytes(P1), bytes(P2), bytes(P3)][i].clone()
                        }
                        _ => state.profiles[i].clone(),
                    }
                }
                _ => return Err(Error::Invalid("no such report".into())),
            };
            buf[..data.len()].copy_from_slice(&data);
            Ok(data.len())
        }
    }

    impl FeatureDevice for Fake {
        fn send_feature(&self, data: &[u8]) -> Result<()> {
            let mut state = self.state();
            state.sent.push(data.to_vec());
            if !state.forgetful {
                state.profiles[(data[0] - 0xF3) as usize] = data.to_vec();
            }
            Ok(())
        }
    }

    fn temp_backups(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-g300-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// Removes a test's backup folder, so that test runs leave nothing in the temporary folder.
    fn forget(backups: &std::path::Path) {
        let _ = std::fs::remove_dir_all(backups);
    }

    fn editable(fake: &Fake, name: &str) -> G300Task {
        let mut task = G300Task::start(Connection::Write(Box::new(fake.clone())), temp_backups(name)).unwrap();
        task.settle = Duration::ZERO;
        task.read_back_wait = Duration::ZERO;
        task
    }

    fn settings(number: u8) -> G300Settings {
        let raw = [P1, P2, P3][number as usize - 1];
        parse_profile(number, &bytes(raw)).unwrap().settings()
    }

    #[test]
    fn unchanged_settings_encode_to_the_same_bytes() {
        for (number, raw) in [(1, P1), (2, P2), (3, P3)] {
            assert_eq!(encode_profile(number, &bytes(raw), &settings(number)).unwrap(), bytes(raw), "profile {number}");
        }
    }

    #[test]
    fn only_the_edited_fields_change() {
        let mut s = settings(1);
        s.color = Rgb { r: 255, g: 0, b: 0 };
        s.report_rate_hz = 500;
        s.dpi_levels = vec![250, 750, 1500, 2500];
        s.default_level = 3;
        s.shift_dpi = Some(1500);
        s.buttons[3] = G300Button::Mouse { button: 4 };
        s.buttons[8] = G300Button::DpiShift;
        let mut expected = bytes(P1);
        expected[1..8].copy_from_slice(&[0x01, 0x03, 0x01, 0x03, 0x06, 0x8A, 0x06]);
        expected[17..20].copy_from_slice(&[0x04, 0x00, 0x00]);
        expected[32..35].copy_from_slice(&[0x0E, 0x00, 0x00]);
        assert_eq!(encode_profile(1, &bytes(P1), &s).unwrap(), expected);

        // Turning the shift off keeps its DPI in the low bits, as LGS does.
        let mut s = settings(2);
        s.shift_dpi = None;
        assert_eq!(encode_profile(2, &bytes(P2), &s).unwrap()[7], 0x44);
        // A key with modifiers.
        let mut s = settings(3);
        s.buttons[6] = G300Button::Key { modifiers: 0x05, key: 0x3A };
        assert_eq!(encode_profile(3, &bytes(P3), &s).unwrap()[26..29], [0x00, 0x05, 0x3A]);
    }

    #[test]
    fn values_the_g300_cannot_take_are_refused() {
        let refused = |change: fn(&mut G300Settings)| {
            let mut s = settings(1);
            change(&mut s);
            encode_profile(1, &bytes(P1), &s).is_err()
        };
        assert!(refused(|s| s.buttons[0] = G300Button::Mouse { button: 3 }), "left button");
        assert!(refused(|s| s.buttons[1] = G300Button::None), "right button");
        assert!(refused(|s| s.dpi_levels[0] = 300), "not a step of 250");
        assert!(refused(|s| s.dpi_levels[0] = 2750), "above 2500");
        assert!(refused(|s| s.dpi_levels[0] = 0), "zero");
        assert!(refused(|s| s.dpi_levels.truncate(3)), "three levels");
        assert!(refused(|s| s.default_level = 4), "default level");
        assert!(refused(|s| s.shift_dpi = Some(100)), "shift DPI");
        assert!(refused(|s| s.color = Rgb { r: 128, g: 0, b: 0 }), "half red");
        assert!(refused(|s| s.report_rate_hz = 333), "report rate");
        assert!(refused(|s| s.buttons[4] = G300Button::Mouse { button: 6 }), "mouse button 6");
        assert!(refused(|s| s.buttons[4] = G300Button::Unknown { code: 0x20, modifiers: 0, key: 0 }), "unknown");
        assert!(refused(|s| s.buttons[4] = G300Button::Key { modifiers: 0, key: 0x66 }), "unknown key");
        assert!(refused(|s| s.buttons.truncate(8)), "eight buttons");
        assert!(encode_profile(1, &bytes(P2), &settings(1)).is_err(), "report of another profile");
        let mut odd = bytes(P1);
        odd[3] |= 0x10;
        assert!(encode_profile(1, &odd, &settings(1)).is_err(), "unknown bits in a DPI level");
    }

    #[test]
    fn a_read_only_g300_cannot_save() {
        let fake = Fake::new();
        let mut task = G300Task::start(Connection::Read(Box::new(fake.clone())), temp_backups("ro")).unwrap();
        let mut s = settings(1);
        s.report_rate_hz = 125;
        assert!(task.save(1, &s).is_err());
        assert!(fake.state().sent.is_empty());
    }

    #[test]
    fn a_save_is_backed_up_written_once_and_read_back() {
        let fake = Fake::new();
        fake.state().slow_reads = 2;
        let mut task = editable(&fake, "save");
        let mut s = settings(2);
        s.color = Rgb { r: 0, g: 0, b: 255 };
        let info = task.save(2, &s).unwrap();
        let written = encode_profile(2, &bytes(P2), &s).unwrap();
        assert_eq!(fake.state().sent, std::slice::from_ref(&written));
        assert_eq!(info.profiles[1].color, Rgb { r: 0, g: 0, b: 255 });

        let id = task.last_backup.clone().unwrap();
        for (n, raw) in [(1, P1), (2, P2), (3, P3)] {
            assert_eq!(config::read_backup_file_in(&task.backups, &id, &profile_file(n)).unwrap(), bytes(raw));
        }
        assert!(task.pending_undo_path().exists());
        task.keep_changes();
        assert!(!task.pending_undo_path().exists());
        assert!(task.undo_at.is_none());

        // The same settings again: nothing is sent.
        task.save(2, &s).unwrap();
        assert_eq!(fake.state().sent.len(), 1);
        forget(&task.backups);
    }

    #[test]
    fn a_save_that_is_not_kept_is_undone() {
        let fake = Fake::new();
        let mut task = editable(&fake, "undo");
        let mut s = settings(3);
        s.dpi_levels = vec![1000, 1250, 1500, 1750];
        task.save(3, &s).unwrap();
        task.undo_at = Some(Instant::now());
        task.tick().unwrap();
        assert_eq!(fake.state().profiles[2], bytes(P3));
        assert_eq!(fake.state().sent.len(), 2);
        assert!(!task.pending_undo_path().exists());
        assert_eq!(task.info.profiles[2].dpi_levels, [1000, 1000, 1000, 1000]);
        forget(&task.backups);
    }

    #[test]
    fn a_write_the_mouse_does_not_keep_is_reported_and_nothing_else_is_sent() {
        let fake = Fake::new();
        fake.state().forgetful = true;
        let mut task = editable(&fake, "forgetful");
        let mut s = settings(1);
        s.report_rate_hz = 250;
        let error = task.save(1, &s).unwrap_err().to_string();
        assert!(error.contains("reads back"), "{error}");
        assert!(error.contains("backup was put back"), "{error}");
        // The mouse still holds the old profile: the undo has nothing to write.
        assert_eq!(fake.state().sent.len(), 1);
        assert!(!task.pending_undo_path().exists());
        forget(&task.backups);
    }

    #[test]
    fn a_save_left_pending_is_undone_when_editing_is_on_again() {
        let fake = Fake::new();
        let name = "pending";
        let mut task = editable(&fake, name);
        let mut s = settings(1);
        s.color = Rgb { r: 0, g: 255, b: 0 };
        task.save(1, &s).unwrap();
        // PeriHub closed during the countdown.
        let backups = task.backups.clone();
        drop(task);
        assert!(backups.join("pending-undo").exists());
        let read_only = Fake(fake.0.clone());
        let task = G300Task::start(Connection::Read(Box::new(read_only)), backups.clone()).unwrap();
        assert!(task.info.pending_undo);
        assert_eq!(fake.state().sent.len(), 1, "read-only: nothing sent");

        let mut task = G300Task::start(Connection::Write(Box::new(fake.clone())), backups.clone()).unwrap();
        task.settle = Duration::ZERO;
        assert_eq!(fake.state().profiles[0], bytes(P1));
        assert!(!task.info.pending_undo);
        assert!(!backups.join("pending-undo").exists());
        assert!(task.info().unwrap().profiles[0].color == Rgb { r: 255, g: 255, b: 255 });
        forget(&backups);
    }
}

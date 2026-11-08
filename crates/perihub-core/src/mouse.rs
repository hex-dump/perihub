//! Logitech mouse state and operations, run on the device's dedicated thread.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{self, BackupEntry, write_backup};
use crate::conflicts;
use crate::error::{Error, HidppErrorCode, Result};
use crate::hidpp::macros::{self, Macro, Packer};
use crate::hidpp::models::MouseModel;
use crate::hidpp::profile::{self, ButtonAction, LedEffect, ProfileSettings};
use crate::hidpp::{Battery, DpiCapabilities, FirmwareEntity, HidppDevice, LedZone, OnboardInfo, ROM_SECTOR_BASE};
use crate::worker::DeviceTask;

/// Profile formats verified byte by byte on a real device. Others can be read
/// but not written.
const WRITABLE_PROFILE_FORMATS: &[u8] = &[2];
const ONBOARD_MODE: u8 = 1;
const HOST_MODE: u8 = 2;
/// How often the mouse is checked for another program having taken control of it.
const MODE_CHECK: Duration = Duration::from_secs(5);
/// After a save to an untested model, the backup goes back by itself unless the change is kept
/// (see [`MouseTask::keep_changes`]). The interface counts down 15 seconds and undoes it itself;
/// this is the fallback if the window was closed or stopped answering.
const UNDO_AFTER: Duration = Duration::from_secs(20);
/// A save that was not kept is undone when the mouse is opened again, but not a day later.
const PENDING_UNDO_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct MouseInfo {
    pub product_id: u16,
    /// Model name from PeriHub's list, e.g. "G502 HERO".
    pub model: String,
    /// Name reported by the mouse itself.
    pub name: String,
    /// Tested with PeriHub: unverified models are read-only.
    pub verified: bool,
    /// Untested model that the user chose to edit anyway, and that meets every condition for it
    /// (see [`experimental_allowed`]).
    pub experimental: bool,
    /// A save to this untested mouse was neither kept nor undone (PeriHub was closed, or the mouse
    /// unplugged, during the countdown), and the mouse is read-only now: turning experimental
    /// editing on again puts the backup back.
    pub pending_undo: bool,
    /// Connected through a LIGHTSPEED receiver instead of its cable.
    pub receiver: bool,
    /// Which receiver and slot, for test reports.
    pub link: Option<ReceiverLink>,
    /// Another program left the mouse under software control, so its onboard profiles are not in use.
    pub host_mode: bool,
    pub firmware: Vec<FirmwareEntity>,
    pub dpi: DpiCapabilities,
    pub report_rates_ms: Vec<u8>,
    pub led_zones: Vec<LedZone>,
    pub onboard: OnboardInfo,
    pub writable: bool,
}

/// Where a mouse behind a LIGHTSPEED receiver is.
#[derive(Debug, Clone, Serialize)]
pub struct ReceiverLink {
    /// USB product id of the receiver.
    pub receiver_id: u16,
    pub slot: u8,
    /// Id the mouse reports through the receiver.
    pub wireless_id: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct MouseProfile {
    pub number: u8,
    pub enabled: bool,
    pub settings: ProfileSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct MouseState {
    pub active_profile: u8,
    pub active_dpi_index: u8,
    pub current_dpi: u16,
    pub profiles: Vec<MouseProfile>,
    /// Bytes free for new macros: the sectors that no profile and no macro use.
    pub macro_space: u32,
}

/// What changes without PeriHub: the active profile and DPI, which the mouse's own buttons
/// change, and the battery of a wireless mouse.
#[derive(Debug, Clone, Serialize)]
pub struct MouseLive {
    pub active_profile: u8,
    pub active_dpi_index: u8,
    pub current_dpi: u16,
    pub battery: Option<Battery>,
}

pub struct MouseTask {
    dev: HidppDevice,
    model: &'static MouseModel,
    pub info: MouseInfo,
    /// Another program controls the mouse now (shared with the manager, for the status).
    host_mode: Arc<AtomicBool>,
    last_mode_check: Instant,
    /// Last valid active profile, restored if the mouse comes back without one.
    last_profile: u8,
    /// A DPI or LED preview is shown instead of the saved settings.
    previewing: bool,
    /// Backup taken before the last write, to undo it (see [`MouseTask::undo_last_write`]).
    last_backup: Option<String>,
    /// When the last save to an untested model is undone, unless it is kept.
    undo_at: Option<Instant>,
    /// Set before the manager opens the mouse again: the thread stops at its next tick.
    closed: bool,
}

/// Whether an untested model may be edited after the user turned experimental editing on. Only
/// a mouse whose memory looks like the G502 HERO's, the one PeriHub was verified on: the same
/// profile format and sector size. And only with its cable: a write over the radio that stops
/// halfway has never been tried.
pub fn experimental_allowed(enabled: bool, onboard: &OnboardInfo, receiver: bool) -> bool {
    enabled
        && !receiver
        && WRITABLE_PROFILE_FORMATS.contains(&onboard.profile_format)
        && onboard.sector_size as usize == profile::WRITE_SECTOR_SIZE
}

/// Whether `new` changes an LED slot of the profile that is not one of the mouse's LED zones.
fn unknown_led_slots_changed(current: &ProfileSettings, new: &ProfileSettings, zones: &[u8]) -> bool {
    (0..profile::LED_SLOTS).any(|i| !zones.contains(&(i as u8)) && new.leds.get(i) != current.leds.get(i))
}

/// What to do after reading the mouse's mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeAction {
    /// The onboard profiles are in use.
    Fine,
    /// Software control, left alone: the interface tells the user.
    Report,
    /// Software control that nobody uses any more (e.g. after a wake-up): back to the onboard profiles.
    SwitchBack,
}

/// Untested models are only read, and a program that is running (G HUB) takes the mouse again
/// at once, so only a verified mouse that nothing else controls is switched back.
fn mode_action(mode: u8, verified: bool, conflict_running: bool) -> ModeAction {
    if mode == ONBOARD_MODE {
        ModeAction::Fine
    } else if verified && !conflict_running {
        ModeAction::SwitchBack
    } else {
        ModeAction::Report
    }
}

/// How to go back to the saved settings of the active profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reload {
    /// Switch to this other profile and back. Selecting the same profile again keeps a preview
    /// (verified on the G502 HERO).
    Through(u8),
    /// Host mode, then onboard mode again: drops a DPI preview on the G502 HERO and keeps the
    /// profile and DPI level (verified on 2026-11-05). For mice with a single profile, such as
    /// the G403 HERO, where there is no other profile to go through.
    ModeSwitch,
}

/// Mice with more profiles keep the way verified since 0.1.0. Better to go through an enabled
/// profile: the behavior with disabled ones is not verified.
/// The profile to switch to after a restore, if the backup disabled the active one. With no active
/// profile (0, another program controls the mouse) the mouse is left as it is.
fn profile_after_restore(active: u8, enabled: &[bool]) -> Option<u8> {
    if active == 0 {
        return None;
    }
    let active_enabled = enabled.get((active as usize).wrapping_sub(1)).copied().unwrap_or(false);
    if active_enabled {
        return None;
    }
    enabled.iter().position(|&on| on).map(|i| i as u8 + 1)
}

fn reload_method(active: u8, enabled: &[bool]) -> Reload {
    let others = || (1..=enabled.len() as u8).filter(|&n| n != active);
    match others().find(|&n| enabled[n as usize - 1]).or_else(|| others().next()) {
        Some(other) => Reload::Through(other),
        None => Reload::ModeSwitch,
    }
}

/// Features that some mice lack (no LEDs, a different polling rate feature) leave an empty list.
fn missing_as_empty<T>(result: Result<Vec<T>>) -> Result<Vec<T>> {
    match result {
        Err(Error::FeatureMissing(_)) => Ok(Vec::new()),
        other => other,
    }
}

impl MouseTask {
    /// `experimental`: the user turned experimental editing on for this untested model. It is used
    /// only if the mouse meets [`experimental_allowed`].
    pub fn open(mut dev: HidppDevice, model: &'static MouseModel, experimental: bool) -> Result<Self> {
        let onboard = dev.onboard_info()?;
        if onboard.memory_model != 1 || onboard.macro_format != 1 {
            return Err(Error::Protocol("onboard profile memory uses an unknown format".into()));
        }
        let experimental = !model.verified && experimental_allowed(experimental, &onboard, dev.is_behind_receiver());
        let info = MouseInfo {
            product_id: model.product_id,
            model: model.name.into(),
            name: dev.device_name()?,
            verified: model.verified,
            experimental,
            pending_undo: false,
            receiver: dev.is_behind_receiver(),
            link: None,
            host_mode: false,
            firmware: dev.firmware()?,
            dpi: dev.dpi_capabilities(0)?,
            report_rates_ms: missing_as_empty(dev.report_rates())?,
            led_zones: missing_as_empty(dev.led_zones())?,
            writable: (model.verified || experimental) && WRITABLE_PROFILE_FORMATS.contains(&onboard.profile_format),
            onboard,
        };
        let mut task = Self {
            dev,
            model,
            info,
            host_mode: Arc::new(AtomicBool::new(false)),
            last_mode_check: Instant::now(),
            last_profile: 1,
            previewing: false,
            last_backup: None,
            undo_at: None,
            closed: false,
        };
        task.check_mode()?;
        task.info.host_mode = task.host_mode.load(Ordering::SeqCst);
        task.finish_pending_undo();
        let active = task.dev.active_profile()?;
        if (1..=task.profile_count()).contains(&active) {
            task.last_profile = active;
        }
        Ok(task)
    }

    /// Set while another program controls the mouse; read by the manager for the status.
    pub fn host_mode_flag(&self) -> Arc<AtomicBool> {
        self.host_mode.clone()
    }

    /// Reads the mouse's mode and acts on it (see [`mode_action`]). G HUB, for example, takes the
    /// mouse when it starts and after every wake-up.
    fn check_mode(&mut self) -> Result<()> {
        self.last_mode_check = Instant::now();
        let mode = self.dev.onboard_mode()?;
        // Processes are listed only when the mouse is not in its onboard mode.
        let conflict_running = mode != ONBOARD_MODE && !conflicts::running().is_empty();
        let host_mode = match mode_action(mode, self.model.verified, conflict_running) {
            ModeAction::Fine => false,
            ModeAction::Report => true,
            ModeAction::SwitchBack => {
                log::info!("the mouse was left under software control: switching back to its onboard profiles");
                self.dev.set_onboard_mode(ONBOARD_MODE)?;
                // After a wake-up the G502 HERO came back with no active profile (0).
                let active = self.dev.active_profile()?;
                if !(1..=self.profile_count()).contains(&active) {
                    self.dev.set_active_profile(self.last_profile)?;
                }
                false
            }
        };
        if self.host_mode.swap(host_mode, Ordering::SeqCst) != host_mode {
            log::info!("mouse under the control of another program: {host_mode}");
        }
        Ok(())
    }

    fn sector_size(&self) -> u16 {
        self.info.onboard.sector_size
    }

    fn profile_count(&self) -> u8 {
        self.info.onboard.profile_count
    }

    fn check_number(&self, number: u8) -> Result<()> {
        if (1..=self.profile_count()).contains(&number) {
            Ok(())
        } else {
            Err(Error::Invalid(format!("profile {number}")))
        }
    }

    fn check_writable(&self) -> Result<()> {
        if self.info.writable {
            Ok(())
        } else {
            Err(Error::Invalid("saving is not supported on this mouse model yet".into()))
        }
    }

    /// Changes that stay in the mouse's RAM (active profile, DPI level, previews): a tested model,
    /// or an untested one with experimental editing.
    fn check_controllable(&self) -> Result<()> {
        if self.model.verified || self.info.experimental {
            Ok(())
        } else {
            Err(Error::Invalid("this mouse model is read-only until it has been tested".into()))
        }
    }

    /// The list of enabled profiles (sector 0) is written only on tested models: restoring it has
    /// not been tried on real hardware yet.
    fn check_tested(&self) -> Result<()> {
        if self.model.verified {
            Ok(())
        } else {
            Err(Error::Invalid("enabling or disabling profiles needs a tested mouse model".into()))
        }
    }

    pub fn state(&mut self) -> Result<MouseState> {
        let directory = self.dev.read_sector(0, self.sector_size())?;
        let enabled = if profile::crc_is_valid(&directory) {
            profile::parse_directory(&directory, self.profile_count() as usize)
        } else {
            vec![true; self.profile_count() as usize]
        };
        let mut profiles = Vec::new();
        let mut macro_sectors = HashMap::new();
        let mut used = self.reserved_sectors(&directory);
        for number in 1..=self.profile_count() {
            let sector = self.read_profile_sector(number)?;
            let mut settings = ProfileSettings::parse(&sector, self.info.onboard.button_count as usize)?;
            self.read_macros(&mut settings, &mut macro_sectors)?;
            for (s, o) in settings.macro_addresses() {
                used.extend(self.macro_sectors(&mut macro_sectors, s, o)?);
            }
            profiles.push(MouseProfile { number, enabled: enabled[number as usize - 1], settings });
        }
        let free = (self.profile_count() + 1..self.info.onboard.sector_count).filter(|s| !used.contains(s)).count();
        Ok(MouseState {
            active_profile: self.read_active_profile()?,
            active_dpi_index: self.dev.active_dpi_index()?,
            current_dpi: self.dev.sensor_dpi(0)?.0,
            profiles,
            macro_space: free as u32 * (self.sector_size() as u32 - 2),
        })
    }

    /// The directory and the profile sectors, by number and as the directory lists them:
    /// macros never go there.
    fn reserved_sectors(&self, directory: &[u8]) -> HashSet<u8> {
        let mut reserved: HashSet<u8> = (0..=self.profile_count()).collect();
        if profile::crc_is_valid(directory) {
            let listed = profile::directory_sectors(directory, self.profile_count() as usize);
            reserved.extend(listed.into_iter().filter_map(|s| u8::try_from(s).ok()));
        }
        reserved
    }

    /// User profile sector; if it was never written (invalid CRC), the factory one applies.
    fn read_profile_sector(&mut self, number: u8) -> Result<Vec<u8>> {
        let sector = self.dev.read_sector(number as u16, self.sector_size())?;
        if profile::crc_is_valid(&sector) {
            return Ok(sector);
        }
        self.read_factory_sector(number)
    }

    /// Reads a sector through `cache`, since profiles can share macro sectors.
    fn read_cached(&mut self, cache: &mut HashMap<u16, Vec<u8>>, id: u16) -> Result<Vec<u8>> {
        if let Some(data) = cache.get(&id) {
            return Ok(data.clone());
        }
        let data = self.dev.read_sector(id, self.sector_size())?;
        cache.insert(id, data.clone());
        Ok(data)
    }

    /// Reads the macro at `sector`/`offset`. `None` if PeriHub cannot read it; errors talking to
    /// the mouse are returned.
    fn read_macro(&mut self, cache: &mut HashMap<u16, Vec<u8>>, sector: u8, offset: u16) -> Result<Option<Macro>> {
        match macros::parse(|id| self.read_cached(cache, id), sector, offset) {
            Ok(content) => Ok(Some(content)),
            Err(e @ (Error::Hid(_) | Error::Timeout)) => Err(e),
            Err(e) => {
                log::warn!("macro at sector {sector:#04x} offset {offset:#06x}: {e}");
                Ok(None)
            }
        }
    }

    /// Sectors used by the macro at `sector`/`offset`. If its commands cannot be followed, at
    /// least the sector it starts in.
    fn macro_sectors(&mut self, cache: &mut HashMap<u16, Vec<u8>>, sector: u8, offset: u16) -> Result<Vec<u8>> {
        match macros::sectors_used(|id| self.read_cached(cache, id), sector, offset) {
            Ok(sectors) => Ok(sectors),
            Err(e @ (Error::Hid(_) | Error::Timeout)) => Err(e),
            Err(_) => Ok(vec![sector]),
        }
    }

    /// Fills in the content of the macros bound to the profile's buttons.
    fn read_macros(&mut self, settings: &mut ProfileSettings, cache: &mut HashMap<u16, Vec<u8>>) -> Result<()> {
        for action in settings.buttons.iter_mut().chain(settings.gshift_buttons.iter_mut()) {
            if let ButtonAction::Macro { sector, offset, content } = action {
                *content = self.read_macro(cache, *sector, *offset)?;
            }
        }
        Ok(())
    }

    /// Gives every macro of the profile being saved an address. A macro whose content is already in
    /// the mouse, on any button of any profile, uses that copy; the others are packed into sectors
    /// that no profile uses, returned to be written before the profile. Sectors in use, including
    /// those of the macros this save replaces, are never overwritten: if writing stops halfway,
    /// every profile still points to intact macros. Replaced macros free their sectors for the
    /// next save.
    fn place_macros(&mut self, settings: &mut ProfileSettings) -> Result<Vec<(u8, Vec<u8>)>> {
        let buttons = self.info.onboard.button_count as usize;
        let mut cache = HashMap::new();
        let directory = self.dev.read_sector(0, self.sector_size())?;
        let mut used = self.reserved_sectors(&directory);
        // Macros in the mouse now.
        let mut existing = Vec::new();
        for n in 1..=self.profile_count() {
            let sector = self.read_profile_sector(n)?;
            for (s, o) in ProfileSettings::parse(&sector, buttons)?.macro_addresses() {
                used.extend(self.macro_sectors(&mut cache, s, o)?);
                if !existing.iter().any(|(address, _)| *address == (s, o)) {
                    let content = self.read_macro(&mut cache, s, o)?;
                    existing.push(((s, o), content));
                }
            }
        }
        // Macros of this profile that are new or changed.
        let mut new = Vec::new();
        for action in settings.buttons.iter_mut().chain(settings.gshift_buttons.iter_mut()) {
            let ButtonAction::Macro { sector, offset, content } = action else { continue };
            let current = existing.iter().find(|(address, _)| *address == (*sector, *offset));
            match (content, current) {
                // Unchanged, or one PeriHub cannot read (it is kept byte for byte).
                (Some(content), Some((_, Some(old)))) if content == old => {}
                (None, Some(_)) => {}
                (None, None) => {
                    return Err(Error::Invalid("a button points to a macro that is not in the mouse".into()));
                }
                (Some(content), _) => {
                    content.validate()?;
                    // The same macro on another button or profile is shared, not written twice.
                    match existing.iter().find(|(_, old)| old.as_ref() == Some(&*content)) {
                        Some((address, _)) => (*sector, *offset) = *address,
                        None => new.push(action),
                    }
                }
            }
        }
        if new.is_empty() {
            return Ok(Vec::new());
        }
        let free = (self.profile_count() + 1..self.info.onboard.sector_count).filter(|s| !used.contains(s));
        let mut packer = Packer::new(free, self.sector_size());
        let mut placed: Vec<(Macro, (u8, u16))> = Vec::new();
        for action in new {
            let ButtonAction::Macro { sector, offset, content: Some(content) } = action else { unreachable!() };
            // The same macro on two buttons is written once.
            let address = match placed.iter().find(|(m, _)| m == content) {
                Some((_, address)) => *address,
                None => {
                    let address = packer.place(content)?;
                    placed.push((content.clone(), address));
                    address
                }
            };
            (*sector, *offset) = address;
        }
        let mut sectors = packer.finish();
        for (_, data) in &mut sectors {
            profile::write_crc(data);
        }
        Ok(sectors)
    }

    fn read_factory_sector(&mut self, number: u8) -> Result<Vec<u8>> {
        let rom = if number <= self.info.onboard.rom_profile_count { number } else { 1 };
        let mut sector = self.dev.read_sector(ROM_SECTOR_BASE + rom as u16, self.sector_size())?;
        profile::write_crc(&mut sector);
        Ok(sector)
    }

    /// Saves every sector to a backup and returns them.
    fn backup_all(&mut self, label: &str) -> Result<Vec<Vec<u8>>> {
        let mut files = Vec::new();
        for sector in 0..self.info.onboard.sector_count as u16 {
            files.push((format!("sector_{sector:04x}.bin"), self.dev.read_sector(sector, self.sector_size())?));
        }
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        let dir = write_backup(&self.model.backup_dir(), label, &refs)?;
        // The folder name only: the full path would put the user's name in the log file.
        let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        log::info!("mouse backup saved: {id}");
        self.last_backup = Some(id);
        Ok(files.into_iter().map(|(_, data)| data).collect())
    }

    fn write_verified(&mut self, sector: u16, data: &[u8]) -> Result<()> {
        self.dev.write_sector(sector, data, &self.info.onboard)?;
        let back = self.dev.read_sector(sector, self.sector_size())?;
        if back != data {
            return Err(Error::Protocol(format!(
                "sector {sector:#06x} read back differs from what was written (backup in {})",
                crate::config::config_dir().join("backups").display()
            )));
        }
        Ok(())
    }

    /// Reloads the active profile after it was written. On the G502 HERO a written profile already
    /// takes effect (DPI and polling, verified on 2026-11-05); the reload also drops any preview.
    fn reload_if_active(&mut self, number: u8) -> Result<()> {
        if self.dev.active_profile()? == number {
            self.reload_active()?;
        }
        Ok(())
    }

    /// Goes back to the saved settings of the active profile, dropping DPI and LED previews (RAM only).
    fn reload_active(&mut self) -> Result<()> {
        let active = self.dev.active_profile()?;
        let directory = self.dev.read_sector(0, self.sector_size())?;
        let enabled = if profile::crc_is_valid(&directory) {
            profile::parse_directory(&directory, self.profile_count() as usize)
        } else {
            vec![true; self.profile_count() as usize]
        };
        match reload_method(active, &enabled) {
            Reload::Through(other) => {
                self.dev.set_active_profile(other)?;
                self.dev.set_active_profile(active)?;
            }
            Reload::ModeSwitch => {
                let level = self.dev.active_dpi_index()?;
                self.dev.set_onboard_mode(HOST_MODE)?;
                self.dev.set_onboard_mode(ONBOARD_MODE)?;
                self.dev.set_active_dpi_index(level)?;
            }
        }
        self.previewing = false;
        Ok(())
    }

    /// Raw read of a sector (diagnostics).
    pub fn read_raw_sector(&mut self, sector: u16) -> Result<Vec<u8>> {
        self.dev.read_sector(sector, self.sector_size())
    }

    /// Cancels DPI and LED previews by going back to the saved settings.
    pub fn revert_preview(&mut self) -> Result<()> {
        self.reload_active()
    }

    /// Cancels a preview left by an interface that went away.
    pub fn revert_preview_if_any(&mut self) -> Result<()> {
        if self.previewing { self.revert_preview() } else { Ok(()) }
    }

    pub fn save_profile(&mut self, number: u8, settings: &ProfileSettings) -> Result<MouseState> {
        self.check_writable()?;
        self.check_number(number)?;
        let caps = &self.info.dpi;
        settings.validate(self.info.onboard.button_count as usize, caps.min, caps.max, &self.info.report_rates_ms)?;
        if caps.step > 0 && settings.dpi_levels.iter().any(|d| !(d - caps.min).is_multiple_of(caps.step)) {
            return Err(Error::Invalid(format!("DPI values must be multiples of {}", caps.step)));
        }
        let mut sector = self.read_profile_sector(number)?;
        if !self.model.verified {
            self.check_led_slots(&sector, settings)?;
        }
        let mut settings = settings.clone();
        let macro_sectors = self.place_macros(&mut settings)?;
        settings.apply(&mut sector)?;
        self.backup_before_writing(&format!("profile{number}"))?;
        let written = (|| {
            // Macros first: if writing stops halfway, the profile still points to the old ones.
            for (id, data) in &macro_sectors {
                self.write_verified(*id as u16, data)?;
            }
            self.write_verified(number as u16, &sector)?;
            self.reload_if_active(number)
        })();
        self.after_writing(written)?;
        self.state()
    }

    pub fn restore_factory_profile(&mut self, number: u8) -> Result<MouseState> {
        self.check_writable()?;
        self.check_number(number)?;
        let sector = self.read_factory_sector(number)?;
        self.backup_before_writing(&format!("factory{number}"))?;
        let written = (|| {
            self.write_verified(number as u16, &sector)?;
            self.reload_if_active(number)
        })();
        self.after_writing(written)?;
        self.state()
    }

    /// Backs up the memory before a save. On an untested model the backup must be one PeriHub can
    /// put back, and the undo is armed before the first write, so that a save that stops halfway
    /// is undone too, also if PeriHub is closed or the mouse unplugged (see [`Self::finish_pending_undo`]).
    fn backup_before_writing(&mut self, label: &str) -> Result<()> {
        let sectors = self.backup_all(label)?;
        if !self.info.experimental {
            return Ok(());
        }
        check_backup(&sectors, &self.info.onboard).map_err(|e| {
            Error::Invalid(format!("nothing was saved: PeriHub could not undo a save on this mouse ({e})"))
        })?;
        let id = self.last_backup.clone().unwrap_or_default();
        std::fs::write(self.pending_undo_path(), &id)?;
        self.undo_at = Some(Instant::now() + UNDO_AFTER);
        Ok(())
    }

    /// On an untested model a failed save is undone at once.
    fn after_writing(&mut self, written: Result<()>) -> Result<()> {
        let Err(error) = written else { return Ok(()) };
        if !self.info.experimental {
            return Err(error);
        }
        match self.undo_last_write() {
            Ok(_) => Err(Error::Protocol(format!("{error}. The backup was put back."))),
            Err(undo) => Err(Error::Protocol(format!(
                "{error}. Putting the backup back failed too ({undo}): try Backups…, or Logitech G HUB."
            ))),
        }
    }

    /// File that marks a save to an untested model that was neither kept nor undone yet. It keeps
    /// the backup's id, so that the save is undone even after PeriHub was closed.
    fn pending_undo_path(&self) -> std::path::PathBuf {
        config::config_dir().join("backups").join(self.model.backup_dir()).join("pending-undo")
    }

    /// The user checked the mouse after a save and keeps the changes.
    pub fn keep_changes(&mut self) {
        self.undo_at = None;
        let _ = std::fs::remove_file(self.pending_undo_path());
    }

    /// When the mouse is opened: a save that was neither kept nor undone (PeriHub closed, mouse
    /// unplugged during the countdown) is undone now, if the mouse can be written to. A day later
    /// the backup is too old to put back blindly: the mark is dropped.
    fn finish_pending_undo(&mut self) {
        let path = self.pending_undo_path();
        let Ok(id) = std::fs::read_to_string(&path) else { return };
        let id = id.trim().to_string();
        let stamp = id.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if now.saturating_sub(stamp) > PENDING_UNDO_MAX_AGE.as_secs() {
            log::warn!("a save that was not kept is too old to undo now: backup {id} left alone");
            let _ = std::fs::remove_file(&path);
            return;
        }
        if !self.info.writable {
            self.info.pending_undo = true;
            return;
        }
        log::warn!("a save to this mouse was not kept before PeriHub closed or the mouse was unplugged: undoing it");
        self.last_backup = Some(id);
        if let Err(e) = self.undo_last_write() {
            log::warn!("undo failed: {e}");
        }
    }

    /// Before the manager opens the mouse again (experimental editing turned off): a save that was
    /// not kept is undone while the mouse can still be written to, then the thread stops.
    pub fn close(&mut self) -> Result<()> {
        if self.undo_at.is_some() {
            log::warn!("experimental editing turned off before a save was kept: undoing it");
            if let Err(e) = self.undo_last_write() {
                log::warn!("undo failed: {e}");
            }
        }
        self.closed = true;
        Ok(())
    }

    /// On an untested model, LED slots that the mouse does not report as zones keep their bytes:
    /// PeriHub only knows what they mean on the G502 HERO.
    fn check_led_slots(&self, sector: &[u8], settings: &ProfileSettings) -> Result<()> {
        let current = ProfileSettings::parse(sector, self.info.onboard.button_count as usize)?;
        let zones: Vec<u8> = self.info.led_zones.iter().map(|z| z.index).collect();
        if unknown_led_slots_changed(&current, settings, &zones) {
            return Err(Error::Invalid("this mouse has no such LED zone".into()));
        }
        Ok(())
    }

    /// Puts back the memory saved before the last write: the "Keep these changes?" countdown on an
    /// untested model calls this when it is not confirmed.
    pub fn undo_last_write(&mut self) -> Result<MouseRestore> {
        self.undo_at = None;
        let id = self.last_backup.clone().ok_or_else(|| Error::Invalid("nothing to undo".into()))?;
        let restore = self.restore_backup(&id)?;
        self.last_backup = None;
        let _ = std::fs::remove_file(self.pending_undo_path());
        log::info!("last write undone: backup {id} put back");
        Ok(restore)
    }

    /// The backups saved for this mouse model, newest first.
    pub fn backups(&self) -> Vec<BackupEntry> {
        config::list_backups(&self.model.backup_dir())
    }

    /// Writes back the memory saved in a backup, after backing up the current one. Only sectors that
    /// differ are written, each one read back and compared.
    pub fn restore_backup(&mut self, id: &str) -> Result<MouseRestore> {
        self.check_writable()?;
        let onboard = self.info.onboard.clone();
        let device = self.model.backup_dir();
        let backup = (0..onboard.sector_count as u16)
            .map(|sector| config::read_backup_file(&device, id, &format!("sector_{sector:04x}.bin")))
            .collect::<Result<Vec<_>>>()?;
        check_backup(&backup, &onboard)?;
        let current = (0..onboard.sector_count as u16)
            .map(|sector| self.dev.read_sector(sector, onboard.sector_size))
            .collect::<Result<Vec<_>>>()?;
        let order = restore_order(&current, &backup, onboard.profile_count as usize);
        if order.contains(&0) {
            self.check_tested()?;
        }
        if !order.is_empty() {
            self.backup_all("restore")?;
            for &sector in &order {
                self.write_verified(sector, &backup[sector as usize])?;
            }
            let enabled = profile::parse_directory(&backup[0], onboard.profile_count as usize);
            match profile_after_restore(self.dev.active_profile()?, &enabled) {
                Some(other) => {
                    self.dev.set_active_profile(other)?;
                    self.previewing = false;
                }
                None => self.reload_active()?,
            }
            log::info!("mouse backup restored: {id}, sectors {order:?}");
        }
        let profiles =
            order.iter().filter(|&&s| (1..=onboard.profile_count as u16).contains(&s)).map(|&s| s as u8).collect();
        Ok(MouseRestore {
            state: self.state()?,
            profiles,
            macros: order.iter().any(|&s| s > onboard.profile_count as u16),
            directory: order.contains(&0),
        })
    }

    pub fn set_profile_enabled(&mut self, number: u8, enabled: bool) -> Result<MouseState> {
        self.check_writable()?;
        self.check_tested()?;
        self.check_number(number)?;
        let mut flags = self.state()?.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>();
        flags[number as usize - 1] = enabled;
        if !flags.iter().any(|&f| f) {
            return Err(Error::Invalid("at least one profile must stay enabled".into()));
        }
        let directory = profile::build_directory(&flags, self.sector_size() as usize);
        self.backup_all("directory")?;
        self.write_verified(0, &directory)?;
        self.state()
    }

    pub fn active_profile(&mut self) -> Result<u8> {
        self.read_active_profile()
    }

    /// Active profile and DPI, which the mouse's buttons change without PeriHub, and the battery.
    pub fn live(&mut self) -> Result<MouseLive> {
        let battery = match self.dev.battery() {
            Ok(battery) => battery,
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
            // An odd battery report must not hide the DPI.
            Err(e) => {
                log::debug!("mouse battery: {e}");
                None
            }
        };
        Ok(MouseLive {
            active_profile: self.read_active_profile()?,
            active_dpi_index: self.dev.active_dpi_index()?,
            current_dpi: self.dev.sensor_dpi(0)?.0,
            battery,
        })
    }

    fn read_active_profile(&mut self) -> Result<u8> {
        let active = self.dev.active_profile()?;
        if (1..=self.profile_count()).contains(&active) {
            self.last_profile = active;
        }
        Ok(active)
    }

    pub fn set_active_profile(&mut self, number: u8) -> Result<()> {
        self.check_controllable()?;
        self.check_number(number)?;
        match self.dev.set_active_profile(number) {
            // The mouse refuses while another program controls it: check, then try once more.
            Err(Error::Hidpp(HidppErrorCode(0x02))) => {
                self.check_mode()?;
                if self.host_mode.load(Ordering::SeqCst) {
                    return Err(Error::Invalid("another program, such as Logitech G HUB, controls the mouse".into()));
                }
                self.dev.set_active_profile(number)?;
            }
            other => other?,
        }
        self.last_profile = number;
        Ok(())
    }

    pub fn set_active_dpi_index(&mut self, index: u8) -> Result<()> {
        self.check_controllable()?;
        self.dev.set_active_dpi_index(index)
    }

    /// DPI preview (not saved: the mouse goes back to the profile at the next level change).
    pub fn preview_dpi(&mut self, dpi: u16) -> Result<()> {
        self.check_controllable()?;
        let caps = &self.info.dpi;
        if dpi < caps.min || dpi > caps.max {
            return Err(Error::Invalid(format!("{dpi} DPI")));
        }
        self.dev.set_sensor_dpi(0, dpi)?;
        self.previewing = true;
        Ok(())
    }

    pub fn preview_led(&mut self, zone: u8, effect: &LedEffect) -> Result<()> {
        self.check_controllable()?;
        let zone = self
            .info
            .led_zones
            .iter()
            .find(|z| z.index == zone)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("LED zone {zone}")))?;
        self.dev.preview_led_effect(&zone, effect)?;
        self.previewing = true;
        Ok(())
    }
}

impl DeviceTask for MouseTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.closed {
            return Err(Error::NotConnected);
        }
        if self.undo_at.is_some_and(|at| Instant::now() >= at) {
            log::warn!("a save to an untested mouse was not kept: undoing it");
            if let Err(e) = self.undo_last_write() {
                log::warn!("undo failed: {e}");
            }
        }
        // tick also runs after every request: the mode is read only every MODE_CHECK.
        if self.last_mode_check.elapsed() >= MODE_CHECK {
            match self.check_mode() {
                Ok(()) => {}
                // The mouse is gone, or asleep behind its receiver: the manager opens it again
                // when it is back.
                Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
                // Anything else (a busy mouse, a refused command) must not close the connection.
                Err(e) => log::debug!("mouse mode check: {e}"),
            }
        }
        let next = MODE_CHECK.saturating_sub(self.last_mode_check.elapsed());
        Ok(match self.undo_at {
            Some(at) => next.min(at.saturating_duration_since(Instant::now())),
            None => next,
        })
    }
}

/// What restoring a backup changed in the mouse's memory.
#[derive(Debug, Clone, Serialize)]
pub struct MouseRestore {
    pub state: MouseState,
    /// Profiles whose settings changed.
    pub profiles: Vec<u8>,
    /// Whether the macros stored in the mouse changed.
    pub macros: bool,
    /// Whether the list of enabled profiles changed.
    pub directory: bool,
}

/// Checks that a backup's sectors fit this mouse and hold a valid profile list, so restoring them
/// cannot leave the mouse with a broken memory.
fn check_backup(backup: &[Vec<u8>], onboard: &OnboardInfo) -> Result<()> {
    let size = onboard.sector_size as usize;
    if backup.len() != onboard.sector_count as usize || backup.iter().any(|s| s.len() != size) {
        return Err(Error::Invalid("this backup does not match the memory of this mouse".into()));
    }
    let directory = &backup[0];
    if !profile::crc_is_valid(directory) {
        return Err(Error::Invalid("the profile list in this backup is damaged".into()));
    }
    for address in profile::directory_sectors(directory, onboard.profile_count as usize) {
        let sector = backup.get(address as usize).filter(|_| address != 0);
        if !sector.is_some_and(|s| profile::crc_is_valid(s)) {
            return Err(Error::Invalid(format!("profile sector {address:#06x} in this backup is damaged")));
        }
    }
    Ok(())
}

/// Sectors that differ from the backup, in the order they are written: macros first, then the
/// profiles, then the profile list, so a profile never points to macros that are not there yet.
///
/// A macro sector that is blank in the backup is left as it is: no profile of the backup points to
/// it, so what it holds now is unused memory, and writing a blank sector has never been tried on a
/// real mouse.
fn restore_order(current: &[Vec<u8>], backup: &[Vec<u8>], profile_count: usize) -> Vec<u16> {
    let mut profiles = profile::directory_sectors(&backup[0], profile_count);
    profiles.extend(profile::directory_sectors(&current[0], profile_count));
    let rank = |sector: u16| match sector {
        0 => 2,
        s if profiles.contains(&s) => 1,
        _ => 0,
    };
    let blank = |data: &[u8]| data.iter().all(|&b| b == 0x00) || data.iter().all(|&b| b == 0xFF);
    let mut changed: Vec<u16> = (0..backup.len() as u16)
        .filter(|&s| current[s as usize] != backup[s as usize])
        .filter(|&s| rank(s) > 0 || !blank(&backup[s as usize]))
        .collect();
    changed.sort_by_key(|&s| (rank(s), s));
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_back_only_a_verified_mouse_nobody_else_controls() {
        assert_eq!(mode_action(ONBOARD_MODE, true, true), ModeAction::Fine);
        assert_eq!(mode_action(2, true, false), ModeAction::SwitchBack);
        assert_eq!(mode_action(2, true, true), ModeAction::Report);
        assert_eq!(mode_action(2, false, false), ModeAction::Report);
    }

    #[test]
    fn reload_goes_through_another_profile_when_there_is_one() {
        // As before: the first other enabled profile, or else the first other one.
        assert_eq!(reload_method(4, &[true; 5]), Reload::Through(1));
        assert_eq!(reload_method(1, &[true; 5]), Reload::Through(2));
        assert_eq!(reload_method(1, &[true, false, true, true, true]), Reload::Through(3));
        assert_eq!(reload_method(1, &[true, false]), Reload::Through(2));
        // A single profile (G403 HERO): host mode and back.
        assert_eq!(reload_method(1, &[true]), Reload::ModeSwitch);
    }

    /// The real G502 HERO memory of the fixtures (sectors 0–5), with empty macro sectors 6–15.
    fn g502_memory() -> (Vec<Vec<u8>>, OnboardInfo) {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/g502");
        let mut sectors: Vec<Vec<u8>> =
            (0..6).map(|s| std::fs::read(format!("{dir}/sector_{s:04x}.bin")).unwrap()).collect();
        let size = sectors[0].len();
        sectors.resize(16, vec![0; size]);
        let onboard = OnboardInfo {
            memory_model: 1,
            profile_format: 2,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 5,
            button_count: 11,
            sector_count: 16,
            sector_size: size as u16,
            has_gshift: true,
            has_dpi_shift: true,
        };
        (sectors, onboard)
    }

    #[test]
    fn experimental_editing_needs_a_memory_like_the_g502_and_the_cable() {
        let (_, g502) = g502_memory();
        assert!(experimental_allowed(true, &g502, false));
        assert!(!experimental_allowed(false, &g502, false), "turned off");
        assert!(!experimental_allowed(true, &g502, true), "behind a receiver");
        let other_format = OnboardInfo { profile_format: 3, ..g502.clone() };
        assert!(!experimental_allowed(true, &other_format, false), "another profile format");
        let small_sectors = OnboardInfo { sector_size: 240, ..g502.clone() };
        assert!(!experimental_allowed(true, &small_sectors, false), "smaller sectors (G502 X)");
        let big_sectors = OnboardInfo { sector_size: 512, ..g502 };
        assert!(!experimental_allowed(true, &big_sectors, false), "bigger sectors");
    }

    #[test]
    fn led_slots_that_are_not_zones_of_the_mouse_keep_their_bytes() {
        let (memory, onboard) = g502_memory();
        let current = ProfileSettings::parse(&memory[1], onboard.button_count as usize).unwrap();
        let mut new = current.clone();
        assert!(!unknown_led_slots_changed(&current, &new, &[]));
        new.leds[1] = LedEffect::Off;
        assert_ne!(new.leds[1], current.leds[1], "the fixture must have an effect in slot 1");
        assert!(!unknown_led_slots_changed(&current, &new, &[0, 1]), "both zones exist");
        assert!(unknown_led_slots_changed(&current, &new, &[0]), "only zone 0 exists");
        assert!(unknown_led_slots_changed(&current, &new, &[]), "a mouse without LEDs");
    }

    #[test]
    fn an_untested_mouse_with_a_blank_profile_list_cannot_be_undone_so_it_is_not_saved() {
        // backup_before_writing refuses a save when check_backup refuses the backup just taken.
        let (mut memory, onboard) = g502_memory();
        memory[0] = vec![0xFF; onboard.sector_size as usize];
        assert!(check_backup(&memory, &onboard).is_err());
    }

    #[test]
    fn a_real_backup_passes_the_checks() {
        let (backup, onboard) = g502_memory();
        check_backup(&backup, &onboard).unwrap();
    }

    #[test]
    fn damaged_or_foreign_backups_are_refused() {
        let (backup, onboard) = g502_memory();
        let mut short = backup.clone();
        short.pop();
        assert!(check_backup(&short, &onboard).is_err(), "missing sector");
        let mut small = backup.clone();
        small[7].truncate(240);
        assert!(check_backup(&small, &onboard).is_err(), "sector of another size");
        let mut directory = backup.clone();
        directory[0][1] ^= 0xFF;
        assert!(check_backup(&directory, &onboard).is_err(), "damaged profile list");
        let mut profile = backup.clone();
        profile[3][10] ^= 0x01;
        assert!(check_backup(&profile, &onboard).is_err(), "damaged profile");
    }

    #[test]
    fn restore_writes_only_what_changed_macros_first_and_the_list_last() {
        let (backup, _) = g502_memory();
        assert!(restore_order(&backup, &backup, 5).is_empty());
        let mut current = backup.clone();
        current[0][4] ^= 1; // a profile was disabled
        current[4][20] ^= 1; // profile 4 was edited
        current[2][7] ^= 1; // profile 2 was edited
        assert_eq!(restore_order(&current, &backup, 5), [2, 4, 0]);

        let mut backup = backup;
        backup[7][..4].copy_from_slice(&[0x43, 0x00, 0x1B, 0xFF]); // a macro the backup had
        current[7] = vec![0; backup[7].len()];
        assert_eq!(restore_order(&current, &backup, 5), [7, 2, 4, 0]);
    }

    #[test]
    fn restore_leaves_sectors_that_are_blank_in_the_backup() {
        let (backup, _) = g502_memory();
        let mut current = backup.clone();
        current[9][0] = 0x43; // a macro added after the backup, in a sector the backup had empty
        assert!(restore_order(&current, &backup, 5).is_empty());
    }

    #[test]
    fn a_restore_that_disables_the_active_profile_switches_to_an_enabled_one() {
        assert_eq!(profile_after_restore(4, &[true; 5]), None);
        assert_eq!(profile_after_restore(4, &[false, true, true, false, true]), Some(2));
        assert_eq!(profile_after_restore(1, &[false, false, true, true, true]), Some(3));
        // While another program controls the mouse no profile is active (0).
        assert_eq!(profile_after_restore(0, &[true; 5]), None);
    }
}

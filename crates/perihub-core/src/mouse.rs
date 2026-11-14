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
use crate::hidpp::{
    Access, Battery, BatteryState, DpiCapabilities, FeatureEntry, FirmwareEntity, HidppDevice, LedZone, OnboardInfo,
    ROM_SECTOR_BASE,
};
use crate::worker::DeviceTask;

/// Profile formats PeriHub writes, with one layout for both, as libratbag and Solaar do. The others
/// can be read but not written.
const WRITABLE_PROFILE_FORMATS: &[u8] = &[2, 3];
/// Sector sizes PeriHub writes: 256 bytes, or 255 with the CRC one byte earlier (other mice, and
/// newer G502 HERO firmware: issue #20).
const WRITABLE_SECTOR_SIZES: std::ops::RangeInclusive<usize> =
    profile::MIN_WRITE_SECTOR_SIZE..=profile::VERIFIED_SECTOR_SIZE;
/// Memories PeriHub has saved to on real mice, with their USB cable: (profile format, sector size).
/// Format 2 in 256-byte sectors on the G502 HERO, format 3 in 255-byte sectors on the G502 X
/// Wireless of issue #15. The memory format decides how a save is laid out, not the model: any
/// mouse whose memory reports one of these counts as tested. The same model can change format
/// with its firmware, so it is read from the mouse every time.
const TESTED_MEMORY: &[(u8, u16)] = &[(2, 256), (3, 255)];
const ONBOARD_MODE: u8 = 1;
const HOST_MODE: u8 = 2;
/// How often the mouse is checked for another program having taken control of it.
const MODE_CHECK: Duration = Duration::from_secs(5);
/// After an experimental save, the backup goes back by itself unless the change is kept
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
    /// Tested: its memory format (or the model itself) was tested on a real mouse. See [`support`].
    pub verified: bool,
    /// Its memory uses a format PeriHub can write but has never saved to on a real mouse:
    /// saves are experimental, and each one is undone unless the user keeps it.
    pub experimental: bool,
    /// A save to this mouse was neither kept nor undone (PeriHub was closed, or the mouse
    /// unplugged, during the countdown), and the mouse cannot be written to now: the backup goes
    /// back when it can be (with its cable, or through its receiver on a tested memory).
    pub pending_undo: bool,
    /// Connected through a LIGHTSPEED receiver instead of its cable.
    pub receiver: bool,
    /// Which receiver and slot, for test reports.
    pub link: Option<ReceiverLink>,
    /// Another program left the mouse under software control, so its onboard profiles are not in use.
    pub host_mode: bool,
    /// PeriHub gives the mouse its onboard profiles back by itself once no other program controls
    /// it: models tested as a whole only (see [`mode_action`]).
    pub switches_back: bool,
    pub firmware: Vec<FirmwareEntity>,
    pub dpi: DpiCapabilities,
    pub report_rates_ms: Vec<u8>,
    pub led_zones: Vec<LedZone>,
    /// The HID++ features the mouse lists, for test reports: they show what a mouse has that
    /// PeriHub does not read yet (such as lighting through another feature than 0x8070).
    pub features: Vec<FeatureEntry>,
    pub onboard: OnboardInfo,
    pub writable: bool,
    /// PeriHub knows how to write the memory format the mouse reports: with its cable (and in the
    /// interface, for an untested format) it can be saved to.
    pub writable_format: bool,
    /// The mouse's profile list when it was connected (for test reports;
    /// [`MouseState::profile_list`] is the state now).
    pub profile_list: ProfileList,
}

/// The list in the mouse's memory (sector 0) that says which profiles it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileList {
    /// The mouse has its own list and uses the profiles saved in its memory.
    Own,
    /// Nothing was ever saved to the mouse: the sector is blank, and the mouse uses its factory
    /// profiles. The first save creates the list.
    Factory,
    /// Something is there, but its checksum does not match: PeriHub cannot tell which profiles
    /// the mouse uses, and does not save over it.
    Damaged,
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
    /// Without a list of its own ([`ProfileList::Factory`]) the mouse uses its factory profiles,
    /// which are then the ones in `profiles`.
    pub profile_list: ProfileList,
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
    /// When the last experimental save is undone, unless it is kept.
    undo_at: Option<Instant>,
    /// The mouse's profile list, as last read (see [`MouseTask::read_directory`]).
    list: ProfileList,
}

/// How far PeriHub goes with a mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Support {
    /// Tested (see [`MouseInfo::verified`]): the profile list can be created and changed, and the
    /// active profile and DPI level switched, also through a receiver.
    pub tested: bool,
    /// Saves are possible but untested: each one can be undone (see [`MouseInfo::experimental`]).
    pub experimental: bool,
    /// Profiles can be saved to the mouse's memory.
    pub writable: bool,
    /// PeriHub knows how to write the mouse's memory format (with the cable, and a tested format or
    /// the interface's undo).
    pub writable_format: bool,
}

impl Support {
    fn access(&self) -> Access {
        if self.writable {
            Access::Full
        } else if self.tested {
            Access::Ram
        } else {
            Access::ReadOnly
        }
    }
}

/// What PeriHub does with a mouse, from the memory format it reports:
/// - a tested memory (see [`TESTED_MEMORY`]), or a model tested as a whole: supported;
/// - another memory PeriHub can write: experimental saves, if `untested_saves` (the interface asks
///   whether to keep each one; the command line never saves to such a mouse);
/// - any other memory: read-only.
///
/// Through a LIGHTSPEED receiver, as G HUB does, but experimental and only on a tested
/// memory: PeriHub has never saved over the radio on a real mouse, so each save is undone unless
/// the user keeps it, and a memory nobody has tested is not tried over the radio on top of that.
pub fn support(model: &MouseModel, onboard: &OnboardInfo, receiver: bool, untested_saves: bool) -> Support {
    let known = WRITABLE_PROFILE_FORMATS.contains(&onboard.profile_format)
        && WRITABLE_SECTOR_SIZES.contains(&(onboard.sector_size as usize));
    let tested = model.verified || (known && TESTED_MEMORY.contains(&(onboard.profile_format, onboard.sector_size)));
    let writable = known && if receiver { tested && untested_saves } else { tested || untested_saves };
    Support { tested, experimental: writable && (!tested || receiver), writable, writable_format: known }
}

/// Below this charge a save over the radio is refused: a mouse that turns off halfway leaves a
/// profile half written until the undo can run.
const MIN_BATTERY_FOR_RADIO_SAVE: u8 = 15;

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

/// The switch back happens by itself as soon as the mouse is connected, so it stays on models
/// tested as a whole; and a program that is running (G HUB) takes the mouse again at once, so
/// only a mouse that nothing else controls is switched back.
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

/// Mice with more profiles keep the way verified since 0.1.0. Better to go through an enabled
/// profile: the behavior with disabled ones is not verified.
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
    /// `dev` comes read-only: what else PeriHub may send depends on the memory format the mouse
    /// reports (see [`support`]). `untested_saves`: whether a mouse whose memory format is untested
    /// may be saved to (only the interface, which asks whether to keep each save).
    pub fn open(mut dev: HidppDevice, model: &'static MouseModel, untested_saves: bool) -> Result<Self> {
        let onboard = dev.onboard_info()?;
        if onboard.memory_model != 1 || onboard.macro_format != 1 {
            return Err(Error::Protocol("onboard profile memory uses an unknown format".into()));
        }
        let support = support(model, &onboard, dev.is_behind_receiver(), untested_saves);
        dev.grant(support.access());
        let info = MouseInfo {
            product_id: model.product_id,
            model: model.name.into(),
            name: dev.device_name()?,
            verified: support.tested,
            experimental: support.experimental,
            pending_undo: false,
            receiver: dev.is_behind_receiver(),
            link: None,
            host_mode: false,
            switches_back: model.verified,
            firmware: dev.firmware()?,
            dpi: dev.dpi_capabilities(0)?,
            report_rates_ms: missing_as_empty(dev.report_rates())?,
            led_zones: missing_as_empty(dev.led_zones())?,
            // Only for the test report: a list that cannot be read leaves it empty.
            features: match dev.feature_table() {
                Ok(features) => features,
                Err(Error::Unreachable) => return Err(Error::Unreachable),
                Err(_) => Vec::new(),
            },
            writable: support.writable,
            writable_format: support.writable_format,
            profile_list: ProfileList::Own,
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
            list: ProfileList::Own,
        };
        task.read_directory()?;
        task.info.profile_list = task.list;
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
                    self.select_profile(self.last_profile)?;
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

    /// Changes that stay in the mouse's RAM (active profile, DPI level, previews): a tested mouse,
    /// or one with experimental saves.
    fn check_controllable(&self) -> Result<()> {
        if self.info.verified || self.info.experimental {
            Ok(())
        } else {
            Err(Error::Invalid("PeriHub only reads this mouse: its memory format is not one it can write yet".into()))
        }
    }

    /// The list of enabled profiles (sector 0) is written only on tested mice: on an experimental
    /// one a list that went wrong could not be put back with the usual undo.
    fn check_tested(&self) -> Result<()> {
        // Experimental saves (an untested memory, or through the receiver) leave the list alone.
        if self.info.verified && !self.info.experimental {
            Ok(())
        } else {
            Err(Error::Invalid("enabling or disabling profiles needs a tested memory format".into()))
        }
    }

    /// The profile list (sector 0), if the mouse has a valid one of its own. Otherwise
    /// [`Self::list`] says why not:
    /// - a blank sector means that nothing was ever saved to the mouse, which uses its factory
    ///   profiles (libratbag reads a list without a valid CRC that way, Solaar one that starts
    ///   with four blank bytes: PeriHub wants both). Some mice, the G305 for one, refuse to read
    ///   the sector at all then;
    /// - anything else without a valid CRC is a damaged list, which is never taken for a blank one:
    ///   creating a new list would replace profiles the mouse may be using.
    fn read_directory(&mut self) -> Result<Option<Vec<u8>>> {
        let (list, directory) = match self.dev.read_sector(0, self.sector_size()) {
            Ok(sector) if profile::crc_is_valid(&sector) => (ProfileList::Own, Some(sector)),
            Ok(sector) if profile::is_blank_directory(&sector) => (ProfileList::Factory, None),
            Ok(_) => (ProfileList::Damaged, None),
            Err(Error::Hidpp(HidppErrorCode(0x02))) => (ProfileList::Factory, None),
            Err(e) => return Err(e),
        };
        if list != self.list {
            log::info!("mouse profile list: {list:?}");
        }
        self.list = list;
        Ok(directory)
    }

    /// The mouse uses its factory profiles: it has no profile list of its own.
    fn factory_memory(&self) -> bool {
        self.list == ProfileList::Factory
    }

    /// Which profiles are enabled: what the mouse's list says, or without one the factory list
    /// (sector 0x0100). If no list can be read, every profile counts as enabled.
    fn enabled_profiles(&mut self, directory: Option<&[u8]>) -> Result<Vec<bool>> {
        let count = self.profile_count() as usize;
        if let Some(directory) = directory {
            return Ok(profile::parse_directory(directory, count));
        }
        if !self.factory_memory() {
            return Ok(vec![true; count]);
        }
        let factory = match self.dev.read_sector(ROM_SECTOR_BASE, self.sector_size()) {
            Ok(sector) => {
                profile::parse_factory_directory(&sector, count, self.info.onboard.rom_profile_count as usize)
            }
            Err(Error::Hidpp(_)) => None,
            Err(e) => return Err(e),
        };
        Ok(factory.unwrap_or_else(|| vec![true; count]))
    }

    /// How many profiles can be selected: all of them, or only the factory ones while the mouse
    /// has no profile list.
    fn selectable_profiles(&self) -> u8 {
        if self.factory_memory() {
            self.profile_count().min(self.info.onboard.rom_profile_count)
        } else {
            self.profile_count()
        }
    }

    /// Selects a profile where the mouse keeps the ones it uses: its own memory, or the factory
    /// profiles while it has no profile list.
    fn select_profile(&mut self, number: u8) -> Result<()> {
        self.dev.set_active_profile(self.factory_memory(), number)
    }

    /// Before a save, with the profile list just read. A damaged list is left alone. Creating the
    /// list of a mouse that has none is for tested mice: on an experimental one the save could not
    /// be undone, since the backup holds no list to put back.
    fn check_list_for_saving(&self) -> Result<()> {
        match self.list {
            ProfileList::Own => Ok(()),
            ProfileList::Factory if self.info.verified => Ok(()),
            ProfileList::Factory => Err(Error::Invalid(
                "nothing was saved: this mouse has never had profiles saved to its memory and uses its factory ones. \
                 PeriHub creates that first profile list only on mice whose memory format has been tested: save a \
                 profile once with Logitech G HUB (or Onboard Memory Manager), then try again"
                    .into(),
            )),
            ProfileList::Damaged => Err(Error::Invalid(
                "nothing was saved: the profile list in the mouse's memory is damaged (its checksum does not match), \
                 so PeriHub cannot tell which profiles the mouse uses. Restore a backup with Backups…, or save a \
                 profile once with Logitech G HUB (or Onboard Memory Manager). Please send a mouse test report too"
                    .into(),
            )),
        }
    }

    /// For a mouse without a profile list, or a profile about to be enabled: the factory copy of
    /// each of these profile sectors that holds no profile the mouse uses, so that the list never
    /// points to an empty sector. Without a list (`listed` false) that is whatever the sector holds.
    fn missing_profile_sectors(
        &mut self,
        numbers: impl Iterator<Item = u8>,
        listed: bool,
    ) -> Result<Vec<(u8, Vec<u8>)>> {
        let mut sectors = Vec::new();
        for number in numbers {
            let current = self.dev.read_sector(number as u16, self.sector_size())?;
            if listed && profile::crc_is_valid(&current) {
                continue;
            }
            let factory = self.read_factory_sector(number)?;
            if current != factory {
                sectors.push((number, factory));
            }
        }
        Ok(sectors)
    }

    /// After the mouse got its first profile list: from the factory profile it was using to the
    /// same profile in its own memory, or to `fallback` if that one is not enabled.
    fn adopt_own_profiles(&mut self, enabled: &[bool], fallback: u8) -> Result<()> {
        self.list = ProfileList::Own;
        let active = self.dev.active_profile()?;
        let usable = |n: u8| enabled.get((n as usize).wrapping_sub(1)).copied().unwrap_or(false);
        let target = if usable(active) {
            active
        } else if usable(fallback) {
            fallback
        } else {
            enabled.iter().position(|&on| on).map_or(fallback, |i| i as u8 + 1)
        };
        log::info!("the mouse had no profile list: created, switching to profile {target} of its own memory");
        match self.select_profile(target) {
            Ok(()) => {
                self.last_profile = target;
                self.previewing = false;
                Ok(())
            }
            // Everything is saved by now: a mouse that refuses the switch starts using its new
            // list when it is plugged in again, and the save must not be reported as failed.
            Err(Error::Hidpp(code)) => {
                log::warn!("the mouse refused to switch to its new profile list ({code:?})");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn state(&mut self) -> Result<MouseState> {
        let directory = self.read_directory()?;
        let enabled = self.enabled_profiles(directory.as_deref())?;
        let mut profiles = Vec::new();
        let mut macro_sectors = HashMap::new();
        let mut used = self.reserved_sectors(directory.as_deref());
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
            profile_list: self.list,
        })
    }

    /// The directory and the profile sectors, by number and as the directory lists them:
    /// macros never go there.
    fn reserved_sectors(&self, directory: Option<&[u8]>) -> HashSet<u8> {
        let mut reserved: HashSet<u8> = (0..=self.profile_count()).collect();
        if let Some(directory) = directory {
            let listed = profile::directory_sectors(directory, self.profile_count() as usize);
            reserved.extend(listed.into_iter().filter_map(|s| u8::try_from(s).ok()));
        }
        reserved
    }

    /// The profile as the mouse uses it: its sector in the mouse's memory, or the factory profile
    /// if that sector was never written (invalid CRC) or the mouse has no profile list at all (a
    /// sector left there by an older list is not in use). Goes by the list as last read.
    fn read_profile_sector(&mut self, number: u8) -> Result<Vec<u8>> {
        if !self.factory_memory() {
            let sector = self.dev.read_sector(number as u16, self.sector_size())?;
            if profile::crc_is_valid(&sector) {
                return Ok(sector);
            }
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
        let directory = self.read_directory()?;
        let mut used = self.reserved_sectors(directory.as_deref());
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
        let directory = self.read_directory()?;
        let enabled = self.enabled_profiles(directory.as_deref())?;
        match reload_method(active, &enabled[..self.selectable_profiles() as usize]) {
            Reload::Through(other) => {
                self.select_profile(other)?;
                self.select_profile(active)?;
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
        // A mouse nothing was ever saved to has no profile list: the save creates it, with the
        // factory copy of every other profile, so the mouse goes on as before except for this one.
        let listed = self.read_directory()?.is_some();
        self.check_list_for_saving()?;
        let mut sector = self.read_profile_sector(number)?;
        if !self.model.verified {
            self.check_led_slots(&sector, settings)?;
        }
        let mut settings = settings.clone();
        let macro_sectors = self.place_macros(&mut settings)?;
        settings.apply(&mut sector)?;
        let (others, new_list) = if listed {
            (Vec::new(), None)
        } else {
            let others = self.missing_profile_sectors((1..=self.profile_count()).filter(|&n| n != number), false)?;
            let enabled = self.enabled_profiles(None)?;
            (others, Some((profile::build_directory(&enabled, self.sector_size() as usize), enabled)))
        };
        self.backup_before_writing(&format!("profile{number}"))?;
        let written = (|| {
            // Macros first: if writing stops halfway, the profile still points to the old ones.
            for (id, data) in &macro_sectors {
                self.write_verified(*id as u16, data)?;
            }
            for (id, data) in &others {
                self.write_verified(*id as u16, data)?;
            }
            self.write_verified(number as u16, &sector)?;
            match &new_list {
                // The list last: until it is there, the mouse keeps using its factory profiles.
                Some((directory, enabled)) => {
                    self.write_verified(0, directory)?;
                    self.adopt_own_profiles(enabled, number)
                }
                None => self.reload_if_active(number),
            }
        })();
        self.after_writing(written)?;
        self.state()
    }

    pub fn restore_factory_profile(&mut self, number: u8) -> Result<MouseState> {
        self.check_writable()?;
        self.check_number(number)?;
        // Without a profile list the mouse already uses its factory profiles.
        self.read_directory()?;
        if self.factory_memory() {
            return self.state();
        }
        self.check_list_for_saving()?;
        let sector = self.read_factory_sector(number)?;
        self.backup_before_writing(&format!("factory{number}"))?;
        let written = (|| {
            self.write_verified(number as u16, &sector)?;
            self.reload_if_active(number)
        })();
        self.after_writing(written)?;
        self.state()
    }

    /// Backs up the memory before a save. On an experimental mouse the backup must be one PeriHub can
    /// put back, and the undo is armed before the first write, so that a save that stops halfway
    /// is undone too, also if PeriHub is closed or the mouse unplugged (see [`Self::finish_pending_undo`]).
    fn backup_before_writing(&mut self, label: &str) -> Result<()> {
        if self.info.receiver {
            self.check_battery_for_radio_save()?;
        }
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

    /// On an experimental mouse a failed save is undone at once.
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

    /// Through the receiver: a battery that is nearly empty and not charging stops the save.
    fn check_battery_for_radio_save(&mut self) -> Result<()> {
        let battery = match self.dev.battery() {
            Ok(battery) => battery,
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
            Err(_) => None,
        };
        match battery {
            Some(b) if b.state == BatteryState::Discharging && b.percent < MIN_BATTERY_FOR_RADIO_SAVE => {
                Err(Error::Invalid(format!(
                    "nothing was saved: the battery is at {}%. Charge the mouse, or connect it with its USB cable, and save again",
                    b.percent
                )))
            }
            _ => Ok(()),
        }
    }

    /// File that marks an experimental save that was neither kept nor undone yet. It keeps
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

    /// On a model not tested as a whole, LED slots that the mouse does not report as zones keep
    /// their bytes: PeriHub only knows what they mean on the G502 HERO.
    fn check_led_slots(&self, sector: &[u8], settings: &ProfileSettings) -> Result<()> {
        let current = ProfileSettings::parse(sector, self.info.onboard.button_count as usize)?;
        let zones: Vec<u8> = self.info.led_zones.iter().map(|z| z.index).collect();
        if unknown_led_slots_changed(&current, settings, &zones) {
            return Err(Error::Invalid("this mouse has no such LED zone".into()));
        }
        Ok(())
    }

    /// Puts back the memory saved before the last write: the "Keep these changes?" countdown on an
    /// experimental mouse calls this when it is not confirmed.
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
                    self.list = ProfileList::Own;
                    self.select_profile(other)?;
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
        let listed = self.read_directory()?.is_some();
        self.check_list_for_saving()?;
        let mut flags = self.state()?.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>();
        flags[number as usize - 1] = enabled;
        if !flags.iter().any(|&f| f) {
            return Err(Error::Invalid("at least one profile must stay enabled".into()));
        }
        // The list must not point to a sector that holds no profile: a profile that was never
        // saved gets its factory copy first. Without a list yet, that is every profile.
        let missing = if !listed {
            self.missing_profile_sectors(1..=self.profile_count(), false)?
        } else if enabled {
            self.missing_profile_sectors(std::iter::once(number), true)?
        } else {
            Vec::new()
        };
        let directory = profile::build_directory(&flags, self.sector_size() as usize);
        self.backup_all("directory")?;
        for (id, data) in &missing {
            self.write_verified(*id as u16, data)?;
        }
        self.write_verified(0, &directory)?;
        if !listed {
            self.adopt_own_profiles(&flags, number)?;
        }
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
        if number > self.selectable_profiles() {
            return Err(Error::Invalid(format!(
                "profile {number} is not in use yet: the mouse has only its factory profiles until one is saved to it"
            )));
        }
        match self.select_profile(number) {
            // The mouse refuses while another program controls it: check, then try once more.
            Err(Error::Hidpp(HidppErrorCode(0x02))) => {
                self.check_mode()?;
                if self.host_mode.load(Ordering::SeqCst) {
                    return Err(Error::Invalid("another program, such as Logitech G HUB, controls the mouse".into()));
                }
                self.select_profile(number)?;
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
        if self.undo_at.is_some_and(|at| Instant::now() >= at) {
            log::warn!("an experimental save was not kept: undoing it");
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
        let blank = directory.iter().all(|&b| b == 0xFF) || directory.iter().all(|&b| b == 0x00);
        return Err(Error::Invalid(if blank {
            "this backup is from before anything was saved to the mouse: its memory was empty and it used its \
             factory profiles. To go back to those, use Factory settings on each profile"
                .into()
        } else {
            "the profile list in this backup is damaged".into()
        }));
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
    fn support_follows_the_memory_format_not_the_model() {
        let (_, g502) = g502_memory();
        let g403 = model_of(0xC083);
        let hero = model_of(G502_HERO);
        let supported = Support { tested: true, experimental: false, writable: true, writable_format: true };
        let experimental = Support { tested: false, experimental: true, writable: true, writable_format: true };
        let read_only = Support { tested: false, experimental: false, writable: false, writable_format: true };
        let unknown = Support { writable_format: false, ..read_only };

        // The two tested memories make any model supported: format 2 in 256-byte sectors (G502
        // HERO), format 3 in 255-byte ones (G502 X Wireless, issue #15).
        let format_3 = OnboardInfo { profile_format: 3, sector_size: 255, ..g502.clone() };
        for memory in [&g502, &format_3] {
            assert_eq!(support(g403, memory, false, false), supported);
            assert_eq!(support(g403, memory, false, true), supported);
        }
        // Through a receiver: experimental saves on a tested memory, in the interface only.
        let over_radio = support(g403, &g502, true, true);
        assert_eq!(over_radio, Support { experimental: true, ..supported });
        assert_eq!(over_radio.access(), Access::Full);
        let command_line = support(g403, &g502, true, false);
        assert_eq!(command_line, Support { writable: false, ..supported });
        assert_eq!(command_line.access(), Access::Ram);

        // A memory PeriHub can write but never saved to: experimental, only where saves can be undone.
        let untested = OnboardInfo { sector_size: 255, ..g502.clone() };
        assert_eq!(support(g403, &untested, false, true), experimental);
        assert_eq!(support(g403, &untested, false, false), read_only, "the command line");
        assert_eq!(support(g403, &untested, true, true), read_only, "through a receiver");
        assert_eq!(read_only.access(), Access::ReadOnly);
        // A model tested as a whole stays tested: the G502 HERO with 255-byte sectors (issue #20).
        assert_eq!(support(hero, &untested, false, false), supported);

        // Memories PeriHub cannot write.
        for format in [0, 1, 4, 5, 6, 7] {
            let other = OnboardInfo { profile_format: format, ..g502.clone() };
            assert_eq!(support(g403, &other, false, true), unknown, "profile format {format}");
        }
        for size in [240, 512] {
            let other = OnboardInfo { sector_size: size, ..g502.clone() };
            assert_eq!(support(g403, &other, false, true), unknown, "{size}-byte sectors");
        }
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

    // ---- A simulated mouse: whole saves, as the mouse sees them ----

    use std::sync::Mutex;

    use crate::hidpp::tests::{FakeDevice, reply};
    use crate::hidpp::{feature, models};

    const SIM_FEATURES: &[u16] = &[
        feature::ROOT,
        feature::DEVICE_INFO,
        feature::DEVICE_NAME,
        feature::ADJUSTABLE_DPI,
        feature::REPORT_RATE,
        feature::ONBOARD_PROFILES,
        feature::FEATURE_SET,
    ];

    /// The memory and state of a HID++ mouse with onboard profiles, as far as PeriHub talks to one.
    struct Sim {
        onboard: OnboardInfo,
        /// The mouse's own sectors, from the profile list (0) on.
        user: Vec<Vec<u8>>,
        /// The factory sectors: the list at 0x0100, the profiles after it.
        rom: HashMap<u16, Vec<u8>>,
        active: [u8; 2],
        writing: Option<(u16, usize, Vec<u8>)>,
        /// Sectors written, in order.
        written: Vec<u16>,
        /// Profile addresses selected, in order.
        selected: Vec<[u8; 2]>,
        /// Like the G305: reading the profile list fails until one was written.
        refuses_blank_list: bool,
    }

    impl Sim {
        /// The G502 HERO of the fixtures. Its factory profiles are its saved ones at 400 DPI more,
        /// and the factory list enables the first three.
        fn g502() -> Self {
            let (user, onboard) = g502_memory();
            Self::new(user, onboard)
        }

        fn new(user: Vec<Vec<u8>>, onboard: OnboardInfo) -> Self {
            let size = onboard.sector_size as usize;
            let mut rom = HashMap::new();
            let mut list = vec![0xFF; size];
            for n in 1..=onboard.rom_profile_count {
                let mut sector = user[n as usize].clone();
                let dpi = u16::from_le_bytes([sector[3], sector[4]]) + 400;
                sector[3..5].copy_from_slice(&dpi.to_le_bytes());
                rom.insert(ROM_SECTOR_BASE + n as u16, sector);
                let at = 4 * (n as usize - 1);
                list[at..at + 4].copy_from_slice(&[0x01, n, (n <= 3) as u8, 0xFF]);
            }
            let end = 4 * onboard.rom_profile_count as usize;
            list[end..end + 4].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
            rom.insert(ROM_SECTOR_BASE, list);
            Self {
                onboard,
                user,
                rom,
                active: [0, 1],
                writing: None,
                written: Vec::new(),
                selected: Vec::new(),
                refuses_blank_list: false,
            }
        }

        /// The same mouse before anything was saved to it: an empty memory, on factory profile 1.
        fn blank(mut self) -> Self {
            let size = self.onboard.sector_size as usize;
            self.user.iter_mut().for_each(|sector| *sector = vec![0xFF; size]);
            self.active = [1, 1];
            self
        }

        /// The memory of a mouse with `size`-byte sectors and this profile format: the same
        /// profiles, with the CRC where that sector size puts it.
        fn resized(mut self, size: usize, profile_format: u8) -> Self {
            for sector in self.user.iter_mut().chain(self.rom.values_mut()) {
                sector.truncate(size);
                profile::write_crc(sector);
            }
            self.onboard.sector_size = size as u16;
            self.onboard.profile_format = profile_format;
            self
        }

        fn factory(&self, number: u8) -> Vec<u8> {
            let mut sector = self.rom[&(ROM_SECTOR_BASE + number as u16)].clone();
            profile::write_crc(&mut sector);
            sector
        }

        fn answer(&mut self, feature: u16, function: u8, p: &[u8]) -> std::result::Result<Vec<u8>, u8> {
            const INVALID_ARGUMENT: u8 = 2;
            let info = &self.onboard;
            Ok(match (feature, function) {
                (feature::DEVICE_INFO, 0) => vec![1],
                (feature::DEVICE_INFO, 1) => vec![0, b'U', b'1', b' ', 0x27, 0x03, 0x00, 0x00],
                // The count leaves out the root, at index 0.
                (feature::FEATURE_SET, 0) => vec![SIM_FEATURES.len() as u8 - 1],
                (feature::FEATURE_SET, 1) => {
                    let id = *SIM_FEATURES.get(p[0] as usize).ok_or(INVALID_ARGUMENT)?;
                    vec![(id >> 8) as u8, id as u8, 0, 0]
                }
                (feature::DEVICE_NAME, 0) => vec![4],
                (feature::DEVICE_NAME, 1) => b"Test".to_vec(),
                // 100 to 25600 DPI in steps of 50.
                (feature::ADJUSTABLE_DPI, 1) => vec![0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00],
                (feature::ADJUSTABLE_DPI, 2) => vec![0, 0x03, 0x20, 0x03, 0x20],
                (feature::ADJUSTABLE_DPI, 3) => vec![],
                // 1, 2, 4 and 8 ms.
                (feature::REPORT_RATE, 0) => vec![0x8B],
                (feature::ONBOARD_PROFILES, 0) => {
                    let [hi, lo] = info.sector_size.to_be_bytes();
                    vec![
                        info.memory_model,
                        info.profile_format,
                        info.macro_format,
                        info.profile_count,
                        info.rom_profile_count,
                        info.button_count,
                        info.sector_count,
                        hi,
                        lo,
                        0x0A,
                    ]
                }
                (feature::ONBOARD_PROFILES, 1) => vec![],
                (feature::ONBOARD_PROFILES, 2) => vec![ONBOARD_MODE],
                (feature::ONBOARD_PROFILES, 3) => {
                    let limit = if p[0] == 1 { info.rom_profile_count } else { info.profile_count };
                    if p[0] > 1 || !(1..=limit).contains(&p[1]) {
                        return Err(INVALID_ARGUMENT);
                    }
                    self.active = [p[0], p[1]];
                    self.selected.push(self.active);
                    vec![]
                }
                (feature::ONBOARD_PROFILES, 4) => self.active.to_vec(),
                (feature::ONBOARD_PROFILES, 5) => {
                    let (sector, offset) =
                        (u16::from_be_bytes([p[0], p[1]]), u16::from_be_bytes([p[2], p[3]]) as usize);
                    let data = match self.user.get(sector as usize) {
                        Some(data) => data,
                        None => self.rom.get(&sector).ok_or(INVALID_ARGUMENT)?,
                    };
                    if sector == 0 && self.refuses_blank_list && !profile::crc_is_valid(data) {
                        return Err(INVALID_ARGUMENT);
                    }
                    data.get(offset..offset + 16).ok_or(INVALID_ARGUMENT)?.to_vec()
                }
                (feature::ONBOARD_PROFILES, 6) => {
                    let (sector, length) =
                        (u16::from_be_bytes([p[0], p[1]]), u16::from_be_bytes([p[4], p[5]]) as usize);
                    if sector as usize >= self.user.len() || length != info.sector_size as usize {
                        return Err(INVALID_ARGUMENT);
                    }
                    self.writing = Some((sector, length, Vec::new()));
                    vec![]
                }
                (feature::ONBOARD_PROFILES, 7) => {
                    self.writing.as_mut().ok_or(INVALID_ARGUMENT)?.2.extend_from_slice(&p[..16]);
                    vec![]
                }
                (feature::ONBOARD_PROFILES, 8) => {
                    let (sector, length, data) = self.writing.take().ok_or(INVALID_ARGUMENT)?;
                    self.user[sector as usize] = data.get(..length).ok_or(INVALID_ARGUMENT)?.to_vec();
                    self.written.push(sector);
                    vec![]
                }
                (feature::ONBOARD_PROFILES, 0x0B) => vec![0],
                (feature::ONBOARD_PROFILES, 0x0C) => vec![],
                _ => return Err(INVALID_ARGUMENT),
            })
        }
    }

    fn model_of(product_id: u16) -> &'static MouseModel {
        models::MODELS.iter().find(|m| m.product_id == product_id).unwrap()
    }

    const G502_HERO: u16 = 0xC08B;
    const G502_X_WIRELESS: u16 = 0xC098;
    const G502_X_PLUS: u16 = 0xC095;

    /// PeriHub's connection to the simulated mouse, with its cable.
    fn open_sim(sim: Sim, product_id: u16, untested_saves: bool) -> (MouseTask, Arc<Mutex<Sim>>) {
        open_sim_through(sim, product_id, untested_saves, false)
    }

    /// PeriHub's connection to the simulated mouse, with its cable or through a receiver. The
    /// connection starts read-only, as the manager opens it.
    fn open_sim_through(
        sim: Sim,
        product_id: u16,
        untested_saves: bool,
        receiver: bool,
    ) -> (MouseTask, Arc<Mutex<Sim>>) {
        let sim = Arc::new(Mutex::new(sim));
        let mouse = sim.clone();
        let fake = FakeDevice::new(move |request| {
            let (index, function) = (request[2] as usize, request[3] >> 4);
            if index == 0 && function == 1 {
                return vec![reply(request, &[4, 2, request[6]])];
            }
            if index == 0 && function == 0 {
                let id = u16::from_be_bytes([request[4], request[5]]);
                let found = SIM_FEATURES.iter().position(|&f| f == id).map_or(0, |i| i as u8);
                return vec![reply(request, &[found])];
            }
            match mouse.lock().unwrap().answer(SIM_FEATURES[index], function, &request[4..]) {
                Ok(payload) => vec![reply(request, &payload)],
                Err(code) => {
                    let mut error = reply(request, &[]);
                    error[2..6].copy_from_slice(&[0xFF, request[2], request[3], code]);
                    vec![error]
                }
            }
        });
        let dev = if receiver {
            HidppDevice::behind_receiver(fake, None, 1, true).unwrap()
        } else {
            HidppDevice::new(fake, models::WIRED_INDEX, true).unwrap()
        };
        let task = MouseTask::open(dev, model_of(product_id), untested_saves).unwrap();
        (task, sim)
    }

    fn parsed(sector: &[u8]) -> ProfileSettings {
        ProfileSettings::parse(sector, 11).unwrap()
    }

    /// The settings of `profile` with another first DPI level.
    fn edited(state: &MouseState, profile: u8) -> ProfileSettings {
        let mut settings = state.profiles[profile as usize - 1].settings.clone();
        settings.dpi_levels[0] = 3250;
        settings
    }

    fn forget_test_files() {
        let _ = std::fs::remove_dir_all(config::config_dir());
    }

    #[test]
    fn a_save_writes_only_that_profile_when_the_mouse_has_a_profile_list() {
        let (mut task, sim) = open_sim(Sim::g502(), G502_HERO, false);
        let state = task.state().unwrap();
        assert_eq!(state.profile_list, ProfileList::Own);
        let before = sim.lock().unwrap().user.clone();

        let state = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(state.profiles[1].settings.dpi_levels[0], 3250);
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.written, [2]);
        assert!(mouse.selected.is_empty(), "profile 2 is not the active one");
        for sector in (0..16).filter(|&s| s != 2) {
            assert_eq!(mouse.user[sector], before[sector], "sector {sector}");
        }
        drop(mouse);

        // The active profile is loaded again, through another profile of the mouse's own memory.
        task.save_profile(1, &edited(&state, 1)).unwrap();
        assert_eq!(sim.lock().unwrap().selected, [[0, 2], [0, 1]]);
        forget_test_files();
    }

    #[test]
    fn a_mouse_nothing_was_saved_to_shows_its_factory_profiles() {
        let mut sim = Sim::g502().blank();
        // A profile left from an older list is not what the mouse uses.
        sim.user[2] = g502_memory().0[2].clone();
        let factory: Vec<ProfileSettings> = (1..=5).map(|n| parsed(&sim.factory(n))).collect();
        let (mut task, sim) = open_sim(sim, G502_HERO, false);

        let state = task.state().unwrap();
        assert_eq!(state.profile_list, ProfileList::Factory);
        assert_eq!(state.active_profile, 1);
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, true, true, false, false]);
        for (profile, factory) in state.profiles.iter().zip(&factory) {
            assert_eq!(&profile.settings, factory, "profile {}", profile.number);
        }

        // Profiles are selected among the factory ones.
        task.set_active_profile(3).unwrap();
        assert_eq!(sim.lock().unwrap().selected, [[1, 3]]);
        assert!(sim.lock().unwrap().written.is_empty());
    }

    #[test]
    fn a_mouse_that_refuses_to_read_a_missing_profile_list_is_read_too() {
        let mut sim = Sim::g502().blank();
        sim.refuses_blank_list = true;
        let (mut task, _) = open_sim(sim, G502_HERO, false);
        assert_eq!(task.state().unwrap().profile_list, ProfileList::Factory);
    }

    #[test]
    fn only_the_factory_profiles_can_be_selected_without_a_profile_list() {
        let (memory, onboard) = g502_memory();
        let sim = Sim::new(memory, OnboardInfo { rom_profile_count: 2, ..onboard }).blank();
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        task.set_active_profile(2).unwrap();
        assert!(task.set_active_profile(3).is_err());
        assert_eq!(sim.lock().unwrap().selected, [[1, 2]]);
        // Profiles past the factory ones start as copies of the first.
        let state = task.state().unwrap();
        assert_eq!(state.profiles[4].settings, state.profiles[0].settings);
    }

    #[test]
    fn the_first_save_creates_the_profile_list() {
        let mut sim = Sim::g502().blank();
        sim.user[4] = g502_memory().0[4].clone();
        let factory: Vec<Vec<u8>> = (0..=5).map(|n| if n == 0 { Vec::new() } else { sim.factory(n) }).collect();
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        let state = task.state().unwrap();

        let state = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(state.profile_list, ProfileList::Own);
        assert_eq!(state.active_profile, 1);
        assert_eq!(state.profiles[1].settings.dpi_levels[0], 3250);
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, true, true, false, false]);

        let mouse = sim.lock().unwrap();
        // Every other profile gets its factory copy, then the saved one, and the list last.
        assert_eq!(mouse.written, [1, 3, 4, 5, 2, 0]);
        for n in [1, 3, 4, 5] {
            assert_eq!(mouse.user[n], factory[n], "profile {n}");
        }
        let mut expected = parsed(&factory[2]);
        expected.dpi_levels[0] = 3250;
        assert_eq!(parsed(&mouse.user[2]), expected);
        assert!(profile::crc_is_valid(&mouse.user[0]));
        assert_eq!(profile::directory_sectors(&mouse.user[0], 5), [1, 2, 3, 4, 5]);
        // From factory profile 1 to profile 1 of its own memory.
        assert_eq!(mouse.selected, [[0, 1]]);
        drop(mouse);

        // From now on a save writes that profile only.
        sim.lock().unwrap().written.clear();
        task.save_profile(3, &edited(&state, 3)).unwrap();
        assert_eq!(sim.lock().unwrap().written, [3]);
        forget_test_files();
    }

    #[test]
    fn enabling_a_profile_creates_the_missing_list_too() {
        let (mut task, sim) = open_sim(Sim::g502().blank(), G502_HERO, false);
        let state = task.set_profile_enabled(4, true).unwrap();
        assert_eq!(state.profile_list, ProfileList::Own);
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, true, true, true, false]);
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.written, [1, 2, 3, 4, 5, 0]);
        assert_eq!(mouse.selected, [[0, 1]]);
        forget_test_files();
    }

    #[test]
    fn a_profile_that_was_never_saved_gets_its_factory_copy_before_it_is_enabled() {
        let mut sim = Sim::g502();
        sim.user[0] = profile::build_directory(&[true, true, true, false, true], 256);
        sim.user[4] = vec![0xFF; 256];
        let factory = sim.factory(4);
        let (mut task, sim) = open_sim(sim, G502_HERO, false);

        task.set_profile_enabled(4, true).unwrap();
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.written, [4, 0]);
        assert_eq!(mouse.user[4], factory);
        drop(mouse);

        // Disabling writes the list only, and so does enabling a profile that is there.
        sim.lock().unwrap().written.clear();
        task.set_profile_enabled(4, false).unwrap();
        task.set_profile_enabled(4, true).unwrap();
        assert_eq!(sim.lock().unwrap().written, [0, 0]);
        forget_test_files();
    }

    #[test]
    fn restoring_factory_settings_writes_nothing_to_a_mouse_without_a_profile_list() {
        let (mut task, sim) = open_sim(Sim::g502().blank(), G502_HERO, false);
        assert_eq!(task.restore_factory_profile(2).unwrap().profile_list, ProfileList::Factory);
        assert!(sim.lock().unwrap().written.is_empty());
    }

    #[test]
    fn a_damaged_profile_list_is_never_taken_for_a_blank_one() {
        let mut sim = Sim::g502();
        // A list with its entries and a checksum that does not match.
        sim.user[0][254] ^= 0xFF;
        let saved: Vec<ProfileSettings> = (1..=5).map(|n| parsed(&sim.user[n])).collect();
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        assert_eq!(task.info.profile_list, ProfileList::Damaged);

        // Read as before: the profiles in the mouse's memory, all enabled.
        let state = task.state().unwrap();
        assert_eq!(state.profile_list, ProfileList::Damaged);
        assert!(state.profiles.iter().all(|p| p.enabled));
        for (profile, saved) in state.profiles.iter().zip(&saved) {
            assert_eq!(&profile.settings, saved);
        }

        // Nothing is written over it: no profile, no factory copies, no new list.
        let error = task.save_profile(2, &edited(&state, 2)).unwrap_err().to_string();
        assert!(error.contains("damaged"), "{error}");
        assert!(task.set_profile_enabled(2, false).is_err());
        assert!(task.restore_factory_profile(2).is_err());
        assert!(sim.lock().unwrap().written.is_empty());

        // Profiles are still selected in the mouse's own memory.
        task.set_active_profile(2).unwrap();
        assert_eq!(sim.lock().unwrap().selected, [[0, 2]]);
        forget_test_files();
    }

    /// The G502 X Wireless of issue #15, as its test report describes it.
    fn g502_x() -> Sim {
        let (memory, onboard) = g502_memory();
        Sim::new(memory, OnboardInfo { rom_profile_count: 2, ..onboard }).resized(255, 3)
    }

    /// A memory PeriHub can write but has never saved to on a real mouse: format 2 in 255-byte
    /// sectors, on a model that was not tested as a whole.
    fn untested_memory() -> Sim {
        let (memory, onboard) = g502_memory();
        Sim::new(memory, OnboardInfo { rom_profile_count: 2, ..onboard }).resized(255, 2)
    }

    #[test]
    fn a_model_nobody_tested_is_supported_when_its_memory_format_was_tested() {
        // The G502 X PLUS with the G502 X Wireless's memory: format 3, 255-byte sectors.
        let (mut task, sim) = open_sim(g502_x(), G502_X_PLUS, false);
        assert!(task.info.verified && task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        task.save_profile(2, &edited(&state, 2)).unwrap();
        assert!(!task.pending_undo_path().exists(), "no 15-second undo");
        task.set_profile_enabled(3, false).unwrap();
        task.set_active_profile(2).unwrap();
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.written, [2, 0]);
        assert_eq!(mouse.selected, [[0, 2]]);
        drop(mouse);
        forget_test_files();
    }

    #[test]
    fn a_memory_format_perihub_cannot_write_is_locked_down_to_the_hidpp_layer() {
        let (memory, onboard) = g502_memory();
        let format_4 = Sim::new(memory, onboard).resized(256, 4);
        let (mut task, sim) = open_sim(format_4, G502_X_PLUS, true);
        assert!(!task.info.verified && !task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        assert!(task.save_profile(2, &edited(&state, 2)).is_err());
        assert!(task.set_active_profile(2).is_err());
        // Not even a call straight to the device gets through.
        assert!(task.dev.set_active_profile(false, 2).is_err());
        assert!(task.dev.set_onboard_mode(HOST_MODE).is_err());
        let mouse = sim.lock().unwrap();
        assert!(mouse.written.is_empty() && mouse.selected.is_empty());
    }

    #[test]
    fn the_feature_list_is_read_from_a_read_only_mouse_behind_a_receiver() {
        // Issue #22: a G903 HERO, profile format 4 with 255-byte sectors, through its receiver.
        let (memory, onboard) = g502_memory();
        let format_4 = Sim::new(memory, onboard).resized(255, 4);
        let (task, _) = open_sim_through(format_4, G502_X_PLUS, true, true);
        assert!(!task.info.writable && !task.info.writable_format && !task.info.experimental);
        let ids: Vec<u16> = task.info.features.iter().map(|f| f.id).collect();
        assert_eq!(ids, SIM_FEATURES[1..]);
    }

    #[test]
    fn a_save_through_a_receiver_is_experimental_and_can_be_undone() {
        // A tested memory (issue #4): saved over the radio, with the 15-second undo.
        let (mut task, sim) = open_sim_through(g502_x(), G502_X_WIRELESS, true, true);
        assert!(task.info.verified && task.info.writable && task.info.experimental);
        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();
        let saved = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(saved.profiles[1].settings.dpi_levels[0], 3250);
        assert!(task.pending_undo_path().exists(), "the undo is armed");
        assert_eq!(sim.lock().unwrap().written, [2]);
        task.undo_last_write().unwrap();
        assert_eq!(sim.lock().unwrap().user, before);
        assert!(!task.pending_undo_path().exists());
        // The profile list stays as it is.
        assert!(task.set_profile_enabled(2, false).is_err());
        task.set_active_profile(2).unwrap();
        forget_test_files();

        // The command line never saves over the radio: the profile is switched, nothing is written.
        let (mut task, sim) = open_sim_through(g502_x(), G502_X_WIRELESS, false, true);
        assert!(task.info.verified && !task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        assert!(task.save_profile(2, &edited(&state, 2)).is_err());
        let sector = sim.lock().unwrap().user[2].clone();
        assert!(task.dev.write_sector(2, &sector, &task.info.onboard).is_err());
        task.set_active_profile(2).unwrap();
        assert!(sim.lock().unwrap().written.is_empty());
        assert_eq!(sim.lock().unwrap().selected, [[0, 2]]);

        // An untested one: only read.
        let (mut task, sim) = open_sim_through(untested_memory(), G502_X_PLUS, true, true);
        assert!(!task.info.verified && !task.info.writable && !task.info.experimental);
        assert!(task.set_active_profile(2).is_err());
        assert!(sim.lock().unwrap().selected.is_empty());
    }

    #[test]
    fn the_command_line_does_not_save_to_an_untested_memory() {
        let (mut task, sim) = open_sim(untested_memory(), G502_X_PLUS, false);
        assert!(!task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        assert!(task.save_profile(2, &edited(&state, 2)).is_err());
        assert!(sim.lock().unwrap().written.is_empty());
    }

    #[test]
    fn the_g502_hero_with_255_byte_sectors_still_creates_its_profile_list() {
        let sim = Sim::g502().resized(255, 2).blank();
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        assert!(task.info.verified && task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        assert_eq!(state.profile_list, ProfileList::Factory);
        let state = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(state.profile_list, ProfileList::Own);
        assert_eq!(sim.lock().unwrap().written.last(), Some(&0));
        forget_test_files();
    }

    #[test]
    fn an_experimental_save_to_255_byte_sectors_can_be_kept_or_undone() {
        let (mut task, sim) = open_sim(untested_memory(), G502_X_PLUS, true);
        assert!(task.info.writable && task.info.experimental && !task.info.verified);
        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();

        let saved = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(saved.profiles[1].settings.dpi_levels[0], 3250);
        assert!(task.pending_undo_path().exists(), "the undo is armed");
        {
            let mouse = sim.lock().unwrap();
            assert_eq!(mouse.written, [2]);
            assert_eq!(mouse.user[2].len(), 255);
            assert!(profile::crc_is_valid(&mouse.user[2]));
            // Nothing but the DPI level and the CRC changed.
            let changed: Vec<usize> = (0..255).filter(|&i| mouse.user[2][i] != before[2][i]).collect();
            assert_eq!(changed, [3, 4, 253, 254]);
        }

        task.undo_last_write().unwrap();
        assert_eq!(sim.lock().unwrap().user, before);
        assert!(!task.pending_undo_path().exists());

        // The profile list stays as it is on an experimental mouse.
        assert!(task.set_profile_enabled(2, false).is_err());

        task.save_profile(2, &edited(&state, 2)).unwrap();
        task.keep_changes();
        assert!(!task.pending_undo_path().exists());
        assert_eq!(parsed(&sim.lock().unwrap().user[2]).dpi_levels[0], 3250);
        forget_test_files();
    }

    #[test]
    fn the_g502_x_wireless_is_supported_straight_away() {
        let (mut task, sim) = open_sim(g502_x(), G502_X_WIRELESS, false);
        assert!(task.info.verified && task.info.writable && !task.info.experimental);
        let state = task.state().unwrap();
        let saved = task.save_profile(2, &edited(&state, 2)).unwrap();
        assert_eq!(saved.profiles[1].settings.dpi_levels[0], 3250);
        assert!(!task.pending_undo_path().exists(), "verified mice do not need a 15-second undo");
        assert_eq!(sim.lock().unwrap().written, [2]);
        forget_test_files();
    }

    #[test]
    fn a_macro_longer_than_a_255_byte_sector_is_saved_and_read_back() {
        use crate::hidpp::macros::{MacroKind, MacroStep};
        let (mut task, sim) = open_sim(g502_x(), G502_X_WIRELESS, false);
        let state = task.state().unwrap();
        // 120 key presses: more than one sector of 253 bytes of macro.
        let press: Vec<MacroStep> = (0..120u8)
            .flat_map(|i| {
                let key = 0x04 + i % 26;
                [MacroStep::KeyDown { modifiers: 0, key }, MacroStep::KeyUp { modifiers: 0, key }]
            })
            .collect();
        let long = Macro { kind: MacroKind::Once, press, release: Vec::new() };
        let mut settings = state.profiles[1].settings.clone();
        settings.buttons[5] = ButtonAction::Macro { sector: 0, offset: 0, content: Some(long.clone()) };

        let saved = task.save_profile(2, &settings).unwrap();
        let ButtonAction::Macro { content, .. } = &saved.profiles[1].settings.buttons[5] else {
            panic!("button 6 holds no macro: {:?}", saved.profiles[1].settings.buttons[5]);
        };
        assert_eq!(content.as_ref(), Some(&long));
        let mouse = sim.lock().unwrap();
        // The macro's sectors first, the profile that points to them last.
        assert!(mouse.written.len() >= 3, "{:?}", mouse.written);
        assert_eq!(mouse.written.last(), Some(&2));
        assert!(mouse.written[..mouse.written.len() - 1].iter().all(|&s| s > 5));
        for &sector in &mouse.written {
            assert_eq!(mouse.user[sector as usize].len(), 255);
            assert!(profile::crc_is_valid(&mouse.user[sector as usize]), "sector {sector}");
        }
        drop(mouse);
        task.keep_changes();
        forget_test_files();
    }

    #[test]
    fn an_experimental_mouse_without_a_profile_list_is_not_saved_to() {
        let (mut task, sim) = open_sim(untested_memory().blank(), G502_X_PLUS, true);
        let state = task.state().unwrap();
        assert_eq!(state.profile_list, ProfileList::Factory);
        let error = task.save_profile(1, &edited(&state, 1)).unwrap_err().to_string();
        assert!(error.contains("never had profiles saved"), "{error}");
        assert!(sim.lock().unwrap().written.is_empty());
        assert!(!task.pending_undo_path().exists());
        forget_test_files();
    }
}

//! Logitech mouse state and operations, run on the device's dedicated thread.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{self, BackupEntry, write_backup};
use crate::confirmed;
use crate::conflicts;
use crate::error::{Error, HidppErrorCode, Result};
use crate::hidpp::macros::{self, Macro, Packer};
use crate::hidpp::models::MouseModel;
use crate::hidpp::profile::{self, ButtonAction, ExtendedProfile, LedEffect, ProfileSettings};
use crate::hidpp::{
    Access, Battery, BatteryState, DpiCapabilities, ExtendedDpiNow, FeatureEntry, FirmwareEntity, HidppDevice, LedZone,
    OnboardInfo, ROM_SECTOR_BASE, feature,
};
use crate::worker::DeviceTask;

/// Profile formats PeriHub writes, with one layout for all three, as libratbag and Solaar do: format 4 (the
/// G903 HERO's, issue #115) only adds fields after the ones PeriHub writes (docs/protocols/logitech-onboard-profiles.md).
/// The others can be read but not written.
pub const WRITABLE_PROFILE_FORMATS: &[u8] = &[2, 3, 4];
/// Sector sizes PeriHub writes: 256 bytes, or 255 with the CRC one byte earlier (other mice, and
/// newer G502 HERO firmware: issue #20).
pub const WRITABLE_SECTOR_SIZES: std::ops::RangeInclusive<usize> =
    profile::MIN_WRITE_SECTOR_SIZE..=profile::VERIFIED_SECTOR_SIZE;
/// Whether a memory format (profile format, sector size) was saved to on a real mouse, with its USB
/// cable: the table is `confirmed::MEMORY`, where `cargo xtask confirm` adds the formats testers
/// confirm. The memory format decides how a save is laid out, not the model.
fn tested_memory(profile_format: u8, sector_size: u16) -> bool {
    confirmed::memory_tested(profile_format, sector_size)
}
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
    /// Connected over Bluetooth Low Energy: the id the mouse has there (the G604's `b024`). PeriHub only reads
    /// such a mouse (see [`MouseTask::open`]): none of the changes the other connections allow is tried over
    /// Bluetooth yet.
    pub bluetooth: Option<u16>,
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
    /// What the PRO X 2 family's extended features (0x2202, 0x8061) and format 7 profiles report,
    /// which PeriHub only reads for now.
    pub extended: Option<ExtendedReads>,
    pub led_zones: Vec<LedZone>,
    /// The HID++ features the mouse lists, for test reports: they show what a mouse has that
    /// PeriHub does not read yet.
    pub features: Vec<FeatureEntry>,
    pub onboard: OnboardInfo,
    pub writable: bool,
    /// PeriHub knows how to write the memory format the mouse reports: with its cable (and in the
    /// interface, for an untested format) it can be saved to.
    pub writable_format: bool,
    /// The mouse's profile list when it was connected (for test reports;
    /// [`MouseState::profile_list`] is the state now).
    pub profile_list: ProfileList,
    /// Which profiles are enabled (`true`: selectable in the mouse's profile list or factory
    /// profiles). Updated whenever the profile list is read or saved.
    #[serde(default)]
    pub enabled_profiles: Vec<bool>,
    /// Every request and answer of the reading when PeriHub connected, in full, as a transcript
    /// (`crate::replay`) in JSON, for test reports. Nothing in it tells the unit apart.
    pub transcript: String,
}

impl MouseInfo {
    /// Connected by radio, through a receiver or over Bluetooth: it has a battery to watch, and the
    /// polling rate of its wireless connection counts.
    fn over_radio(&self) -> bool {
        self.receiver || self.bluetooth.is_some()
    }
}

/// What mice with the extended DPI and polling features (0x2202, 0x8061) and format 7 profiles
/// tell, read-only, for the mouse page and the test report. A read that failed is in `errors`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExtendedReads {
    /// The Y axis, when it has a range of its own (the X axis is [`MouseInfo::dpi`]).
    pub dpi_y: Option<DpiCapabilities>,
    pub dpi_now: Option<ExtendedDpiNow>,
    /// Polling rates in Hz on each connection.
    pub rates_wired_hz: Vec<u16>,
    pub rates_wireless_hz: Vec<u16>,
    /// The rate in use on the connection the mouse is on.
    pub rate_now_hz: Option<u16>,
    /// What the mouse sent, as received, so that a wrong reading shows in the report.
    pub raw: Vec<String>,
    /// One line for each format 7 profile.
    pub profiles: Vec<String>,
    pub errors: Vec<String>,
}

impl ExtendedReads {
    /// Reads what the mouse has, with read commands only. Errors other than the mouse going
    /// away are kept in the result.
    fn read(dev: &mut HidppDevice) -> Result<Self> {
        let wireless = dev.is_behind_receiver();
        // The polling rate feature numbers two connections, the cable and the LIGHTSPEED receiver: it says
        // nothing about Bluetooth, so nothing is asked of it there.
        let rates = !dev.is_over_bluetooth();
        let mut out = Self::default();
        let noted = |errors: &mut Vec<String>, what: &str, error: Error| match error {
            Error::Unreachable => Err(Error::Unreachable),
            error => {
                errors.push(format!("{what}: {error}"));
                Ok(())
            }
        };
        if dev.has_feature(feature::EXTENDED_DPI) {
            match dev.extended_dpi(0) {
                Ok(dpi) => {
                    match dev.extended_sensor_dpi(0, &dpi) {
                        Ok(now) => out.dpi_now = Some(now),
                        Err(error) => noted(&mut out.errors, "DPI now", error)?,
                    }
                    out.dpi_y = dpi.y;
                }
                Err(error) => noted(&mut out.errors, "DPI (0x2202)", error)?,
            }
        }
        if rates && dev.has_feature(feature::EXTENDED_REPORT_RATE) {
            for (name, on_receiver) in [("cable", false), ("receiver", true)] {
                match dev.extended_report_rates(on_receiver) {
                    Ok((rates, raw)) => {
                        out.raw.push(format!("rates {name}: {:02x} {:02x}", raw[0], raw[1]));
                        if on_receiver { out.rates_wireless_hz = rates } else { out.rates_wired_hz = rates }
                    }
                    Err(error) => noted(&mut out.errors, &format!("polling rates ({name})"), error)?,
                }
            }
            match dev.extended_report_rate(wireless) {
                Ok((hz, index)) => {
                    out.raw.push(format!("rate in use: index {index}"));
                    out.rate_now_hz = hz;
                }
                Err(error) => noted(&mut out.errors, "polling rate in use", error)?,
            }
        }
        Ok(out)
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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

/// Said when a change is asked of a mouse connected over Bluetooth.
const BLUETOOTH_READ_ONLY: &str =
    "PeriHub only reads a mouse connected over Bluetooth: use its LIGHTSPEED receiver to change it";

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
    /// Enabled profiles from the last profile list read (shared with the manager, for the status).
    enabled_profiles_state: Arc<Mutex<Vec<bool>>>,
    last_mode_check: Instant,
    /// The slow battery read for a mouse on its receiver (see [`crate::battery::Schedule`]).
    battery_read: crate::battery::Schedule,
    /// The mouse answered and has no battery feature: it is not asked again.
    no_battery: bool,
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

impl Drop for MouseTask {
    /// A mouse that went away (or fell asleep behind its receiver) shows no battery any more.
    fn drop(&mut self) {
        crate::battery::forget(&self.battery_id());
    }
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
/// - a tested memory (see [`tested_memory`]), or a model tested as a whole: supported;
/// - another memory PeriHub can write: experimental saves, if `untested_saves` (the interface asks
///   whether to keep each one; the command line never saves to such a mouse);
/// - any other memory: read-only.
///
/// Through a LIGHTSPEED receiver, as G HUB does, but experimental and only on a tested
/// memory: only one mouse has been saved to over the radio (issue #28), so each save is undone unless
/// the user keeps it, and a memory nobody has tested is not tried over the radio on top of that.
pub fn support(model: &MouseModel, onboard: &OnboardInfo, receiver: bool, untested_saves: bool) -> Support {
    let known = WRITABLE_PROFILE_FORMATS.contains(&onboard.profile_format)
        && WRITABLE_SECTOR_SIZES.contains(&(onboard.sector_size as usize));
    let tested = known && (model.verified || tested_memory(onboard.profile_format, onboard.sector_size));
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
    /// (verified on the G502 HERO). Always an enabled profile: see [`reload_method`].
    Through(u8),
    /// Host mode, then onboard mode again: drops a DPI preview on the G502 HERO and keeps the
    /// profile and DPI level (verified on 2026-11-05). For mice with a single profile, such as
    /// the G403 HERO, where there is no other profile to go through.
    ModeSwitch,
    /// The other profiles are all disabled in the mouse's list: nothing is sent. A profile the list
    /// disables is never selected (libratbag enables it first; a G303 Shroud Edition most likely
    /// answered "invalid argument" to that switch, issue #34). A written profile takes effect by itself on the
    /// G502 HERO (verified on 2026-11-05); a preview stays until the mouse changes level.
    Stay,
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

/// Mice with more profiles keep the way verified since 0.1.0, through another enabled profile only.
fn reload_method(active: u8, enabled: &[bool]) -> Reload {
    if enabled.len() <= 1 {
        return Reload::ModeSwitch;
    }
    match (1..=enabled.len() as u8).find(|&n| n != active && enabled[n as usize - 1]) {
        Some(other) => Reload::Through(other),
        None => Reload::Stay,
    }
}

/// The mouse's lighting zones: from 0x8070, or from 0x8071 on a mouse that has that one instead (the G903 HERO,
/// issue #115), which PeriHub only reads and shows: which LED slot of a profile holds which 0x8071 zone is not
/// known (docs/protocols/logitech-onboard-profiles.md), so their profile slots are left alone. A 0x8071 answer
/// PeriHub cannot use leaves the list empty instead of the mouse unopened.
///
/// `zones` is what 0x8070 gave (`None` without that feature); 0x8071 is asked only of a mouse whose feature table
/// lists it, so that the others are asked nothing more than before.
fn read_led_zones(
    dev: &mut HidppDevice,
    zones: Option<Vec<LedZone>>,
    features: &[FeatureEntry],
) -> Result<Vec<LedZone>> {
    if let Some(zones) = zones {
        return Ok(zones);
    }
    if !features.iter().any(|f| f.id == feature::RGB_EFFECTS) {
        return Ok(Vec::new());
    }
    match dev.rgb_led_zones() {
        Ok(zones) => Ok(zones),
        Err(Error::FeatureMissing(_)) => Ok(Vec::new()),
        Err(Error::Hidpp(code)) => {
            log::warn!("the lighting zones (0x8071) could not be read: {code:?}");
            Ok(Vec::new())
        }
        Err(other) => Err(other),
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
        let mut support = support(model, &onboard, dev.is_behind_receiver(), untested_saves);
        if dev.is_over_bluetooth() {
            // Nothing has been written to a mouse over Bluetooth: the connection stays read-only, whatever the
            // memory format. The profiles, DPI, rates and battery are all read as on the other connections.
            support = Support { tested: false, experimental: false, writable: false, ..support };
        }
        dev.grant(support.access());
        dev.set_label(model.name.to_string());
        let name = dev.device_name()?;
        let firmware = dev.firmware()?;
        let dpi = dev.any_dpi_capabilities()?;
        let report_rates_ms = missing_as_empty(dev.report_rates())?;
        let color_led_zones = match dev.led_zones() {
            Err(Error::FeatureMissing(_)) => None,
            other => Some(other?),
        };
        // For the test report and the lighting zones: a list that cannot be read leaves it empty.
        let features = match dev.feature_table() {
            Ok(features) => features,
            Err(Error::Unreachable) => return Err(Error::Unreachable),
            Err(_) => Vec::new(),
        };
        let mut led_zones = read_led_zones(&mut dev, color_led_zones, &features)?;
        for zone in led_zones.iter_mut().filter(|z| z.feature == feature::COLOR_LED_EFFECTS) {
            zone.slot = crate::hidpp::profile_led_slot(model.product_id, zone.index, zone.location);
        }
        let info = MouseInfo {
            product_id: model.product_id,
            model: model.name.into(),
            name,
            verified: support.tested,
            experimental: support.experimental,
            pending_undo: false,
            receiver: dev.is_behind_receiver(),
            bluetooth: if dev.is_over_bluetooth() { model.bluetooth_id } else { None },
            link: None,
            host_mode: false,
            switches_back: model.verified && support.writable_format,
            firmware,
            dpi,
            report_rates_ms,
            extended: None,
            led_zones,
            features,
            writable: support.writable,
            writable_format: support.writable_format,
            profile_list: ProfileList::Own,
            enabled_profiles: vec![true; onboard.profile_count as usize],
            onboard,
            transcript: String::new(),
        };
        let enabled_profiles_state = Arc::new(Mutex::new(info.enabled_profiles.clone()));
        let mut task = Self {
            dev,
            model,
            info,
            host_mode: Arc::new(AtomicBool::new(false)),
            enabled_profiles_state,
            last_mode_check: Instant::now(),
            battery_read: crate::battery::Schedule::new(Instant::now()),
            no_battery: false,
            last_profile: 1,
            previewing: false,
            last_backup: None,
            undo_at: None,
            list: ProfileList::Own,
        };
        let directory = task.read_directory()?;
        task.info.profile_list = task.list;
        let count = task.profile_count() as usize;
        let initial_enabled = match directory.as_deref() {
            Some(dir) => profile::parse_directory(dir, count),
            None if task.factory_memory() => (1..=count).map(|n| n as u8 <= task.selectable_profiles()).collect(),
            None => vec![true; count],
        };
        task.sync_enabled_profiles(&initial_enabled);
        task.last_profile = initial_enabled.iter().position(|&on| on).map_or(1, |i| i as u8 + 1);
        task.read_extended()?;
        task.check_mode()?;
        task.info.host_mode = task.host_mode.load(Ordering::SeqCst);
        task.finish_pending_undo();
        let active = task.dev.active_profile()?;
        if (1..=task.profile_count()).contains(&active) {
            task.last_profile = active;
        }
        let device = crate::replay::Device {
            vendor_id: "046d".into(),
            product_id: format!("{:04x}", task.info.bluetooth.unwrap_or(model.product_id)),
            name: format!("Logitech {}", model.name),
        };
        task.info.transcript = task.dev.transcript(device, crate::replay::Family::HidppMouse).to_json();
        task.dev.stop_recording();
        Ok(task)
    }

    /// The reads of mice with the extended features, and the profiles of memory format 7 as lines
    /// for the test report. On an unknown/unwritable memory format, also reads profile 1 and
    /// factory profile 1 once (read-only) so its layout lands in the test report transcript. Nothing
    /// is written.
    fn read_extended(&mut self) -> Result<()> {
        let mut reads = ExtendedReads::read(&mut self.dev)?;
        if ExtendedProfile::is_format(self.info.onboard.profile_format) {
            for number in 1..=self.profile_count() {
                let line = self
                    .read_profile_sector(number)
                    .and_then(|sector| ExtendedProfile::parse(&sector, self.info.onboard.button_count as usize))
                    .map(|profile| profile.summary(number));
                match line {
                    Ok(line) => reads.profiles.push(line),
                    Err(Error::Unreachable) => return Err(Error::Unreachable),
                    Err(error) => reads.errors.push(format!("profile {number}: {error}")),
                }
            }
        } else if !self.info.writable_format && self.sector_size() >= 16 {
            let size = self.sector_size();
            let user_sector = !self.factory_memory() && self.profile_count() >= 1;
            let rom_sector = self.info.onboard.rom_profile_count >= 1;
            let mut read_opt = |sector: u16| -> Result<()> {
                match self.dev.read_sector(sector, size) {
                    Ok(_) => Ok(()),
                    Err(Error::Unreachable) => Err(Error::Unreachable),
                    Err(Error::Hidpp(_) | Error::Protocol(_)) => Ok(()),
                    Err(e) => Err(e),
                }
            };
            if user_sector {
                read_opt(1)?;
            }
            if rom_sector {
                read_opt(ROM_SECTOR_BASE + 1)?;
            }
        }
        self.info.extended = (!reads.is_empty()).then_some(reads);
        Ok(())
    }

    /// One profile sector as the page shows it: the layout depends on the memory format.
    fn parse_profile(&self, sector: &[u8]) -> Result<ProfileSettings> {
        let buttons = self.info.onboard.button_count as usize;
        if ExtendedProfile::is_format(self.info.onboard.profile_format) {
            Ok(ExtendedProfile::parse(sector, buttons)?.settings(self.info.over_radio()))
        } else {
            ProfileSettings::parse(sector, buttons)
        }
    }

    /// The DPI level in use. A mouse PeriHub only reads can answer this with an error (the
    /// onboard profiles of a PRO X 2 are not in use while another program controls it).
    fn active_dpi_index(&mut self) -> Result<u8> {
        match self.dev.active_dpi_index() {
            Err(Error::Hidpp(_)) if !self.info.verified => Ok(0),
            other => other,
        }
    }

    /// Set while another program controls the mouse; read by the manager for the status.
    pub fn host_mode_flag(&self) -> Arc<AtomicBool> {
        self.host_mode.clone()
    }

    /// Enabled profile flags (`1..=profile_count`); read by the manager for the status.
    pub fn enabled_profiles_flag(&self) -> Arc<Mutex<Vec<bool>>> {
        self.enabled_profiles_state.clone()
    }

    fn sync_enabled_profiles(&mut self, enabled: &[bool]) {
        self.info.enabled_profiles = enabled.to_vec();
        *self.enabled_profiles_state.lock().unwrap() = enabled.to_vec();
    }

    /// Reads the mouse's mode and acts on it (see [`mode_action`]). G HUB, for example, takes the
    /// mouse when it starts and after every wake-up.
    fn check_mode(&mut self) -> Result<()> {
        self.last_mode_check = Instant::now();
        let mode = self.dev.onboard_mode()?;
        // Processes are listed only when the mouse is not in its onboard mode.
        let conflict_running = mode != ONBOARD_MODE && !conflicts::running().is_empty();
        let host_mode = match mode_action(mode, self.info.switches_back, conflict_running) {
            ModeAction::Fine => false,
            ModeAction::Report => true,
            // Read-only mode: the mouse stays as it is, and the interface says another program has it.
            ModeAction::SwitchBack if crate::writes::read_only() => true,
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
        crate::writes::refuse_if_read_only()?;
        if self.info.writable {
            Ok(())
        } else if self.info.bluetooth.is_some() {
            Err(Error::Invalid(BLUETOOTH_READ_ONLY.into()))
        } else {
            Err(Error::Invalid("saving is not supported on this mouse model yet".into()))
        }
    }

    /// Changes that stay in the mouse's RAM (active profile, DPI level, previews): a tested mouse,
    /// or one with experimental saves.
    fn check_controllable(&self) -> Result<()> {
        crate::writes::refuse_if_read_only()?;
        if self.info.verified || self.info.experimental {
            Ok(())
        } else if self.info.bluetooth.is_some() {
            Err(Error::Invalid(BLUETOOTH_READ_ONLY.into()))
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
        self.sync_enabled_profiles(&enabled);
        let mut profiles = Vec::new();
        let mut macro_sectors = HashMap::new();
        let mut used = self.reserved_sectors(directory.as_deref());
        for number in 1..=self.profile_count() {
            let sector = self.read_profile_sector(number)?;
            let mut settings = self.parse_profile(&sector)?;
            self.read_macros(&mut settings, &mut macro_sectors)?;
            for (s, o) in settings.macro_addresses() {
                used.extend(self.macro_sectors(&mut macro_sectors, s, o)?);
            }
            profiles.push(MouseProfile { number, enabled: enabled[number as usize - 1], settings });
        }
        let free = (self.profile_count() + 1..self.info.onboard.sector_count).filter(|s| !used.contains(s)).count();
        Ok(MouseState {
            active_profile: self.read_active_profile()?,
            active_dpi_index: self.active_dpi_index()?,
            current_dpi: self.dev.current_dpi()?,
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
            Reload::Stay => {
                log::info!("no other enabled profile to reload profile {active} through: left as it is");
                return Ok(());
            }
        }
        self.previewing = false;
        Ok(())
    }

    /// After every sector of a save or restore was written and read back. Loading the new settings
    /// by switching profiles is a convenience: a mouse that refuses the switch keeps what was written,
    /// so the save is not reported as failed, and an experimental one is not undone (issue #34).
    fn reload_after_writing(&mut self, reload: Result<()>) -> Result<()> {
        match reload {
            // Only while the mouse is still in onboard mode: a single-profile mouse refusing to go
            // back from host mode must not be reported as saved.
            Err(Error::Hidpp(code)) if matches!(self.dev.onboard_mode(), Ok(ONBOARD_MODE)) => {
                log::warn!(
                    "the mouse refused to load the profile again after a write ({code:?}): the memory is written"
                );
                Ok(())
            }
            other => other,
        }
    }

    /// Raw read of a sector (diagnostics).
    pub fn read_raw_sector(&mut self, sector: u16) -> Result<Vec<u8>> {
        self.dev.read_sector(sector, self.sector_size())
    }

    /// Cancels DPI and LED previews by going back to the saved settings.
    pub fn revert_preview(&mut self) -> Result<()> {
        self.reload_active()?;
        if self.previewing {
            return Err(Error::Invalid(
                "the preview stays until the mouse changes DPI level or is unplugged: PeriHub cancels it by switching \
                 to another enabled profile and back, and this mouse has none"
                    .into(),
            ));
        }
        Ok(())
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
                None => {
                    let reload = self.reload_if_active(number);
                    self.reload_after_writing(reload)
                }
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
            let reload = self.reload_if_active(number);
            self.reload_after_writing(reload)
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
        // In read-only mode the undo waits, with its backup, for a connection that may send it.
        if !self.info.writable || crate::writes::read_only() {
            self.info.pending_undo = true;
            return;
        }
        log::warn!("a save to this mouse was not kept before PeriHub closed or the mouse was unplugged: undoing it");
        self.last_backup = Some(id);
        if let Err(e) = self.undo_last_write() {
            log::warn!("undo failed: {e}");
        }
    }

    /// On a model not tested as a whole, LED slots that are not the slot of one of the mouse's zones keep
    /// their bytes: PeriHub only knows what they mean on the G502 HERO.
    fn check_led_slots(&self, sector: &[u8], settings: &ProfileSettings) -> Result<()> {
        let current = ProfileSettings::parse(sector, self.info.onboard.button_count as usize)?;
        let zones: Vec<u8> = self.info.led_zones.iter().filter_map(|z| z.slot).collect();
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
            let reload = match profile_after_restore(self.dev.active_profile()?, &enabled) {
                Some(other) => {
                    self.list = ProfileList::Own;
                    self.select_profile(other).map(|()| self.previewing = false)
                }
                None => self.reload_active(),
            };
            self.reload_after_writing(reload)?;
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
        self.battery_read.read(Instant::now());
        let battery = match self.dev.battery() {
            Ok(battery) => battery,
            Err(e @ (Error::Hid(_) | Error::Unreachable)) => return Err(e),
            // An odd battery report must not hide the DPI.
            Err(e) => {
                log::debug!("mouse battery: {e}");
                None
            }
        };
        crate::battery::report(&self.battery_id(), &self.info.model, battery.map(|b| b.reading()));
        Ok(MouseLive {
            active_profile: self.read_active_profile()?,
            active_dpi_index: self.active_dpi_index()?,
            current_dpi: self.dev.current_dpi()?,
            battery,
        })
    }

    fn battery_id(&self) -> String {
        format!("mouse:{:04x}", self.info.product_id)
    }

    fn read_active_profile(&mut self) -> Result<u8> {
        let active = self.dev.active_profile()?;
        if (1..=self.profile_count()).contains(&active) {
            self.last_profile = active;
        }
        Ok(active)
    }

    /// Whether profile `number` can be switched to (`true` = enabled in the profile list or
    /// selectable among the factory profiles). Refreshes [`MouseInfo::enabled_profiles`].
    pub fn is_profile_enabled(&mut self, number: u8) -> Result<bool> {
        self.check_number(number)?;
        if number > self.selectable_profiles() {
            return Ok(false);
        }
        let directory = self.read_directory()?;
        let enabled = self.enabled_profiles(directory.as_deref())?;
        self.sync_enabled_profiles(&enabled);
        Ok(enabled.get(number as usize - 1).copied().unwrap_or(false))
    }

    pub fn set_active_profile(&mut self, number: u8) -> Result<()> {
        self.check_controllable()?;
        self.check_number(number)?;
        if number > self.selectable_profiles() {
            return Err(Error::Invalid(format!(
                "profile {number} is not in use yet: the mouse has only its factory profiles until one is saved to it"
            )));
        }
        if !self.is_profile_enabled(number)? {
            return Err(Error::Invalid(format!("profile {number} is disabled on the mouse")));
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
        if zone.feature != feature::COLOR_LED_EFFECTS {
            return Err(Error::Invalid(
                "this mouse's lights (0x8071) are only read: PeriHub neither previews nor changes them".into(),
            ));
        }
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
        // Only a mouse on its receiver has a battery to watch. An error, a sleeping mouse included,
        // waits for the next time: the mode check above ends the thread if the mouse is gone.
        if self.info.over_radio() && !self.no_battery && self.battery_read.due(Instant::now()) {
            match self.dev.battery() {
                Ok(Some(battery)) => {
                    crate::battery::report(&self.battery_id(), &self.info.model, Some(battery.reading()))
                }
                Ok(None) => self.no_battery = true,
                Err(e) => log::debug!("mouse battery: {e}"),
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
        // The first other enabled profile.
        assert_eq!(reload_method(4, &[true; 5]), Reload::Through(1));
        assert_eq!(reload_method(1, &[true; 5]), Reload::Through(2));
        assert_eq!(reload_method(1, &[true, false, true, true, true]), Reload::Through(3));
        // Never a disabled one (issue #34): with no other enabled profile nothing is switched.
        assert_eq!(reload_method(1, &[true, false]), Reload::Stay);
        assert_eq!(reload_method(1, &[true, false, false, false, false]), Reload::Stay);
        assert_eq!(reload_method(3, &[false; 5]), Reload::Stay);
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

        // Format 4 has the layout of formats 2 and 3 where PeriHub writes (the G903 HERO of issue #115):
        // experimental with the cable, read-only through a receiver until a tester confirms it.
        let format_4 = OnboardInfo { profile_format: 4, sector_size: 255, ..g502.clone() };
        assert_eq!(support(g403, &format_4, false, true), experimental);
        assert_eq!(support(g403, &format_4, false, false), read_only, "the command line");
        assert_eq!(support(g403, &format_4, true, true), read_only, "through a receiver");

        // Memories PeriHub cannot write.
        for format in [0, 1, 5, 6, 7, 10] {
            let other = OnboardInfo { profile_format: format, ..g502.clone() };
            assert_eq!(support(g403, &other, false, true), unknown, "profile format {format}");
        }
        // Even a model tested on its original format becomes read-only once firmware moves to an unknown format.
        let g502_x = model_of(G502_X_WIRELESS);
        let format_10 = OnboardInfo { profile_format: 10, sector_size: 255, ..g502.clone() };
        assert_eq!(support(g502_x, &format_10, false, true), unknown);
        assert_eq!(support(g502_x, &format_10, true, true), unknown);
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

    /// The features of a mouse with LED effects (0x8070), such as the G502 HERO.
    const LED_SIM_FEATURES: &[u16] = &[
        feature::ROOT,
        feature::DEVICE_INFO,
        feature::DEVICE_NAME,
        feature::ADJUSTABLE_DPI,
        feature::REPORT_RATE,
        feature::ONBOARD_PROFILES,
        feature::FEATURE_SET,
        feature::COLOR_LED_EFFECTS,
    ];

    /// The features of a mouse with RGB effects (0x8071) instead of 0x8070.
    const RGB_SIM_FEATURES: &[u16] = &[
        feature::ROOT,
        feature::DEVICE_INFO,
        feature::DEVICE_NAME,
        feature::ADJUSTABLE_DPI,
        feature::REPORT_RATE,
        feature::ONBOARD_PROFILES,
        feature::FEATURE_SET,
        feature::RGB_EFFECTS,
    ];

    /// The features of a PRO X 2 style mouse: extended DPI and polling rate instead of 0x2201 and 0x8060.
    const PRO_FEATURES: &[u16] = &[
        feature::ROOT,
        feature::DEVICE_INFO,
        feature::DEVICE_NAME,
        feature::EXTENDED_DPI,
        feature::EXTENDED_REPORT_RATE,
        feature::ONBOARD_PROFILES,
        feature::FEATURE_SET,
    ];

    /// The memory and state of a HID++ mouse with onboard profiles, as far as PeriHub talks to one.
    struct Sim {
        features: &'static [u16],
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
        /// Answers "invalid argument" to the selection of a profile its own list disables, as the
        /// G303 Shroud Edition of issue #34 most likely did.
        refuses_disabled_profiles: bool,
        /// Answers "invalid argument" to every profile switch.
        refuses_switches: bool,
        /// Answers "invalid argument" to the start of every memory write.
        refuses_writes: bool,
        /// Profile switches answered with an error.
        refused_switches: Vec<[u8; 2]>,
        /// The type and version of each feature in the feature table, from index 1 (0 0 if not given).
        table: Vec<(u8, u8)>,
        name: &'static str,
        /// 0x0003 function 0: the entity count first.
        device_info: Vec<u8>,
        /// 0x0003 function 1, one answer for each entity.
        firmware: Vec<Vec<u8>>,
        /// The 0x2202 DPI list of both axes, as the pages carry it.
        dpi_list: Vec<u8>,
        /// 0x2202 function 5.
        dpi_now: Vec<u8>,
        /// The 0x8061 polling rate index in use, on the cable and through the receiver.
        rate_in_use: [u8; 2],
        /// 0x2201 function 2: the DPI now and the default one.
        dpi_sensor: [u16; 2],
        /// 0x1000 function 0, if the mouse has that battery feature.
        battery: Vec<u8>,
        /// 0x8070 functions 0 and 1 (the zones, the first zone), and function 2 for each of its effects.
        led_info: Vec<u8>,
        led_zone: Vec<u8>,
        /// When set, the location of each 0x8070 zone, in place of the one in `led_zone` (which every zone shares).
        led_locations: Vec<u16>,
        led_effects: Vec<Vec<u8>>,
        /// 0x8071 function 0: the zone count (asked with FF FF), each zone (zone FF), each effect of each zone.
        rgb_info: Vec<u8>,
        rgb_zones: Vec<Vec<u8>>,
        rgb_effects: Vec<Vec<Vec<u8>>>,
        /// Bytes 0x8100 function 0 sends after the ones PeriHub reads.
        onboard_extra: Vec<u8>,
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
                features: SIM_FEATURES,
                onboard,
                user,
                rom,
                active: [0, 1],
                writing: None,
                written: Vec::new(),
                selected: Vec::new(),
                refuses_blank_list: false,
                refuses_disabled_profiles: false,
                refuses_switches: false,
                refuses_writes: false,
                refused_switches: Vec::new(),
                table: Vec::new(),
                name: "Test",
                device_info: vec![1],
                firmware: vec![vec![0, b'U', b'1', b' ', 0x27, 0x03, 0x00, 0x00]],
                // 100 to 25600 DPI in steps of 50.
                dpi_list: vec![0x00, 0x64, 0xE0, 0x32, 0x64, 0x00],
                dpi_now: vec![0, 0x04, 0xB0, 0x03, 0x20, 0x04, 0xB0, 0x03, 0x20, 2],
                rate_in_use: [3, 4],
                dpi_sensor: [800, 800],
                battery: Vec::new(),
                led_info: Vec::new(),
                led_zone: Vec::new(),
                led_locations: Vec::new(),
                led_effects: Vec::new(),
                rgb_info: Vec::new(),
                rgb_zones: Vec::new(),
                rgb_effects: Vec::new(),
                onboard_extra: Vec::new(),
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

        /// A PRO X 2 style mouse: profile format 7 in 255-byte sectors, 16 buttons, the extended
        /// DPI and polling features, and no profile saved to it yet (it uses its factory profiles).
        fn pro() -> Self {
            let (memory, onboard) = g502_memory();
            let mut sim = Self::new(memory, OnboardInfo { button_count: 16, ..onboard }).resized(255, 7);
            for n in 1..=5u8 {
                let sector =
                    format7_sector(&format!("P{n}"), &[(400 * n as u16, 800 * n as u16, 2), (1600, 1600, 2)], 6, 3);
                sim.user[n as usize] = sector.clone();
                sim.rom.insert(ROM_SECTOR_BASE + n as u16, sector);
            }
            sim.features = PRO_FEATURES;
            sim
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
                (feature::DEVICE_INFO, 0) => self.device_info.clone(),
                (feature::DEVICE_INFO, 1) => self.firmware.get(p[0] as usize).ok_or(INVALID_ARGUMENT)?.clone(),
                // The count leaves out the root, at index 0.
                (feature::FEATURE_SET, 0) => vec![self.features.len() as u8 - 1],
                (feature::FEATURE_SET, 1) => {
                    let id = *self.features.get(p[0] as usize).ok_or(INVALID_ARGUMENT)?;
                    let (kind, version) = self.table.get(p[0] as usize - 1).copied().unwrap_or_default();
                    vec![(id >> 8) as u8, id as u8, kind, version]
                }
                // Sensor 0 has an X and a Y axis and a lift-off distance.
                (feature::EXTENDED_DPI, 1) => vec![0, 0, 3],
                // The same list on both axes, 13 bytes in each page.
                (feature::EXTENDED_DPI, 2) => {
                    let mut page = vec![p[0], p[1], p[2]];
                    page.extend(self.dpi_list.iter().skip(13 * p[2] as usize).take(13));
                    page.resize(16, 0);
                    page
                }
                (feature::EXTENDED_DPI, 5) => self.dpi_now.clone(),
                // On the cable up to 1000 Hz (bits 0 to 3), through the receiver up to 8000 Hz.
                (feature::EXTENDED_REPORT_RATE, 0) => vec![0, if p[0] == 0 { 0x0F } else { 0x7F }],
                (feature::EXTENDED_REPORT_RATE, 2) => vec![self.rate_in_use[(p[0] != 0) as usize]],
                (feature::DEVICE_NAME, 0) => vec![self.name.len() as u8],
                (feature::DEVICE_NAME, 1) => {
                    let mut chunk: Vec<u8> = self.name.bytes().skip(p[0] as usize).take(16).collect();
                    chunk.resize(16, 0);
                    chunk
                }
                // 100 to 25600 DPI in steps of 50.
                (feature::ADJUSTABLE_DPI, 1) => vec![0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00],
                (feature::ADJUSTABLE_DPI, 2) => {
                    let [now, default] = self.dpi_sensor;
                    vec![0, (now >> 8) as u8, now as u8, (default >> 8) as u8, default as u8]
                }
                (feature::BATTERY_STATUS, 0) if !self.battery.is_empty() => self.battery.clone(),
                (feature::COLOR_LED_EFFECTS, 0) => self.led_info.clone(),
                (feature::COLOR_LED_EFFECTS, 1) => match self.led_locations.get(p[0] as usize) {
                    Some(location) => {
                        let [high, low] = location.to_be_bytes();
                        let mut zone = self.led_zone.clone();
                        zone[..3].copy_from_slice(&[p[0], high, low]);
                        zone
                    }
                    None => self.led_zone.clone(),
                },
                (feature::COLOR_LED_EFFECTS, 2) => self.led_effects.get(p[1] as usize).ok_or(INVALID_ARGUMENT)?.clone(),
                (feature::RGB_EFFECTS, 0) => match (p[0], p[1]) {
                    (0xFF, 0xFF) if !self.rgb_info.is_empty() => self.rgb_info.clone(),
                    (zone, 0xFF) => self.rgb_zones.get(zone as usize).ok_or(INVALID_ARGUMENT)?.clone(),
                    (zone, effect) => self
                        .rgb_effects
                        .get(zone as usize)
                        .and_then(|effects| effects.get(effect as usize))
                        .ok_or(INVALID_ARGUMENT)?
                        .clone(),
                },
                (feature::ADJUSTABLE_DPI, 3) => vec![],
                // 1, 2, 4 and 8 ms.
                (feature::REPORT_RATE, 0) => vec![0x8B],
                (feature::ONBOARD_PROFILES, 0) => {
                    let [hi, lo] = info.sector_size.to_be_bytes();
                    let mut answer = vec![
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
                    ];
                    answer.extend(&self.onboard_extra);
                    answer
                }
                (feature::ONBOARD_PROFILES, 1) => vec![],
                (feature::ONBOARD_PROFILES, 2) => vec![ONBOARD_MODE],
                (feature::ONBOARD_PROFILES, 3) => {
                    let limit = if p[0] == 1 { info.rom_profile_count } else { info.profile_count };
                    let list = &self.user[0];
                    let own = p[0] == 0 && profile::crc_is_valid(list);
                    let refused = p[0] > 1
                        || !(1..=limit).contains(&p[1])
                        || self.refuses_switches
                        || (self.refuses_disabled_profiles
                            && own
                            && !profile::parse_directory(list, info.profile_count as usize)[p[1] as usize - 1]);
                    if refused {
                        self.refused_switches.push([p[0], p[1]]);
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
                    if sector as usize >= self.user.len() || length != info.sector_size as usize || self.refuses_writes
                    {
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

    /// The simulated mouse as a HID connection. `reply_index`: the device index its answers carry, when it is
    /// not the one it was asked on (a Bluetooth mouse asked on 0xFF may answer on 0x00).
    fn sim_transport(sim: Sim, reply_index: Option<u8>) -> (FakeDevice, Arc<Mutex<Sim>>) {
        let features = sim.features;
        let sim = Arc::new(Mutex::new(sim));
        let mouse = sim.clone();
        let fake = FakeDevice::new(move |request| {
            let (index, function) = (request[2] as usize, request[3] >> 4);
            let mut replies = if index == 0 && function == 1 {
                vec![reply(request, &[4, 2, request[6]])]
            } else if index == 0 && function == 0 {
                let id = u16::from_be_bytes([request[4], request[5]]);
                let found = features.iter().position(|&f| f == id).map_or(0, |i| i as u8);
                vec![reply(request, &[found])]
            } else {
                match mouse.lock().unwrap().answer(features[index], function, &request[4..]) {
                    Ok(payload) => vec![reply(request, &payload)],
                    Err(code) => {
                        let mut error = reply(request, &[]);
                        error[2..6].copy_from_slice(&[0xFF, request[2], request[3], code]);
                        vec![error]
                    }
                }
            };
            if let Some(index) = reply_index {
                replies.iter_mut().for_each(|r| r[1] = index);
            }
            replies
        });
        (fake, sim)
    }

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
        let (fake, sim) = sim_transport(sim, None);
        let dev = if receiver {
            HidppDevice::behind_receiver(fake, None, 1, true).unwrap()
        } else {
            HidppDevice::new(fake, models::WIRED_INDEX, true).unwrap()
        };
        let task = MouseTask::open(dev, model_of(product_id), untested_saves).unwrap();
        (task, sim)
    }

    /// A format 7 profile sector as the PRO X 2 family stores it, with `rate_wireless` and
    /// `rate_wired` as indexes of the polling rate list.
    fn format7_sector(name: &str, dpi: &[(u16, u16, u8)], rate_wireless: u8, rate_wired: u8) -> Vec<u8> {
        let mut s = vec![0xFF; 255];
        s[0] = rate_wireless;
        s[1] = rate_wired;
        s[2] = 1; // default level: the second
        s[3] = 0;
        for i in 0..5 {
            let (x, y, lod) = dpi.get(i).copied().unwrap_or((0, 0, 0));
            let at = 4 + 5 * i;
            s[at..at + 2].copy_from_slice(&x.to_le_bytes());
            s[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
            s[at + 4] = lod;
        }
        s[37] = 0;
        s[44..46].copy_from_slice(&120u16.to_le_bytes());
        s[46..48].copy_from_slice(&900u16.to_le_bytes());
        s[48..52].copy_from_slice(&[0x80, 0x01, 0x00, 0x01]); // button 1: left click
        s[160..208].fill(0);
        for (i, unit) in name.encode_utf16().enumerate() {
            s[160 + 2 * i..162 + 2 * i].copy_from_slice(&unit.to_le_bytes());
        }
        s[208..252].fill(0);
        profile::write_crc(&mut s);
        s
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
        let format_5 = Sim::new(memory, onboard).resized(256, 5);
        let (mut task, sim) = open_sim(format_5, G502_X_PLUS, true);
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
        // Issue #22: a G903 HERO, profile format 4 with 255-byte sectors, through its receiver. PeriHub can
        // write format 4, but nobody has tested it yet: through the receiver it only reads.
        let (memory, onboard) = g502_memory();
        let format_4 = Sim::new(memory.clone(), onboard.clone()).resized(255, 4);
        let (task, _) = open_sim_through(format_4, G502_X_PLUS, true, true);
        assert!(!task.info.writable && task.info.writable_format && !task.info.experimental);
        let ids: Vec<u16> = task.info.features.iter().map(|f| f.id).collect();
        assert_eq!(ids, SIM_FEATURES[1..]);
        // A format PeriHub cannot write, the same way.
        let format_5 = Sim::new(memory, onboard).resized(255, 5);
        let (task, _) = open_sim_through(format_5, G502_X_PLUS, true, true);
        assert!(!task.info.writable && !task.info.writable_format && !task.info.experimental);
        assert_eq!(task.info.features.len(), SIM_FEATURES.len() - 1);
    }

    // ---- The G903 HERO of issue #115: profile format 4, lights through 0x8071 ----

    const G903_HERO: u16 = 0xC091;

    /// The feature table of the G903 HERO, as the test report of issue #115 lists it (the same as in the debug log of
    /// libratbag's issue #774, a G903 HERO on its cable).
    const G903_FEATURES: &[u16] = &[
        feature::ROOT,
        0x0001,
        0x0003,
        0x0005,
        0x1D4B,
        0x0020,
        0x1001,
        feature::RGB_EFFECTS,
        feature::ONBOARD_PROFILES,
        0x8110,
        feature::REPORT_RATE,
        feature::ADJUSTABLE_DPI,
        0x00C2,
        0x1802,
        0x1803,
        0x1805,
        0x1806,
        0x1811,
        0x1830,
        0x1890,
        0x1891,
        0x18A1,
        0x1801,
        0x18B1,
        0x1DF3,
        0x1E00,
        0x1EB0,
        0x1863,
        0x1E22,
        0x2121,
        0x18C0,
    ];
    /// The type (flags) and version of each G903 HERO feature after the root.
    const G903_TABLE: &[(u8, u8)] = &[
        (0, 1),
        (0, 2),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 2),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 1),
        (0, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 4),
        (0x60, 0),
        (0x60, 0),
        (0x60, 4),
        (0x60, 4),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x40, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0, 0),
        (0x60, 0),
    ];

    /// A G903 HERO as the test report of issue #115 shows it: its name, firmware, feature table, memory (format 4,
    /// 16 sectors of 255 bytes, 5 profiles, 1 factory profile, 12 buttons), DPI range, polling rates, active profile
    /// and its profile list, byte for byte (profile 1 enabled, 2 to 5 disabled). Its two 0x8071 zones (location 1
    /// with 4 effects, location 2 with 4 effects) are what a G903 HERO answered in libratbag's issue #774. Made up:
    /// the unit id (12 34 56 78, which the transcript leaves out), the ids of the zones' effects (off, fixed, colour
    /// cycle, breathing) and the profiles themselves (the G502's, in 255-byte sectors), which the report did not read.
    fn g903_hero() -> Sim {
        let (memory, _) = g502_memory();
        let onboard = OnboardInfo {
            memory_model: 1,
            profile_format: 4,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 1,
            button_count: 12,
            sector_count: 16,
            sector_size: 255,
            has_gshift: true,
            has_dpi_shift: true,
        };
        let mut sim = Sim::new(memory, onboard).resized(255, 4);
        let mut list = vec![0xFF; 255];
        for n in 1..=5u8 {
            let at = 4 * (n as usize - 1);
            list[at..at + 4].copy_from_slice(&[0x00, n, (n == 1) as u8, 0xFF]);
        }
        list[253..].copy_from_slice(&[0x40, 0x37]);
        sim.user[0] = list;
        sim.features = G903_FEATURES;
        sim.table = G903_TABLE.to_vec();
        sim.name = "G903 LIGHTSPEED Wireless Gaming Mouse w/ HERO";
        sim.device_info = vec![0x03, 0x12, 0x34, 0x56, 0x78, 0x00, 0x0C, 0x40, 0x87, 0xC0, 0x91, 0, 0, 0, 0, 0];
        sim.firmware = vec![
            vec![0x01, b'B', b'L', b'1', 0x06, 0x01, 0x00, 0x13, 0, 0, 0, 0x04, 0x70, 0x72, 0xFE, 0],
            vec![0x00, b'M', b'P', b'M', 0x23, 0x01, 0x00, 0x13, 0x01, 0x40, 0x87, 0x04, 0x70, 0x72, 0xFE, 0],
            vec![0x05, 0, 0, 0, 0, 0, 0, 0xAE],
        ];
        sim.onboard_extra = vec![0x04];
        sim.rgb_info = vec![0xFF, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x04];
        sim.rgb_zones = vec![vec![0x00, 0x00, 0x00, 0x01, 0x04, 0x07], vec![0x01, 0x00, 0x00, 0x02, 0x04, 0x00]];
        sim.rgb_effects = (0..2u8)
            .map(|zone| {
                vec![
                    vec![zone, 0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                    vec![zone, 1, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00],
                    vec![zone, 2, 0x00, 0x03, 0x00, 0x04, 0x27, 0x10],
                    vec![zone, 3, 0x00, 0x0A, 0x00, 0x05, 0x0F, 0xA0],
                ]
            })
            .collect();
        sim
    }

    /// What the zones of a mouse are: (zone, location, feature, profile slot).
    fn zones_of(info: &MouseInfo) -> Vec<(u8, u16, u16, Option<u8>)> {
        info.led_zones.iter().map(|z| (z.index, z.location, z.feature, z.slot)).collect()
    }

    #[test]
    fn the_dpi_indicator_of_a_wired_g502_hero_is_saved_in_the_second_led_block() {
        // On a real G502 HERO the first LED block of a profile lights the G logo and the second the DPI indicator,
        // which 0x8070 reports as zone 0 at location 1 (checked on 2026-11-21).
        let (memory, onboard) = g502_memory();
        let mut sim = Sim::new(memory, onboard);
        sim.features = LED_SIM_FEATURES;
        sim.led_info = vec![2, 0, 3, 0, 4];
        sim.led_zone = vec![0, 0, 1, 4];
        sim.led_locations = vec![1, 2];
        sim.led_effects = vec![
            vec![0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 1, 0, 1, 0, 0, 0, 0],
            vec![0, 2, 0, 3, 0xC0, 5, 3, 0xE8],
            vec![0, 3, 0, 0x0A, 0xC1, 5, 0, 0x3C],
        ];
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        let led = feature::COLOR_LED_EFFECTS;
        assert_eq!(zones_of(&task.info), [(0, 1, led, Some(1)), (1, 2, led, Some(0))]);

        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();
        let active = state.active_profile;
        let mut settings = state.profiles[active as usize - 1].settings.clone();
        let red = LedEffect::Fixed { color: crate::color::Rgb { r: 0xFF, g: 0, b: 0 } };
        assert_ne!(settings.leds[1], red, "the fixture must not have it already");
        settings.leds[1] = red;
        task.save_profile(active, &settings).unwrap();
        let mouse = sim.lock().unwrap();
        let sector = *mouse.written.last().unwrap() as usize;
        let changed: Vec<usize> =
            (0..mouse.user[sector].len()).filter(|&i| mouse.user[sector][i] != before[sector][i]).collect();
        let size = mouse.user[sector].len();
        assert!(
            !changed.is_empty() && changed.iter().all(|&i| (219..230).contains(&i) || i >= size - 2),
            "{changed:?}"
        );
    }

    #[test]
    fn the_profile_list_of_the_g903_hero_is_the_one_its_report_shows() {
        let list = &g903_hero().user[0];
        assert!(profile::crc_is_valid(list), "the CRC the mouse sent matches the bytes before it");
        assert_eq!(profile::parse_directory(list, 5), [true, false, false, false, false]);
    }

    #[test]
    fn a_g903_hero_with_its_cable_saves_format_4_as_an_experimental_feature_and_only_shows_its_8071_zones() {
        let (fake, sim) = sim_transport(g903_hero(), None);
        let requests = fake.written.clone();
        let dev = HidppDevice::new(fake, models::WIRED_INDEX, true).unwrap();
        let mut task = MouseTask::open(dev, model_of(G903_HERO), true).unwrap();
        assert!(task.info.writable && task.info.writable_format && task.info.experimental && !task.info.verified);
        // The zones of 0x8071 are read and shown, with no profile slot: the sources disagree on which slot is which.
        let rgb = feature::RGB_EFFECTS;
        assert_eq!(zones_of(&task.info), [(0, 1, rgb, None), (1, 2, rgb, None)]);
        assert_eq!(task.info.led_zones[1].effects, [0, 1, 3, 10]);
        // 0x8071 is only read: no preview in RAM.
        assert!(task.preview_led(0, &LedEffect::Off).is_err());

        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();
        let settings = edited(&state, 1);
        assert_eq!((settings.buttons.len(), settings.gshift_buttons.len()), (12, 12));
        // A change of a profile LED slot is refused, before anything is written.
        let mut lights = settings.clone();
        lights.leds[1] = LedEffect::Fixed { color: crate::color::Rgb { r: 0x12, g: 0x34, b: 0x56 } };
        let error = task.save_profile(1, &lights).unwrap_err().to_string();
        assert!(error.contains("no such LED zone"), "{error}");
        assert!(sim.lock().unwrap().written.is_empty());

        task.save_profile(1, &settings).unwrap();
        assert!(task.pending_undo_path().exists(), "the 15-second undo is armed");
        {
            let mouse = sim.lock().unwrap();
            assert_eq!(mouse.written, [1]);
            assert!(mouse.selected.is_empty(), "profile 1 is the only one enabled: no switch");
            // The first DPI level and the CRC: not the four button slots past the twelfth, nor the lights.
            let changed: Vec<usize> = (0..255).filter(|&i| mouse.user[1][i] != before[1][i]).collect();
            assert_eq!(changed, [3, 4, 253, 254]);
        }
        task.undo_last_write().unwrap();
        assert_eq!(sim.lock().unwrap().user, before);

        // Only function 0 of 0x8071 reached the mouse.
        let index = G903_FEATURES.iter().position(|&f| f == rgb).unwrap() as u8;
        for request in requests.lock().unwrap().iter().filter(|r| r[2] == index) {
            assert_eq!(request[3] >> 4, 0, "{request:02x?}");
        }
        forget_test_files();
    }

    #[test]
    fn through_its_receiver_a_g903_hero_is_read_with_its_lights_and_not_saved_to() {
        let (mut task, sim) = open_sim_through(g903_hero(), G903_HERO, true, true);
        let info = task.info.clone();
        assert!(info.receiver && info.writable_format && !info.writable && !info.experimental && !info.verified);
        assert_eq!(zones_of(&info).len(), 2);
        let state = task.state().unwrap();
        assert_eq!(state.profiles[0].settings.buttons.len(), 12);
        assert!(task.save_profile(1, &edited(&state, 1)).is_err());
        assert!(task.preview_led(1, &LedEffect::Off).is_err());
        let mouse = sim.lock().unwrap();
        assert!(mouse.written.is_empty() && mouse.selected.is_empty());
    }

    /// The replay test of the G903 HERO (`tests/fixtures/transcripts`) holds what PeriHub records when it opens
    /// the mouse of issue #115 through its receiver: run with PERIHUB_WRITE_TRANSCRIPTS=1 to write it again.
    #[test]
    fn the_replay_of_the_g903_hero_is_what_perihub_records_of_it() {
        let (task, _) = open_sim_through(g903_hero(), G903_HERO, true, true);
        assert_eq!(task.info.name, "G903 LIGHTSPEED Wireless Gaming Mouse w/ HERO");
        let text = &task.info.transcript;
        assert!(!text.contains("12 34 56 78"), "the unit id stays out of the transcript");
        let recorded = crate::replay::Transcript::from_json(text).unwrap();
        assert_eq!(recorded.access, crate::replay::Access::ReadOnly);
        crate::replay::tests::recorded_fixture("logitech-g903-hero-receiver.json", &recorded);
    }

    #[test]
    fn a_mouse_whose_8071_zones_cannot_be_read_still_opens() {
        let mut sim = untested_memory();
        sim.features = RGB_SIM_FEATURES;
        // No answer to the zone count: "invalid argument".
        let (task, _) = open_sim(sim, G903_HERO, true);
        assert!(task.info.led_zones.is_empty());
        forget_test_files();
    }

    const PRO_2: u16 = 0xC09A;
    const G303_SHROUD: u16 = 0xC097;

    #[test]
    fn a_pro_x_2_is_read_through_its_extended_features_and_nothing_is_changed() {
        let (mut task, sim) = open_sim_through(Sim::pro(), PRO_2, true, true);
        assert_eq!(task.info.model, "PRO 2 LIGHTSPEED");
        // Format 7 is not one PeriHub writes: nothing is tested, nothing is writable, even asked to.
        assert!(!task.info.verified && !task.info.writable && !task.info.writable_format && !task.info.experimental);

        // 0x2202 stands in for 0x2201, and 0x8061 for 0x8060.
        assert_eq!(task.info.dpi, DpiCapabilities { min: 100, max: 25600, step: 50, list: Vec::new() });
        assert!(task.info.report_rates_ms.is_empty());
        let extended = task.info.extended.clone().unwrap();
        assert_eq!(extended.dpi_y, Some(task.info.dpi.clone()));
        let now = extended.dpi_now.unwrap();
        assert_eq!((now.x, now.default_x, now.y, now.lod), (1200, 800, Some(1200), Some(2)));
        assert_eq!(extended.rates_wired_hz, [125, 250, 500, 1000]);
        assert_eq!(extended.rates_wireless_hz, [125, 250, 500, 1000, 2000, 4000, 8000]);
        assert_eq!(extended.rate_now_hz, Some(2000), "through the receiver");
        assert!(extended.errors.is_empty(), "{:?}", extended.errors);
        assert_eq!(extended.profiles.len(), 5);
        assert_eq!(
            extended.profiles[1],
            "Profile 2 \"P2\": DPI 800x1600 1600 (level 2 default, 1 shift; lift-off 2 2), \
             polling 8000 Hz wireless / 1000 Hz cable, BHOP 0, timeouts 120 / 900"
        );

        // The page shows the profiles, with the rate of the connection in use.
        let state = task.state().unwrap();
        assert_eq!(state.current_dpi, 1200);
        let settings = &state.profiles[0].settings;
        assert_eq!(settings.dpi_levels, [400, 1600]);
        assert_eq!((settings.report_rate_hz, settings.report_rate_ms), (Some(8000), 0));
        assert_eq!(settings.buttons[0], ButtonAction::Mouse { button: 1 });
        assert_eq!((settings.buttons.len(), settings.gshift_buttons.len()), (16, 16));
        assert!(task.live().unwrap().current_dpi == 1200);

        // Reads only: no change reaches the mouse, not even a profile switch or a DPI preview.
        assert!(task.save_profile(1, settings).is_err());
        assert!(task.set_active_profile(2).is_err());
        assert!(task.preview_dpi(800).is_err());
        assert!(task.dev.set_onboard_mode(ONBOARD_MODE).is_err(), "buttons stay as they are");
        let mouse = sim.lock().unwrap();
        assert!(mouse.written.is_empty() && mouse.selected.is_empty());
    }

    #[test]
    fn with_its_cable_a_pro_x_2_shows_the_rate_of_the_cable() {
        let (mut task, _) = open_sim(Sim::pro(), PRO_2, true);
        assert_eq!(task.info.extended.as_ref().unwrap().rate_now_hz, Some(1000));
        let state = task.state().unwrap();
        assert_eq!(state.profiles[0].settings.report_rate_hz, Some(1000));
        assert_eq!(state.profiles[0].settings.report_rate_ms, 1);
    }

    const PRO_X_2_DEX: u16 = 0xC0A0;

    /// The feature table of the PRO X 2 DEX in issue #32, hidden features included, and the type
    /// and version of each feature.
    const DEX_FEATURES: &[u16] = &[
        feature::ROOT,
        0x0001,
        0x0003,
        0x0005,
        0x1D4B,
        0x0020,
        0x1004,
        0x2250,
        0x2251,
        0x2202,
        0x8090,
        0x80E0,
        0x8061,
        0x8100,
        0x8110,
        0x1500,
        0x1801,
        0x1802,
        0x1803,
        0x1806,
        0x1817,
        0x1805,
        0x1830,
        0x1875,
        0x1861,
        0x1890,
        0x18A1,
        0x1E00,
        0x1E02,
        0x1E22,
        0x1602,
        0x1EB0,
        0x18B1,
    ];
    const DEX_TABLE: &[(u8, u8)] = &[
        (0, 2),
        (0, 7),
        (0, 5),
        (0, 0),
        (0, 0),
        (0, 5),
        (0, 1),
        (0, 0),
        (0, 0),
        (0, 3),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0x70, 0),
        (0x70, 0),
        (0x70, 1),
        (0x70, 8),
        (0x70, 0),
        (0x60, 0),
        (0x70, 0),
        (0x70, 0),
        (0x70, 1),
        (0x68, 9),
        (0x70, 0),
        (0x40, 0),
        (0x60, 0),
        (0x70, 1),
        (0, 0),
        (0x70, 0),
        (0x70, 0),
    ];

    /// A format 7 profile of the PRO X 2 DEX as its test report shows them: these DPI levels at
    /// lift-off 2, the first one the default, 1000 Hz on both connections, BHOP 255, timeouts 60 and
    /// 300, no name. The buttons and lights are not in the report: those of [`format7_sector`].
    fn dex_sector(dpi: &[u16]) -> Vec<u8> {
        let levels: Vec<(u16, u16, u8)> = dpi.iter().map(|&d| (d, d, 2)).collect();
        let mut s = format7_sector("", &levels, 3, 3);
        s[2] = 0; // default level: the first
        s[37] = 255; // BHOP
        s[44..46].copy_from_slice(&60u16.to_le_bytes());
        s[46..48].copy_from_slice(&300u16.to_le_bytes());
        profile::write_crc(&mut s);
        s
    }

    /// The PRO X 2 DEX of issue #32 through its receiver, as its two test reports describe it. Not in
    /// the reports, so made up: the unit id (12 34 56 78, which the transcript must leave out), the
    /// firmware entity types, the steps of its DPI list (957 values from 100 to 44000: 25 apart up to
    /// 4000, then 50), the buttons and lights of its profiles and its profile list (the G502's).
    fn pro_x_2_dex() -> Sim {
        let (memory, onboard) = g502_memory();
        let onboard = OnboardInfo {
            profile_count: 5,
            rom_profile_count: 1,
            button_count: 5,
            sector_count: 16,
            has_gshift: true,
            has_dpi_shift: true,
            ..onboard
        };
        let mut sim = Sim::new(memory, onboard).resized(255, 7);
        sim.user[1] = dex_sector(&[1200]);
        sim.user[2..=5].fill(dex_sector(&[800, 1200, 1600, 2400, 3200]));
        sim.rom.insert(ROM_SECTOR_BASE + 1, sim.user[1].clone());
        sim.features = DEX_FEATURES;
        sim.table = DEX_TABLE.to_vec();
        sim.name = "PRO X 2 DEX";
        sim.device_info = vec![0x02, 0x12, 0x34, 0x56, 0x78, 0x00, 0x0C, 0x40, 0xB8, 0xC0, 0xA0, 0, 0, 0, 0x01, 0];
        sim.firmware =
            vec![vec![1, b'B', b'L', b'2', 0x38, 0x01, 0x00, 0x05], vec![0, b'M', b'P', b'M', 0x37, 0x03, 0x00, 0x07]];
        sim.dpi_list = vec![0x00, 0x64, 0xE0, 0x19, 0x0F, 0xA0, 0xE0, 0x32, 0xAB, 0xE0, 0x00, 0x00];
        sim.dpi_now = vec![0, 0x04, 0xB0, 0x04, 0xB0, 0x04, 0xB0, 0x04, 0xB0, 2];
        sim.rate_in_use = [3, 3];
        sim
    }

    /// The replay test of the PRO X 2 DEX (`tests/fixtures/transcripts`) holds what PeriHub records
    /// when it opens the mouse of issue #32: run with PERIHUB_WRITE_TRANSCRIPTS=1 to write it again.
    #[test]
    fn the_replay_of_the_pro_x_2_dex_is_what_perihub_records_of_it() {
        let (task, _) = open_sim_through(pro_x_2_dex(), PRO_X_2_DEX, true, true);
        let extended = task.info.extended.clone().unwrap();
        // As the test report of issue #32 read it.
        assert_eq!(task.info.name, "PRO X 2 DEX");
        assert_eq!((task.info.dpi.min, task.info.dpi.max, task.info.dpi.list.len()), (100, 44000, 957));
        assert_eq!(extended.raw, ["rates cable: 00 0f", "rates receiver: 00 7f", "rate in use: index 3"]);
        assert_eq!(
            extended.profiles[0],
            "Profile 1: DPI 1200 (level 1 default, 1 shift; lift-off 2), polling 1000 Hz wireless / 1000 Hz cable, \
             BHOP 255, timeouts 60 / 300"
        );
        let text = &task.info.transcript;
        assert!(!text.contains("12 34 56 78"), "the unit id stays out of the transcript");
        let recorded = crate::replay::Transcript::from_json(text).unwrap();
        assert_eq!(recorded.access, crate::replay::Access::ReadOnly);
        crate::replay::tests::recorded_fixture("logitech-pro-x-2-dex.json", &recorded);
    }

    #[test]
    fn a_g303_shroud_edition_follows_its_memory_format_like_any_other_model() {
        // Issue #34: format 3 in 255-byte sectors, through a receiver.
        let model = models::find_wireless(0x409E).unwrap();
        assert_eq!((model.product_id, model.name), (G303_SHROUD, "G303 Shroud Edition"));
        let onboard = g502_x().onboard;
        assert_eq!(
            support(model, &onboard, true, true),
            Support { tested: true, experimental: true, writable: true, writable_format: true },
            "saves over the radio are experimental, with the 15-second undo"
        );
        let from_cli = support(model, &onboard, true, false);
        assert!(from_cli.tested && !from_cli.writable);

        let (task, _) = open_sim_through(g502_x(), G303_SHROUD, true, true);
        assert!(task.info.verified && task.info.writable && task.info.experimental);
        assert!(task.info.extended.is_none(), "no extended features to read");
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

    /// The G303 Shroud Edition of issue #34, as its test report describes it: format 3 in 255-byte
    /// sectors, one factory profile, six buttons and a profile list of its own. The report does not
    /// say which profiles that list enables: here only the first, and the mouse refuses to switch to
    /// the others.
    fn g303_shroud() -> Sim {
        let (memory, onboard) = g502_memory();
        let onboard = OnboardInfo { rom_profile_count: 1, button_count: 6, ..onboard };
        let mut sim = Sim::new(memory, onboard).resized(255, 3);
        sim.user[0] = profile::build_directory(&[true, false, false, false, false], 255);
        sim.refuses_disabled_profiles = true;
        sim
    }

    #[test]
    fn a_save_never_switches_to_a_disabled_profile_to_reload_the_active_one() {
        // Issue #34: the save of the active profile, and then its undo, ended in "invalid argument".
        let (mut task, sim) = open_sim_through(g303_shroud(), G303_SHROUD, true, true);
        assert!(task.info.writable && task.info.experimental);
        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();
        assert_eq!(state.active_profile, 1);
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, false, false, false, false]);

        let saved = task.save_profile(1, &edited(&state, 1)).unwrap();
        assert_eq!(saved.profiles[0].settings.dpi_levels[0], 3250);
        assert!(task.pending_undo_path().exists(), "the undo is armed");
        {
            let mouse = sim.lock().unwrap();
            assert_eq!(mouse.written, [1]);
            assert!(mouse.selected.is_empty() && mouse.refused_switches.is_empty(), "no switch to a disabled profile");
        }

        // The undo puts the profile back the same way.
        task.undo_last_write().unwrap();
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.user, before);
        assert_eq!(mouse.written, [1, 1]);
        assert!(mouse.selected.is_empty() && mouse.refused_switches.is_empty());
        drop(mouse);
        assert!(!task.pending_undo_path().exists());
        forget_test_files();
    }

    #[test]
    fn a_refused_profile_switch_after_a_verified_write_neither_fails_nor_undoes_the_save() {
        let mut sim = g303_shroud();
        sim.user[0] = profile::build_directory(&[true; 5], 255);
        sim.refuses_switches = true;
        let (mut task, sim) = open_sim_through(sim, G303_SHROUD, true, true);
        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();

        // Saved and read back: the refused switch to profile 2 and back is only logged.
        let saved = task.save_profile(1, &edited(&state, 1)).unwrap();
        assert_eq!(saved.profiles[0].settings.dpi_levels[0], 3250);
        assert_eq!(sim.lock().unwrap().written, [1]);
        assert!(task.pending_undo_path().exists(), "the 15-second undo is still there");

        // The undo succeeds too, instead of "Putting the backup back failed too".
        task.undo_last_write().unwrap();
        assert_eq!(sim.lock().unwrap().user, before);
        assert!(sim.lock().unwrap().selected.is_empty());
        assert!(!task.pending_undo_path().exists());
        forget_test_files();
    }

    #[test]
    fn a_refused_first_write_leaves_the_mouse_untouched_and_the_undo_writes_nothing() {
        let mut sim = g303_shroud();
        sim.refuses_writes = true;
        let (mut task, sim) = open_sim_through(sim, G303_SHROUD, true, true);
        let before = sim.lock().unwrap().user.clone();
        let state = task.state().unwrap();

        let error = task.save_profile(1, &edited(&state, 1)).unwrap_err().to_string();
        assert!(error.contains("invalid argument (0x02)"), "{error}");
        assert!(error.contains("The backup was put back"), "{error}");
        let mouse = sim.lock().unwrap();
        assert_eq!(mouse.user, before);
        assert!(mouse.written.is_empty() && mouse.selected.is_empty());
        // The start of the write was refused: no data, no end of write followed it.
        assert!(mouse.writing.is_none());
        drop(mouse);
        assert!(!task.pending_undo_path().exists(), "nothing left to undo");
        forget_test_files();
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
    fn read_only_mode_refuses_saves_switches_and_previews() {
        let (mut task, sim) = open_sim(g502_x(), G502_X_WIRELESS, false);
        let state = task.state().unwrap();
        let mut sector = vec![0u8; task.sector_size() as usize];
        crate::hidpp::profile::write_crc(&mut sector);
        let _read_only = crate::writes::test::read_only_mode();
        assert!(matches!(task.save_profile(2, &edited(&state, 2)), Err(Error::ReadOnly)));
        assert!(matches!(task.restore_factory_profile(2), Err(Error::ReadOnly)));
        assert!(matches!(task.set_active_profile(2), Err(Error::ReadOnly)));
        assert!(matches!(task.preview_dpi(800), Err(Error::ReadOnly)));
        // Below the checks of the task, the connection itself refuses the memory write.
        assert!(matches!(task.dev.write_sector(2, &sector, &task.info.onboard), Err(Error::ReadOnly)));
        let mouse = sim.lock().unwrap();
        assert!(mouse.written.is_empty() && mouse.selected.is_empty() && mouse.writing.is_none());
        drop(mouse);
        assert!(task.state().is_ok(), "reads still work");
        assert!(!task.pending_undo_path().exists());
        forget_test_files();
    }

    #[test]
    fn a_mouse_opens_in_read_only_mode_without_a_write() {
        let _read_only = crate::writes::test::read_only_mode();
        let (task, sim) = open_sim(g502_x(), G502_X_WIRELESS, false);
        assert!(task.info.verified);
        let mouse = sim.lock().unwrap();
        assert!(mouse.written.is_empty() && mouse.selected.is_empty());
        drop(mouse);
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

    // ---- The G604, from the answers of a real one ----

    const G604: u16 = 0x4085;

    /// The feature table of a G604 through its LIGHTSPEED receiver, as the debug log of libratbag's issue #1477
    /// (a G604 on Linux, May 2023) shows it: 32 features after the root, hidden ones included. PeriHub never
    /// calls those that are not in `ALLOWED_FEATURES`: 0x0020 (reset) and 0x00C2 (firmware update) are there.
    const G604_FEATURES: &[u16] = &[
        feature::ROOT,
        0x0001,
        0x0003,
        0x0005,
        0x1D4B,
        0x0020,
        feature::BATTERY_STATUS,
        feature::COLOR_LED_EFFECTS,
        0x1300,
        feature::ONBOARD_PROFILES,
        0x8110,
        feature::REPORT_RATE,
        feature::ADJUSTABLE_DPI,
        0x00C2,
        0x1802,
        0x1803,
        0x1805,
        0x1806,
        0x1813,
        0x1830,
        0x1890,
        0x1891,
        0x1861,
        0x1801,
        0x18B1,
        0x1DF3,
        0x1E00,
        0x1EB0,
        0x1E22,
        0x2121,
        0x18C0,
        0x1814,
        0x1815,
    ];
    /// The type (flags) and version of each G604 feature after the root.
    const G604_TABLE: &[(u8, u8)] = &[
        (0, 1),
        (0, 2),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 4),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 1),
        (0, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 4),
        (0x60, 0),
        (0x60, 0),
        (0x60, 5),
        (0x60, 5),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x60, 0),
        (0x40, 0),
        (0x60, 0),
        (0x60, 0),
        (0, 0),
        (0x60, 0),
        (0, 1),
        (0, 1),
    ];

    /// A G604 as that log shows it: memory format 3 in 255-byte sectors, 5 profiles with one factory profile,
    /// 16 buttons, the profile list and profiles 1 to 5 and the macro sectors 6 and 7 as the mouse held them
    /// (profile 3 active, profiles 4 and 5 disabled), 100 to 25600 DPI in steps of 50 (4750 now, 800 the default), the
    /// polling intervals 1, 2, 4 and 8 ms, one logo LED zone with four effects, and 90 % of battery. The log
    /// has no name, firmware or unit id: those are made up (and the unit id stays out of the transcript).
    fn g604() -> Sim {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/g604");
        let mut user: Vec<Vec<u8>> =
            (0..8).map(|s| std::fs::read(format!("{dir}/sector_{s:04x}.bin")).unwrap()).collect();
        // The sectors the log did not read.
        user.resize(16, vec![0xFF; 255]);
        let onboard = OnboardInfo {
            memory_model: 1,
            profile_format: 3,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 1,
            button_count: 16,
            sector_count: 16,
            sector_size: 255,
            has_gshift: true,
            has_dpi_shift: true,
        };
        let mut sim = Sim::new(user, onboard);
        sim.features = G604_FEATURES;
        sim.table = G604_TABLE.to_vec();
        sim.name = "G604 LIGHTSPEED";
        sim.device_info = vec![0x01, 0x12, 0x34, 0x56, 0x78, 0x00, 0x06, 0xB0, 0x24, 0x40, 0x85, 0, 0, 0, 0, 0];
        sim.active = [0, 3];
        sim.dpi_sensor = [4750, 800];
        sim.battery = vec![0x5A, 0x32, 0x00];
        sim.led_info = vec![1, 0, 3, 0, 4];
        sim.led_zone = vec![0, 0, 1, 4];
        sim.led_effects = vec![
            vec![0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 1, 0, 1, 0, 0, 0, 0],
            vec![0, 2, 0, 3, 0xC0, 5, 3, 0xE8],
            vec![0, 3, 0, 0x0A, 0xC1, 5, 0, 0x3C],
        ];
        sim
    }

    #[test]
    fn a_g604_is_read_through_its_receiver_as_its_own_answers_describe_it() {
        let (mut task, _) = open_sim_through(g604(), G604, true, true);
        let info = task.info.clone();
        assert_eq!((info.model.as_str(), info.product_id), ("G604", 0x4085));
        assert_eq!((info.onboard.profile_format, info.onboard.sector_size, info.onboard.button_count), (3, 255, 16));
        assert_eq!((info.dpi.min, info.dpi.max, info.dpi.step), (100, 25600, 50));
        assert_eq!(info.report_rates_ms, [1, 2, 4, 8]);
        assert_eq!(info.led_zones.len(), 1);
        assert_eq!((info.led_zones[0].location, info.led_zones[0].effects.clone()), (1, vec![0, 1, 3, 10]));
        assert_eq!(info.features.len(), 32, "the whole feature table is listed for the test report");
        assert!(info.extended.is_none(), "no extended DPI or polling rate: it has 0x2201 and 0x8060");
        assert_eq!(info.profile_list, ProfileList::Own);
        // Format 3 in 255-byte sectors is a tested memory: over the radio saves are experimental, with the undo.
        assert!(info.verified && info.writable && info.experimental && info.receiver && info.bluetooth.is_none());

        let state = task.state().unwrap();
        assert_eq!(state.active_profile, 3, "the mouse says 3, as the G502 HERO does: a profile number from 1");
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, true, true, false, false]);
        let first = &state.profiles[0].settings;
        assert_eq!((first.report_rate_ms, first.dpi_levels.clone(), first.default_dpi_index), (1, vec![4000], 0));
        assert_eq!(first.buttons.len(), 16);
        assert_eq!(first.buttons[0], ButtonAction::Mouse { button: 1 });
        assert_eq!(first.buttons[2], ButtonAction::Mouse { button: 3 });
        assert_eq!(first.buttons[3], ButtonAction::Special { function: profile::SpecialFunction::GShift });
        assert!(matches!(first.buttons[6], ButtonAction::Macro { sector: 6, offset: 0, .. }));
        // The last three slots hold functions PeriHub has no name for: they are kept as they are.
        assert_eq!(first.buttons[13], ButtonAction::Raw { bytes: [0x90, 0x10, 0xFF, 0x00] });
        assert!(first.gshift_buttons.iter().all(|b| *b == ButtonAction::Disabled));
        assert_eq!(state.profiles[2].settings.dpi_levels, [4750]);
        assert_eq!(state.profiles[4].settings.dpi_levels, [400, 800, 1600, 3200]);
        assert_eq!(state.profiles[4].settings.default_dpi_index, 1);
        assert_eq!(state.profiles[1].settings.buttons[6], ButtonAction::Key { modifiers: 0x00, key: 0x6C });

        let live = task.live().unwrap();
        assert_eq!(live.battery, Some(Battery { percent: 90, estimated: false, state: BatteryState::Discharging }));
        assert_eq!((live.active_profile, live.active_dpi_index, live.current_dpi), (3, 0, 4750));
        forget_test_files();
    }

    /// Opens the simulated mouse over Bluetooth Low Energy, where its answers come on device index 0x00.
    fn open_sim_over_bluetooth(sim: Sim, product_id: u16, untested_saves: bool) -> (MouseTask, Arc<Mutex<Sim>>) {
        let (fake, sim) = sim_transport(sim, Some(0x00));
        let dev = HidppDevice::over_bluetooth(fake, true).unwrap();
        let task = MouseTask::open(dev, model_of(product_id), untested_saves).unwrap();
        (task, sim)
    }

    #[test]
    fn a_g604_over_bluetooth_is_read_like_through_its_receiver_and_never_changed() {
        let model = models::find_bluetooth(0xB024).unwrap();
        assert_eq!(model.product_id, G604);
        // Even with saves allowed in the interface, and a memory format that was tested.
        let (mut task, sim) = open_sim_over_bluetooth(g604(), G604, true);
        let info = task.info.clone();
        assert_eq!((info.model.as_str(), info.bluetooth, info.receiver), ("G604", Some(0xB024), false));
        assert!(!info.verified && !info.writable && !info.experimental);
        assert!(info.writable_format, "the memory is one PeriHub can write: only the connection is the reason");
        assert_eq!((info.dpi.min, info.dpi.max, info.dpi.step), (100, 25600, 50));
        assert_eq!(info.report_rates_ms, [1, 2, 4, 8]);
        assert_eq!(info.profile_list, ProfileList::Own);
        let state = task.state().unwrap();
        assert_eq!((state.active_profile, state.profiles.len()), (3, 5));
        assert_eq!(state.profiles[0].settings.dpi_levels, [4000]);
        // The battery is read, as through a receiver.
        let live = task.live().unwrap();
        assert_eq!(live.battery.map(|b| b.percent), Some(90));
        assert_eq!(live.current_dpi, 4750);

        // The transcript for the test report names the Bluetooth id, and holds no unit id.
        assert!(info.transcript.contains("b024") && !info.transcript.contains("12 34 56 78"));

        // Nothing is changed, and what is asked is refused before it is sent.
        assert!(task.set_active_profile(2).unwrap_err().to_string().contains("Bluetooth"));
        assert!(task.set_active_dpi_index(1).is_err());
        assert!(task.preview_dpi(1600).is_err());
        assert!(task.save_profile(1, &edited(&state, 1)).is_err());
        assert!(task.set_profile_enabled(2, false).is_err());
        {
            let mouse = sim.lock().unwrap();
            assert!(mouse.written.is_empty() && mouse.selected.is_empty());
        }
        // Not even the DPI is written to the sensor, or the mode of the mouse switched.
        assert!(task.dev.set_sensor_dpi(0, 1600).is_err());
        assert!(task.dev.set_onboard_mode(HOST_MODE).is_err());
        forget_test_files();
    }

    #[test]
    fn a_mouse_over_bluetooth_that_does_not_answer_is_asleep_not_broken() {
        let silent = FakeDevice::new(|_| Vec::new());
        // The ping of the connection finds nobody: not an error to show, the manager asks again.
        assert!(matches!(HidppDevice::over_bluetooth(silent, true), Err(Error::Unreachable)));
    }

    #[test]
    fn set_active_profile_refuses_a_disabled_profile_before_sending_anything() {
        let mut sim = Sim::g502();
        sim.user[0] = profile::build_directory(&[true, true, false, true, false], 256);
        sim.refuses_disabled_profiles = true;
        let (mut task, sim) = open_sim(sim, G502_HERO, false);
        assert_eq!(task.info.enabled_profiles, vec![true, true, false, true, false]);

        let err = task.set_active_profile(3).unwrap_err().to_string();
        assert!(err.contains("disabled"), "{err}");
        {
            let mouse = sim.lock().unwrap();
            assert!(mouse.selected.is_empty() && mouse.refused_switches.is_empty());
        }

        // Enabling profile 3 updates `enabled_profiles` and lets the switch proceed.
        let state = task.set_profile_enabled(3, true).unwrap();
        assert_eq!(state.profiles.iter().map(|p| p.enabled).collect::<Vec<_>>(), [true, true, true, true, false]);
        assert_eq!(task.info.enabled_profiles, vec![true, true, true, true, false]);
        task.set_active_profile(3).unwrap();
        assert_eq!(sim.lock().unwrap().selected, [[0, 3]]);
        forget_test_files();
    }

    #[test]
    fn a_known_model_with_an_unknown_memory_format_stays_read_only_and_records_profile_one() {
        let (memory, onboard) = g502_memory();
        let onboard = OnboardInfo { rom_profile_count: 2, ..onboard };
        let mut sim = Sim::new(memory, onboard).resized(255, 10);
        sim.features = PRO_FEATURES;
        let (mut task, sim) = open_sim(sim, G502_X_WIRELESS, true);
        assert!(!task.info.verified);
        assert!(!task.info.writable);
        assert!(!task.info.writable_format);
        assert!(!task.info.experimental);
        assert!(!task.info.switches_back);

        // Both user sector 1 (00 01) and factory sector 1 (01 01) were recorded read-only in the transcript.
        assert!(task.info.transcript.contains("00 01 00 00"));
        assert!(task.info.transcript.contains("01 01 00 00"));

        // Mode switches back and profile writes stay off for the unknown memory format.
        let state = task.state().unwrap();
        assert!(task.save_profile(1, &edited(&state, 1)).is_err());
        assert!(task.set_active_profile(2).is_err());
        assert!(task.dev.set_onboard_mode(ONBOARD_MODE).is_err());
        assert!(sim.lock().unwrap().written.is_empty());
    }
}

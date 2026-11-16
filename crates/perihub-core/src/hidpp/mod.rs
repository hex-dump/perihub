//! Logitech HID++ 2.0 protocol (mice such as the G502 HERO, with a cable or through a receiver).
//!
//! References: libratbag (MIT) and data read from the real device.

pub mod direct;
pub mod host;
pub mod lighting;
pub mod macros;
pub mod models;
pub mod profile;
pub mod receiver;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use hidapi::HidDevice;
use serde::Serialize;

use crate::error::{Error, HidppErrorCode, Result};
use profile::LedEffect;
use receiver::Presence;

const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;
const SOFTWARE_ID: u8 = 0x0B;
const TIMEOUT: Duration = Duration::from_millis(1000);
/// HID++ 1.0 error message, which a receiver sends for a mouse it cannot reach.
const HIDPP10_ERROR: u8 = 0x8F;

pub mod feature {
    pub const ROOT: u16 = 0x0000;
    pub const FEATURE_SET: u16 = 0x0001;
    pub const DEVICE_INFO: u16 = 0x0003;
    pub const DEVICE_NAME: u16 = 0x0005;
    pub const BATTERY_STATUS: u16 = 0x1000;
    pub const BATTERY_VOLTAGE: u16 = 0x1001;
    pub const UNIFIED_BATTERY: u16 = 0x1004;
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
    pub const REPORT_RATE: u16 = 0x8060;
    pub const COLOR_LED_EFFECTS: u16 = 0x8070;
    pub const ONBOARD_PROFILES: u16 = 0x8100;
    pub const KEYBOARD_LAYOUT_2: u16 = 0x4540;
    pub const BRIGHTNESS_CONTROL: u16 = 0x8040;
    pub const RGB_EFFECTS: u16 = 0x8071;
    pub const PER_KEY_LIGHTING_V2: u16 = 0x8081;
    pub const PROFILE_MANAGEMENT: u16 = 0x8101;
    pub const CONTROL_LIST: u16 = 0x1B10;
    pub const FULL_KEY_CUSTOMIZATION: u16 = 0x1B05;
    pub const DISABLE_CONTROLS: u16 = 0x4523;
    pub const LOGI_MODIFIERS: u16 = 0x8051;
}

/// A HID connection, as [`HidppDevice`] uses it. Tests replace the device with a simulated one.
pub trait Transport: Send {
    fn write(&self, data: &[u8]) -> Result<()>;
    /// Reads one report into `buf`, waiting up to `millis`. Returns 0 if none arrived.
    fn read_timeout(&self, buf: &mut [u8], millis: i32) -> Result<usize>;
}

impl Transport for HidDevice {
    fn write(&self, data: &[u8]) -> Result<()> {
        HidDevice::write(self, data)?;
        Ok(())
    }

    fn read_timeout(&self, buf: &mut [u8], millis: i32) -> Result<usize> {
        Ok(HidDevice::read_timeout(self, buf, millis)?)
    }
}

/// The only HID++ features the app may use. The others (DFU, Logitech internal
/// features 0x18xx, 0x00c2…) must never be touched.
const ALLOWED_FEATURES: &[u16] = &[
    feature::ROOT,
    feature::FEATURE_SET,
    feature::DEVICE_INFO,
    feature::DEVICE_NAME,
    feature::BATTERY_STATUS,
    feature::BATTERY_VOLTAGE,
    feature::UNIFIED_BATTERY,
    feature::ADJUSTABLE_DPI,
    feature::REPORT_RATE,
    feature::COLOR_LED_EFFECTS,
    feature::ONBOARD_PROFILES,
    feature::KEYBOARD_LAYOUT_2,
    feature::BRIGHTNESS_CONTROL,
    feature::RGB_EFFECTS,
    feature::PER_KEY_LIGHTING_V2,
    feature::PROFILE_MANAGEMENT,
    feature::CONTROL_LIST,
    feature::FULL_KEY_CUSTOMIZATION,
    feature::DISABLE_CONTROLS,
    feature::LOGI_MODIFIERS,
];

/// The only (feature, function) pairs allowed on a read-only device: they read state and
/// change nothing, not even in RAM.
const READ_ONLY_CALLS: &[(u16, u8)] = &[
    (feature::FEATURE_SET, 0),
    (feature::FEATURE_SET, 1),
    (feature::DEVICE_INFO, 0),
    (feature::DEVICE_INFO, 1),
    (feature::DEVICE_NAME, 0),
    (feature::DEVICE_NAME, 1),
    (feature::DEVICE_NAME, 2),
    (feature::BATTERY_STATUS, 0),
    (feature::BATTERY_VOLTAGE, 0),
    (feature::UNIFIED_BATTERY, 1),
    (feature::ADJUSTABLE_DPI, 1),
    (feature::ADJUSTABLE_DPI, 2),
    (feature::REPORT_RATE, 0),
    (feature::REPORT_RATE, 1),
    (feature::COLOR_LED_EFFECTS, 0),
    (feature::COLOR_LED_EFFECTS, 1),
    (feature::COLOR_LED_EFFECTS, 2),
    (feature::ONBOARD_PROFILES, 0),
    (feature::ONBOARD_PROFILES, 2),
    (feature::ONBOARD_PROFILES, 4),
    (feature::ONBOARD_PROFILES, 5),
    (feature::ONBOARD_PROFILES, 0x0B),
    (feature::KEYBOARD_LAYOUT_2, 0),
    (feature::BRIGHTNESS_CONTROL, 0),
    (feature::BRIGHTNESS_CONTROL, 1),
    (feature::BRIGHTNESS_CONTROL, 3),
    (feature::RGB_EFFECTS, 0),
    (feature::PER_KEY_LIGHTING_V2, 0),
    // The newer keyboards' features no public document describes: only function 0, which other
    // projects asked on a G515 without effect (docs/protocols/logitech-keyboard-lighting.md).
    (feature::PROFILE_MANAGEMENT, 0),
    // Profile files: function 8 loads one into the keyboard's read buffer, function 12 reads 16 bytes
    // of it. Only asked on request, with no other Logitech program running (it shares the buffer).
    (feature::PROFILE_MANAGEMENT, 8),
    (feature::PROFILE_MANAGEMENT, 0x0C),
    (feature::FULL_KEY_CUSTOMIZATION, 0),
    (feature::DISABLE_CONTROLS, 0),
    (feature::LOGI_MODIFIERS, 0),
    // The list of the keyboard's controls: its size, then one entry at a time.
    (feature::CONTROL_LIST, 0),
    (feature::CONTROL_LIST, 1),
];

/// Functions that read or change, by their first parameter: 0 reads ("get"), anything else changes
/// (see docs/protocols/logitech-keyboard-lighting.md). A read-only device gets them only with 0.
const READ_ONLY_GETS: &[(u16, u8)] = &[
    // Startup and shutdown effects (the change is written to the device's memory).
    (feature::RGB_EFFECTS, 3),
    // Who drives the lights: the device or a program.
    (feature::RGB_EFFECTS, 5),
    // Idle and sleep timers of the lights (the change is likely kept in the device's memory).
    (feature::RGB_EFFECTS, 7),
    // Who drives the keyboard: itself, with its saved profiles, or a program (host mode).
    (feature::PROFILE_MANAGEMENT, 6),
];

/// What a program sends while it drives a keyboard's lights in host mode, as Logitech G HUB does
/// with its on-board memory mode off (docs/protocols/logitech-keyboard-lighting.md): everything
/// stays in the keyboard's RAM. Nothing else that changes is sent, not even with [`Access::Host`]:
/// no timers or startup effects (written to the keyboard's memory), no profile files, no power-off.
fn is_host_call(feature: u16, function: u8, params: &[u8]) -> bool {
    let first = params.first().copied().unwrap_or(0);
    match (feature, function) {
        (feature::BRIGHTNESS_CONTROL, 2) => true,
        // Host mode on (05) and back to the keyboard's own profiles (03).
        (feature::PROFILE_MANAGEMENT, 6) => matches!(first, 0x05 | 0x03),
        // Take the lights with the keyboard telling when it is idle (01 03 06), give them back (01 00 00).
        (feature::RGB_EFFECTS, 5) => params.starts_with(&[1, 3, 6]) || params.starts_with(&[1, 0, 0]),
        // An effect, kept in RAM only: the last byte is the persistence, 1 = RAM.
        (feature::RGB_EFFECTS, 1) => params.len() == 13 && params[12] == 0x01,
        // Lights off when idle (01 03 00), on again (01 01 00).
        (feature::RGB_EFFECTS, 8) => params.starts_with(&[1, 3, 0]) || params.starts_with(&[1, 1, 0]),
        (feature::PER_KEY_LIGHTING_V2, 1 | 5 | 6) => true,
        // End of a frame, in RAM (persistence 0).
        (feature::PER_KEY_LIGHTING_V2, 7) => first == 0,
        _ => false,
    }
}

fn is_read_call(feature: u16, function: u8, params: &[u8]) -> bool {
    READ_ONLY_CALLS.contains(&(feature, function))
        || (READ_ONLY_GETS.contains(&(feature, function)) && params.first().copied().unwrap_or(0) == 0)
}

/// Mouse memory sectors reserved for the factory profiles (read-only).
pub const ROM_SECTOR_BASE: u16 = 0x0100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DpiCapabilities {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    /// Allowed values, if the sensor exposes a list instead of a range.
    pub list: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedZone {
    pub index: u8,
    /// 1 = primary, 2 = logo (per the HID++ 0x8070 specification).
    pub location: u16,
    pub effects: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnboardInfo {
    pub memory_model: u8,
    pub profile_format: u8,
    pub macro_format: u8,
    pub profile_count: u8,
    pub rom_profile_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
    pub has_gshift: bool,
    pub has_dpi_shift: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirmwareEntity {
    pub kind: u8,
    pub name: String,
    pub version: String,
}

/// One entry of a device's feature table (0x0001 function 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FeatureEntry {
    pub id: u16,
    /// Flags: 0x80 obsolete, 0x40 hidden, 0x20 engineering.
    pub kind: u8,
    pub version: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryState {
    Discharging,
    Charging,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Battery {
    pub percent: u8,
    /// Worked out from the voltage or from a coarse level: the mouse reports no percentage.
    pub estimated: bool,
    pub state: BatteryState,
}

/// A battery feature's id, its reply as it came, and what PeriHub reads in it.
pub type BatteryReading = (u16, [u8; 16], Option<Battery>);

/// Charge of a single-cell lithium battery by voltage (mV), on its usual discharge curve.
const VOLTAGE_CURVE: &[(u16, u8)] = &[
    (4150, 100),
    (4050, 90),
    (3970, 80),
    (3910, 70),
    (3860, 60),
    (3820, 50),
    (3790, 40),
    (3760, 30),
    (3730, 20),
    (3690, 10),
    (3600, 5),
    (3500, 0),
];

/// One entry of the 0x0003 firmware list. The G502 X lists ten empty entries (no name,
/// version 00.00.0000) after its real ones: those are left out.
fn firmware_entity(r: &[u8]) -> Option<FirmwareEntity> {
    let name = String::from_utf8_lossy(&r[1..4]).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
    if name.is_empty() && r[4..8].iter().all(|&b| b == 0) {
        return None;
    }
    Some(FirmwareEntity { kind: r[0], name, version: format!("{:02x}.{:02x}.{:02x}{:02x}", r[4], r[5], r[6], r[7]) })
}

fn percent_from_voltage(millivolts: u16) -> u8 {
    let (top, bottom) = (VOLTAGE_CURVE[0], VOLTAGE_CURVE[VOLTAGE_CURVE.len() - 1]);
    if millivolts >= top.0 {
        return top.1;
    }
    if millivolts <= bottom.0 {
        return bottom.1;
    }
    let (high, low) = VOLTAGE_CURVE
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .find(|(_, low)| millivolts > low.0)
        .expect("between the ends of the curve");
    let span = (high.0 - low.0) as u32;
    (low.1 as u32 + (millivolts - low.0) as u32 * (high.1 - low.1) as u32 / span) as u8
}

/// 0x1004 function 1: charge in percent (0 if the mouse only knows a coarse level), the level
/// (bits: 8 full, 4 good, 2 low, 1 critical) and the charging state.
fn parse_unified_battery(r: &[u8; 16]) -> Option<Battery> {
    let state = match r[2] {
        1 | 2 => BatteryState::Charging,
        3 => BatteryState::Full,
        _ => BatteryState::Discharging,
    };
    if r[0] > 0 {
        return Some(Battery { percent: r[0].min(100), estimated: false, state });
    }
    let percent = match r[1] {
        level if level & 8 != 0 => 100,
        level if level & 4 != 0 => 60,
        level if level & 2 != 0 => 20,
        level if level & 1 != 0 => 5,
        _ => return None,
    };
    Some(Battery { percent, estimated: true, state })
}

/// 0x1000 function 0: charge in percent (0 if unknown), the next level, then the state:
/// 0 discharging, 1 charging, 2 almost full, 3 full, 4 charging slowly, 5–6 errors.
fn parse_battery_status(r: &[u8; 16]) -> Option<Battery> {
    let state = match r[2] {
        1 | 2 | 4 => BatteryState::Charging,
        3 => BatteryState::Full,
        _ => BatteryState::Discharging,
    };
    match (r[0], state) {
        (0, BatteryState::Full) => Some(Battery { percent: 100, estimated: true, state }),
        (0, _) => None,
        (percent, _) => Some(Battery { percent: percent.min(100), estimated: false, state }),
    }
}

/// 0x1001 function 0: voltage in mV, then flags: bit 7 on external power, and then bit 0 charged.
fn parse_battery_voltage(r: &[u8; 16]) -> Option<Battery> {
    let millivolts = u16::from_be_bytes([r[0], r[1]]);
    if millivolts == 0 {
        return None;
    }
    let state = match r[2] {
        flags if flags & 0x80 == 0 => BatteryState::Discharging,
        flags if flags & 0x01 != 0 => BatteryState::Full,
        _ => BatteryState::Charging,
    };
    Some(Battery { percent: percent_from_voltage(millivolts), estimated: true, state })
}

/// 0x0003 function 0: entity count, unit id (4 bytes), transports (2 bytes), then up to three
/// product ids (6 bytes), one for each transport bit set, from bit 0 up.
fn wireless_id_from_device_info(r: &[u8; 16]) -> Option<u16> {
    const RECEIVER_BIT: u8 = 2;
    let transports = r[6];
    if transports & (1 << RECEIVER_BIT) == 0 {
        return None;
    }
    let position = (transports & ((1 << RECEIVER_BIT) - 1)).count_ones() as usize;
    let at = 7 + 2 * position;
    Some(u16::from_be_bytes([r[at], r[at + 1]]))
}

/// HID++ 1.0 errors a receiver sends for one of its slots: 0x04 connection failed and
/// 0x09 resource error for a mouse that is off or asleep, 0x08 unknown device for an empty slot.
fn unreachable_code(code: u8) -> bool {
    matches!(code, 0x04 | 0x08 | 0x09)
}

/// What PeriHub may send to a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Only [`READ_ONLY_CALLS`]: nothing changes, not even in RAM.
    ReadOnly,
    /// Also changes that stay in the device's RAM (active profile, DPI level, previews), but
    /// nothing written to its onboard memory.
    Ram,
    /// Everything in [`ALLOWED_FEATURES`], onboard memory writes included.
    Full,
    /// The reads, and what drives a keyboard's lights in host mode (see [`is_host_call`]).
    Host,
}

/// Onboard memory writes (0x8100 functions 6, 7 and 8), and the changes of the 0x8071 startup
/// and shutdown effects and light timers: the calls that outlast a power cycle.
fn is_memory_write(feature: u16, function: u8, params: &[u8]) -> bool {
    (feature == feature::ONBOARD_PROFILES && matches!(function, 6..=8))
        || (feature == feature::RGB_EFFECTS && matches!(function, 3 | 7) && params.first().copied().unwrap_or(0) != 0)
        // The newer keyboards' profiles and key tables: never with RAM access, only host mode's own calls.
        || (matches!(
            feature,
            feature::PROFILE_MANAGEMENT
                | feature::FULL_KEY_CUSTOMIZATION
                | feature::DISABLE_CONTROLS
                | feature::LOGI_MODIFIERS
                | feature::CONTROL_LIST
        ) && !is_read_call(feature, function, params))
}

/// A report the device sent on its own (software id 0), such as a lighting feature telling that
/// the keyboard is idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub feature: u16,
    /// The event number (the high 4 bits of byte 3).
    pub event: u8,
    pub data: [u8; 16],
}

/// At most this many events wait to be read: older ones are dropped.
const MAX_EVENTS: usize = 64;

pub struct HidppDevice {
    /// Events that came while PeriHub waited for a reply, by feature index.
    events: std::collections::VecDeque<(u8, u8, [u8; 16])>,
    dev: Box<dyn Transport>,
    device_index: u8,
    access: Access,
    /// Feature indexes, 0 for a feature the device does not have.
    features: HashMap<u16, u8>,
    /// Reached through a LIGHTSPEED receiver, in the slot `device_index`.
    behind_receiver: bool,
    /// The receiver's short HID++ collection, when it has its own path (Windows).
    short: Option<Box<dyn Transport>>,
}

impl HidppDevice {
    /// `read_only` limits the device to [`READ_ONLY_CALLS`] until [`Self::grant`] says otherwise.
    pub fn new(dev: impl Transport + 'static, device_index: u8, read_only: bool) -> Result<Self> {
        Self::open(Box::new(dev), None, device_index, false, read_only)
    }

    /// The mouse in `slot` of a LIGHTSPEED receiver. `long` is the receiver's long HID++
    /// collection; `short` its short one, when that has a path of its own (Windows).
    pub fn behind_receiver(
        long: impl Transport + 'static,
        short: Option<Box<dyn Transport>>,
        slot: u8,
        read_only: bool,
    ) -> Result<Self> {
        Self::open(Box::new(long), short, slot, true, read_only)
    }

    fn open(
        dev: Box<dyn Transport>,
        short: Option<Box<dyn Transport>>,
        device_index: u8,
        behind_receiver: bool,
        read_only: bool,
    ) -> Result<Self> {
        let access = if read_only { Access::ReadOnly } else { Access::Full };
        let mut device = Self {
            events: std::collections::VecDeque::new(),
            dev,
            device_index,
            access,
            features: HashMap::new(),
            behind_receiver,
            short,
        };
        let (major, _) = device.protocol_version()?;
        if major < 2 {
            return Err(Error::Protocol(format!("HID++ {major}.x is not supported")));
        }
        Ok(device)
    }

    pub fn is_behind_receiver(&self) -> bool {
        self.behind_receiver
    }

    /// Changes what PeriHub may send. A mouse is opened read-only, and [`crate::mouse::MouseTask`]
    /// grants more once it has read which memory format the mouse uses.
    pub fn grant(&mut self, access: Access) {
        self.access = access;
    }

    fn transact(&mut self, index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        match self.exchange(index, function, params) {
            // On Windows the receiver says that the mouse is asleep on its other collection: ask there.
            Err(Error::Timeout) if self.behind_receiver => {
                let (channel, long) = match self.short.as_deref() {
                    Some(short) => (short, Some(&*self.dev)),
                    None => (&*self.dev, None),
                };
                match receiver::ping(channel, long, self.device_index)? {
                    Presence::Online => Err(Error::Timeout),
                    Presence::Asleep | Presence::Empty => Err(Error::Unreachable),
                }
            }
            other => other,
        }
    }

    fn exchange(&mut self, index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        if params.len() > LONG_LEN - 4 {
            return Err(Error::Invalid("too many HID++ parameters".into()));
        }
        let address = (function << 4) | SOFTWARE_ID;
        let mut packet = [0u8; LONG_LEN];
        packet[0] = REPORT_LONG;
        packet[1] = self.device_index;
        packet[2] = index;
        packet[3] = address;
        packet[4..4 + params.len()].copy_from_slice(params);
        self.dev.write(&packet)?;

        let deadline = Instant::now() + TIMEOUT;
        let mut buf = [0u8; 64];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout);
            }
            let n = self.dev.read_timeout(&mut buf, remaining.as_millis().max(1) as i32)?;
            // The mouse's keyboard/media reports also arrive on this interface: ignore them.
            if n < 7 || !(buf[0] == REPORT_SHORT || buf[0] == REPORT_LONG) || buf[1] != self.device_index {
                continue;
            }
            if self.keep_event(&buf[..n]) {
                continue;
            }
            if buf[2] == index && buf[3] == address {
                let mut out = [0u8; 16];
                let len = (n - 4).min(16);
                out[..len].copy_from_slice(&buf[4..4 + len]);
                return Ok(out);
            }
            if buf[2] == 0xFF && buf[3] == index && buf[4] == address {
                return Err(Error::Hidpp(HidppErrorCode(buf[5])));
            }
            if buf[0] == REPORT_SHORT && buf[2] == HIDPP10_ERROR && buf[3] == index && buf[4] == address {
                return Err(if unreachable_code(buf[5]) {
                    Error::Unreachable
                } else {
                    Error::Protocol(format!("HID++ 1.0 error {:#04x}", buf[5]))
                });
            }
        }
    }

    /// Keeps `report` if it is an event of this device (software id 0, a feature index, not an
    /// error): it is no reply to anything PeriHub asked.
    fn keep_event(&mut self, report: &[u8]) -> bool {
        let event = report.len() >= 7
            && (report[0] == REPORT_SHORT || report[0] == REPORT_LONG)
            && report[1] == self.device_index
            && report[2] != 0
            && report[2] != 0xFF
            && report[2] < 0x80
            && report[3] & 0x0F == 0;
        if event {
            let mut data = [0u8; 16];
            let len = (report.len() - 4).min(16);
            data[..len].copy_from_slice(&report[4..4 + len]);
            if self.events.len() == MAX_EVENTS {
                self.events.pop_front();
            }
            self.events.push_back((report[2], report[3] >> 4, data));
        }
        event
    }

    /// The events the device sent since the last call: those that came while PeriHub waited for a
    /// reply, and those waiting now, read for up to `wait_ms`. On Windows a receiver has two
    /// collections, and both are read (issue #4).
    pub fn events(&mut self, wait_ms: i32) -> Result<Vec<Event>> {
        let mut buf = [0u8; 64];
        let deadline = Instant::now() + Duration::from_millis(wait_ms.max(0) as u64);
        loop {
            let mut any = false;
            let n = self.dev.read_timeout(&mut buf, 1)?;
            if n > 0 {
                any = true;
                let report = buf;
                self.keep_event(&report[..n]);
            }
            if let Some(short) = &self.short {
                let n = short.read_timeout(&mut buf, 1)?;
                if n > 0 {
                    any = true;
                    let report = buf;
                    self.keep_event(&report[..n]);
                }
            }
            if !any && Instant::now() >= deadline {
                break;
            }
        }
        let ids: HashMap<u8, u16> = self.features.iter().map(|(&id, &index)| (index, id)).collect();
        Ok(self
            .events
            .drain(..)
            .filter_map(|(index, event, data)| ids.get(&index).map(|&feature| Event { feature, event, data }))
            .collect())
    }

    fn feature_index(&mut self, id: u16) -> Result<u8> {
        if !ALLOWED_FEATURES.contains(&id) {
            return Err(Error::Invalid(format!("HID++ feature {id:#06x} is not allowed")));
        }
        if id == feature::ROOT {
            return Ok(0);
        }
        let index = match self.features.get(&id) {
            Some(&index) => index,
            None => {
                let index = self.transact(0, 0, &id.to_be_bytes())?[0];
                self.features.insert(id, index);
                index
            }
        };
        if index == 0 { Err(Error::FeatureMissing(id)) } else { Ok(index) }
    }

    fn call(&mut self, feature: u16, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        match self.access {
            Access::ReadOnly if !is_read_call(feature, function, params) => {
                return Err(Error::Invalid("PeriHub only reads this mouse".into()));
            }
            Access::Ram if is_memory_write(feature, function, params) => {
                return Err(Error::Invalid("PeriHub does not save to this mouse's memory".into()));
            }
            Access::Host if !is_read_call(feature, function, params) && !is_host_call(feature, function, params) => {
                return Err(Error::Invalid("PeriHub does not send this to this keyboard".into()));
            }
            _ => {}
        }
        let index = self.feature_index(feature)?;
        self.transact(index, function, params)
    }

    pub fn has_feature(&mut self, id: u16) -> bool {
        self.feature_index(id).is_ok()
    }

    pub fn protocol_version(&mut self) -> Result<(u8, u8)> {
        let reply = self.transact(0, 1, &[0, 0, 0x5A])?;
        Ok((reply[0], reply[1]))
    }

    pub fn device_name(&mut self) -> Result<String> {
        let length = self.call(feature::DEVICE_NAME, 0, &[])?[0] as usize;
        let mut name = Vec::with_capacity(length);
        while name.len() < length {
            let chunk = self.call(feature::DEVICE_NAME, 1, &[name.len() as u8])?;
            let take = (length - name.len()).min(16);
            name.extend_from_slice(&chunk[..take]);
        }
        Ok(String::from_utf8_lossy(&name).trim_end_matches('\0').to_string())
    }

    /// 0x0005 function 2: what kind of device it is. Logitech's HID++ 2.0 specification lists 0
    /// keyboard, 1 remote control, 2 numpad, 3 mouse, 4 touchpad, 5 trackball, 6 presenter and 7
    /// receiver; newer devices may report other values.
    pub fn device_type(&mut self) -> Result<u8> {
        Ok(self.call(feature::DEVICE_NAME, 2, &[])?[0])
    }

    /// The id the mouse has behind a LIGHTSPEED receiver. 0x0003 function 0 lists one product id
    /// for each way the mouse connects, in the order of the transport bits: Bluetooth (bit 0),
    /// Bluetooth Low Energy (1), receiver (2), USB (3).
    pub fn wireless_id(&mut self) -> Result<Option<u16>> {
        Ok(wireless_id_from_device_info(&self.device_info()?))
    }

    /// 0x0003 function 0 as the device sends it: firmware entity count, unit id, transport bits
    /// and the product ids.
    pub fn device_info(&mut self) -> Result<[u8; 16]> {
        self.call(feature::DEVICE_INFO, 0, &[])
    }

    /// The device's feature table (0x0001). It only lists the features: none of them is called.
    pub fn feature_table(&mut self) -> Result<Vec<FeatureEntry>> {
        // The count leaves out the root feature, which has index 0.
        let count = self.call(feature::FEATURE_SET, 0, &[])?[0];
        let mut out = Vec::with_capacity(count as usize);
        for index in 1..=count {
            let r = self.call(feature::FEATURE_SET, 1, &[index])?;
            out.push(FeatureEntry { id: u16::from_be_bytes([r[0], r[1]]), kind: r[2], version: r[3] });
        }
        Ok(out)
    }

    /// The battery, from whichever battery feature the mouse has, or `None` without one.
    pub fn battery(&mut self) -> Result<Option<Battery>> {
        Ok(self.battery_reading()?.and_then(|(_, _, battery)| battery))
    }

    /// The battery feature the device has, its reply as it came, and what PeriHub reads in it, or
    /// `None` without a battery feature.
    pub fn battery_reading(&mut self) -> Result<Option<BatteryReading>> {
        type Parse = fn(&[u8; 16]) -> Option<Battery>;
        let features: [(u16, u8, Parse); 3] = [
            (feature::UNIFIED_BATTERY, 1, parse_unified_battery),
            (feature::BATTERY_STATUS, 0, parse_battery_status),
            (feature::BATTERY_VOLTAGE, 0, parse_battery_voltage),
        ];
        for (feature, function, parse) in features {
            match self.call(feature, function, &[]) {
                Ok(reply) => return Ok(Some((feature, reply, parse(&reply)))),
                Err(Error::FeatureMissing(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    pub fn firmware(&mut self) -> Result<Vec<FirmwareEntity>> {
        let count = self.call(feature::DEVICE_INFO, 0, &[])?[0];
        let mut out = Vec::new();
        for entity in 0..count {
            let r = self.call(feature::DEVICE_INFO, 1, &[entity])?;
            if let Some(entity) = firmware_entity(&r) {
                out.push(entity);
            }
        }
        Ok(out)
    }

    // ---- 0x2201 adjustable DPI ----

    pub fn dpi_capabilities(&mut self, sensor: u8) -> Result<DpiCapabilities> {
        let r = self.call(feature::ADJUSTABLE_DPI, 1, &[sensor])?;
        let mut caps = DpiCapabilities { min: u16::MAX, max: 0, step: 0, list: Vec::new() };
        for pair in r[1..].as_chunks::<2>().0 {
            let value = u16::from_be_bytes([pair[0], pair[1]]);
            if value == 0 {
                break;
            }
            if value > 0xE000 {
                caps.step = value - 0xE000;
            } else {
                caps.min = caps.min.min(value);
                caps.max = caps.max.max(value);
                caps.list.push(value);
            }
        }
        if caps.step > 0 {
            caps.list.clear();
        }
        if caps.max == 0 {
            return Err(Error::Protocol("empty DPI list".into()));
        }
        Ok(caps)
    }

    /// Returns (current DPI, default DPI).
    pub fn sensor_dpi(&mut self, sensor: u8) -> Result<(u16, u16)> {
        let r = self.call(feature::ADJUSTABLE_DPI, 2, &[sensor])?;
        Ok((u16::from_be_bytes([r[1], r[2]]), u16::from_be_bytes([r[3], r[4]])))
    }

    /// Changes the DPI immediately, without writing to the mouse's memory.
    pub fn set_sensor_dpi(&mut self, sensor: u8, dpi: u16) -> Result<()> {
        let [hi, lo] = dpi.to_be_bytes();
        self.call(feature::ADJUSTABLE_DPI, 3, &[sensor, hi, lo])?;
        Ok(())
    }

    // ---- 0x8060 polling rate ----

    /// Supported intervals in milliseconds (1 ms = 1000 Hz).
    pub fn report_rates(&mut self) -> Result<Vec<u8>> {
        let mask = self.call(feature::REPORT_RATE, 0, &[])?[0];
        Ok((0..8).filter(|bit| mask & (1 << bit) != 0).map(|bit| bit + 1).collect())
    }

    pub fn report_rate(&mut self) -> Result<u8> {
        Ok(self.call(feature::REPORT_RATE, 1, &[])?[0])
    }

    // ---- 0x8070 LED effects ----

    pub fn led_zones(&mut self) -> Result<Vec<LedZone>> {
        let count = self.call(feature::COLOR_LED_EFFECTS, 0, &[])?[0];
        let mut zones = Vec::new();
        for zone in 0..count {
            let info = self.call(feature::COLOR_LED_EFFECTS, 1, &[zone])?;
            let mut effects = Vec::new();
            for effect in 0..info[3] {
                let e = self.call(feature::COLOR_LED_EFFECTS, 2, &[zone, effect])?;
                effects.push(u16::from_be_bytes([e[2], e[3]]));
            }
            zones.push(LedZone { index: zone, location: u16::from_be_bytes([info[1], info[2]]), effects });
        }
        Ok(zones)
    }

    /// Applies a LED effect immediately, in RAM only (preview): saved profiles are not touched.
    /// `zone.effects` is needed because here the mouse wants the effect's position in its list,
    /// while saved profiles use the effect code.
    pub fn preview_led_effect(&mut self, zone: &LedZone, effect: &LedEffect) -> Result<()> {
        let mut encoded = effect.encode();
        let position = zone
            .effects
            .iter()
            .position(|&id| id == encoded[0] as u16)
            .ok_or_else(|| Error::Invalid("effect not supported by this zone".into()))?;
        encoded[0] = position as u8;
        let mut params = [0u8; 13];
        params[0] = zone.index;
        params[1..12].copy_from_slice(&encoded);
        params[12] = 0;
        self.call(feature::COLOR_LED_EFFECTS, 3, &params)?;
        Ok(())
    }

    // ---- 0x8100 onboard profiles ----

    pub fn onboard_info(&mut self) -> Result<OnboardInfo> {
        let r = self.call(feature::ONBOARD_PROFILES, 0, &[])?;
        Ok(OnboardInfo {
            memory_model: r[0],
            profile_format: r[1],
            macro_format: r[2],
            profile_count: r[3],
            rom_profile_count: r[4],
            button_count: r[5],
            sector_count: r[6],
            sector_size: u16::from_be_bytes([r[7], r[8]]),
            has_gshift: r[9] & 0x03 == 0x02,
            has_dpi_shift: (r[9] & 0x0C) >> 2 == 0x02,
        })
    }

    /// 1 = onboard profiles in the mouse's memory, 2 = host (software) control.
    pub fn onboard_mode(&mut self) -> Result<u8> {
        Ok(self.call(feature::ONBOARD_PROFILES, 2, &[])?[0])
    }

    pub fn set_onboard_mode(&mut self, mode: u8) -> Result<()> {
        if mode != 1 && mode != 2 {
            return Err(Error::Invalid(format!("mode {mode}")));
        }
        self.call(feature::ONBOARD_PROFILES, 1, &[mode])?;
        Ok(())
    }

    /// Active profile, numbered from 1.
    pub fn active_profile(&mut self) -> Result<u8> {
        Ok(self.call(feature::ONBOARD_PROFILES, 4, &[])?[1])
    }

    /// Selects a profile by its address: sector `number` of the mouse's own memory, or of the
    /// factory profiles (0x01nn, which Solaar selects the same way) with `factory`.
    pub fn set_active_profile(&mut self, factory: bool, number: u8) -> Result<()> {
        self.call(feature::ONBOARD_PROFILES, 3, &[factory as u8, number])?;
        Ok(())
    }

    pub fn active_dpi_index(&mut self) -> Result<u8> {
        Ok(self.call(feature::ONBOARD_PROFILES, 0x0B, &[])?[0])
    }

    pub fn set_active_dpi_index(&mut self, index: u8) -> Result<()> {
        if index > 4 {
            return Err(Error::Invalid(format!("DPI level {index}")));
        }
        self.call(feature::ONBOARD_PROFILES, 0x0C, &[index])?;
        Ok(())
    }

    pub fn read_sector(&mut self, sector: u16, size: u16) -> Result<Vec<u8>> {
        let size = size as usize;
        // Reads come in blocks of 16 bytes: a smaller sector cannot be read this way.
        if size < 16 {
            return Err(Error::Protocol(format!("the mouse reports {size}-byte sectors")));
        }
        let mut data = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            // The firmware rejects reads past the end of the sector: the last block is aligned to size-16.
            let at = if size - offset < 16 { size - 16 } else { offset };
            let [s_hi, s_lo] = sector.to_be_bytes();
            let [o_hi, o_lo] = (at as u16).to_be_bytes();
            let chunk = self.call(feature::ONBOARD_PROFILES, 5, &[s_hi, s_lo, o_hi, o_lo])?;
            data[at..at + 16].copy_from_slice(&chunk);
            offset = at + 16;
        }
        Ok(data)
    }

    /// Writes a user sector (directory, profile or macros). Factory sectors cannot be written.
    pub fn write_sector(&mut self, sector: u16, data: &[u8], info: &OnboardInfo) -> Result<()> {
        if sector >= info.sector_count as u16 {
            return Err(Error::Invalid(format!("sector {sector:#06x} is not writable")));
        }
        if data.len() != info.sector_size as usize {
            return Err(Error::Invalid("wrong sector size".into()));
        }
        if !profile::crc_is_valid(data) {
            return Err(Error::Invalid("invalid sector CRC".into()));
        }
        let [s_hi, s_lo] = sector.to_be_bytes();
        let [l_hi, l_lo] = (data.len() as u16).to_be_bytes();
        self.call(feature::ONBOARD_PROFILES, 6, &[s_hi, s_lo, 0, 0, l_hi, l_lo])?;
        // Data goes in blocks of 16 bytes. When the size is not a multiple of 16 (255 bytes on some
        // firmware) the last block is shorter: the rest of its report is zeros, past the length
        // announced above.
        for chunk in data.chunks(16) {
            self.call(feature::ONBOARD_PROFILES, 7, chunk)?;
        }
        self.call(feature::ONBOARD_PROFILES, 8, &[])?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    type Responder = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>> + Send>;

    /// A simulated HID connection: `respond` turns each report written into the reports read back.
    pub(crate) struct FakeDevice {
        respond: Mutex<Responder>,
        pub(crate) pending: Arc<Mutex<VecDeque<Vec<u8>>>>,
        pub(crate) written: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl FakeDevice {
        pub(crate) fn new(respond: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) -> Self {
            Self {
                respond: Mutex::new(Box::new(respond)),
                pending: Arc::new(Mutex::new(VecDeque::new())),
                written: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Transport for FakeDevice {
        fn write(&self, data: &[u8]) -> Result<()> {
            self.written.lock().unwrap().push(data.to_vec());
            let replies = (self.respond.lock().unwrap())(data);
            self.pending.lock().unwrap().extend(replies);
            Ok(())
        }

        fn read_timeout(&self, buf: &mut [u8], millis: i32) -> Result<usize> {
            match self.pending.lock().unwrap().pop_front() {
                Some(report) => {
                    buf[..report.len()].copy_from_slice(&report);
                    Ok(report.len())
                }
                None => {
                    std::thread::sleep(Duration::from_millis(millis.clamp(0, 5) as u64));
                    Ok(0)
                }
            }
        }
    }

    /// The reply a mouse sends to `request`, with `payload` after the four header bytes.
    pub(crate) fn reply(request: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut report = vec![0u8; LONG_LEN];
        report[0] = REPORT_LONG;
        report[1..4].copy_from_slice(&request[1..4]);
        report[4..4 + payload.len()].copy_from_slice(payload);
        report
    }

    /// The HID++ 1.0 error a receiver sends for `request`.
    pub(crate) fn receiver_error(request: &[u8], code: u8) -> Vec<u8> {
        vec![REPORT_SHORT, request[1], HIDPP10_ERROR, request[2], request[3], code, 0]
    }

    /// A HID++ 2.0 mouse whose features are listed in `features` by index (index 0 is the root).
    pub(crate) fn fake_mouse(features: &'static [u16], answer: fn(u16, u8) -> Vec<u8>) -> FakeDevice {
        FakeDevice::new(move |request| {
            let (index, function) = (request[2] as usize, request[3] >> 4);
            if index == 0 && function == 1 {
                return vec![reply(request, &[4, 2, request[6]])];
            }
            if index == 0 && function == 0 {
                let id = u16::from_be_bytes([request[4], request[5]]);
                let found = features.iter().position(|&f| f == id).map_or(0, |i| i as u8);
                return vec![reply(request, &[found])];
            }
            vec![reply(request, &answer(features[index], function))]
        })
    }

    #[test]
    fn battery_comes_from_the_feature_the_mouse_has() {
        const FEATURES: &[u16] = &[feature::ROOT, feature::BATTERY_VOLTAGE];
        let fake = fake_mouse(FEATURES, |_, _| vec![0x0F, 0x82, 0x00]);
        let written = fake.written.clone();
        let mut dev = HidppDevice::new(fake, models::WIRED_INDEX, true).unwrap();
        let battery = dev.battery().unwrap().unwrap();
        // 3970 mV, on battery.
        assert_eq!(battery, Battery { percent: 80, estimated: true, state: BatteryState::Discharging });
        let lookups = written.lock().unwrap().len();
        dev.battery().unwrap();
        // The missing features are not asked for again: only the voltage is read.
        assert_eq!(written.lock().unwrap().len(), lookups + 1);
    }

    #[test]
    fn empty_firmware_entries_are_left_out() {
        let mut r = [0u8; 16];
        r[..8].copy_from_slice(&[0x01, b'B', b'L', b'1', 0x48, 0x00, 0x00, 0x14]);
        let entity = firmware_entity(&r).unwrap();
        assert_eq!((entity.kind, entity.name.as_str(), entity.version.as_str()), (1, "BL1", "48.00.0014"));
        // A name padded with zeros instead of spaces.
        r[..8].copy_from_slice(&[0x02, b'U', 0, 0, 0x01, 0x02, 0x00, 0x03]);
        assert_eq!(firmware_entity(&r).unwrap().name, "U");
        r[..8].copy_from_slice(&[0x02, 0, 0, 0, 0, 0, 0, 0]);
        assert!(firmware_entity(&r).is_none());
        r[..8].copy_from_slice(&[0x02, b' ', b' ', b' ', 0, 0, 0, 0]);
        assert!(firmware_entity(&r).is_none());
    }

    #[test]
    fn battery_reports_are_decoded() {
        let mut r = [0u8; 16];
        r[..3].copy_from_slice(&[57, 4, 0]);
        assert_eq!(
            parse_unified_battery(&r),
            Some(Battery { percent: 57, estimated: false, state: BatteryState::Discharging })
        );
        r[..3].copy_from_slice(&[0, 2, 1]);
        assert_eq!(
            parse_unified_battery(&r),
            Some(Battery { percent: 20, estimated: true, state: BatteryState::Charging })
        );
        r[..3].copy_from_slice(&[90, 100, 2]);
        assert_eq!(
            parse_battery_status(&r),
            Some(Battery { percent: 90, estimated: false, state: BatteryState::Charging })
        );
        r[..3].copy_from_slice(&[0, 0, 0]);
        assert_eq!(parse_battery_status(&r), None);
        r[..3].copy_from_slice(&[0x10, 0x68, 0x81]);
        assert_eq!(
            parse_battery_voltage(&r),
            Some(Battery { percent: 100, estimated: true, state: BatteryState::Full })
        );
        r[..3].copy_from_slice(&[0x0E, 0x10, 0x80]);
        assert_eq!(
            parse_battery_voltage(&r),
            Some(Battery { percent: 5, estimated: true, state: BatteryState::Charging })
        );
        assert_eq!(percent_from_voltage(3000), 0);
        assert_eq!(percent_from_voltage(3840), 55);
    }

    #[test]
    fn wireless_id_follows_the_transport_bits() {
        let mut r = [0u8; 16];
        // Receiver and USB: the receiver's id comes first.
        r[6] = 0b1100;
        r[7..11].copy_from_slice(&[0x40, 0x93, 0xC0, 0x94]);
        assert_eq!(wireless_id_from_device_info(&r), Some(0x4093));
        // Bluetooth, receiver and USB: the receiver's id is the second one.
        r[6] = 0b1101;
        r[7..13].copy_from_slice(&[0xB0, 0x15, 0x40, 0x79, 0xC0, 0x88]);
        assert_eq!(wireless_id_from_device_info(&r), Some(0x4079));
        // USB only, like the G502 HERO.
        r[6] = 0b1000;
        assert_eq!(wireless_id_from_device_info(&r), None);
    }

    #[test]
    fn a_mouse_the_receiver_cannot_reach_is_reported_at_once() {
        let fake = FakeDevice::new(|request| {
            if request[2] == 0 && request[3] >> 4 == 1 {
                vec![reply(request, &[4, 2, request[6]])]
            } else {
                vec![receiver_error(request, 0x09)]
            }
        });
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        let started = Instant::now();
        assert!(matches!(dev.firmware(), Err(Error::Unreachable)));
        assert!(started.elapsed() < TIMEOUT);
    }

    /// The reports `write_sector` sends for a sector of `size` bytes, after the feature lookup.
    fn sector_write_reports(size: usize) -> Vec<Vec<u8>> {
        const FEATURES: &[u16] = &[feature::ROOT, feature::ONBOARD_PROFILES];
        let fake = fake_mouse(FEATURES, |_, _| vec![]);
        let written = fake.written.clone();
        let mut dev = HidppDevice::new(fake, models::WIRED_INDEX, false).unwrap();
        let info = OnboardInfo {
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
        let mut data: Vec<u8> = (0..size).map(|i| i as u8).collect();
        profile::write_crc(&mut data);
        dev.write_sector(3, &data, &info).unwrap();
        let reports = written.lock().unwrap();
        reports.iter().filter(|r| r[2] == 1).cloned().collect()
    }

    /// A report to feature index 1 (onboard profiles) of the fake mouse.
    fn onboard_report(function: u8, params: &[u8]) -> Vec<u8> {
        let mut report = vec![0u8; LONG_LEN];
        report[..4].copy_from_slice(&[REPORT_LONG, models::WIRED_INDEX, 1, (function << 4) | SOFTWARE_ID]);
        report[4..4 + params.len()].copy_from_slice(params);
        report
    }

    #[test]
    fn a_256_byte_sector_is_written_in_sixteen_full_blocks() {
        let mut data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        profile::write_crc(&mut data);
        let mut expected = vec![onboard_report(6, &[0, 3, 0, 0, 0x01, 0x00])];
        expected.extend(data.chunks(16).map(|block| onboard_report(7, block)));
        expected.push(onboard_report(8, &[]));
        assert_eq!(sector_write_reports(256), expected);
    }

    /// Some mice report 255-byte sectors (issue #20): 255 bytes are announced, and the last
    /// of the 16 blocks carries 15 of them.
    #[test]
    fn a_255_byte_sector_ends_with_a_shorter_block() {
        let reports = sector_write_reports(255);
        assert_eq!(reports.len(), 18);
        assert_eq!(reports[0], onboard_report(6, &[0, 3, 0, 0, 0x00, 0xFF]));
        let mut data: Vec<u8> = (0..255).map(|i| i as u8).collect();
        profile::write_crc(&mut data);
        let sent: Vec<u8> = reports[1..17].iter().flat_map(|r| r[4..20].to_vec()).collect();
        assert_eq!(sent[..255], data[..]);
        assert_eq!(sent[255], 0, "padding after the announced length");
        assert_eq!(reports[17], onboard_report(8, &[]));
    }

    #[test]
    fn read_only_devices_cannot_change_anything() {
        let writes = [
            (feature::BATTERY_STATUS, 2),
            (feature::BATTERY_VOLTAGE, 1),
            (feature::UNIFIED_BATTERY, 2),
            (feature::ADJUSTABLE_DPI, 3),
            (feature::COLOR_LED_EFFECTS, 3),
            (feature::ONBOARD_PROFILES, 1),
            (feature::ONBOARD_PROFILES, 3),
            (feature::ONBOARD_PROFILES, 6),
            (feature::ONBOARD_PROFILES, 7),
            (feature::ONBOARD_PROFILES, 8),
            (feature::ONBOARD_PROFILES, 0x0C),
        ];
        for (feature, function) in writes {
            assert!(!is_read_call(feature, function, &[]), "{feature:#06x}/{function}");
        }
        assert!(READ_ONLY_CALLS.iter().all(|(feature, _)| ALLOWED_FEATURES.contains(feature)));
        assert!(READ_ONLY_GETS.iter().all(|(feature, _)| ALLOWED_FEATURES.contains(feature)));
    }

    #[test]
    fn of_the_lighting_features_a_read_only_device_gets_only_the_reads() {
        use feature::{BRIGHTNESS_CONTROL, PER_KEY_LIGHTING_V2, RGB_EFFECTS};
        // 0x8040: brightness and on/off are set by functions 2 and 4.
        assert!(!is_read_call(BRIGHTNESS_CONTROL, 2, &[0, 50]));
        assert!(!is_read_call(BRIGHTNESS_CONTROL, 4, &[1]));
        // 0x8071: the effect (1, 2), sync (6), power mode (8, 9) and anything past them.
        for function in [1, 2, 4, 6, 8, 9, 0x0E] {
            assert!(!is_read_call(RGB_EFFECTS, function, &[0]), "0x8071/{function}");
        }
        // Functions 3, 5 and 7 read with a first byte 0, and change with anything else.
        for function in [3, 5, 7] {
            assert!(is_read_call(RGB_EFFECTS, function, &[0, 0, 1]), "0x8071/{function} get");
            assert!(is_read_call(RGB_EFFECTS, function, &[]), "0x8071/{function} get, no params");
            assert!(!is_read_call(RGB_EFFECTS, function, &[1, 3, 4]), "0x8071/{function} set");
        }
        // 0x8081: only the list of keys; every function after 0 sets colours.
        for function in 1..=7 {
            assert!(!is_read_call(PER_KEY_LIGHTING_V2, function, &[0]), "0x8081/{function}");
        }
        // With RAM changes allowed, what stays after a power cycle is still refused.
        assert!(is_memory_write(RGB_EFFECTS, 3, &[1, 0, 1]));
        assert!(is_memory_write(RGB_EFFECTS, 7, &[1, 0, 0]));
        assert!(!is_memory_write(RGB_EFFECTS, 3, &[0, 0, 1]));
    }

    #[test]
    fn access_decides_what_reaches_the_device() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let fake = FakeDevice::new(move |request| {
            log.lock().unwrap().push((request[2], request[3] >> 4));
            // IRoot: HID++ 4.2, and every feature at index 1.
            match (request[2], request[3] >> 4) {
                (0, 1) => vec![reply(request, &[4, 2, request[6]])],
                (0, _) => vec![reply(request, &[1])],
                _ => vec![reply(request, &[])],
            }
        });
        let mut dev = HidppDevice::new(fake, models::WIRED_INDEX, true).unwrap();
        let info = OnboardInfo {
            memory_model: 1,
            profile_format: 2,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 5,
            button_count: 11,
            sector_count: 16,
            sector_size: 256,
            has_gshift: false,
            has_dpi_shift: false,
        };
        let mut sector = vec![0u8; 256];
        profile::write_crc(&mut sector);
        let sent_calls = |sent: &Arc<Mutex<Vec<(u8, u8)>>>| sent.lock().unwrap().iter().filter(|c| c.0 != 0).count();

        // Read-only: neither a RAM change nor a memory write leaves PeriHub.
        assert!(dev.set_active_profile(false, 2).is_err());
        assert!(dev.write_sector(1, &sector, &info).is_err());
        assert_eq!(sent_calls(&sent), 0);

        // RAM: the profile is switched, the memory is not written.
        dev.grant(Access::Ram);
        dev.set_active_profile(false, 2).unwrap();
        assert!(dev.write_sector(1, &sector, &info).is_err());
        assert_eq!(sent_calls(&sent), 1);

        dev.grant(Access::Full);
        dev.write_sector(1, &sector, &info).unwrap();
        assert_eq!(sent_calls(&sent), 1 + 1 + 16 + 1);
    }

    #[test]
    fn host_mode_sends_only_what_drives_the_lights_in_ram() {
        use feature::{BRIGHTNESS_CONTROL, PER_KEY_LIGHTING_V2, PROFILE_MANAGEMENT, RGB_EFFECTS};
        let mut effect = vec![0u8, 1, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        effect.push(1);
        let sent = [
            (BRIGHTNESS_CONTROL, 2, vec![0, 50]),
            (PROFILE_MANAGEMENT, 6, vec![5]),
            (PROFILE_MANAGEMENT, 6, vec![3]),
            (RGB_EFFECTS, 5, vec![1, 3, 6]),
            (RGB_EFFECTS, 5, vec![1, 0, 0]),
            (RGB_EFFECTS, 1, effect.clone()),
            (RGB_EFFECTS, 8, vec![1, 3, 0]),
            (RGB_EFFECTS, 8, vec![1, 1, 0]),
            (PER_KEY_LIGHTING_V2, 1, vec![4, 0xFF, 0, 0]),
            (PER_KEY_LIGHTING_V2, 5, vec![4, 10, 0xFF, 0, 0]),
            (PER_KEY_LIGHTING_V2, 6, vec![0xFF, 0, 0, 4, 5]),
            (PER_KEY_LIGHTING_V2, 7, vec![0]),
        ];
        for (feature, function, params) in &sent {
            assert!(is_host_call(*feature, *function, params), "{feature:#06x}/{function} {params:02x?}");
        }
        let mut kept = effect.clone();
        *kept.last_mut().unwrap() = 2;
        let refused = [
            // Kept in the keyboard's memory: the effect with another persistence, the startup and
            // shutdown effects, the timers, the end of a frame that is saved.
            (RGB_EFFECTS, 1, kept),
            (RGB_EFFECTS, 1, effect[..12].to_vec()),
            (RGB_EFFECTS, 3, vec![1, 0, 1, 1]),
            (RGB_EFFECTS, 7, vec![1, 0, 0, 0, 60, 1, 0x0E]),
            (PER_KEY_LIGHTING_V2, 7, vec![1]),
            // Other modes, and the power-off that takes the keyboard offline.
            (RGB_EFFECTS, 5, vec![1, 1, 0]),
            (RGB_EFFECTS, 8, vec![1, 2, 0]),
            (RGB_EFFECTS, 9, vec![1]),
            (PROFILE_MANAGEMENT, 6, vec![1]),
            (PROFILE_MANAGEMENT, 6, vec![2]),
            (PROFILE_MANAGEMENT, 6, vec![4]),
            // Profile files, key tables, game mode: never.
            (PROFILE_MANAGEMENT, 1, vec![0]),
            (PROFILE_MANAGEMENT, 4, vec![0]),
            (PROFILE_MANAGEMENT, 0x0D, vec![0]),
            (feature::FULL_KEY_CUSTOMIZATION, 1, vec![1]),
            (feature::DISABLE_CONTROLS, 1, vec![1]),
            (feature::LOGI_MODIFIERS, 1, vec![1]),
            (feature::CONTROL_LIST, 2, vec![1]),
            (feature::ONBOARD_PROFILES, 6, vec![0]),
            (feature::ADJUSTABLE_DPI, 3, vec![0, 3, 0x20]),
        ];
        for (feature, function, params) in &refused {
            assert!(
                !is_host_call(*feature, *function, params) && !is_read_call(*feature, *function, params),
                "{feature:#06x}/{function} {params:02x?}"
            );
        }
        // And none of the new keyboard features changes anything with RAM access.
        for (feature, function, params) in &refused[11..18] {
            assert!(is_memory_write(*feature, *function, params), "{feature:#06x}/{function}");
        }
    }

    /// What a simulated device got: feature index, function and the first parameters.
    type Sent = Arc<Mutex<Vec<(u8, u8, Vec<u8>)>>>;

    /// A keyboard in slot 1 with 0x8071 at index 1 and 0x8101 at index 2; it answers every call.
    fn host_keyboard(events_with_reply: Vec<Vec<u8>>) -> (FakeDevice, Sent) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let mut extra = Some(events_with_reply);
        let fake = FakeDevice::new(move |request| {
            match (request[2], request[3] >> 4) {
                (0, 1) => return vec![reply(request, &[4, 2, request[6]])],
                (0, 0) => {
                    let id = u16::from_be_bytes([request[4], request[5]]);
                    let index = match id {
                        feature::RGB_EFFECTS => 1,
                        feature::PROFILE_MANAGEMENT => 2,
                        _ => 0,
                    };
                    return vec![reply(request, &[index])];
                }
                _ => {}
            }
            log.lock().unwrap().push((request[2], request[3] >> 4, request[4..8].to_vec()));
            // The first call gets the events before its reply, as a keyboard sends them.
            let mut out = extra.take().unwrap_or_default();
            out.push(reply(request, &[request[4]]));
            out
        });
        (fake, sent)
    }

    fn event_report(index: u8, event: u8, data: &[u8]) -> Vec<u8> {
        let mut report = vec![0u8; LONG_LEN];
        report[0] = REPORT_LONG;
        report[1] = 1;
        report[2] = index;
        report[3] = event << 4;
        report[4..4 + data.len()].copy_from_slice(data);
        report
    }

    #[test]
    fn host_access_refuses_the_rest_before_anything_is_sent() {
        let (fake, sent) = host_keyboard(Vec::new());
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        assert!(dev.call(feature::RGB_EFFECTS, 5, &[1, 3, 6]).is_err(), "read-only first");
        dev.grant(Access::Host);
        dev.call(feature::RGB_EFFECTS, 5, &[1, 3, 6]).unwrap();
        dev.call(feature::PROFILE_MANAGEMENT, 6, &[5]).unwrap();
        assert!(dev.call(feature::RGB_EFFECTS, 7, &[1, 0, 0, 0, 60]).is_err());
        assert!(dev.call(feature::RGB_EFFECTS, 9, &[1]).is_err());
        assert!(dev.call(feature::PROFILE_MANAGEMENT, 6, &[1]).is_err());
        dev.call(feature::RGB_EFFECTS, 5, &[0]).unwrap();
        let calls: Vec<(u8, u8, u8)> = sent.lock().unwrap().iter().map(|(i, f, p)| (*i, *f, p[0])).collect();
        assert_eq!(calls, [(1, 5, 1), (2, 6, 5), (1, 5, 0)]);
    }

    #[test]
    fn events_that_come_during_a_call_are_kept_for_later() {
        // Idle (0x8071 event 1, byte 0 = 0) before the reply, a receiver notification (0x41) and an
        // event of a feature PeriHub never asked for (index 5): only the first is given back.
        let (fake, _) =
            host_keyboard(vec![event_report(1, 1, &[0]), event_report(0x41, 0, &[0x10]), event_report(5, 0, &[7])]);
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        let reply = dev.call(feature::RGB_EFFECTS, 5, &[0]).unwrap();
        assert_eq!(reply[0], 0, "the reply, not the event");
        let events = dev.events(0).unwrap();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!((events[0].feature, events[0].event, events[0].data[0]), (feature::RGB_EFFECTS, 1, 0));
        assert!(dev.events(0).unwrap().is_empty(), "given once");
    }

    #[test]
    fn events_waiting_on_the_connection_are_read() {
        let (fake, _) = host_keyboard(Vec::new());
        let pending = fake.pending.clone();
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        dev.call(feature::RGB_EFFECTS, 5, &[0]).unwrap();
        assert!(dev.events(0).unwrap().is_empty());
        // In use again (byte 0 = 1), sent on its own after the call; then a short report.
        pending.lock().unwrap().push_back(event_report(1, 1, &[1]));
        pending.lock().unwrap().push_back(vec![REPORT_SHORT, 1, 1, 0x10, 0, 0, 0]);
        let events = dev.events(0).unwrap();
        let read: Vec<(u16, u8, u8)> = events.iter().map(|e| (e.feature, e.event, e.data[0])).collect();
        assert_eq!(read, [(feature::RGB_EFFECTS, 1, 1), (feature::RGB_EFFECTS, 1, 0)]);
    }
}

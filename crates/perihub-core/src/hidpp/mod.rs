//! Logitech HID++ 2.0 protocol (mice such as the G502 HERO, with a cable or through a receiver).
//!
//! References: libratbag (MIT) and data read from the real device.

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
    /// Keyboard international layouts: a keyboard's layout code.
    pub const KEYBOARD_LAYOUT: u16 = 0x4540;
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
    pub const EXTENDED_DPI: u16 = 0x2202;
    pub const REPORT_RATE: u16 = 0x8060;
    pub const EXTENDED_REPORT_RATE: u16 = 0x8061;
    pub const COLOR_LED_EFFECTS: u16 = 0x8070;
    pub const ONBOARD_PROFILES: u16 = 0x8100;
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
    feature::KEYBOARD_LAYOUT,
    feature::ADJUSTABLE_DPI,
    feature::EXTENDED_DPI,
    feature::REPORT_RATE,
    feature::EXTENDED_REPORT_RATE,
    feature::COLOR_LED_EFFECTS,
    feature::ONBOARD_PROFILES,
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
    (feature::KEYBOARD_LAYOUT, 0),
    (feature::ADJUSTABLE_DPI, 1),
    (feature::ADJUSTABLE_DPI, 2),
    (feature::EXTENDED_DPI, 0),
    (feature::EXTENDED_DPI, 1),
    (feature::EXTENDED_DPI, 2),
    (feature::EXTENDED_DPI, 5),
    (feature::REPORT_RATE, 0),
    (feature::REPORT_RATE, 1),
    (feature::EXTENDED_REPORT_RATE, 0),
    (feature::EXTENDED_REPORT_RATE, 2),
    (feature::COLOR_LED_EFFECTS, 0),
    (feature::COLOR_LED_EFFECTS, 1),
    (feature::COLOR_LED_EFFECTS, 2),
    (feature::ONBOARD_PROFILES, 0),
    (feature::ONBOARD_PROFILES, 2),
    (feature::ONBOARD_PROFILES, 4),
    (feature::ONBOARD_PROFILES, 5),
    (feature::ONBOARD_PROFILES, 0x0B),
];

fn is_read_call(feature: u16, function: u8) -> bool {
    READ_ONLY_CALLS.contains(&(feature, function))
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

/// What the extended DPI feature (0x2202) reports about a sensor: a DPI range for each axis, and
/// whether the lift-off distance is adjustable too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtendedDpi {
    pub x: DpiCapabilities,
    /// Only when the sensor has a separate Y axis.
    pub y: Option<DpiCapabilities>,
    pub has_lod: bool,
}

/// The DPI a 0x2202 sensor uses now, and its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExtendedDpiNow {
    pub x: u16,
    pub default_x: u16,
    pub y: Option<u16>,
    pub default_y: Option<u16>,
    /// Lift-off distance: 1 low, 2 medium, 3 high (as the sensors of libratbag's tested mouse answer).
    pub lod: Option<u8>,
}

/// Polling rates of the extended report rate feature (0x8061), by bit or index: 125 Hz up to 8000 Hz.
pub const EXTENDED_RATES_HZ: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// The polling rate of a 0x8061 index, as it is also stored in format 7 profiles.
pub fn extended_rate_hz(index: u8) -> Option<u16> {
    EXTENDED_RATES_HZ.get(index as usize).copied()
}

/// How many DPI list pages are read before giving up on a list that never ends.
const MAX_DPI_PAGES: u8 = 16;
/// Longest DPI list kept as a list.
const MAX_DPI_VALUES: usize = 1024;

/// Reads a 0x2202 DPI list: 16-bit values, a value of 0 ends it, and a value whose top three bits
/// are set (`0xE000 | step`) is followed by an end value and fills in the DPI values after the
/// previous one, `step` apart, up to that end. `None` while the list has not ended in `data`.
fn parse_extended_dpi_list(data: &[u8]) -> Option<Result<DpiCapabilities>> {
    let mut values: Vec<u16> = Vec::new();
    let mut words = data.as_chunks::<2>().0.iter().map(|w| u16::from_be_bytes(*w));
    while let Some(word) = words.next() {
        if word == 0 {
            return Some(capabilities_from_values(values));
        }
        if word >> 13 == 0b111 {
            let step = word & 0x1FFF;
            let end = words.next()?;
            let Some(&last) = values.last() else {
                return Some(Err(Error::Protocol("DPI list starts with a step".into())));
            };
            if step == 0 || end < last {
                return Some(Err(Error::Protocol("DPI list with a wrong step".into())));
            }
            let count = (end - last) / step;
            if values.len() + count as usize > MAX_DPI_VALUES {
                return Some(Err(Error::Protocol("DPI list is too long".into())));
            }
            values.extend((1..=count).map(|i| last + i * step));
        } else {
            values.push(word);
        }
        if values.len() > MAX_DPI_VALUES {
            return Some(Err(Error::Protocol("DPI list is too long".into())));
        }
    }
    None
}

/// A range (smallest, largest, step) when the values are evenly spaced, else the list itself.
fn capabilities_from_values(values: Vec<u16>) -> Result<DpiCapabilities> {
    let (Some(&min), Some(&max)) = (values.iter().min(), values.iter().max()) else {
        return Err(Error::Protocol("empty DPI list".into()));
    };
    let step = if values.len() > 2 { values[1].wrapping_sub(values[0]) } else { 0 };
    let even = step > 0 && values.windows(2).all(|w| w[1].wrapping_sub(w[0]) == step);
    if even {
        Ok(DpiCapabilities { min, max, step, list: Vec::new() })
    } else {
        Ok(DpiCapabilities { min, max, step: 0, list: values })
    }
}

#[derive(Debug, Clone, Serialize)]
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

impl Battery {
    /// For [`crate::battery::report`]: percent, charging, full.
    pub fn reading(&self) -> (u8, bool, bool) {
        (self.percent, self.state == BatteryState::Charging, self.state == BatteryState::Full)
    }
}

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
}

/// Onboard memory writes (0x8100 functions 6, 7 and 8): the calls that outlast a power cycle.
fn is_memory_write(feature: u16, function: u8) -> bool {
    feature == feature::ONBOARD_PROFILES && matches!(function, 6..=8)
}

pub struct HidppDevice {
    dev: Box<dyn Transport>,
    device_index: u8,
    access: Access,
    /// Feature indexes, 0 for a feature the device does not have.
    features: HashMap<u16, u8>,
    /// Reached through a LIGHTSPEED receiver, in the slot `device_index`.
    behind_receiver: bool,
    /// The receiver's short HID++ collection, when it has its own path (Windows).
    short: Option<Box<dyn Transport>>,
    /// The device's name in the write log.
    label: String,
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
        let label = if behind_receiver {
            format!("Logitech device in receiver slot {device_index}")
        } else {
            "Logitech device".to_string()
        };
        let mut device = Self { dev, device_index, access, features: HashMap::new(), behind_receiver, short, label };
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

    /// Names the device in the write log.
    pub fn set_label(&mut self, label: String) {
        self.label = label;
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
        // Every call that is not a read changes something, if only in RAM: none in read-only mode.
        let change = (!is_read_call(feature, function)).then(|| format!("HID++ {feature:#06x} function {function}"));
        if let Some(kind) = &change {
            crate::writes::allow(&self.label, kind, params)?;
        }
        match self.access {
            Access::ReadOnly if !is_read_call(feature, function) => {
                return Err(Error::Invalid("PeriHub only reads this mouse".into()));
            }
            Access::Ram if is_memory_write(feature, function) => {
                return Err(Error::Invalid("PeriHub does not save to this mouse's memory".into()));
            }
            _ => {}
        }
        let index = self.feature_index(feature)?;
        let result = self.transact(index, function, params);
        if let Some(kind) = &change {
            crate::writes::sent(&self.label, kind, params, &result);
        }
        result
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
        type Parse = fn(&[u8; 16]) -> Option<Battery>;
        let features: [(u16, u8, Parse); 3] = [
            (feature::UNIFIED_BATTERY, 1, parse_unified_battery),
            (feature::BATTERY_STATUS, 0, parse_battery_status),
            (feature::BATTERY_VOLTAGE, 0, parse_battery_voltage),
        ];
        for (feature, function, parse) in features {
            match self.call(feature, function, &[]) {
                Ok(reply) => return Ok(parse(&reply)),
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

    // ---- 0x4540 keyboard international layouts ----

    /// The keyboard's layout code: function 0, first byte, as Solaar and OpenRGB read it. What the
    /// codes mean is not documented (see [`crate::kblayout::logitech`]).
    pub fn keyboard_layout_code(&mut self) -> Result<u8> {
        Ok(self.call(feature::KEYBOARD_LAYOUT, 0, &[])?[0])
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

    // ---- 0x2202 extended adjustable DPI ----

    /// The DPI ranges of sensor `sensor`, with an X axis, maybe a Y axis, and maybe a lift-off
    /// distance. Reads only.
    pub fn extended_dpi(&mut self, sensor: u8) -> Result<ExtendedDpi> {
        let axes = self.call(feature::EXTENDED_DPI, 1, &[sensor])?;
        if axes[0] != sensor {
            return Err(Error::Protocol("DPI axes answered for another sensor".into()));
        }
        let (has_y, has_lod) = (axes[2] & 1 != 0, axes[2] & 2 != 0);
        let x = self.extended_dpi_axis(sensor, 0)?;
        let y = if has_y { Some(self.extended_dpi_axis(sensor, 1)?) } else { None };
        Ok(ExtendedDpi { x, y, has_lod })
    }

    /// One axis list, which comes in pages of 13 bytes.
    fn extended_dpi_axis(&mut self, sensor: u8, axis: u8) -> Result<DpiCapabilities> {
        let mut data = Vec::new();
        for page in 0..MAX_DPI_PAGES {
            let r = self.call(feature::EXTENDED_DPI, 2, &[sensor, axis, page])?;
            if r[0] != sensor || r[1] != axis || r[2] != page {
                return Err(Error::Protocol("DPI list page out of order".into()));
            }
            data.extend_from_slice(&r[3..]);
            if let Some(caps) = parse_extended_dpi_list(&data) {
                return caps;
            }
        }
        Err(Error::Protocol("DPI list does not end".into()))
    }

    /// The DPI sensor `sensor` uses now, as the mouse sends it, with the Y axis and lift-off
    /// distance only if `dpi` says the sensor has them.
    pub fn extended_sensor_dpi(&mut self, sensor: u8, dpi: &ExtendedDpi) -> Result<ExtendedDpiNow> {
        let r = self.call(feature::EXTENDED_DPI, 5, &[sensor])?;
        let word = |at: usize| u16::from_be_bytes([r[at], r[at + 1]]);
        Ok(ExtendedDpiNow {
            x: word(1),
            default_x: word(3),
            y: dpi.y.is_some().then(|| word(5)),
            default_y: dpi.y.is_some().then(|| word(7)),
            lod: dpi.has_lod.then_some(r[9]),
        })
    }

    /// The DPI the mouse uses now, from whichever DPI feature it has.
    pub fn current_dpi(&mut self) -> Result<u16> {
        match self.sensor_dpi(0) {
            Err(Error::FeatureMissing(_)) => {
                let dpi = self.extended_dpi(0)?;
                Ok(self.extended_sensor_dpi(0, &dpi)?.x)
            }
            other => Ok(other?.0),
        }
    }

    /// The DPI ranges, from whichever DPI feature the mouse has (the X axis of 0x2202).
    pub fn any_dpi_capabilities(&mut self) -> Result<DpiCapabilities> {
        match self.dpi_capabilities(0) {
            Err(Error::FeatureMissing(_)) => Ok(self.extended_dpi(0)?.x),
            other => other,
        }
    }

    // ---- 0x8061 extended polling rate ----

    /// The polling rates in Hz that the mouse offers on a connection (`wireless`: through a
    /// LIGHTSPEED receiver, else with the cable), and the bytes it sent, for test reports.
    pub fn extended_report_rates(&mut self, wireless: bool) -> Result<(Vec<u16>, [u8; 2])> {
        let r = self.call(feature::EXTENDED_REPORT_RATE, 0, &[wireless as u8])?;
        let mask = u16::from_be_bytes([r[0], r[1]]);
        let rates = (0..EXTENDED_RATES_HZ.len()).filter(|bit| mask & (1 << bit) != 0).map(|i| EXTENDED_RATES_HZ[i]);
        Ok((rates.collect(), [r[0], r[1]]))
    }

    /// The polling rate in use on a connection: its Hz and the index the mouse sent.
    pub fn extended_report_rate(&mut self, wireless: bool) -> Result<(Option<u16>, u8)> {
        let index = self.call(feature::EXTENDED_REPORT_RATE, 2, &[wireless as u8])?[0];
        Ok((extended_rate_hz(index), index))
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
        pending: Mutex<VecDeque<Vec<u8>>>,
        pub(crate) written: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl FakeDevice {
        pub(crate) fn new(respond: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) -> Self {
            Self {
                respond: Mutex::new(Box::new(respond)),
                pending: Mutex::new(VecDeque::new()),
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
    fn an_extended_dpi_list_is_read_across_pages() {
        // 100 to 25600 DPI in steps of 50, on the PRO X 2: one value, a step, its end, the terminator.
        let range = [0x00, 0x64, 0xE0, 0x32, 0x64, 0x00, 0x00, 0x00];
        let caps = parse_extended_dpi_list(&range).unwrap().unwrap();
        assert_eq!(caps, DpiCapabilities { min: 100, max: 25600, step: 50, list: Vec::new() });

        // A step cut between two pages is not a list yet.
        assert!(parse_extended_dpi_list(&range[..3]).is_none());
        assert!(parse_extended_dpi_list(&range[..4]).is_none(), "its end value is on the next page");
        assert!(parse_extended_dpi_list(&range[..6]).is_none(), "no terminator yet");

        // Values that are not evenly spaced stay a list; the step fills in the ones between.
        let uneven = [0x01, 0x90, 0x03, 0x20, 0x06, 0x40, 0xE1, 0x90, 0x0C, 0x80, 0x00, 0x00];
        let caps = parse_extended_dpi_list(&uneven).unwrap().unwrap();
        assert_eq!((caps.min, caps.max, caps.step), (400, 3200, 0));
        assert_eq!(caps.list, [400, 800, 1600, 2000, 2400, 2800, 3200]);

        assert!(parse_extended_dpi_list(&[0xE0, 0x32, 0x64, 0x00]).unwrap().is_err(), "a step needs a first value");
        assert!(parse_extended_dpi_list(&[0x00, 0x64, 0xE0, 0x00, 0x64, 0x00]).unwrap().is_err(), "a step of 0");
        assert!(parse_extended_dpi_list(&[0x00, 0x00]).unwrap().is_err(), "an empty list");
    }

    /// A PRO X 2 style mouse: 0x2202 at index 1 and 0x8061 at index 2.
    fn extended_mouse() -> FakeDevice {
        FakeDevice::new(|request| {
            let (index, function) = (request[2], request[3] >> 4);
            let p = &request[4..];
            let payload: Vec<u8> = match (index, function) {
                (0, 1) => vec![4, 2, request[6]],
                (0, 0) => {
                    let id = u16::from_be_bytes([p[0], p[1]]);
                    vec![match id {
                        feature::EXTENDED_DPI => 1,
                        feature::EXTENDED_REPORT_RATE => 2,
                        _ => 0,
                    }]
                }
                (1, 1) => vec![p[0], 0, 3],
                // Pages of 13 bytes: the X list is 100, 200 … 600 (12 bytes) and the first byte of a step, which
                // goes on in the next page; the Y list fits in one.
                (1, 2) => {
                    let (axis, page) = (p[1], p[2]);
                    let x: [&[u8]; 2] = [
                        &[0, 100, 0, 200, 1, 44, 1, 144, 1, 244, 2, 88, 0xE0],
                        &[0x32, 0x13, 0x88, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                    ];
                    let y: &[u8] = &[0, 100, 0xE0, 0x32, 0x64, 0, 0, 0, 0, 0, 0, 0, 0];
                    let data = if axis == 0 {
                        x[page as usize]
                    } else if page == 0 {
                        y
                    } else {
                        &[0; 13]
                    };
                    [&[p[0], axis, page], data].concat()
                }
                (1, 5) => vec![0, 0x04, 0xB0, 0x03, 0x20, 0x04, 0xB0, 0x03, 0x20, 2],
                (2, 0) => vec![0, if p[0] == 0 { 0x0F } else { 0x7F }],
                (2, 2) => vec![if p[0] == 0 { 3 } else { 6 }],
                _ => return vec![],
            };
            vec![reply(request, &payload)]
        })
    }

    #[test]
    fn the_extended_dpi_and_polling_rate_of_a_pro_x_2_are_read() {
        let mut dev = HidppDevice::new(extended_mouse(), 0xFF, true).unwrap();
        let dpi = dev.extended_dpi(0).unwrap();
        assert!(dpi.has_lod && dpi.y.is_some());
        assert_eq!((dpi.x.min, dpi.x.max, dpi.x.step), (100, 5000, 0));
        assert_eq!(dpi.x.list.len(), 6 + 88, "six values, then every 50 from 650 to 5000");
        assert_eq!(dpi.y.as_ref().unwrap().max, 25600);
        let now = dev.extended_sensor_dpi(0, &dpi).unwrap();
        assert_eq!((now.x, now.default_x, now.y, now.default_y, now.lod), (1200, 800, Some(1200), Some(800), Some(2)));
        assert_eq!(dev.current_dpi().unwrap(), 1200, "through whichever DPI feature the mouse has");
        assert_eq!(dev.any_dpi_capabilities().unwrap().max, 5000);

        assert_eq!(dev.extended_report_rates(false).unwrap(), (vec![125, 250, 500, 1000], [0, 0x0F]));
        assert_eq!(dev.extended_report_rates(true).unwrap().0, [125, 250, 500, 1000, 2000, 4000, 8000]);
        assert_eq!(dev.extended_report_rate(true).unwrap(), (Some(8000), 6));
        assert_eq!(dev.extended_report_rate(false).unwrap(), (Some(1000), 3));
    }

    #[test]
    fn the_extended_features_are_read_only_calls_and_nothing_else_gets_through() {
        let fake = extended_mouse();
        let written = fake.written.clone();
        let mut dev = HidppDevice::new(fake, 0xFF, true).unwrap();
        let before = written.lock().unwrap().len();
        // Setting the DPI (function 6 of 0x2202) or the rate (function 3 of 0x8061), or anything
        // else of those features, is refused on a read-only mouse.
        for (feature, function) in [
            (feature::EXTENDED_DPI, 6),
            (feature::EXTENDED_DPI, 3),
            (feature::EXTENDED_REPORT_RATE, 3),
            (feature::EXTENDED_REPORT_RATE, 1),
        ] {
            assert!(dev.call(feature, function, &[0, 0, 0, 0, 0, 0]).is_err(), "{feature:04x} {function}");
        }
        assert_eq!(written.lock().unwrap().len(), before, "nothing was even sent");
        // The reads are sent.
        dev.extended_dpi(0).unwrap();
        assert!(written.lock().unwrap().len() > before);
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

    #[test]
    fn read_only_mode_refuses_every_call_that_is_not_a_read() {
        const FEATURES: &[u16] = &[feature::ROOT, feature::ADJUSTABLE_DPI, feature::ONBOARD_PROFILES];
        let fake = fake_mouse(FEATURES, |_, _| vec![0, 0x06, 0x40, 0x06, 0x40]);
        let written = fake.written.clone();
        let mut dev = HidppDevice::new(fake, models::WIRED_INDEX, false).unwrap();
        dev.set_label("hidpp read-only test".into());
        assert_eq!(dev.access, Access::Full);
        let info = OnboardInfo {
            memory_model: 1,
            profile_format: 2,
            macro_format: 1,
            profile_count: 1,
            rom_profile_count: 1,
            button_count: 11,
            sector_count: 4,
            sector_size: 16,
            has_gshift: false,
            has_dpi_shift: false,
        };
        let _read_only = crate::writes::test::read_only_mode();
        let before = written.lock().unwrap().len();
        assert!(matches!(dev.set_sensor_dpi(0, 800), Err(Error::ReadOnly)));
        let mut sector = vec![0u8; 16];
        profile::write_crc(&mut sector);
        assert!(matches!(dev.write_sector(1, &sector, &info), Err(Error::ReadOnly)));
        assert_eq!(written.lock().unwrap().len(), before, "nothing was sent");
        // Reads still go out, the ping of the device included.
        dev.sensor_dpi(0).unwrap();
        dev.protocol_version().unwrap();
        let reports = written.lock().unwrap()[before..].to_vec();
        assert!(reports.iter().filter(|r| r[2] != 0).all(|r| is_read_call(FEATURES[r[2] as usize], r[3] >> 4)));
        let log = crate::writes::test::entries_of("hidpp read-only test");
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].kind, "HID++ 0x2201 function 3");
        assert!(log.iter().all(|e| e.result == crate::writes::REFUSED));
    }

    #[test]
    fn the_battery_is_read_in_read_only_mode() {
        // The slow battery read of the warnings (logitech.rs, mouse.rs) only reads: it goes on.
        const FEATURES: &[u16] = &[feature::ROOT, feature::UNIFIED_BATTERY];
        let fake = fake_mouse(FEATURES, |_, _| vec![55, 0x04, 0x00, 0x00]);
        let mut dev = HidppDevice::new(fake, models::WIRED_INDEX, false).unwrap();
        dev.set_label("hidpp battery read-only test".into());
        let _read_only = crate::writes::test::read_only_mode();
        let battery = dev.battery().unwrap().expect("a battery");
        assert_eq!(battery.percent, 55);
        assert!(crate::writes::test::entries_of("hidpp battery read-only test").is_empty(), "no write, none refused");
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
            assert!(!is_read_call(feature, function), "{feature:#06x}/{function}");
        }
        assert!(READ_ONLY_CALLS.iter().all(|(feature, _)| ALLOWED_FEATURES.contains(feature)));
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
}

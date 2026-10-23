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
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
    pub const REPORT_RATE: u16 = 0x8060;
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
    feature::ADJUSTABLE_DPI,
    feature::REPORT_RATE,
    feature::COLOR_LED_EFFECTS,
    feature::ONBOARD_PROFILES,
];

/// The only (feature, function) pairs allowed on a read-only device: they read state and
/// change nothing, not even in RAM.
const READ_ONLY_CALLS: &[(u16, u8)] = &[
    (feature::DEVICE_INFO, 0),
    (feature::DEVICE_INFO, 1),
    (feature::DEVICE_NAME, 0),
    (feature::DEVICE_NAME, 1),
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
];

fn is_read_call(feature: u16, function: u8) -> bool {
    READ_ONLY_CALLS.contains(&(feature, function))
}

/// Mouse memory sectors reserved for the factory profiles (read-only).
pub const ROM_SECTOR_BASE: u16 = 0x0100;

#[derive(Debug, Clone, Serialize)]
pub struct DpiCapabilities {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    /// Allowed values, if the sensor exposes a list instead of a range.
    pub list: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedZone {
    pub index: u8,
    /// 1 = primary, 2 = logo (per the HID++ 0x8070 specification).
    pub location: u16,
    pub effects: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
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

pub struct HidppDevice {
    dev: Box<dyn Transport>,
    device_index: u8,
    read_only: bool,
    /// Feature indexes, 0 for a feature the device does not have.
    features: HashMap<u16, u8>,
    /// Reached through a LIGHTSPEED receiver, in the slot `device_index`.
    behind_receiver: bool,
    /// The receiver's short HID++ collection, when it has its own path (Windows).
    short: Option<Box<dyn Transport>>,
}

impl HidppDevice {
    /// `read_only` limits the device to [`READ_ONLY_CALLS`] for the whole session.
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
        let mut device = Self { dev, device_index, read_only, features: HashMap::new(), behind_receiver, short };
        let (major, _) = device.protocol_version()?;
        if major < 2 {
            return Err(Error::Protocol(format!("HID++ {major}.x is not supported")));
        }
        Ok(device)
    }

    pub fn is_behind_receiver(&self) -> bool {
        self.behind_receiver
    }

    fn transact(&mut self, index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        match self.exchange(index, function, params) {
            // On Windows the receiver says that the mouse is asleep on its other collection: ask there.
            Err(Error::Timeout) if self.behind_receiver => {
                let channel = self.short.as_deref().unwrap_or(&*self.dev);
                match receiver::ping(channel, self.device_index)? {
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
        if self.read_only && !is_read_call(feature, function) {
            return Err(Error::Invalid("this mouse model is read-only until it has been tested with PeriHub".into()));
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

    /// The id the mouse has behind a LIGHTSPEED receiver. 0x0003 function 0 lists one product id
    /// for each way the mouse connects, in the order of the transport bits: Bluetooth (bit 0),
    /// Bluetooth Low Energy (1), receiver (2), USB (3).
    pub fn wireless_id(&mut self) -> Result<Option<u16>> {
        let r = self.call(feature::DEVICE_INFO, 0, &[])?;
        Ok(wireless_id_from_device_info(&r))
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
            out.push(FirmwareEntity {
                kind: r[0],
                name: String::from_utf8_lossy(&r[1..4]).trim().to_string(),
                version: format!("{:02x}.{:02x}.{:02x}{:02x}", r[4], r[5], r[6], r[7]),
            });
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

    pub fn set_active_profile(&mut self, number: u8) -> Result<()> {
        self.call(feature::ONBOARD_PROFILES, 3, &[0, number])?;
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
        if data.len() != info.sector_size as usize || !data.len().is_multiple_of(16) {
            return Err(Error::Invalid("wrong sector size".into()));
        }
        if !profile::crc_is_valid(data) {
            return Err(Error::Invalid("invalid sector CRC".into()));
        }
        let [s_hi, s_lo] = sector.to_be_bytes();
        let [l_hi, l_lo] = (data.len() as u16).to_be_bytes();
        self.call(feature::ONBOARD_PROFILES, 6, &[s_hi, s_lo, 0, 0, l_hi, l_lo])?;
        for chunk in data.as_chunks::<16>().0 {
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
}

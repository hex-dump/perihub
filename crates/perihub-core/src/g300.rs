//! Logitech G300 (`046d:c246`): read only. The G300 does not speak HID++: its three profiles are
//! feature reports of 35 bytes (0xF3, 0xF4, 0xF5) and the active profile is report 0xF0. Layout from
//! libratbag's G300 driver (MIT), checked byte by byte against a real G300 and Logitech Gaming
//! Software in issue #16.
//!
//! PeriHub only ever asks the mouse for these reports: [`FeatureSource`] has no way to send one.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::color::Rgb;
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
/// DPI values are stored in steps of 250, from 1 (250 DPI) to 10 (2500 DPI).
const DPI_STEP: u16 = 250;
/// The mouse's buttons change the active profile and DPI level: read again this often.
const REFRESH: Duration = Duration::from_secs(2);

/// Reads feature reports, nothing else: the G300 is read only in PeriHub.
pub trait FeatureSource: Send {
    /// Fills `buf` (report id in byte 0) and returns how many bytes came back.
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize>;
}

impl FeatureSource for hidapi::HidDevice {
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(self.get_feature_report(buf)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// Byte 7, whose meaning is not fully known (libratbag: "dpi index for shift, but something
    /// else too"). On the G300 of issue #16, 0x04 went with a DPI shift of 1000.
    pub shift_byte: u8,
    pub buttons: Vec<G300Button>,
    /// The bytes as read, for test reports.
    pub raw: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

pub fn parse_profile(number: u8, r: &[u8]) -> Result<G300Profile> {
    if r.len() < PROFILE_LEN || r[0] != REPORT_PROFILES[number as usize - 1] {
        return Err(Error::Protocol(format!("unexpected profile report: {}", hex(r))));
    }
    let on = |bit: u8| if r[1] & bit != 0 { 255 } else { 0 };
    let levels = &r[3..7];
    Ok(G300Profile {
        number,
        color: Rgb { r: on(0x01), g: on(0x02), b: on(0x04) },
        report_rate_hz: match r[2] {
            0x00 => Some(1000),
            0x01 => Some(125),
            0x02 => Some(250),
            0x03 => Some(500),
            _ => None,
        },
        dpi_levels: levels.iter().map(|l| (l & 0x7F) as u16 * DPI_STEP).collect(),
        default_level: levels.iter().position(|l| l & 0x80 != 0).map(|i| i as u8),
        shift_byte: r[7],
        buttons: (0..BUTTONS).map(|i| button(&r[BUTTONS_AT + 3 * i..BUTTONS_AT + 3 * i + 3])).collect(),
        raw: hex(&r[..PROFILE_LEN]),
    })
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
}

pub struct G300Task {
    dev: Box<dyn FeatureSource>,
    pub info: G300Info,
    last_refresh: Instant,
}

impl G300Task {
    pub fn open(dev: impl FeatureSource + 'static) -> Result<Self> {
        let mut task = Self {
            dev: Box::new(dev),
            info: G300Info {
                product_id: PRODUCT_ID,
                model: "G300".into(),
                active_profile: 1,
                active_level: 0,
                profiles: Vec::new(),
                active_raw: String::new(),
            },
            last_refresh: Instant::now(),
        };
        for (i, id) in REPORT_PROFILES.into_iter().enumerate() {
            let mut buf = [0u8; PROFILE_LEN];
            buf[0] = id;
            let n = task.dev.get_feature(&mut buf)?;
            task.info.profiles.push(parse_profile(i as u8 + 1, &buf[..n])?);
        }
        task.read_active()?;
        log::info!(
            "Logitech G300: profile {} active, profiles {}",
            task.info.active_profile,
            task.info.profiles.iter().map(|p| p.raw.clone()).collect::<Vec<_>>().join(" | ")
        );
        Ok(task)
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
}

impl DeviceTask for G300Task {
    fn tick(&mut self) -> Result<Duration> {
        if self.last_refresh.elapsed() >= REFRESH {
            self.last_refresh = Instant::now();
            // A mouse that stops answering is gone: the manager opens it again when it is back.
            self.read_active()?;
        }
        Ok(REFRESH)
    }
}

#[cfg(test)]
mod tests {
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

        // Profile 3: green, 500 Hz, G4 back, G5 forward, G6 space, G7 Ctrl alone, G9 DPI cycle.
        let p3 = parse_profile(3, &bytes(P3)).unwrap();
        assert_eq!(p3.color, Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(p3.report_rate_hz, Some(500));
        assert_eq!(p3.buttons[3..5], [G300Button::Mouse { button: 4 }, G300Button::Mouse { button: 5 }]);
        assert_eq!(p3.buttons[5], key(0, 0x2C));
        assert_eq!(p3.buttons[6], key(0x01, 0));
        assert_eq!(p3.buttons[8], G300Button::DpiCycle);

        assert_eq!(parse_active(&ACTIVE).unwrap(), (1, 1));
    }

    #[test]
    fn reports_of_another_kind_are_refused() {
        assert!(parse_profile(1, &bytes(P2)).is_err(), "report F4 is profile 2");
        assert!(parse_profile(1, &bytes(P1)[..20]).is_err(), "too short");
        assert!(parse_active(&[0xF1, 0, 0, 0]).is_err());
    }

    /// A G300 that answers with the reports of issue #16 and counts what it was asked.
    struct Fake;

    impl FeatureSource for Fake {
        fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
            let data = match buf[0] {
                0xF0 => ACTIVE.to_vec(),
                0xF3 => bytes(P1),
                0xF4 => bytes(P2),
                0xF5 => bytes(P3),
                _ => return Err(Error::Invalid("no such report".into())),
            };
            buf[..data.len()].copy_from_slice(&data);
            Ok(data.len())
        }
    }

    #[test]
    fn opening_reads_the_three_profiles_and_the_active_one() {
        let mut task = G300Task::open(Fake).unwrap();
        let info = task.info().unwrap();
        assert_eq!(info.profiles.len(), 3);
        assert_eq!(info.active_profile, 1);
        assert_eq!(info.active_level, 1);
    }
}

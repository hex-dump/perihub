//! Format of the profiles stored in the mouse's memory (HID++ 0x8100, format "G303"/2).
//!
//! Changes are always applied on top of the sector's original bytes: fields we do not
//! manage (unmodeled G-Shift buttons, alternate LEDs, name, macros…) stay intact.

use serde::{Deserialize, Serialize};

use super::macros::Macro;
use crate::color::Rgb;
use crate::error::{Error, Result};

const OFF_REPORT_RATE: usize = 0;
const OFF_DEFAULT_DPI: usize = 1;
const OFF_SHIFT_DPI: usize = 2;
const OFF_DPI: usize = 3;
const OFF_BUTTONS: usize = 32;
const OFF_GSHIFT_BUTTONS: usize = 96;
const OFF_LEDS: usize = 208;
const LED_LEN: usize = 11;

pub const DPI_SLOTS: usize = 5;
pub const BUTTON_SLOTS: usize = 16;
pub const LED_SLOTS: usize = 2;
/// Sector size of the G502 HERO PeriHub was verified on.
pub const VERIFIED_SECTOR_SIZE: usize = 256;
/// Smallest sector PeriHub writes. Newer firmware of the same mice reports 255 bytes instead of
/// 256: the profile has the same layout, only the CRC is one byte earlier, in the last two bytes.
pub const MIN_WRITE_SECTOR_SIZE: usize = 255;
/// Smallest sector that holds every field PeriHub reads, and the CRC after them. Some mice
/// (the G502 X) report sectors smaller than 255 bytes: they can be read, not written.
const READ_SECTOR_SIZE: usize = OFF_LEDS + LED_LEN * LED_SLOTS + 2;

pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let temp = (crc >> 8) ^ byte as u16;
        crc <<= 8;
        let mut quick = temp ^ (temp >> 4);
        crc ^= quick;
        quick <<= 5;
        crc ^= quick;
        quick <<= 7;
        crc ^= quick;
    }
    crc
}

pub fn crc_is_valid(sector: &[u8]) -> bool {
    let n = sector.len();
    n >= 2 && crc_ccitt(&sector[..n - 2]) == u16::from_be_bytes([sector[n - 2], sector[n - 1]])
}

pub fn write_crc(sector: &mut [u8]) {
    let n = sector.len();
    let crc = crc_ccitt(&sector[..n - 2]);
    sector[n - 2..].copy_from_slice(&crc.to_be_bytes());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialFunction {
    TiltLeft,
    TiltRight,
    DpiUp,
    DpiDown,
    DpiCycle,
    DpiDefault,
    DpiShift,
    ProfileNext,
    ProfilePrev,
    ProfileCycle,
    GShift,
}

impl SpecialFunction {
    const ALL: [SpecialFunction; 11] = [
        Self::TiltLeft,
        Self::TiltRight,
        Self::DpiUp,
        Self::DpiDown,
        Self::DpiCycle,
        Self::DpiDefault,
        Self::DpiShift,
        Self::ProfileNext,
        Self::ProfilePrev,
        Self::ProfileCycle,
        Self::GShift,
    ];

    fn code(self) -> u8 {
        Self::ALL.iter().position(|&f| f == self).unwrap() as u8 + 1
    }

    fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get((code as usize).checked_sub(1)?).copied()
    }
}

/// What a mouse button does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    /// Mouse button, 1 = left, 2 = right, 3 = middle, 4 = back, 5 = forward…
    Mouse {
        button: u8,
    },
    /// Keyboard key (HID usage) with the standard HID modifiers (bit 0 = left Ctrl, 1 = left Shift, 2 = left Alt, 3 = left Win…).
    Key {
        modifiers: u8,
        key: u8,
    },
    /// Media key (usage from the HID Consumer page, e.g. 0x00CD = play/pause).
    Consumer {
        usage: u16,
    },
    Special {
        function: SpecialFunction,
    },
    Disabled,
    /// Macro in the mouse's memory, starting at `offset` in `sector`. `content` is filled in when
    /// the mouse is read, and stays `None` if the macro uses commands PeriHub cannot read yet: such a
    /// macro is kept as it is. When saving, a macro whose content changed is written to free memory
    /// and the address is updated (see `MouseTask::save_profile`).
    Macro {
        sector: u8,
        offset: u16,
        #[serde(default)]
        content: Option<Macro>,
    },
    /// Unsupported type: kept byte for byte.
    Raw {
        bytes: [u8; 4],
    },
}

impl ButtonAction {
    pub fn decode(b: [u8; 4]) -> Self {
        match b {
            [0x80, 0x01, hi, lo] => {
                let mask = u16::from_be_bytes([hi, lo]);
                if mask.count_ones() == 1 {
                    return Self::Mouse { button: mask.trailing_zeros() as u8 + 1 };
                }
            }
            [0x80, 0x02, modifiers, key] => return Self::Key { modifiers, key },
            [0x80, 0x03, hi, lo] => return Self::Consumer { usage: u16::from_be_bytes([hi, lo]) },
            // The last two bytes carry no data for these functions: the G502 HERO has FF FF there,
            // libratbag ignores them and Solaar writes FF 00. They are kept when saving (see `put_button`).
            [0x90, code, _, _] => {
                if let Some(function) = SpecialFunction::from_code(code) {
                    return Self::Special { function };
                }
            }
            [0xFF, 0xFF, 0xFF, 0xFF] => return Self::Disabled,
            // [memory 0, sector, offset]. Sector 0 is the profile directory: all zeros is not a macro.
            [0x00, sector, hi, lo] if sector != 0 => {
                return Self::Macro { sector, offset: u16::from_be_bytes([hi, lo]), content: None };
            }
            _ => {}
        }
        Self::Raw { bytes: b }
    }

    pub fn encode(&self) -> Result<[u8; 4]> {
        Ok(match *self {
            Self::Mouse { button } => {
                if !(1..=16).contains(&button) {
                    return Err(Error::Invalid(format!("mouse button {button}")));
                }
                let [hi, lo] = (1u16 << (button - 1)).to_be_bytes();
                [0x80, 0x01, hi, lo]
            }
            Self::Key { modifiers, key } => [0x80, 0x02, modifiers, key],
            Self::Consumer { usage } => {
                let [hi, lo] = usage.to_be_bytes();
                [0x80, 0x03, hi, lo]
            }
            Self::Special { function } => [0x90, function.code(), 0xFF, 0xFF],
            Self::Disabled => [0xFF; 4],
            Self::Macro { sector, offset, .. } => {
                let [hi, lo] = offset.to_be_bytes();
                [0x00, sector, hi, lo]
            }
            Self::Raw { bytes } => bytes,
        })
    }
}

/// Effect of a LED zone, in the 11-byte format shared by profiles and 0x8070 commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LedEffect {
    Off,
    Fixed {
        color: Rgb,
    },
    Breathing {
        color: Rgb,
        period_ms: u16,
        brightness: u8,
    },
    Cycle {
        period_ms: u16,
        brightness: u8,
    },
    /// Unsupported effect: kept byte for byte.
    Raw {
        bytes: [u8; LED_LEN],
    },
}

impl LedEffect {
    pub fn decode(b: &[u8]) -> Self {
        let raw: [u8; LED_LEN] = b[..LED_LEN].try_into().unwrap();
        let zero = |range: std::ops::Range<usize>| raw[range].iter().all(|&v| v == 0);
        let brightness = |v: u8| if v == 0 { 100 } else { v };
        match raw[0] {
            0x00 if zero(1..LED_LEN) => Self::Off,
            0x01 if zero(4..LED_LEN) => Self::Fixed { color: Rgb::from_bytes(&raw[1..4]) },
            0x0A if zero(6..7) && zero(8..LED_LEN) => Self::Breathing {
                color: Rgb::from_bytes(&raw[1..4]),
                period_ms: u16::from_be_bytes([raw[4], raw[5]]),
                brightness: brightness(raw[7]),
            },
            0x03 if zero(1..6) && zero(9..LED_LEN) => {
                Self::Cycle { period_ms: u16::from_be_bytes([raw[6], raw[7]]), brightness: brightness(raw[8]) }
            }
            _ => Self::Raw { bytes: raw },
        }
    }

    pub fn encode(&self) -> [u8; LED_LEN] {
        // 100% is encoded as 0, like the firmware does.
        let intensity = |v: u8| if v >= 100 { 0 } else { v.max(1) };
        let mut out = [0u8; LED_LEN];
        match *self {
            Self::Off => {}
            Self::Fixed { color } => {
                out[0] = 0x01;
                out[1..4].copy_from_slice(&color.to_bytes());
            }
            Self::Breathing { color, period_ms, brightness } => {
                out[0] = 0x0A;
                out[1..4].copy_from_slice(&color.to_bytes());
                out[4..6].copy_from_slice(&period_ms.to_be_bytes());
                out[7] = intensity(brightness);
            }
            Self::Cycle { period_ms, brightness } => {
                out[0] = 0x03;
                out[6..8].copy_from_slice(&period_ms.to_be_bytes());
                out[8] = intensity(brightness);
            }
            Self::Raw { bytes } => out = bytes,
        }
        out
    }
}

/// Profile settings that the interface can edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSettings {
    /// Polling interval in ms (1 = 1000 Hz, 2 = 500 Hz, 4 = 250 Hz, 8 = 125 Hz).
    pub report_rate_ms: u8,
    /// Active DPI levels (1 to 5).
    pub dpi_levels: Vec<u16>,
    pub default_dpi_index: u8,
    /// Level used while the "DPI Shift" (sniper) button is held.
    pub shift_dpi_index: u8,
    pub buttons: Vec<ButtonAction>,
    /// Button functions while G-Shift is held.
    pub gshift_buttons: Vec<ButtonAction>,
    pub leds: Vec<LedEffect>,
}

impl ProfileSettings {
    pub fn parse(sector: &[u8], button_count: usize) -> Result<Self> {
        check_layout(sector, button_count, READ_SECTOR_SIZE)?;
        let dpi_levels: Vec<u16> = (0..DPI_SLOTS)
            .map(|i| u16::from_le_bytes([sector[OFF_DPI + 2 * i], sector[OFF_DPI + 2 * i + 1]]))
            .take_while(|&dpi| dpi != 0 && dpi != 0xFFFF)
            .collect();
        let button =
            |base: usize, i: usize| ButtonAction::decode(sector[base + 4 * i..base + 4 * i + 4].try_into().unwrap());
        Ok(Self {
            report_rate_ms: sector[OFF_REPORT_RATE],
            dpi_levels,
            default_dpi_index: sector[OFF_DEFAULT_DPI],
            shift_dpi_index: sector[OFF_SHIFT_DPI],
            buttons: (0..button_count).map(|i| button(OFF_BUTTONS, i)).collect(),
            gshift_buttons: (0..button_count).map(|i| button(OFF_GSHIFT_BUTTONS, i)).collect(),
            leds: (0..LED_SLOTS).map(|i| LedEffect::decode(&sector[OFF_LEDS + LED_LEN * i..])).collect(),
        })
    }

    /// Checks that the values are acceptable for the mouse.
    pub fn validate(&self, button_count: usize, dpi_min: u16, dpi_max: u16, rates_ms: &[u8]) -> Result<()> {
        if !rates_ms.contains(&self.report_rate_ms) {
            return Err(Error::Invalid(format!("polling interval {} ms", self.report_rate_ms)));
        }
        if self.dpi_levels.is_empty() || self.dpi_levels.len() > DPI_SLOTS {
            return Err(Error::Invalid("between 1 and 5 DPI levels are required".into()));
        }
        if let Some(dpi) = self.dpi_levels.iter().find(|&&d| d < dpi_min || d > dpi_max) {
            return Err(Error::Invalid(format!("{dpi} DPI is outside the {dpi_min}–{dpi_max} range")));
        }
        let levels = self.dpi_levels.len() as u8;
        if self.default_dpi_index >= levels || self.shift_dpi_index >= levels {
            return Err(Error::Invalid("default or shift DPI level does not exist".into()));
        }
        if self.buttons.len() != button_count || self.gshift_buttons.len() != button_count {
            return Err(Error::Invalid("wrong number of buttons".into()));
        }
        if self.leds.len() != LED_SLOTS {
            return Err(Error::Invalid("wrong number of LED zones".into()));
        }
        for action in self.buttons.iter().chain(&self.gshift_buttons) {
            action.encode()?;
        }
        Ok(())
    }

    /// (sector, offset) of every macro bound to a button, G-Shift layer included.
    pub fn macro_addresses(&self) -> Vec<(u8, u16)> {
        self.buttons
            .iter()
            .chain(&self.gshift_buttons)
            .filter_map(|action| match action {
                ButtonAction::Macro { sector, offset, .. } => Some((*sector, *offset)),
                _ => None,
            })
            .collect()
    }

    /// Writes the settings on top of an existing sector and recomputes the CRC.
    pub fn apply(&self, sector: &mut [u8]) -> Result<()> {
        let button_count = self.buttons.len();
        check_layout(sector, button_count, MIN_WRITE_SECTOR_SIZE)?;
        if self.dpi_levels.len() > DPI_SLOTS
            || self.gshift_buttons.len() != button_count
            || self.leds.len() != LED_SLOTS
        {
            return Err(Error::Invalid("incomplete profile".into()));
        }
        sector[OFF_REPORT_RATE] = self.report_rate_ms;
        sector[OFF_DEFAULT_DPI] = self.default_dpi_index;
        sector[OFF_SHIFT_DPI] = self.shift_dpi_index;
        for i in 0..DPI_SLOTS {
            let dpi = self.dpi_levels.get(i).copied().unwrap_or(0);
            sector[OFF_DPI + 2 * i..OFF_DPI + 2 * i + 2].copy_from_slice(&dpi.to_le_bytes());
        }
        for (i, action) in self.buttons.iter().enumerate() {
            put_button(&mut sector[OFF_BUTTONS + 4 * i..OFF_BUTTONS + 4 * i + 4], action)?;
        }
        for (i, action) in self.gshift_buttons.iter().enumerate() {
            put_button(&mut sector[OFF_GSHIFT_BUTTONS + 4 * i..OFF_GSHIFT_BUTTONS + 4 * i + 4], action)?;
        }
        for (i, led) in self.leds.iter().enumerate() {
            sector[OFF_LEDS + LED_LEN * i..OFF_LEDS + LED_LEN * (i + 1)].copy_from_slice(&led.encode());
        }
        write_crc(sector);
        Ok(())
    }
}

/// Writes a button binding. A special function that is already there keeps its last two bytes.
fn put_button(slot: &mut [u8], action: &ButtonAction) -> Result<()> {
    let current = ButtonAction::decode(slot[..4].try_into().unwrap());
    if let (ButtonAction::Special { function: old }, ButtonAction::Special { function: new }) = (&current, action)
        && old == new
    {
        return Ok(());
    }
    slot.copy_from_slice(&action.encode()?);
    Ok(())
}

fn check_layout(sector: &[u8], button_count: usize, min_size: usize) -> Result<()> {
    if button_count > BUTTON_SLOTS {
        return Err(Error::Invalid(format!("unsupported profile format: {button_count} buttons")));
    }
    if sector.len() < min_size {
        return Err(Error::Invalid(format!("unsupported profile format: {}-byte sectors", sector.len())));
    }
    Ok(())
}

/// List of enabled profiles, read from sector 0x0000.
pub fn parse_directory(sector: &[u8], profile_count: usize) -> Vec<bool> {
    let mut enabled = vec![false; profile_count];
    for entry in sector.as_chunks::<4>().0.iter().take(profile_count) {
        let address = u16::from_be_bytes([entry[0], entry[1]]);
        if address == 0xFFFF {
            break;
        }
        if let Some(slot) = (address as usize).checked_sub(1).filter(|&i| i < profile_count) {
            enabled[slot] = entry[2] != 0;
        }
    }
    enabled
}

/// Sectors the directory (sector 0x0000) lists as profiles: macros must never be written there.
pub fn directory_sectors(sector: &[u8], profile_count: usize) -> Vec<u16> {
    sector
        .as_chunks::<4>()
        .0
        .iter()
        .take(profile_count)
        .map(|entry| u16::from_be_bytes([entry[0], entry[1]]))
        .take_while(|&address| address != 0xFFFF)
        .collect()
}

pub fn build_directory(enabled: &[bool], sector_size: usize) -> Vec<u8> {
    let mut data = vec![0xFF; sector_size];
    let mut at = 0;
    for (i, &on) in enabled.iter().enumerate() {
        data[at..at + 4].copy_from_slice(&[0x00, i as u8 + 1, on as u8, 0x00]);
        at += 4;
    }
    data[at..at + 4].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
    write_crc(&mut data);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 256-byte profile with valid CRC, every button disabled.
    fn blank_sector() -> Vec<u8> {
        let mut sector = vec![0xFF; VERIFIED_SECTOR_SIZE];
        sector[OFF_REPORT_RATE] = 1;
        sector[OFF_DEFAULT_DPI] = 0;
        sector[OFF_SHIFT_DPI] = 0;
        sector[OFF_DPI..OFF_DPI + 2].copy_from_slice(&800u16.to_le_bytes());
        sector[OFF_DPI + 2..OFF_DPI + 10].fill(0);
        sector[OFF_LEDS..OFF_LEDS + LED_LEN * LED_SLOTS].fill(0);
        write_crc(&mut sector);
        sector
    }

    #[test]
    fn special_functions_ignore_their_last_two_bytes() {
        for tail in [[0xFF, 0xFF], [0xFF, 0x00], [0x00, 0x00], [0x00, 0x01]] {
            let action = ButtonAction::decode([0x90, 0x05, tail[0], tail[1]]);
            assert_eq!(action, ButtonAction::Special { function: SpecialFunction::DpiCycle }, "{tail:02x?}");
        }
        // Codes past G-Shift (battery, profile select…) stay raw: some of them use the last byte.
        assert!(matches!(ButtonAction::decode([0x90, 0x0D, 0xFF, 0x02]), ButtonAction::Raw { .. }));
        assert!(matches!(ButtonAction::decode([0x90, 0x00, 0xFF, 0xFF]), ButtonAction::Raw { .. }));
    }

    #[test]
    fn saving_keeps_the_bytes_of_an_unchanged_special_function() {
        let mut original = blank_sector();
        original[OFF_BUTTONS + 4 * 5..OFF_BUTTONS + 4 * 6].copy_from_slice(&[0x90, 0x05, 0xFF, 0x00]);
        write_crc(&mut original);
        let mut settings = ProfileSettings::parse(&original, 6).unwrap();
        assert_eq!(settings.buttons[5], ButtonAction::Special { function: SpecialFunction::DpiCycle });

        let mut copy = original.clone();
        settings.apply(&mut copy).unwrap();
        assert_eq!(copy, original);

        // A different function is written the usual way.
        settings.buttons[5] = ButtonAction::Special { function: SpecialFunction::DpiUp };
        settings.apply(&mut copy).unwrap();
        assert_eq!(copy[OFF_BUTTONS + 20..OFF_BUTTONS + 24], [0x90, 0x03, 0xFF, 0xFF]);
    }

    #[test]
    fn smaller_sectors_are_read_but_not_written() {
        let full = blank_sector();
        let mut small = full[..READ_SECTOR_SIZE].to_vec();
        write_crc(&mut small);
        let settings = ProfileSettings::parse(&small, 11).unwrap();
        assert_eq!(settings.dpi_levels, [800]);
        let err = settings.apply(&mut small).unwrap_err().to_string();
        assert!(err.contains("232-byte sectors"), "{err}");

        let too_small = &full[..READ_SECTOR_SIZE - 1];
        let err = ProfileSettings::parse(too_small, 11).unwrap_err().to_string();
        assert!(err.contains("231-byte sectors"), "{err}");
        assert!(ProfileSettings::parse(&full, 17).unwrap_err().to_string().contains("17 buttons"));
        let mut g502_x = full[..240].to_vec();
        write_crc(&mut g502_x);
        assert!(settings.apply(&mut g502_x).unwrap_err().to_string().contains("240-byte sectors"));
    }

    /// Newer firmware of the G502 HERO reports 255-byte sectors (issue #20): same fields, CRC one
    /// byte earlier.
    #[test]
    fn a_255_byte_sector_is_saved_with_its_crc_in_the_last_two_bytes() {
        let full = blank_sector();
        let mut sector = full[..255].to_vec();
        write_crc(&mut sector);
        let mut settings = ProfileSettings::parse(&sector, 11).unwrap();
        settings.dpi_levels = vec![400, 1600];
        settings.leds[0] = LedEffect::Fixed { color: Rgb { r: 0x12, g: 0x34, b: 0x56 } };

        let mut written = sector.clone();
        settings.apply(&mut written).unwrap();
        assert_eq!(written.len(), 255);
        assert!(crc_is_valid(&written));
        assert_eq!(ProfileSettings::parse(&written, 11).unwrap(), settings);
        // The same change on a 256-byte sector gives the same bytes, apart from where the CRC goes.
        let mut big = full.clone();
        settings.apply(&mut big).unwrap();
        assert_eq!(written[..253], big[..253]);
    }
}

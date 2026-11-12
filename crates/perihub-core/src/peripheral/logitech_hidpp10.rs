//! The Logitech mice of the HID++ 1.0 years: G9, G9x, G500, G500s, G700 and G700s on their cable.
//! Logitech's own software for them, Logitech Gaming Software, is no longer offered. The protocol is in `docs/protocols/logitech-hidpp10.md`, from libratbag's HID++
//! 1.0 driver (MIT, see `THIRD-PARTY-NOTICES.md`); Solaar names their interface. None has been
//! tried with PeriHub.
//!
//! PeriHub reads the profile in use from the mouse's flash memory, the DPI in use and the polling
//! rate. The only change it sends is the DPI in use, on the G500s and the G700s: a register in RAM
//! that the mouse forgets when it is unplugged or when its own DPI buttons are pressed. Nothing is
//! ever written to the flash memory: its commands are not in the list [`Io`] lets through.

use std::time::Duration;

use super::task::Driver;
use super::{ButtonView, Command, DpiLimits, Effect, Io, Kind, Limits, Model, Protocol, Reading, Settings, hex};
use crate::error::{Error, Result};

/// The configuration collections of interface 1 (Solaar names the interface).
const INTERFACE: i32 = 1;
const USAGE_PAGE: u16 = 0xFF00;

const SHORT: u8 = 0x10;
const LONG: u8 = 0x11;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;
/// A mouse on its cable answers as device 0.
const DEVICE: u8 = 0x00;
const ERROR: u8 = 0x8F;

const GET_REGISTER: u8 = 0x81;
const SET_LONG_REGISTER: u8 = 0x82;
const GET_LONG_REGISTER: u8 = 0x83;

const PROFILE: u8 = 0x0F;
const RESOLUTION: u8 = 0x63;
const POLLING: u8 = 0x64;
const READ_MEMORY: u8 = 0xA2;
const FIRMWARE: u8 = 0xF1;

const TIMEOUT: Duration = Duration::from_millis(1000);

/// Flash pages: 0 is RAM, 1 the profile directory, 2 to 31 profiles and macros.
const PAGE_SIZE: usize = 512;
const DIRECTORY_PAGE: u8 = 1;
const LAST_PAGE: u8 = 31;
/// Bytes per memory read.
const CHUNK: usize = 16;

/// What the profile register says is in use.
const PROFILE_INDEX: u8 = 0x00;
const PROFILE_ADDRESS: u8 = 0x01;
const PROFILE_EEPROM: u8 = 0xEE;
const PROFILE_FACTORY: u8 = 0xFF;

const DPI_MODES: usize = 5;

/// How a model lays out a profile (libratbag's profile types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// G500, G500s, G9x: colour, five DPI modes of 16 bits, 13 buttons.
    G500,
    /// G700, G700s: five DPI modes of 8 bits for X and Y, 13 buttons.
    G700,
    /// G9: colour, five DPI modes of 8 bits, 10 buttons.
    G9,
}

/// How the DPI codes of a model turn into DPI (libratbag's `DpiRange` and `DpiList`).
#[derive(Debug, Clone, Copy)]
pub enum DpiTable {
    /// Code `i` from 1: `min + step × i`, rounded to 25. `step` in hundredths of a DPI.
    Range { min: u16, max: u16, step: u32 },
    /// Code `0x80 + i`: the `i`th value.
    List(&'static [u16]),
}

impl DpiTable {
    fn dpi(&self, code: u16) -> Option<u16> {
        match *self {
            DpiTable::Range { min, max, step } => {
                let codes = (max as u32 - min as u32) * 100 / step;
                if code == 0 || code as u32 > codes {
                    return None;
                }
                let value = min as u32 * 100 + step * code as u32;
                Some(((value + 1250) / 2500 * 25) as u16)
            }
            DpiTable::List(list) => code.checked_sub(0x80).and_then(|i| list.get(i as usize)).copied(),
        }
    }

    fn code(&self, dpi: u16) -> Option<u16> {
        (0..=0xFF).find(|&code| self.dpi(code) == Some(dpi))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub layout: Layout,
    pub dpi: DpiTable,
    pub profiles: u8,
    /// The profile stores a colour for the mouse's light.
    pub color: bool,
    /// The DPI in use may be changed: only where libratbag's table agrees with its own fallback
    /// (code × 50) and with the mouse's published range.
    pub dpi_changes: bool,
}

const RANGE_8200: DpiTable = DpiTable::Range { min: 0, max: 8200, step: 5000 };
const RANGE_5700: DpiTable = DpiTable::Range { min: 0, max: 5700, step: 2353 };
const G9_LIST: &[u16] = &[200, 400, 600, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3200];

const fn model(product_id: &'static [u16], name: &'static str, params: Params) -> Model {
    Model {
        vendor_id: 0x046D,
        product_ids: product_id,
        brand: "Logitech",
        name,
        kind: Kind::Mouse,
        interface: Some(INTERFACE),
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::LogitechHidpp10(params),
    }
}

/// The G700 and G700s through their receiver (`c531`) are left out: only the cable.
pub const MODELS: &[Model] = &[
    model(
        &[0xC048],
        "G9",
        Params { layout: Layout::G9, dpi: DpiTable::List(G9_LIST), profiles: 5, color: true, dpi_changes: false },
    ),
    model(
        &[0xC066],
        "G9x",
        Params {
            layout: Layout::G500,
            dpi: DpiTable::Range { min: 200, max: 5700, step: 5000 },
            profiles: 5,
            color: true,
            dpi_changes: false,
        },
    ),
    model(
        &[0xC249],
        "G9x (Call of Duty MW3 Edition)",
        Params { layout: Layout::G500, dpi: RANGE_5700, profiles: 5, color: false, dpi_changes: false },
    ),
    model(
        &[0xC068],
        "G500",
        Params { layout: Layout::G500, dpi: RANGE_5700, profiles: 1, color: false, dpi_changes: false },
    ),
    model(
        &[0xC24E],
        "G500s",
        Params { layout: Layout::G500, dpi: RANGE_8200, profiles: 1, color: false, dpi_changes: true },
    ),
    model(
        &[0xC06B],
        "G700",
        Params { layout: Layout::G700, dpi: RANGE_5700, profiles: 5, color: false, dpi_changes: false },
    ),
    model(
        &[0xC07C],
        "G700s",
        Params { layout: Layout::G700, dpi: RANGE_8200, profiles: 5, color: false, dpi_changes: true },
    ),
];

/// Everything PeriHub may send. Reads of registers and of the flash memory; the only change is the
/// DPI in use, in RAM, and only on the models whose `dpi_changes` is set. Writing a register that is
/// not listed (switching profile, the polling rate, the LEDs) and every command of the flash memory
/// (register 0xA0: erase, write; the "hot" payload, messages 0x92/0x93 and register 0xA1) never reach
/// the mouse.
const READS: &[Command] = &[
    Command { prefix: &[SHORT, DEVICE, GET_REGISTER, PROFILE], at: 0, effect: Effect::Read, name: "profile in use" },
    Command {
        prefix: &[SHORT, DEVICE, GET_REGISTER, RESOLUTION],
        at: 0,
        effect: Effect::Read,
        name: "DPI in use (G9)",
    },
    Command {
        prefix: &[SHORT, DEVICE, GET_LONG_REGISTER, RESOLUTION],
        at: 0,
        effect: Effect::Read,
        name: "DPI in use",
    },
    Command { prefix: &[SHORT, DEVICE, GET_REGISTER, POLLING], at: 0, effect: Effect::Read, name: "polling rate" },
    Command {
        prefix: &[SHORT, DEVICE, GET_LONG_REGISTER, READ_MEMORY],
        at: 0,
        effect: Effect::Read,
        name: "read memory",
    },
    Command { prefix: &[SHORT, DEVICE, GET_REGISTER, FIRMWARE], at: 0, effect: Effect::Read, name: "firmware" },
];
const SET_DPI: Command = Command {
    prefix: &[LONG, DEVICE, SET_LONG_REGISTER, RESOLUTION],
    at: 0,
    effect: Effect::Change,
    name: "DPI in use (RAM)",
};

/// The CRC under a flash page: CRC-16/CCITT, seed 0xFFFF, stored big-endian in its last two bytes.
pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn page_checks(page: &[u8]) -> bool {
    page.len() == PAGE_SIZE && crc_ccitt(&page[..PAGE_SIZE - 2]) == u16::from_be_bytes([page[510], page[511]])
}

/// Offsets in a profile, by layout.
struct Offsets {
    color: Option<usize>,
    dpi: usize,
    dpi_size: usize,
    polling: usize,
    buttons: usize,
    button_count: usize,
    metadata: usize,
}

fn offsets(layout: Layout) -> Offsets {
    match layout {
        Layout::G500 => {
            Offsets { color: Some(0), dpi: 4, dpi_size: 6, polling: 38, buttons: 39, button_count: 13, metadata: 78 }
        }
        Layout::G700 => {
            Offsets { color: None, dpi: 0, dpi_size: 4, polling: 24, buttons: 35, button_count: 13, metadata: 74 }
        }
        Layout::G9 => {
            Offsets { color: Some(0), dpi: 4, dpi_size: 3, polling: 22, buttons: 23, button_count: 10, metadata: 56 }
        }
    }
}

fn key_name(key: u8) -> String {
    match key {
        0x04..=0x1D => format!("{}", (b'A' + key - 0x04) as char),
        0x1E..=0x26 => format!("{}", key - 0x1D),
        0x27 => "0".into(),
        0x28 => "Enter".into(),
        0x29 => "Esc".into(),
        0x2A => "Backspace".into(),
        0x2B => "Tab".into(),
        0x2C => "Space".into(),
        0x3A..=0x45 => format!("F{}", key - 0x39),
        _ => format!("key code {key:02x}"),
    }
}

fn button_action(binding: &[u8]) -> String {
    let word_le = u16::from_le_bytes([binding[1], binding[2]]);
    match binding[0] {
        0x81 => match word_le.trailing_zeros() {
            16 => "Nothing".into(),
            0 => "Left click".into(),
            1 => "Right click".into(),
            2 => "Middle click".into(),
            3 => "Back".into(),
            4 => "Forward".into(),
            bit => format!("Button {}", bit + 1),
        },
        0x82 => {
            let names = ["Ctrl", "Shift", "Alt", "Win", "Right Ctrl", "Right Shift", "Right Alt", "Right Win"];
            let mut parts: Vec<String> =
                (0..8).filter(|bit| binding[1] & (1 << bit) != 0).map(|bit| names[bit].to_string()).collect();
            if binding[2] != 0 {
                parts.push(key_name(binding[2]));
            }
            format!("Key {}", parts.join("+"))
        }
        0x83 => match word_le {
            0x01 => "Scroll left".into(),
            0x02 => "Scroll right".into(),
            0x03 => "Battery level".into(),
            0x04 => "DPI up".into(),
            0x05 => "DPI cycle up".into(),
            0x08 => "DPI down".into(),
            0x09 => "DPI cycle down".into(),
            0x10 => "Profile up".into(),
            0x11 => "Profile cycle up".into(),
            0x20 => "Profile down".into(),
            0x21 => "Profile cycle down".into(),
            code => format!("Special {code:#04x}"),
        },
        0x84 => match u16::from_be_bytes([binding[1], binding[2]]) {
            0xE9 => "Volume up".into(),
            0xEA => "Volume down".into(),
            0xE2 => "Mute".into(),
            0xCD => "Play/pause".into(),
            0xB5 => "Next track".into(),
            0xB6 => "Previous track".into(),
            code => format!("Media key {code:#06x}"),
        },
        0x8F => "Disabled".into(),
        page => format!("Macro (stored on page {page}, offset {})", binding[1]),
    }
}

/// The name Logitech Gaming Software gave the profile, if it left one ("LGS02" and UTF-16).
fn profile_name(metadata: &[u8]) -> Option<String> {
    if metadata.get(..5)? != b"LGS02" {
        return None;
    }
    let units: Vec<u16> = metadata
        .get(5..5 + 46)?
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0 && u != 0xFFFF)
        .collect();
    let name = String::from_utf16_lossy(&units).trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// What the stored profile says: shown as it is, never changed.
fn describe_profile(params: &Params, page: &[u8], details: &mut Vec<(String, String)>) -> Vec<ButtonView> {
    let o = offsets(params.layout);
    if let Some(name) = profile_name(&page[o.metadata..]) {
        details.push(("Profile name".into(), name));
    }
    let mut levels = Vec::new();
    for i in 0..DPI_MODES {
        let at = o.dpi + i * o.dpi_size;
        let (x, y) = match params.layout {
            Layout::G500 => {
                (u16::from_be_bytes([page[at], page[at + 1]]), u16::from_be_bytes([page[at + 2], page[at + 3]]))
            }
            Layout::G700 => (page[at] as u16, page[at + 1] as u16),
            Layout::G9 => (page[at] as u16, page[at] as u16),
        };
        if x == 0 {
            continue;
        }
        let shown =
            |code: u16| params.dpi.dpi(code).map(|d| d.to_string()).unwrap_or_else(|| format!("code {code:#x}"));
        levels.push(if x == y { shown(x) } else { format!("{}×{}", shown(x), shown(y)) });
    }
    details.push(("DPI levels stored".into(), if levels.is_empty() { "none".into() } else { levels.join(" · ") }));
    let polling = page[o.polling];
    if polling != 0 && 1000 % polling as u16 == 0 {
        details.push(("Polling rate stored".into(), format!("{} Hz", 1000 / polling as u16)));
    }
    if let (Some(at), true) = (o.color, params.color) {
        details.push(("Colour stored".into(), format!("#{:02x}{:02x}{:02x}", page[at], page[at + 1], page[at + 2])));
    }
    (0..o.button_count)
        .map(|i| {
            let at = o.buttons + 3 * i;
            ButtonView { name: format!("Button {}", i + 1), action: button_action(&page[at..at + 3]) }
        })
        .collect()
}

pub struct Hidpp10 {
    params: Params,
    /// Read once: some of these mice do not answer it.
    firmware: Option<Option<String>>,
    /// Flash pages read since the mouse was connected. Nothing writes them while PeriHub has it.
    pages: Vec<(u8, Vec<u8>)>,
}

impl Hidpp10 {
    pub fn new(params: Params) -> Self {
        Self { params, firmware: None, pages: Vec::new() }
    }

    /// Sends a request and returns its answer. A HID++ 1.0 error answer is an error.
    fn request(io: &mut Io, report: &[u8], quiet: bool) -> Result<Vec<u8>> {
        let (sub, address) = (report[2], report[3]);
        let answer = io.hidpp_exchange(report, TIMEOUT, quiet, |r| {
            r.len() >= SHORT_LEN
                && matches!(r[0], SHORT | LONG)
                && ((r[2] == sub && r[3] == address)
                    || (r[0] == SHORT && r[2] == ERROR && r[3] == sub && r[4] == address))
        })?;
        if answer[2] == ERROR {
            return Err(Error::Protocol(format!("HID++ 1.0 error {:#04x} on register {address:#04x}", answer[5])));
        }
        if sub == GET_LONG_REGISTER && (answer[0] != LONG || answer.len() < LONG_LEN) {
            return Err(Error::Protocol(format!("register {address:#04x}: a short answer to a long read")));
        }
        Ok(answer)
    }

    fn get(io: &mut Io, register: u8, params: [u8; 3]) -> Result<Vec<u8>> {
        let [a, b, c] = params;
        Self::request(io, &[SHORT, DEVICE, GET_REGISTER, register, a, b, c], false)
    }

    fn get_long(io: &mut Io, register: u8, params: [u8; 3], quiet: bool) -> Result<Vec<u8>> {
        let [a, b, c] = params;
        Self::request(io, &[SHORT, DEVICE, GET_LONG_REGISTER, register, a, b, c], quiet)
    }

    fn read_firmware(io: &mut Io) -> Option<String> {
        let version = Self::get(io, FIRMWARE, [0x01, 0, 0]).ok()?;
        let build = Self::get(io, FIRMWARE, [0x02, 0, 0]).ok()?;
        Some(format!("{:02X}.{:02X}.B{:02X}{:02X}", version[5], version[6], build[5], build[6]))
    }

    /// The DPI codes in use, X and Y.
    fn read_resolution(&self, io: &mut Io) -> Result<(u16, u16)> {
        if self.params.layout == Layout::G9 {
            let r = Self::get(io, RESOLUTION, [0, 0, 0])?;
            Ok((r[4] as u16, r[4] as u16))
        } else {
            let r = Self::get_long(io, RESOLUTION, [0, 0, 0], false)?;
            Ok((u16::from_le_bytes([r[4], r[5]]), u16::from_le_bytes([r[6], r[7]])))
        }
    }

    /// A flash page, read 16 bytes at a time, kept for the connection.
    fn page(&mut self, io: &mut Io, page: u8) -> Result<Vec<u8>> {
        if let Some((_, data)) = self.pages.iter().find(|(p, _)| *p == page) {
            return Ok(data.clone());
        }
        let mut data = Vec::with_capacity(PAGE_SIZE);
        for offset in (0..PAGE_SIZE).step_by(CHUNK) {
            // The offset goes in 16-bit words.
            let r = Self::get_long(io, READ_MEMORY, [page, (offset / 2) as u8, 0], true)?;
            data.extend_from_slice(&r[4..4 + CHUNK]);
        }
        self.pages.push((page, data.clone()));
        Ok(data)
    }

    /// The profile in use, as an index in the directory: `Err` with what to show when there is none
    /// to read.
    fn profile_in_use(answer: &[u8], directory: &[(u8, u8)]) -> std::result::Result<usize, String> {
        match answer[4] {
            PROFILE_INDEX if (answer[5] as usize) < directory.len() => Ok(answer[5] as usize),
            PROFILE_INDEX => Err(format!("number {} is not in the directory", answer[5] as u16 + 1)),
            PROFILE_ADDRESS => directory
                .iter()
                .position(|&(page, offset)| page == answer[5] && offset == answer[6])
                .ok_or_else(|| format!("the one on page {} is not in the directory", answer[5])),
            PROFILE_FACTORY => Err("the factory profile (PeriHub does not switch away from it)".into()),
            _ => Err("one in the EEPROM".into()),
        }
    }
}

impl Driver for Hidpp10 {
    fn commands(&self) -> Vec<Command> {
        let mut commands = READS.to_vec();
        if self.params.dpi_changes {
            commands.push(SET_DPI);
        }
        commands
    }

    fn source(&self) -> &'static str {
        "libratbag (MIT)"
    }

    fn profiles(&self) -> u8 {
        self.params.profiles
    }

    fn can_save(&self) -> bool {
        false
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        if !self.params.dpi_changes {
            io.forbid_changes();
        }
        let mut reading = Reading::default();
        if self.firmware.is_none() {
            self.firmware = Some(Self::read_firmware(io));
        }
        reading.firmware = self.firmware.clone().flatten();
        let mut problems = Vec::new();

        let (x, y) = match self.read_resolution(io) {
            Ok(codes) => codes,
            Err(e @ (Error::Hid(_) | Error::Protocol(_) | Error::Timeout)) => {
                reading.problem = Some(format!("the DPI in use could not be read as documented: {e}"));
                return Ok(reading);
            }
            Err(e) => return Err(e),
        };
        match (self.params.dpi.dpi(x), self.params.dpi.dpi(y)) {
            (Some(dx), Some(dy)) => {
                reading.settings.dpi_levels = vec![dx];
                if dx == dy {
                    if self.params.dpi_changes {
                        reading.limits =
                            Limits { dpi: Some(DpiLimits { min: 50, max: 8200, step: 50 }), ..Default::default() };
                    }
                } else {
                    reading.details.push(("Vertical DPI in use".into(), dy.to_string()));
                }
            }
            _ => problems.push(format!("DPI codes {x:#x}, {y:#x}")),
        }
        match Self::get(io, POLLING, [0, 0, 0]) {
            Ok(r) if r[4] != 0 && 1000 % r[4] as u16 == 0 => reading.settings.polling_hz = Some(1000 / r[4] as u16),
            Ok(r) => problems.push(format!("polling rate code {:#04x}", r[4])),
            Err(e) => reading.details.push(("Polling rate".into(), format!("not read ({e})"))),
        }

        let profile = match Self::get(io, PROFILE, [0, 0, 0]) {
            Ok(r) if matches!(r[4], PROFILE_INDEX | PROFILE_ADDRESS | PROFILE_EEPROM | PROFILE_FACTORY) => Some(r),
            Ok(r) => {
                problems.push(format!("profile register {}", hex(&r[4..7])));
                None
            }
            // libratbag takes an error as a mouse without profiles.
            Err(Error::Protocol(_)) => None,
            Err(e) => return Err(e),
        };
        if let Some(answer) = profile {
            match self.page(io, DIRECTORY_PAGE) {
                Ok(directory) if page_checks(&directory) => {
                    let entries: Vec<(u8, u8)> = directory
                        .chunks(3)
                        .take(self.params.profiles as usize)
                        .take_while(|e| e[0] != 0xFF)
                        .map(|e| (e[0], e[1]))
                        .collect();
                    match Self::profile_in_use(&answer, &entries) {
                        Ok(index) => {
                            reading.active_profile = Some(index as u8 + 1);
                            let (page, offset) = entries[index];
                            if !(2..=LAST_PAGE).contains(&page) || offset != 0 {
                                reading.details.push((
                                    "Profile in use".into(),
                                    format!("stored on page {page} at {offset}, which PeriHub does not read"),
                                ));
                            } else {
                                match self.page(io, page) {
                                    Ok(data) => {
                                        if !page_checks(&data) {
                                            // libratbag: the mouse uses a profile with a wrong CRC all the same.
                                            reading.details.push((
                                                "Profile check".into(),
                                                "its CRC does not match; the mouse uses it all the same".into(),
                                            ));
                                        }
                                        reading.buttons = describe_profile(&self.params, &data, &mut reading.details);
                                    }
                                    Err(e) => {
                                        reading.details.push(("Profile in use".into(), format!("not read ({e})")))
                                    }
                                }
                            }
                        }
                        Err(what) => reading.details.push(("Profile in use".into(), what)),
                    }
                }
                Ok(_) => problems.push("the profile directory's CRC does not match".into()),
                Err(e @ (Error::Hid(_) | Error::Protocol(_) | Error::Timeout)) => {
                    reading.details.push(("Profiles".into(), format!("not read ({e})")))
                }
                Err(e) => return Err(e),
            }
        }
        for (page, data) in &self.pages {
            for (i, line) in data.chunks(64).enumerate() {
                io.note_line(format!("page {page} at {:3}: {}", i * 64, hex(line)));
            }
        }
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected answers: {}", problems.join(", ")));
        }
        Ok(reading)
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let [dpi] = to.dpi_levels.as_slice() else { return Err(Error::Invalid("one DPI value".into())) };
        let (x, y) = self.read_resolution(io)?;
        let now = self.params.dpi.dpi(x).filter(|_| x == y);
        if now.is_none_or(|d| from.dpi_levels != [d]) {
            return Err(Error::Invalid("the mouse's DPI changed since it was read: nothing was sent".into()));
        }
        let code = self.params.dpi.code(*dpi).ok_or_else(|| Error::Invalid(format!("{dpi} DPI")))?;
        let [lo, hi] = code.to_le_bytes();
        let mut report = [0u8; LONG_LEN];
        report[..8].copy_from_slice(&[LONG, DEVICE, SET_LONG_REGISTER, RESOLUTION, lo, hi, lo, hi]);
        Self::request(io, &report, false)?;
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("PeriHub never writes these mice's memory".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// Writing a short register: never in the list.
    const SET_REGISTER: u8 = 0x80;

    /// A simulated mouse: its flash pages, registers, and every report it got.
    struct Mouse {
        pages: Vec<(u8, Vec<u8>)>,
        profile: [u8; 3],
        resolution: (u16, u16),
        polling: u8,
        /// Registers that answer with a HID++ 1.0 error.
        refused: Vec<u8>,
        sent: Vec<Vec<u8>>,
    }

    type Shared = Arc<Mutex<Mouse>>;

    fn with_crc(mut page: Vec<u8>) -> Vec<u8> {
        page.resize(PAGE_SIZE, 0xFF);
        let crc = crc_ccitt(&page[..PAGE_SIZE - 2]);
        page[510..].copy_from_slice(&crc.to_be_bytes());
        page
    }

    /// A G500 profile: DPI 800, 1600, 3200 (codes ×50), 500 Hz, a few buttons, named by LGS.
    fn g500_profile() -> Vec<u8> {
        let mut p = vec![0u8; PAGE_SIZE];
        p[..3].copy_from_slice(&[0x10, 0x20, 0x30]);
        for (i, code) in [16u16, 32, 64].iter().enumerate() {
            let at = 4 + 6 * i;
            p[at..at + 2].copy_from_slice(&code.to_be_bytes());
            p[at + 2..at + 4].copy_from_slice(&code.to_be_bytes());
        }
        p[38] = 2;
        let buttons: [[u8; 3]; 13] = [
            [0x81, 0x01, 0x00],
            [0x81, 0x02, 0x00],
            [0x81, 0x04, 0x00],
            [0x83, 0x04, 0x00],
            [0x83, 0x08, 0x00],
            [0x82, 0x01, 0x06],
            [0x84, 0x00, 0xE9],
            [0x8F, 0, 0],
            [0x05, 0x10, 0x00],
            [0x81, 0x08, 0x00],
            [0x81, 0x10, 0x00],
            [0x81, 0x00, 0x00],
            [0x83, 0x11, 0x00],
        ];
        for (i, b) in buttons.iter().enumerate() {
            p[39 + 3 * i..42 + 3 * i].copy_from_slice(b);
        }
        p[78..83].copy_from_slice(b"LGS02");
        for (i, c) in "Shooter".encode_utf16().enumerate() {
            p[83 + 2 * i..85 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
        with_crc(p)
    }

    fn directory(entries: &[(u8, u8)]) -> Vec<u8> {
        let mut d: Vec<u8> = entries.iter().flat_map(|&(page, offset)| [page, offset, 0x01]).collect();
        d.push(0xFF);
        with_crc(d)
    }

    fn g500s() -> Shared {
        Arc::new(Mutex::new(Mouse {
            pages: vec![(1, directory(&[(2, 0)])), (2, g500_profile())],
            profile: [PROFILE_INDEX, 0, 0],
            resolution: (32, 32),
            polling: 1,
            refused: vec![FIRMWARE],
            sent: Vec::new(),
        }))
    }

    fn short(sub: u8, register: u8, p: [u8; 3]) -> Vec<u8> {
        vec![SHORT, DEVICE, sub, register, p[0], p[1], p[2]]
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(move |r: &[u8]| {
            let mut m = state.lock().unwrap();
            m.sent.push(r.to_vec());
            let (sub, register) = (r[2], r[3]);
            if m.refused.contains(&register) {
                return vec![vec![SHORT, DEVICE, ERROR, sub, register, 0x02, 0]];
            }
            // A mouse report on the same interface, which must be skipped.
            let mut answers = vec![vec![0x02, 0, 0, 0, 0, 0, 0, 0]];
            match (r[0], sub, register) {
                (SHORT, GET_REGISTER, PROFILE) => answers.push(short(sub, register, m.profile)),
                (SHORT, GET_REGISTER, POLLING) => answers.push(short(sub, register, [m.polling, 0, 0])),
                (SHORT, GET_LONG_REGISTER, RESOLUTION) => {
                    let mut a = vec![0u8; LONG_LEN];
                    a[..4].copy_from_slice(&[LONG, DEVICE, sub, register]);
                    a[4..6].copy_from_slice(&m.resolution.0.to_le_bytes());
                    a[6..8].copy_from_slice(&m.resolution.1.to_le_bytes());
                    answers.push(a);
                }
                (SHORT, GET_LONG_REGISTER, READ_MEMORY) => {
                    let page = m.pages.iter().find(|(p, _)| *p == r[4]).map(|(_, d)| d.clone());
                    let Some(page) = page else {
                        return vec![vec![SHORT, DEVICE, ERROR, sub, register, 0x0B, 0]];
                    };
                    let at = r[5] as usize * 2;
                    let mut a = vec![LONG, DEVICE, sub, register];
                    a.extend_from_slice(&page[at..at + CHUNK]);
                    answers.push(a);
                }
                (LONG, SET_LONG_REGISTER, RESOLUTION) => {
                    assert_eq!(r.len(), LONG_LEN);
                    m.resolution = (u16::from_le_bytes([r[4], r[5]]), u16::from_le_bytes([r[6], r[7]]));
                    answers.push(short(sub, register, [0, 0, 0]));
                }
                other => panic!("unexpected request {other:x?}"),
            }
            answers
        });
        hid.0.lock().unwrap().numbered_output = true;
        hid
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-hidpp10-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn sent_changes(state: &Shared) -> Vec<Vec<u8>> {
        state
            .lock()
            .unwrap()
            .sent
            .iter()
            .filter(|r| r[2] != GET_REGISTER && r[2] != GET_LONG_REGISTER)
            .cloned()
            .collect()
    }

    #[test]
    fn dpi_tables_follow_libratbag() {
        assert_eq!(RANGE_8200.dpi(1), Some(50));
        assert_eq!(RANGE_8200.dpi(164), Some(8200));
        assert_eq!(RANGE_8200.dpi(165), None);
        assert_eq!(RANGE_8200.dpi(0), None);
        assert_eq!(RANGE_8200.code(1600), Some(32));
        // 23.53 DPI a code, rounded to 25: some multiples of 25 have no code.
        assert_eq!(RANGE_5700.dpi(1), Some(25));
        assert_eq!(RANGE_5700.dpi(34), Some(800));
        assert_eq!(RANGE_5700.dpi(242), Some(5700));
        assert_eq!(RANGE_5700.dpi(243), None);
        let g9x = DpiTable::Range { min: 200, max: 5700, step: 5000 };
        assert_eq!(g9x.dpi(1), Some(250));
        assert_eq!(g9x.dpi(110), Some(5700));
        assert_eq!(DpiTable::List(G9_LIST).dpi(0x80), Some(200));
        assert_eq!(DpiTable::List(G9_LIST).dpi(0x8F), Some(3200));
        assert_eq!(DpiTable::List(G9_LIST).dpi(0x90), None);
        assert_eq!(DpiTable::List(G9_LIST).dpi(0x7F), None);
    }

    #[test]
    fn crc_is_ccitt_false() {
        // The check value of CRC-16/CCITT-FALSE.
        assert_eq!(crc_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn these_mice_never_reach_the_hidpp_2_code() {
        for model in MODELS {
            for &id in model.product_ids {
                assert!(crate::hidpp::models::find(id).is_none(), "{id:04x}");
                assert!(!crate::hidpp::receiver::is_receiver(id), "{id:04x}");
                assert_eq!(find(0x046D, id).unwrap().name, model.name, "{id:04x}");
            }
        }
        assert!(find(0x046D, 0xC531).is_none(), "the G700 receiver is left out");
        assert!(find(0x046D, 0xC24C).is_none(), "the G400s is not a HID++ mouse");
    }

    #[test]
    fn the_profile_in_use_reads_as_documented_and_only_reads() {
        let state = g500s();
        let task =
            start_for_test(Box::new(fake(state.clone())), find(0x046D, 0xC24E).unwrap(), 0xC24E, false, temp("read"))
                .unwrap();
        let info = &task.info;
        assert_eq!(info.problem, None);
        assert!(!info.editable, "the command line changes nothing");
        assert_eq!(info.settings.dpi_levels, [1600]);
        assert_eq!(info.settings.polling_hz, Some(1000));
        assert_eq!(info.active_profile, Some(1));
        assert_eq!(info.firmware, None, "a refused firmware register is no problem");
        assert!(!info.raw.iter().any(|l| l.contains("no answer")), "an error answer is taken at once, not waited out");
        let detail = |label: &str| info.details.iter().find(|(l, _)| l == label).map(|(_, v)| v.as_str());
        assert_eq!(detail("Profile name"), Some("Shooter"));
        assert_eq!(detail("DPI levels stored"), Some("800 · 1600 · 3200"));
        assert_eq!(detail("Polling rate stored"), Some("500 Hz"));
        assert_eq!(detail("Colour stored"), None, "the G500s has no colour");
        let actions: Vec<&str> = info.buttons.iter().map(|b| b.action.as_str()).collect();
        assert_eq!(
            actions,
            [
                "Left click",
                "Right click",
                "Middle click",
                "DPI up",
                "DPI down",
                "Key Ctrl+C",
                "Volume up",
                "Disabled",
                "Macro (stored on page 5, offset 16)",
                "Back",
                "Forward",
                "Nothing",
                "Profile cycle up"
            ]
        );
        assert!(info.raw.iter().any(|l| l.starts_with("page 2 at   0: 10 20 30")), "{:?}", info.raw);
        assert!(sent_changes(&state).is_empty());
        // 32 reads a page, once per connection.
        let reads = |s: &Shared| s.lock().unwrap().sent.iter().filter(|r| r[3] == READ_MEMORY).count();
        assert_eq!(reads(&state), 64);
    }

    #[test]
    fn the_dpi_in_use_changes_in_ram_and_is_undone() {
        let state = g500s();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), find(0x046D, 0xC24E).unwrap(), 0xC24E, true, temp("change"))
                .unwrap();
        assert!(task.info.editable);
        assert!(!task.info.can_save && !task.info.changes_stay);
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels = vec![2050];
        task.change(&wanted, false, task.info.active_profile).unwrap();
        let mut expected = vec![0u8; LONG_LEN];
        expected[..8].copy_from_slice(&[LONG, DEVICE, SET_LONG_REGISTER, RESOLUTION, 41, 0, 41, 0]);
        assert_eq!(sent_changes(&state), [expected]);
        assert_eq!(task.info.settings.dpi_levels, [2050]);
        assert!(task.change(&wanted, true, None).is_err(), "nothing is saved to memory");
        wanted.dpi_levels = vec![8250];
        assert!(task.change(&wanted, false, None).is_err(), "over 8200");
        task.undo_last_change().unwrap();
        assert_eq!(state.lock().unwrap().resolution, (32, 32));
        let reads: Vec<u8> = state.lock().unwrap().sent.iter().map(|r| r[2]).collect();
        assert!(reads.iter().all(|&s| matches!(s, GET_REGISTER | GET_LONG_REGISTER | SET_LONG_REGISTER)));
    }

    #[test]
    fn models_whose_dpi_codes_disagree_are_read_only() {
        for id in [0xC048u16, 0xC066, 0xC249, 0xC068, 0xC06B] {
            let model = find(0x046D, id).unwrap();
            let Protocol::LogitechHidpp10(params) = model.protocol else { unreachable!() };
            let state = g500s();
            {
                let mut m = state.lock().unwrap();
                m.resolution = match params.dpi {
                    DpiTable::List(_) => (0x83, 0x83),
                    DpiTable::Range { .. } => (34, 34),
                };
                if params.layout == Layout::G9 {
                    m.refused.push(RESOLUTION);
                }
            }
            let task = start_for_test(Box::new(fake(state.clone())), model, id, true, temp("readonly")).unwrap();
            assert!(!task.info.editable, "{id:04x}");
            let driver = Hidpp10::new(params);
            assert!(driver.commands().iter().all(|c| c.effect == Effect::Read), "{id:04x}");
        }
    }

    #[test]
    fn answers_that_do_not_look_as_documented_leave_it_read_only() {
        let cases: [fn(&mut Mouse); 3] = [
            |m| m.resolution = (500, 500),
            |m| m.profile = [0x42, 0, 0],
            |m| {
                let mut d = directory(&[(2, 0)]);
                d[0] ^= 1;
                m.pages[0].1 = d;
            },
        ];
        for (i, case) in cases.iter().enumerate() {
            let state = g500s();
            case(&mut state.lock().unwrap());
            let task = start_for_test(
                Box::new(fake(state.clone())),
                find(0x046D, 0xC24E).unwrap(),
                0xC24E,
                true,
                temp(&format!("odd{i}")),
            )
            .unwrap();
            assert!(task.info.problem.is_some(), "case {i}");
            assert!(!task.info.editable, "case {i}");
        }
    }

    #[test]
    fn the_factory_profile_is_shown_and_never_switched_away_from() {
        let state = g500s();
        state.lock().unwrap().profile = [PROFILE_FACTORY, 0, 0];
        let task =
            start_for_test(Box::new(fake(state.clone())), find(0x046D, 0xC07C).unwrap(), 0xC07C, true, temp("factory"))
                .unwrap();
        assert_eq!(task.info.problem, None);
        assert_eq!(task.info.active_profile, None);
        assert!(task.info.details.iter().any(|(_, v)| v.contains("factory profile")));
        assert!(sent_changes(&state).is_empty());
    }

    #[test]
    fn a_profile_with_a_wrong_crc_is_shown_as_the_mouse_uses_it() {
        let state = g500s();
        state.lock().unwrap().pages[1].1[100] ^= 0xFF;
        let task =
            start_for_test(Box::new(fake(state.clone())), find(0x046D, 0xC24E).unwrap(), 0xC24E, true, temp("crc"))
                .unwrap();
        assert_eq!(task.info.problem, None);
        assert!(task.info.details.iter().any(|(l, _)| l == "Profile check"));
        assert_eq!(task.info.buttons.len(), 13);
    }

    #[test]
    fn the_g700_layout_and_the_profile_by_address() {
        let mut p = vec![0u8; PAGE_SIZE];
        p[..4].copy_from_slice(&[20, 20, 0x22, 0x11]);
        p[4..8].copy_from_slice(&[40, 30, 0x11, 0x11]);
        p[24] = 1;
        for i in 0..13 {
            p[35 + 3 * i..38 + 3 * i].copy_from_slice(&[0x81, 0x01, 0x00]);
        }
        let state = g500s();
        {
            let mut m = state.lock().unwrap();
            m.pages = vec![(1, directory(&[(3, 0), (4, 0)])), (4, with_crc(p))];
            m.profile = [PROFILE_ADDRESS, 4, 0];
        }
        let task =
            start_for_test(Box::new(fake(state.clone())), find(0x046D, 0xC07C).unwrap(), 0xC07C, true, temp("g700"))
                .unwrap();
        assert_eq!(task.info.problem, None);
        assert_eq!(task.info.active_profile, Some(2));
        let detail = |label: &str| task.info.details.iter().find(|(l, _)| l == label).map(|(_, v)| v.clone());
        assert_eq!(detail("DPI levels stored").as_deref(), Some("1000 · 2000×1500"));
        assert_eq!(detail("Polling rate stored").as_deref(), Some("1000 Hz"));
        assert!(task.info.editable);
    }

    #[test]
    fn only_listed_registers_reach_the_mouse() {
        let state = g500s();
        let model = find(0x046D, 0xC24E).unwrap();
        let Protocol::LogitechHidpp10(params) = model.protocol else { unreachable!() };
        let mut io = Io::new(Box::new(fake(state.clone())), Hidpp10::new(params).commands(), true);
        // Erasing a flash page, switching profile, the polling rate: refused before they are sent.
        let erase = [LONG, DEVICE, SET_LONG_REGISTER, 0xA0, 0x02, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Hidpp10::request(&mut io, &erase, false).is_err());
        assert!(Hidpp10::request(&mut io, &short(SET_REGISTER, PROFILE, [0, 1, 0]), false).is_err());
        assert!(Hidpp10::request(&mut io, &short(SET_REGISTER, POLLING, [2, 0, 0]), false).is_err());
        assert!(state.lock().unwrap().sent.is_empty());
    }
}

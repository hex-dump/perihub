//! ASUS ROG and TUF mice. The protocol is in `docs/protocols/asus-rog.md`. The table below comes
//! from libratbag's `data/devices/asus-*.device` (MIT, see `THIRD-PARTY-NOTICES.md`), and each
//! mouse's interface from OpenRGB's detector (a fact, no code). None has been tried with PeriHub.

use std::time::Duration;

use super::task::Driver;
use super::{
    ButtonView, Choice, Command, DpiLimits, Effect, Io, Kind, Led, LedLimits, Limits, Model, Protocol, Reading,
    Settings,
};
use crate::color::Rgb;
use crate::error::{Error, Result};

pub const VENDOR: u16 = 0x0B05;
const PACKET: usize = 64;
const TIMEOUT: Duration = Duration::from_millis(1000);
/// The vendor collection the commands go to.
const USAGE_PAGE: u16 = 0xFF01;

pub const DOUBLE_DPI: u16 = 1 << 0;
pub const STRIX_PROFILE: u16 = 1 << 1;
pub const RAW_BRIGHTNESS: u16 = 1 << 3;
pub const SEPARATE_XY_DPI: u16 = 1 << 4;
pub const SEPARATE_LEDS: u16 = 1 << 5;
pub const BUTTONS_SECONDARY: u16 = 1 << 6;
pub const LED_V2: u16 = 1 << 8;
pub const SETTINGS_V2: u16 = 1 << 9;

const GET_PROFILE: [u8; 2] = [0x12, 0x00];
const GET_LEDS: [u8; 2] = [0x12, 0x03];
const GET_SETTINGS: [u8; 2] = [0x12, 0x04];
const GET_BUTTONS: [u8; 2] = [0x12, 0x05];
const SET_SETTING: [u8; 2] = [0x51, 0x31];
const SET_LED: [u8; 2] = [0x51, 0x28];
const SAVE: [u8; 2] = [0x50, 0x03];
/// The mouse is asleep, away from its receiver, or refuses the request.
const STATUS_ERROR: [u8; 2] = [0xFF, 0xAA];

/// Everything PeriHub may send to an ASUS mouse. Switching profiles (`50 02`), button bindings
/// (`51 21`) and the Omni receiver's identification (`12 12`) are left out on purpose.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &GET_PROFILE, effect: Effect::Read, name: "read profile" },
    Command { prefix: &GET_LEDS, effect: Effect::Read, name: "read LEDs" },
    Command { prefix: &GET_SETTINGS, effect: Effect::Read, name: "read settings" },
    Command { prefix: &GET_BUTTONS, effect: Effect::Read, name: "read buttons" },
    Command { prefix: &SET_SETTING, effect: Effect::Change, name: "set a setting" },
    Command { prefix: &SET_LED, effect: Effect::Change, name: "set an LED" },
    Command { prefix: &SAVE, effect: Effect::Save, name: "save to the mouse's memory" },
];

const POLLING_HZ: [u16; 4] = [125, 250, 500, 1000];
const DEBOUNCE_MS: [u16; 8] = [4, 8, 12, 16, 20, 24, 28, 32];
const DEBOUNCE: &str = "debounce";
const SNAPPING: &str = "angle_snapping";

const DEFAULT_MODES: &[(u8, &str)] = &[(0, "Static"), (1, "Breathing"), (2, "Colour cycle")];
/// The Pugio's colour cycle is mode 3 (libratbag `LedModes=ON;BREATHING;ON;CYCLE`).
const PUGIO_MODES: &[(u8, &str)] = &[(0, "Static"), (1, "Breathing"), (3, "Colour cycle")];

/// The buttons, by position in the mouse's binding list, when its file lists none.
const DEFAULT_BUTTONS: &[u8] = &[0xF0, 0xF1, 0xF2, 0xE4, 0xE5, 0xE6, 0xE8, 0xE9];

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub profiles: u8,
    /// 2 or 4 DPI levels.
    pub dpis: u8,
    pub dpi_min: u16,
    pub dpi_max: u16,
    pub dpi_step: u16,
    pub leds: u8,
    /// Physical buttons, as libratbag counts them.
    pub buttons: u8,
    /// Button code at each position of the first binding group (0: none there).
    pub button_map: &'static [u8],
    /// The second group, on mice with `BUTTONS_SECONDARY`.
    pub button_map2: &'static [u8],
    pub quirks: u16,
    pub modes: &'static [(u8, &'static str)],
}

const fn params(profiles: u8, dpis: u8, range: (u16, u16, u16), leds: u8, buttons: u8, quirks: u16) -> Params {
    Params {
        profiles,
        dpis,
        dpi_min: range.0,
        dpi_max: range.1,
        dpi_step: range.2,
        leds,
        buttons,
        button_map: DEFAULT_BUTTONS,
        button_map2: &[],
        quirks,
        modes: DEFAULT_MODES,
    }
}

const fn mouse(product_ids: &'static [u16], name: &'static str, interface: Option<i32>, p: Params) -> Model {
    Model {
        vendor_id: VENDOR,
        product_ids,
        brand: "ASUS",
        name,
        kind: Kind::Mouse,
        interface,
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::Asus(p),
    }
}

const fn with_buttons(mut p: Params, map: &'static [u8], map2: &'static [u8]) -> Params {
    p.button_map = map;
    p.button_map2 = map2;
    p
}

/// libratbag's ASUS mice, except the ROG Omni receiver and the mice that use it.
pub const MODELS: &[Model] = &[
    mouse(&[0x1816], "ROG GX860 Buzzard", None, params(3, 2, (100, 8200, 100), 0, 10, 0)),
    mouse(&[0x1958], "ROG Chakram Core", Some(0), params(3, 4, (100, 16000, 100), 3, 5, DOUBLE_DPI | STRIX_PROFILE)),
    mouse(&[0x18E3, 0x18E5], "ROG Chakram", Some(0), params(3, 4, (100, 16000, 100), 3, 5, DOUBLE_DPI | STRIX_PROFILE)),
    mouse(
        &[0x1A18, 0x1A1A],
        "ROG Chakram X",
        Some(0),
        with_buttons(
            params(
                5,
                4,
                (100, 36000, 100),
                3,
                14,
                DOUBLE_DPI | RAW_BRIGHTNESS | SEPARATE_XY_DPI | SEPARATE_LEDS | BUTTONS_SECONDARY,
            ),
            &[0xF0, 0xF1, 0xF2, 0, 0, 0xE6, 0xE8, 0xE9, 0xD0, 0xD1, 0xD2, 0xD3],
            &[0, 0, 0xEA, 0xEB, 0xEC, 0xED],
        ),
    ),
    mouse(
        &[0x1845],
        "ROG Gladius II",
        Some(2),
        with_buttons(
            params(3, 2, (100, 12000, 100), 3, 9, 0),
            &[0xF0, 0xF1, 0xF2, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9],
            &[],
        ),
    ),
    mouse(&[0x1877], "ROG Gladius II Origin", Some(2), params(3, 2, (100, 12000, 100), 3, 8, DOUBLE_DPI)),
    mouse(&[0x18CD], "ROG Gladius II Origin PNK LTD", Some(2), params(3, 2, (100, 12000, 100), 3, 8, DOUBLE_DPI)),
    mouse(&[0x1A92], "ROG Harpe Wireless", None, params(5, 4, (100, 36000, 50), 1, 5, 0)),
    mouse(&[0x195C], "ROG Keris", Some(0), params(3, 4, (100, 16000, 100), 2, 8, DOUBLE_DPI)),
    mouse(&[0x1A66, 0x1A68], "ROG Keris Wireless AimPoint", Some(0), params(5, 4, (100, 36000, 50), 1, 7, 0)),
    mouse(&[0x195E, 0x1960], "ROG Keris Wireless", Some(0), params(3, 4, (100, 16000, 100), 2, 8, DOUBLE_DPI)),
    mouse(
        &[0x1846],
        "ROG Pugio",
        Some(2),
        Params {
            modes: PUGIO_MODES,
            ..with_buttons(
                params(3, 2, (100, 7200, 100), 3, 10, 0),
                &[0xF0, 0xF1, 0xF2, 0xE4, 0xE5, 0xE6, 0, 0xE8, 0xE9, 0xE1, 0xE2],
                &[],
            )
        },
    ),
    mouse(
        &[0x1979, 0x1977],
        "ROG Spatha X",
        Some(0),
        with_buttons(
            params(5, 4, (100, 19000, 50), 3, 14, 0),
            &[0xF0, 0xF1, 0xF2, 0xE4, 0xE5, 0xE6, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF],
            &[],
        ),
    ),
    mouse(&[0x18B4], "ROG Strix Carry", None, params(3, 2, (100, 7200, 100), 0, 8, STRIX_PROFILE)),
    mouse(&[0x18E1], "ROG Strix Impact II", Some(0), params(3, 4, (200, 6200, 100), 3, 8, 0)),
    mouse(&[0x1947, 0x1949], "ROG Strix Impact II Wireless", Some(0), params(3, 4, (100, 16000, 100), 3, 8, 0)),
    mouse(&[0x1A88], "ROG Strix Impact III", Some(0), params(3, 4, (100, 12000, 50), 2, 8, LED_V2 | SETTINGS_V2)),
    mouse(&[0x1847], "ROG Strix Impact", Some(2), params(1, 2, (100, 5000, 100), 1, 6, 0)),
    mouse(&[0x1A9B], "TUF Gaming M3 Gen II", Some(2), params(3, 4, (100, 8000, 100), 1, 8, 0)),
    mouse(&[0x1A03], "TUF Gaming M4 Air", None, params(3, 4, (100, 16000, 100), 0, 6, 0)),
    mouse(
        &[0x1C56, 0x1C57],
        "TUF Gaming Mini WL Mouse Miku",
        None,
        params(5, 4, (100, 12000, 100), 1, 8, SEPARATE_XY_DPI | RAW_BRIGHTNESS),
    ),
];

fn button_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0xF0 => "Left",
        0xF1 => "Right",
        0xF2 => "Middle",
        0xE4 | 0xE1 => "Back",
        0xE5 | 0xE2 => "Forward",
        0xE6 => "DPI",
        0xE7 => "DPI target",
        0xE8 => "Wheel up",
        0xE9 => "Wheel down",
        0xEA => "Side button A",
        0xEB => "Side button B",
        0xEC => "Side button C",
        0xED => "Side button D",
        0xEE => "Side button E",
        0xEF => "Side button F",
        0xD0 => "Joystick up",
        0xD1 => "Joystick down",
        0xD2 => "Joystick forward",
        0xD3 => "Joystick back",
        0xD7 => "Joystick axis −Y",
        0xD8 => "Joystick axis +Y",
        0xDA => "Joystick axis −X",
        0xDB => "Joystick axis +X",
        _ => return None,
    })
}

fn action_name(code: u8, kind: u8) -> String {
    match (kind, code) {
        (_, 0xFF) => "Disabled".into(),
        (1, _) => button_name(code).map(str::to_string).unwrap_or_else(|| format!("Button code {code:02x}")),
        (0, 0x04..=0x1D) => format!("Key {}", (b'A' + code - 0x04) as char),
        (0, 0x1E..=0x26) => format!("Key {}", code - 0x1D),
        (0, 0x27) => "Key 0".into(),
        (0, 0x3A..=0x45) => format!("Key F{}", code - 0x39),
        (0, _) => format!("Key code {code:02x}"),
        _ => format!("Action {kind:02x} {code:02x}"),
    }
}

fn u16_at(r: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([r[at], r[at + 1]])
}

pub struct Asus {
    p: Params,
    /// LED bytes as last read (mode, brightness, red, green, blue): the brightness of
    /// `RAW_BRIGHTNESS` mice is sent back as it was, its range being unknown.
    led_bytes: Vec<[u8; 5]>,
}

impl Asus {
    pub fn new(p: Params) -> Self {
        Self { p, led_bytes: Vec::new() }
    }

    fn has(&self, quirk: u16) -> bool {
        self.p.quirks & quirk != 0
    }

    fn query(&self, io: &mut Io, command: [u8; 2], arg: u8) -> Result<[u8; PACKET]> {
        let mut request = [0u8; PACKET];
        request[..2].copy_from_slice(&command);
        request[2] = arg;
        let mut reply = [0u8; PACKET];
        io.output_query(&request, &mut reply, TIMEOUT)?;
        if reply[..2] == STATUS_ERROR {
            return Err(Error::Unreachable);
        }
        Ok(reply)
    }

    /// Sends a command that changes something, and checks the mouse did not refuse it.
    fn set(&self, io: &mut Io, command: [u8; 2], bytes: &[(usize, u8)]) -> Result<()> {
        let mut request = [0u8; PACKET];
        request[..2].copy_from_slice(&command);
        for &(at, b) in bytes {
            request[at] = b;
        }
        let mut reply = [0u8; PACKET];
        io.output_query(&request, &mut reply, TIMEOUT)?;
        if reply[..2] == STATUS_ERROR {
            return Err(Error::Protocol("the mouse refused the change".into()));
        }
        Ok(())
    }

    fn dpi_from(&self, raw: u16) -> u32 {
        let dpi = raw as u32 * 50 + 50;
        if self.has(DOUBLE_DPI) { dpi * 2 } else { dpi }
    }

    fn dpi_to(&self, dpi: u16) -> u16 {
        let dpi = if self.has(DOUBLE_DPI) { dpi / 2 } else { dpi };
        (dpi - 50) / 50
    }

    /// The settings answer: DPI levels and the three other fields, by layout.
    fn parse_settings(&self, r: &[u8; PACKET], problems: &mut Vec<String>) -> (Vec<u32>, u16, u16, u16) {
        let n = self.p.dpis as usize;
        let dpi: Vec<u32> = (0..n).map(|i| self.dpi_from(u16_at(r, 4 + 2 * i))).collect();
        let (rate, debounce, snapping) = if self.has(SETTINGS_V2) {
            ((r[12] & 0x07) as u16, r[14] as u16, r[16] as u16)
        } else {
            let at = 4 + 2 * n;
            (u16_at(r, at), u16_at(r, at + 2), u16_at(r, at + 4))
        };
        if rate as usize >= POLLING_HZ.len() {
            problems.push(format!("polling rate index {rate}"));
        }
        if debounce as usize >= DEBOUNCE_MS.len() {
            problems.push(format!("button response index {debounce}"));
        }
        if snapping > 1 {
            problems.push(format!("angle snapping {snapping}"));
        }
        (dpi, rate, debounce, snapping)
    }

    fn led_limits(&self) -> Vec<LedLimits> {
        (0..self.p.leds)
            .map(|i| LedLimits {
                name: format!("LED {}", i + 1),
                modes: self.p.modes.to_vec(),
                brightness_step: if self.has(RAW_BRIGHTNESS) {
                    None
                } else if self.has(LED_V2) {
                    Some(1)
                } else {
                    Some(25)
                },
                color: true,
            })
            .collect()
    }

    fn read_leds(&mut self, io: &mut Io) -> Result<Vec<Led>> {
        let mut bytes = Vec::new();
        if self.has(SEPARATE_LEDS | LED_V2) {
            for i in 0..self.p.leds {
                let r = self.query(io, GET_LEDS, i)?;
                bytes.push([r[4], r[5], r[6], r[7], r[8]]);
            }
        } else if self.p.leds > 0 {
            let r = self.query(io, GET_LEDS, 0)?;
            for i in 0..self.p.leds as usize {
                let at = 4 + 5 * i;
                bytes.push([r[at], r[at + 1], r[at + 2], r[at + 3], r[at + 4]]);
            }
        }
        self.led_bytes = bytes.clone();
        Ok(bytes
            .iter()
            .map(|b| Led {
                mode: b[0],
                brightness: if self.has(RAW_BRIGHTNESS) {
                    None
                } else if self.has(LED_V2) {
                    Some(b[1])
                } else {
                    Some(b[1].saturating_mul(25))
                },
                color: Rgb::new(b[2], b[3], b[4]),
            })
            .collect())
    }

    fn read_buttons(&self, io: &mut Io) -> Result<Vec<ButtonView>> {
        let mut views = Vec::new();
        let mut groups = vec![(0u8, self.p.button_map)];
        if self.has(BUTTONS_SECONDARY) {
            groups.push((1, self.p.button_map2));
        }
        for (group, map) in groups {
            let r = self.query(io, GET_BUTTONS, group)?;
            for (i, &code) in map.iter().enumerate().take(17) {
                let Some(name) = button_name(code) else { continue };
                if views.len() >= self.p.buttons as usize {
                    break;
                }
                views.push(ButtonView { name: name.into(), action: action_name(r[4 + 2 * i], r[5 + 2 * i]) });
            }
        }
        Ok(views)
    }
}

impl Driver for Asus {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "libratbag (MIT)"
    }

    fn profiles(&self) -> u8 {
        self.p.profiles
    }

    fn can_save(&self) -> bool {
        true
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let mut problems = Vec::new();
        let profile = self.query(io, GET_PROFILE, 0)?;
        let profile_index = if self.has(STRIX_PROFILE) { profile[9] } else { profile[10] };
        if self.p.profiles > 1 && profile_index >= self.p.profiles {
            problems.push(format!("active profile {profile_index}"));
        }
        if profile[11] > self.p.dpis {
            problems.push(format!("active DPI level {}", profile[11]));
        }
        let firmware = format!("{:x}.{:02x}.{:02x}", profile[15], profile[14], profile[13]);

        let settings_reply = self.query(io, GET_SETTINGS, 0)?;
        let (mut dpi, rate, debounce, snapping) = self.parse_settings(&settings_reply, &mut problems);
        if self.has(SEPARATE_XY_DPI) {
            // Four X and Y pairs: the page shows X (libratbag sets only one value per level).
            let xy = self.query(io, GET_SETTINGS, 2)?;
            dpi = (0..self.p.dpis as usize)
                .map(|i| {
                    let x = u16_at(&xy, 4 + 4 * i) as u32 * 50 + 50;
                    if self.has(DOUBLE_DPI) && !self.has(SETTINGS_V2) { x * 2 } else { x }
                })
                .collect();
        }
        for (i, &d) in dpi.iter().enumerate() {
            if d < self.p.dpi_min as u32 || d > self.p.dpi_max as u32 {
                problems.push(format!("DPI level {} reads {d}, outside {}–{}", i + 1, self.p.dpi_min, self.p.dpi_max));
            }
        }
        let leds = self.read_leds(io)?;
        for (i, l) in self.led_bytes.iter().enumerate() {
            let max = if self.has(LED_V2) { 100 } else { 4 };
            if !self.has(RAW_BRIGHTNESS) && l[1] > max {
                problems.push(format!("LED {} brightness {}", i + 1, l[1]));
            }
        }
        let buttons = self.read_buttons(io)?;

        let settings = Settings {
            dpi_levels: dpi.iter().map(|&d| d.min(u16::MAX as u32) as u16).collect(),
            polling_hz: POLLING_HZ.get(rate as usize).copied(),
            options: [(DEBOUNCE.to_string(), debounce), (SNAPPING.to_string(), snapping)].into(),
            leds,
        };
        let limits = Limits {
            dpi: (!self.has(SEPARATE_XY_DPI)).then_some(DpiLimits {
                min: self.p.dpi_min,
                max: self.p.dpi_max,
                step: self.p.dpi_step,
            }),
            polling_hz: POLLING_HZ.to_vec(),
            options: vec![
                Choice {
                    id: DEBOUNCE,
                    label: "Button response",
                    values: DEBOUNCE_MS.iter().enumerate().map(|(i, ms)| (i as u16, format!("{ms} ms"))).collect(),
                },
                Choice { id: SNAPPING, label: "Angle snapping", values: vec![(0, "Off".into()), (1, "On".into())] },
            ],
            leds: self.led_limits(),
        };
        Ok(Reading {
            firmware: Some(firmware),
            battery: None,
            active_profile: (self.p.profiles > 1).then_some(profile_index + 1),
            active_dpi: (profile[11] > 0).then_some(profile[11]),
            settings,
            limits,
            buttons,
            problem: (!problems.is_empty()).then(|| format!("unexpected answer: {}", problems.join(", "))),
        })
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let dpis = self.p.dpis;
        for (i, (&new, &old)) in to.dpi_levels.iter().zip(&from.dpi_levels).enumerate() {
            if new != old {
                if self.has(SEPARATE_XY_DPI) {
                    return Err(Error::Invalid("the DPI of this mouse is not changed".into()));
                }
                let [lo, hi] = self.dpi_to(new).to_le_bytes();
                self.set(io, SET_SETTING, &[(2, i as u8), (4, lo), (5, hi)])?;
            }
        }
        if to.polling_hz != from.polling_hz {
            let index = POLLING_HZ
                .iter()
                .position(|&hz| Some(hz) == to.polling_hz)
                .ok_or_else(|| Error::Invalid("polling rate".into()))?;
            self.set(io, SET_SETTING, &[(2, dpis), (4, index as u8)])?;
        }
        for (field, id) in [(1u8, DEBOUNCE), (2, SNAPPING)] {
            let (new, old) = (to.options.get(id), from.options.get(id));
            if let (Some(&new), true) = (new, new != old) {
                self.set(io, SET_SETTING, &[(2, dpis + field), (4, new as u8)])?;
            }
        }
        for (i, (new, old)) in to.leds.iter().zip(&from.leds).enumerate() {
            if new == old {
                continue;
            }
            let brightness = match new.brightness {
                _ if self.has(RAW_BRIGHTNESS) => self.led_bytes.get(i).map(|b| b[1]).unwrap_or(0),
                Some(b) if self.has(LED_V2) => b,
                Some(b) => b / 25,
                None => return Err(Error::Invalid("LED brightness".into())),
            };
            let Rgb { r, g, b } = new.color;
            self.set(io, SET_LED, &[(2, i as u8), (4, new.mode), (5, brightness), (6, r), (7, g), (8, b)])?;
        }
        Ok(())
    }

    fn save(&mut self, io: &mut Io) -> Result<()> {
        self.set(io, SAVE, &[])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::peripheral::fake::FakeHid;
    use crate::peripheral::task::start_for_test;
    use crate::peripheral::{find, models};

    /// A simulated ASUS mouse with four DPI levels, keeping its settings, LEDs and a saved copy.
    #[derive(Default)]
    struct Mouse {
        dpi: [u16; 4],
        rate: u16,
        debounce: u16,
        snapping: u16,
        leds: [[u8; 5]; 3],
        saved: Option<([u16; 4], u16)>,
        /// Answers `ff aa` to everything.
        asleep: bool,
        /// Ignores changes.
        stubborn: bool,
    }

    fn fake(mouse: Arc<Mutex<Mouse>>, two_dpis: bool) -> FakeHid {
        FakeHid::new(move |req| {
            let mut m = mouse.lock().unwrap();
            let mut r = vec![0u8; 64];
            r[..2].copy_from_slice(&req[..2]);
            if m.asleep {
                r[..2].copy_from_slice(&STATUS_ERROR);
                return vec![r];
            }
            let n = if two_dpis { 2 } else { 4 };
            match [req[0], req[1]] {
                GET_PROFILE => {
                    r[10] = 1;
                    r[11] = 2;
                    r[13..16].copy_from_slice(&[0x03, 0x02, 0x01]);
                }
                GET_SETTINGS => {
                    for i in 0..n {
                        r[4 + 2 * i..6 + 2 * i].copy_from_slice(&m.dpi[i].to_le_bytes());
                    }
                    let at = 4 + 2 * n;
                    r[at..at + 2].copy_from_slice(&m.rate.to_le_bytes());
                    r[at + 2..at + 4].copy_from_slice(&m.debounce.to_le_bytes());
                    r[at + 4..at + 6].copy_from_slice(&m.snapping.to_le_bytes());
                }
                GET_LEDS => {
                    for (i, l) in m.leds.iter().enumerate() {
                        r[4 + 5 * i..9 + 5 * i].copy_from_slice(l);
                    }
                }
                GET_BUTTONS => {
                    for (i, b) in DEFAULT_BUTTONS.iter().enumerate() {
                        r[4 + 2 * i] = *b;
                        r[5 + 2 * i] = 1;
                    }
                    r[4 + 2 * 3] = 0x04;
                    r[5 + 2 * 3] = 0;
                }
                SET_SETTING if !m.stubborn => {
                    let (field, value) = (req[2] as usize, u16::from_le_bytes([req[4], req[5]]));
                    match field {
                        f if f < n => m.dpi[f] = value,
                        f if f == n => m.rate = value,
                        f if f == n + 1 => m.debounce = value,
                        _ => m.snapping = value,
                    }
                }
                SET_LED if !m.stubborn => m.leds[req[2] as usize] = [req[4], req[5], req[6], req[7], req[8]],
                SAVE => m.saved = Some((m.dpi, m.rate)),
                _ => {}
            }
            // A movement report of the mouse on the same interface, before the answer.
            vec![vec![0x01, 0, 3, 0, 0, 0, 0], r]
        })
    }

    fn keris() -> &'static Model {
        find(VENDOR, 0x195C).unwrap()
    }

    fn standard() -> Arc<Mutex<Mouse>> {
        Arc::new(Mutex::new(Mouse {
            // DOUBLE_DPI: 400, 800, 1600, 3200 are stored halved.
            dpi: [3, 7, 15, 31],
            rate: 3,
            debounce: 1,
            snapping: 0,
            leds: [[0, 4, 255, 0, 0], [1, 2, 0, 0, 255], [0; 5]],
            ..Default::default()
        }))
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-asus-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn the_table_is_consistent() {
        let mut ids = Vec::new();
        for m in models() {
            let Protocol::Asus(p) = m.protocol else { continue };
            assert!(p.dpis == 2 || p.dpis == 4, "{}", m.name);
            let double = if p.quirks & DOUBLE_DPI != 0 { 100 } else { 50 };
            assert!(p.dpi_min >= double && p.dpi_step.is_multiple_of(double), "{}: DPI range encodable", m.name);
            assert!((p.dpi_max - p.dpi_min).is_multiple_of(p.dpi_step), "{}", m.name);
            assert!(p.leds <= 3 && p.button_map.len() <= 17, "{}", m.name);
            ids.extend(m.product_ids);
        }
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "a product id appears twice");
        assert!(!ids.contains(&0x1ACE), "the Omni receiver is left out");
        assert!(!ids.contains(&0x1AD7), "the mice that use the Omni receiver are left out");
    }

    #[test]
    fn forbidden_commands_never_leave() {
        let mouse = standard();
        let hid = fake(mouse, false);
        let mut io = Io::new(Box::new(hid.clone()), COMMANDS.to_vec(), true);
        let mut reply = [0u8; 64];
        for command in
            [[0x50, 0x02], [0x51, 0x21], [0x12, 0x12], [0x00, 0x00], [0xEE, 0x01], [0x50, 0x00], [0x51, 0x00]]
        {
            let mut request = [0u8; 64];
            request[..2].copy_from_slice(&command);
            assert!(io.output_query(&request, &mut reply, TIMEOUT).is_err(), "{command:02x?}");
        }
        assert!(hid.sent().is_empty());
    }

    #[test]
    fn a_keris_reads_as_documented() {
        let hid = fake(standard(), false);
        let task = start_for_test(Box::new(hid.clone()), keris(), 0x195C, false, temp("read")).unwrap();
        let info = &task.info;
        assert_eq!(info.firmware.as_deref(), Some("1.02.03"));
        assert_eq!(info.active_profile, Some(2));
        assert_eq!(info.active_dpi, Some(2));
        assert_eq!(info.settings.dpi_levels, [400, 800, 1600, 3200]);
        assert_eq!(info.settings.polling_hz, Some(1000));
        assert_eq!(info.settings.options["debounce"], 1);
        assert_eq!(info.settings.leds.len(), 2, "the Keris has two LEDs");
        assert_eq!(info.settings.leds[0], Led { mode: 0, brightness: Some(100), color: Rgb::new(255, 0, 0) });
        assert_eq!(info.buttons[0], ButtonView { name: "Left".into(), action: "Left".into() });
        assert_eq!(info.buttons[3].action, "Key A");
        assert!(info.problem.is_none());
        assert!(!info.editable, "opened for reading only");
        assert!(hid.sent().iter().all(|r| r[0] == 0x12), "only reads were sent");
        assert!(info.raw.iter().any(|l| l.starts_with("12 04")), "{:?}", info.raw);
    }

    #[test]
    fn an_answer_out_of_range_leaves_the_mouse_read_only() {
        let mouse = standard();
        mouse.lock().unwrap().rate = 9;
        let hid = fake(mouse, false);
        let mut task = start_for_test(Box::new(hid.clone()), keris(), 0x195C, true, temp("odd")).unwrap();
        assert!(task.info.problem.as_deref().unwrap().contains("polling rate index 9"));
        assert!(!task.info.editable);
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 600;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(hid.sent().iter().all(|r| r[0] == 0x12));
    }

    #[test]
    fn an_asleep_mouse_is_unreachable() {
        let mouse = standard();
        mouse.lock().unwrap().asleep = true;
        let result = start_for_test(Box::new(fake(mouse, false)), keris(), 0x195C, true, temp("asleep"));
        assert!(matches!(result, Err(Error::Unreachable)));
    }

    #[test]
    fn a_change_is_backed_up_sent_field_by_field_and_read_back() {
        let mouse = standard();
        let hid = fake(mouse.clone(), false);
        let mut task = start_for_test(Box::new(hid.clone()), keris(), 0x195C, true, temp("change")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[1] = 1000;
        wanted.polling_hz = Some(500);
        wanted.leds[1].color = Rgb::new(0, 255, 0);
        wanted.leds[1].brightness = Some(75);
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(info.settings, wanted);
        let changes: Vec<Vec<u8>> = hid.sent().into_iter().filter(|r| r[0] != 0x12).collect();
        assert_eq!(changes.len(), 3, "DPI level 2, polling rate, LED 2");
        assert_eq!(changes[0][..6], [0x51, 0x31, 1, 0, 9, 0], "1000 DPI halved: (500 - 50) / 50");
        assert_eq!(changes[1][..5], [0x51, 0x31, 4, 0, 2], "field 4: polling rate, index of 500 Hz");
        assert_eq!(changes[2][..9], [0x51, 0x28, 1, 0, 1, 3, 0, 255, 0]);
        assert!(mouse.lock().unwrap().saved.is_none(), "not saved to memory");
        assert!(task.backups().join("pending-undo").exists());
        task.keep_changes();
        assert!(!task.backups().join("pending-undo").exists());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_mouse_does_not_take_are_refused_before_anything_is_sent() {
        let hid = fake(standard(), false);
        let mut task = start_for_test(Box::new(hid.clone()), keris(), 0x195C, true, temp("refused")).unwrap();
        let reads = hid.sent().len();
        for change in [
            (|s: &mut Settings| s.dpi_levels[0] = 16100) as fn(&mut Settings),
            |s| s.dpi_levels[0] = 450,
            |s| s.polling_hz = Some(2000),
            |s| s.leds[0].mode = 4,
            |s| s.leds[0].brightness = Some(60),
            |s| {
                s.options.insert("debounce".into(), 8);
            },
        ] {
            let mut wanted = task.info.settings.clone();
            change(&mut wanted);
            assert!(task.change(&wanted, false, None).is_err(), "{wanted:?}");
        }
        assert!(hid.sent()[reads..].iter().all(|r| r[0] == 0x12), "nothing but reads");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_change_that_does_not_read_back_is_undone_and_reported() {
        let mouse = standard();
        mouse.lock().unwrap().stubborn = true;
        let mut task =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, true, temp("stubborn")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 600;
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("reads back"), "{error}");
        assert!(error.contains("put back"), "{error}");
        assert!(!task.backups().join("pending-undo").exists());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_save_that_is_not_kept_is_undone_and_saved_again() {
        let mouse = standard();
        let mut task =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, true, temp("save")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[3] = 6400;
        task.change(&wanted, true, None).unwrap();
        assert_eq!(mouse.lock().unwrap().saved.unwrap().0[3], 63);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        let m = mouse.lock().unwrap();
        assert_eq!(m.dpi[3], 31, "back to 3200");
        assert_eq!(m.saved.unwrap().0[3], 31, "and saved again");
        drop(m);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_change_left_pending_is_undone_when_the_mouse_is_opened_again() {
        let mouse = standard();
        let backups = temp("pending");
        let mut task =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, true, backups.clone()).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(125);
        task.change(&wanted, false, None).unwrap();
        drop(task);
        assert_eq!(mouse.lock().unwrap().rate, 0);
        let read_only =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, false, backups.clone()).unwrap();
        assert!(read_only.info.pending_undo);
        assert_eq!(mouse.lock().unwrap().rate, 0, "read-only: nothing sent");
        let task =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, true, backups.clone()).unwrap();
        assert_eq!(mouse.lock().unwrap().rate, 3, "back to 1000 Hz");
        assert!(!task.info.pending_undo);
        let _ = std::fs::remove_dir_all(backups);
    }

    #[test]
    fn two_level_layout_puts_the_rate_after_two_dpis() {
        let mouse = standard();
        let gladius = find(VENDOR, 0x1845).unwrap();
        let mut task = start_for_test(Box::new(fake(mouse.clone(), true)), gladius, 0x1845, true, temp("two")).unwrap();
        assert_eq!(task.info.settings.dpi_levels, [200, 400], "no DOUBLE_DPI: 3 → 200, 7 → 400");
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(250);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(mouse.lock().unwrap().rate, 1, "field 2 on a mouse with two levels");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn separate_xy_dpi_is_shown_but_not_changed() {
        let chakram_x = find(VENDOR, 0x1A18).unwrap();
        let mut task = start_for_test(Box::new(fake(standard(), false)), chakram_x, 0x1A18, true, temp("xy")).unwrap();
        assert!(task.info.limits.dpi.is_none());
        assert!(task.info.limits.leds.iter().all(|l| l.brightness_step.is_none()), "RAW_BRIGHTNESS");
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] += 100;
        assert!(task.change(&wanted, false, None).is_err());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn undoing_a_save_puts_back_what_the_memory_held_not_a_kept_apply() {
        let mouse = standard();
        let mut task =
            start_for_test(Box::new(fake(mouse.clone(), false)), keris(), 0x195C, true, temp("memory")).unwrap();
        let mut applied = task.info.settings.clone();
        applied.polling_hz = Some(500);
        task.change(&applied, false, None).unwrap();
        task.keep_changes();
        let mut saved = applied.clone();
        saved.dpi_levels[0] = 600;
        task.change(&saved, true, None).unwrap();
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        let m = mouse.lock().unwrap();
        assert_eq!(m.saved.unwrap(), ([3, 7, 15, 31], 3), "the memory holds the settings from when PeriHub connected");
        drop(m);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_change_made_on_another_profile_is_refused() {
        let hid = fake(standard(), false);
        let mut task = start_for_test(Box::new(hid.clone()), keris(), 0x195C, true, temp("profile")).unwrap();
        assert_eq!(task.info.active_profile, Some(2));
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(250);
        let error = task.change(&wanted, false, Some(1)).unwrap_err().to_string();
        assert!(error.contains("switched to profile 2"), "{error}");
        assert!(hid.sent().iter().all(|r| r[0] == 0x12), "nothing but reads");
        task.change(&wanted, false, Some(2)).unwrap();
        let _ = std::fs::remove_dir_all(task.backups());
    }
}

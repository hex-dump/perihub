//! Keyboards with QMK firmware that speak VIA, and Vial keyboards: what they are, and their
//! lighting. The protocol is in `docs/protocols/qmk-via-vial.md`, from facts read in the QMK and
//! Vial sources (GPL, no code copied). None has been tried with PeriHub.
//!
//! These keyboards have no USB ids of their own: PeriHub knows them by their raw HID collection
//! (usage page `0xFF60`, usage `0x61`). Vial keyboards say so in their serial number and are read
//! without asking. Any other keyboard with that collection is read only after the user turns it on:
//! its firmware may answer raw HID with code of its own.
//!
//! Only the lighting is changed, and only where the protocol version says how: VIA 12 and 13 (the
//! channel format) and Vial's VialRGB. Keymaps, macros, the bootloader jump, the memory and keymap
//! resets, the state of the key matrix and Vial's unlock are never sent.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use super::task::Driver;
use super::{Choice, Command, Effect, Io, Kind, Led, LedLimits, Model, Protocol, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, Result};

/// The raw HID collection of a QMK keyboard.
pub const USAGE_PAGE: u16 = 0xFF60;
pub const USAGE: u16 = 0x61;
/// In the USB serial number of every Vial keyboard.
pub const VIAL_SERIAL: &str = "vial:f64c2b3c";
/// In the serial number of Vial's bootloader, which PeriHub never opens.
pub const VIAL_BOOTLOADER_SERIAL: &str = "vibl:d4f8159c";

/// Every request and answer: a 32-byte report without a report id.
const REPORT: usize = 32;
const TIMEOUT: Duration = Duration::from_millis(500);
/// Byte 0 of the answer to a command the firmware does not handle.
const UNHANDLED: u8 = 0xFF;

const PROTOCOL_VERSION: u8 = 0x01;
const SET_VALUE: u8 = 0x07;
const GET_VALUE: u8 = 0x08;
const SAVE: u8 = 0x09;
const MACRO_COUNT: u8 = 0x0C;
const LAYER_COUNT: u8 = 0x11;
const VIAL: u8 = 0xFE;
const VIAL_KEYBOARD_ID: u8 = 0x00;

/// VIA protocol versions with the channel format: the only ones whose lighting PeriHub changes.
const CHANNEL_FORMAT: [u16; 2] = [0x000C, 0x000D];

/// Values of a lighting channel.
const BRIGHTNESS: u8 = 1;
const EFFECT: u8 = 2;
const SPEED: u8 = 3;
const COLOR: u8 = 4;

/// A lighting module of the channel format. The backlight (1) is left out: turning its breathing on
/// or off writes the memory, and its brightness does not read back as set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Channel {
    id: u8,
    name: &'static str,
    color: bool,
    speed_option: &'static str,
    speed_label: &'static str,
}

const CHANNELS: [Channel; 3] = [
    Channel {
        id: 3,
        name: "Per-key RGB",
        color: true,
        speed_option: "rgb_matrix_speed",
        speed_label: "Per-key RGB speed",
    },
    Channel { id: 2, name: "RGB light", color: true, speed_option: "rgblight_speed", speed_label: "RGB light speed" },
    Channel {
        id: 5,
        name: "Per-key light",
        color: false,
        speed_option: "led_matrix_speed",
        speed_label: "Per-key light speed",
    },
];

const VIALRGB_INFO: u8 = 0x40;
const VIALRGB_MODE: u8 = 0x41;
const VIALRGB_SUPPORTED: u8 = 0x42;
const VIALRGB_VERSION: u16 = 1;
/// Colours sent by a program, one LED at a time: never set by PeriHub.
const VIALRGB_DIRECT: u16 = 1;
const VIALRGB_SPEED: &str = "vialrgb_speed";
/// Pages of supported effects asked for at most (15 effects each).
const VIALRGB_PAGES: usize = 8;
/// Below this highest brightness, steps of 5 % may not read back as set.
const VIALRGB_BRIGHTNESS_MIN_MAX: u8 = 25;

/// Steps the brightness is shown and set in. VIA's brightness may come back a step or two off on
/// the 0–255 scale, so it is read as the nearest step; VialRGB's comes back as set.
const VIA_BRIGHTNESS_STEP: u8 = 10;
const VIALRGB_BRIGHTNESS_STEP: u8 = 5;

/// VialRGB effects by id: the same on every Vial keyboard.
const VIALRGB_EFFECTS: [&str; 45] = [
    "Off",
    "Direct",
    "Solid colour",
    "Alphas and mods",
    "Gradient up/down",
    "Gradient left/right",
    "Breathing",
    "Band saturation",
    "Band brightness",
    "Band pinwheel saturation",
    "Band pinwheel brightness",
    "Band spiral saturation",
    "Band spiral brightness",
    "Cycle all",
    "Cycle left/right",
    "Cycle up/down",
    "Rainbow moving chevron",
    "Cycle out/in",
    "Cycle out/in dual",
    "Cycle pinwheel",
    "Cycle spiral",
    "Dual beacon",
    "Rainbow beacon",
    "Rainbow pinwheels",
    "Raindrops",
    "Jellybean raindrops",
    "Hue breathing",
    "Hue pendulum",
    "Hue wave",
    "Typing heatmap",
    "Digital rain",
    "Solid reactive simple",
    "Solid reactive",
    "Solid reactive wide",
    "Solid reactive multiwide",
    "Solid reactive cross",
    "Solid reactive multicross",
    "Solid reactive nexus",
    "Solid reactive multinexus",
    "Splash",
    "Multisplash",
    "Solid splash",
    "Solid multisplash",
    "Pixel rain",
    "Pixel fractal",
];

const fn read(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 0, effect: Effect::Read, name }
}

const fn change(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 0, effect: Effect::Change, name }
}

const fn save(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 0, effect: Effect::Save, name }
}

/// Everything PeriHub may send to a VIA keyboard.
pub const VIA_COMMANDS: &[Command] = &[
    read(&[PROTOCOL_VERSION], "protocol version"),
    read(&[LAYER_COUNT], "number of layers"),
    read(&[MACRO_COUNT], "number of macros"),
    read(&[GET_VALUE, 3], "get per-key RGB"),
    read(&[GET_VALUE, 2], "get RGB light"),
    read(&[GET_VALUE, 5], "get per-key light"),
    change(&[SET_VALUE, 3, BRIGHTNESS], "set per-key RGB brightness"),
    change(&[SET_VALUE, 3, EFFECT], "set per-key RGB effect"),
    change(&[SET_VALUE, 3, SPEED], "set per-key RGB speed"),
    change(&[SET_VALUE, 3, COLOR], "set per-key RGB colour"),
    change(&[SET_VALUE, 2, BRIGHTNESS], "set RGB light brightness"),
    change(&[SET_VALUE, 2, EFFECT], "set RGB light effect"),
    change(&[SET_VALUE, 2, SPEED], "set RGB light speed"),
    change(&[SET_VALUE, 2, COLOR], "set RGB light colour"),
    change(&[SET_VALUE, 5, BRIGHTNESS], "set per-key light brightness"),
    change(&[SET_VALUE, 5, EFFECT], "set per-key light effect"),
    change(&[SET_VALUE, 5, SPEED], "set per-key light speed"),
    save(&[SAVE, 3], "store the per-key RGB"),
    save(&[SAVE, 2], "store the RGB light"),
    save(&[SAVE, 5], "store the per-key light"),
];

/// Everything PeriHub may send to a Vial keyboard.
pub const VIAL_COMMANDS: &[Command] = &[
    read(&[PROTOCOL_VERSION], "VIA protocol version"),
    read(&[LAYER_COUNT], "number of layers"),
    read(&[MACRO_COUNT], "number of macros"),
    read(&[VIAL, VIAL_KEYBOARD_ID], "Vial keyboard id"),
    read(&[GET_VALUE, VIALRGB_INFO], "VialRGB info"),
    read(&[GET_VALUE, VIALRGB_MODE], "VialRGB mode"),
    read(&[GET_VALUE, VIALRGB_SUPPORTED], "VialRGB supported effects"),
    change(&[SET_VALUE, VIALRGB_MODE], "set VialRGB mode"),
    save(&[SAVE, 0], "store the lighting"),
];

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// A Vial keyboard (by its serial number), else a keyboard the user turned on as VIA.
    pub vial: bool,
}

/// What PeriHub calls a Vial keyboard until it reads its name.
pub static VIAL_MODEL: Model = Model {
    vendor_id: 0,
    product_ids: &[],
    brand: "Vial",
    name: "keyboard",
    kind: Kind::Keyboard,
    interface: None,
    usage_page: Some(USAGE_PAGE),
    protocol: Protocol::Qmk(Params { vial: true }),
};

/// A keyboard with a raw HID collection that the user turned on as a VIA keyboard.
pub static VIA_MODEL: Model = Model {
    vendor_id: 0,
    product_ids: &[],
    brand: "VIA",
    name: "keyboard",
    kind: Kind::Keyboard,
    interface: None,
    usage_page: Some(USAGE_PAGE),
    protocol: Protocol::Qmk(Params { vial: false }),
};

/// How PeriHub treats a raw HID collection it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Not a QMK keyboard's raw HID collection.
    No,
    /// Vial's bootloader: never opened.
    Bootloader,
    Vial,
    /// Maybe a VIA keyboard: opened only if the user turned it on.
    Via,
}

pub fn classify(usage_page: u16, usage: u16, serial: Option<&str>) -> Found {
    let serial = serial.unwrap_or_default();
    if serial.contains(VIAL_BOOTLOADER_SERIAL) {
        Found::Bootloader
    } else if usage_page != USAGE_PAGE || usage != USAGE {
        Found::No
    } else if serial.contains(VIAL_SERIAL) {
        Found::Vial
    } else {
        Found::Via
    }
}

/// Brand and name from the USB strings: "Keychron" and "Q1 Pro" rather than "Keychron Keychron Q1 Pro".
pub fn names(manufacturer: Option<&str>, product: Option<&str>, model: &Model) -> (String, String) {
    let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let brand = clean(manufacturer).unwrap_or_else(|| model.brand.to_string());
    let name = match clean(product) {
        Some(product) => match product.strip_prefix(&brand) {
            Some(rest) if !rest.trim().is_empty() => rest.trim().to_string(),
            _ => product,
        },
        None => model.name.to_string(),
    };
    (brand, name)
}

fn effect_name(effect: u8) -> &'static str {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| (0..=255).map(|n| format!("Effect {n}")).collect())[effect as usize].as_str()
}

/// The colour a hue and saturation show at full brightness (QMK's scales: hue 0–255 is one turn).
fn hs_to_rgb(hue: u8, sat: u8) -> Rgb {
    Rgb::from_hsv(hue as f32 / 256.0, sat as f32 / 255.0, 1.0)
}

fn distance(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2) as u32;
    d(a.r, b.r) + d(a.g, b.g) + d(a.b, b.b)
}

fn to_percent(raw: u8, max: u8, step: u8) -> u8 {
    let percent = raw as f32 * 100.0 / max as f32;
    (((percent / step as f32).round() as u8) * step).min(100)
}

fn from_percent(percent: u8, max: u8) -> u8 {
    (percent as f32 * max as f32 / 100.0).round().min(max as f32) as u8
}

/// The raw values of a light as last read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Raw {
    effect: u16,
    speed: u8,
    hue: u8,
    sat: u8,
    brightness: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Light {
    Channel { channel: Channel, raw: Raw },
    VialRgb { max: u8, supported: Vec<u16>, raw: Raw },
}

impl Light {
    fn raw(&self) -> Raw {
        match self {
            Light::Channel { raw, .. } | Light::VialRgb { raw, .. } => *raw,
        }
    }

    fn color(&self) -> bool {
        match self {
            Light::Channel { channel, .. } => channel.color,
            Light::VialRgb { .. } => true,
        }
    }

    fn speed_option(&self) -> &'static str {
        match self {
            Light::Channel { channel, .. } => channel.speed_option,
            Light::VialRgb { .. } => VIALRGB_SPEED,
        }
    }

    /// The scale of the brightness, and its step in percent.
    fn brightness_scale(&self) -> (u8, u8) {
        match self {
            Light::Channel { .. } => (255, VIA_BRIGHTNESS_STEP),
            Light::VialRgb { max, .. } => (*max, VIALRGB_BRIGHTNESS_STEP),
        }
    }
}

/// Sends a request, padded to 32 bytes, and returns the answer.
fn query(io: &mut Io, request: &[u8]) -> Result<[u8; REPORT]> {
    let mut payload = [0u8; REPORT];
    payload[..request.len()].copy_from_slice(request);
    let mut reply = [0u8; REPORT];
    io.output_query(&payload, &mut reply, TIMEOUT)?;
    Ok(reply)
}

/// A value of a lighting channel: `None` if the keyboard does not handle it, or answers something
/// else (a problem then).
fn get_value(io: &mut Io, channel: &Channel, value: u8, problems: &mut Vec<String>) -> Result<Option<[u8; REPORT]>> {
    let reply = query(io, &[GET_VALUE, channel.id, value])?;
    if reply[0] == UNHANDLED {
        return Ok(None);
    }
    if reply[..3] != [GET_VALUE, channel.id, value] {
        problems.push(format!("{} value {value} answered {}", channel.name, super::hex(&reply[..6])));
        return Ok(None);
    }
    Ok(Some(reply))
}

/// A count the keyboard answers in byte 1, if it handles the command.
fn count(io: &mut Io, command: u8) -> Result<Option<u8>> {
    let reply = query(io, &[command])?;
    Ok((reply[0] == command).then_some(reply[1]))
}

pub struct Qmk {
    p: Params,
    lights: Vec<Light>,
    /// The effect last seen on, per channel: VIA effect numbers have no names, so the page offers off
    /// and that one.
    last_on: HashMap<u8, u8>,
    /// Hue and saturation of each colour read, so that a colour put back is the one read.
    colors: HashMap<Rgb, (u8, u8)>,
    /// Raw brightness of each step read, by light, for the same reason.
    levels: HashMap<(usize, u8), u8>,
}

impl Qmk {
    pub fn new(p: Params) -> Self {
        Self { p, lights: Vec::new(), last_on: HashMap::new(), colors: HashMap::new(), levels: HashMap::new() }
    }

    /// The hue and saturation of a colour: the ones read for it, or the nearest.
    fn hs_of(&self, color: Rgb) -> (u8, u8) {
        if let Some(&hs) = self.colors.get(&color) {
            return hs;
        }
        let mut best = ((0, 0), u32::MAX);
        for hue in 0..=255u8 {
            for sat in 0..=255u8 {
                let d = distance(hs_to_rgb(hue, sat), color);
                if d < best.1 {
                    best = ((hue, sat), d);
                    if d == 0 {
                        return best.0;
                    }
                }
            }
        }
        best.0
    }

    fn raw_brightness(&self, light: usize, percent: u8) -> u8 {
        let (max, _) = self.lights[light].brightness_scale();
        self.levels.get(&(light, percent)).copied().unwrap_or_else(|| from_percent(percent, max))
    }

    /// The lighting of a VIA keyboard with the channel format.
    fn read_channels(&mut self, io: &mut Io, problems: &mut Vec<String>) -> Result<Vec<Light>> {
        let mut lights = Vec::new();
        for channel in CHANNELS {
            // A channel the keyboard does not have answers "unhandled".
            let Some(brightness) = get_value(io, &channel, BRIGHTNESS, problems)? else { continue };
            let effect = get_value(io, &channel, EFFECT, problems)?;
            let speed = get_value(io, &channel, SPEED, problems)?;
            let (Some(effect), Some(speed)) = (effect, speed) else {
                problems.push(format!("{}: no effect or speed", channel.name));
                continue;
            };
            let (hue, sat) = if channel.color {
                match get_value(io, &channel, COLOR, problems)? {
                    Some(color) => (color[3], color[4]),
                    None => {
                        problems.push(format!("{}: no colour", channel.name));
                        continue;
                    }
                }
            } else {
                (0, 0)
            };
            let raw = Raw { effect: effect[3] as u16, speed: speed[3], hue, sat, brightness: brightness[3] };
            lights.push(Light::Channel { channel, raw });
        }
        Ok(lights)
    }

    /// VialRGB, on a Vial keyboard that says it has it.
    fn read_vialrgb(&mut self, io: &mut Io, problems: &mut Vec<String>) -> Result<Option<Light>> {
        let info = query(io, &[GET_VALUE, VIALRGB_INFO])?;
        let version = u16::from_le_bytes([info[2], info[3]]);
        if info[..2] != [GET_VALUE, VIALRGB_INFO] || version != VIALRGB_VERSION || info[4] == 0 {
            problems.push(format!("VialRGB info {}", super::hex(&info[..6])));
            return Ok(None);
        }
        let max = info[4];
        let mut supported = vec![0u16];
        let mut after = 0u16;
        'pages: for _ in 0..VIALRGB_PAGES {
            let [lo, hi] = after.to_le_bytes();
            let page = query(io, &[GET_VALUE, VIALRGB_SUPPORTED, lo, hi])?;
            if page[..2] != [GET_VALUE, VIALRGB_SUPPORTED] {
                problems.push(format!("VialRGB effects {}", super::hex(&page[..6])));
                return Ok(None);
            }
            for pair in page[2..].as_chunks::<2>().0 {
                let id = u16::from_le_bytes(*pair);
                if id == 0xFFFF {
                    break 'pages;
                }
                if id <= after {
                    problems.push(format!("VialRGB effect {id} after {after}"));
                    return Ok(None);
                }
                supported.push(id);
                after = id;
            }
        }
        let mode = query(io, &[GET_VALUE, VIALRGB_MODE])?;
        if mode[..2] != [GET_VALUE, VIALRGB_MODE] {
            problems.push(format!("VialRGB mode {}", super::hex(&mode[..8])));
            return Ok(None);
        }
        let raw = Raw {
            effect: u16::from_le_bytes([mode[2], mode[3]]),
            speed: mode[4],
            hue: mode[5],
            sat: mode[6],
            brightness: mode[7],
        };
        let known = supported.contains(&raw.effect) || raw.effect == VIALRGB_DIRECT;
        if raw.brightness > max || !known || raw.effect > u8::MAX as u16 {
            problems.push(format!("VialRGB mode {}", super::hex(&mode[..8])));
            return Ok(None);
        }
        Ok(Some(Light::VialRgb { max, supported, raw }))
    }

    /// Puts what was read on the page, and remembers the raw values behind it.
    fn show(&mut self, reading: &mut Reading) {
        for (i, light) in self.lights.iter().enumerate() {
            let raw = light.raw();
            let (max, step) = light.brightness_scale();
            let percent = to_percent(raw.brightness, max, step);
            self.levels.insert((i, percent), raw.brightness);
            let color = if light.color() { hs_to_rgb(raw.hue, raw.sat) } else { Rgb::WHITE };
            if light.color() {
                self.colors.insert(color, (raw.hue, raw.sat));
            }
            reading.settings.leds.push(Led { mode: raw.effect as u8, brightness: Some(percent), color });
            reading.settings.options.insert(light.speed_option().to_string(), raw.speed as u16);
            let mut speeds: Vec<u16> = (0..=7).map(|n| n * 32).chain([255]).collect();
            if !speeds.contains(&(raw.speed as u16)) {
                speeds.push(raw.speed as u16);
                speeds.sort_unstable();
            }
            let values = speeds.into_iter().map(|v| (v, format!("{v} of 255"))).collect();
            let (name, modes, brightness_step, label) = match light {
                Light::Channel { channel, raw } => {
                    if raw.effect != 0 {
                        self.last_on.insert(channel.id, raw.effect as u8);
                    }
                    let mut modes = vec![(0, "Off")];
                    if let Some(&on) = self.last_on.get(&channel.id) {
                        modes.push((on, effect_name(on)));
                    }
                    (channel.name, modes, Some(VIA_BRIGHTNESS_STEP), channel.speed_label)
                }
                Light::VialRgb { max, supported, .. } => {
                    let modes = supported
                        .iter()
                        .filter(|&&id| id != VIALRGB_DIRECT)
                        .filter_map(|&id| VIALRGB_EFFECTS.get(id as usize).map(|name| (id as u8, *name)))
                        .collect();
                    let step = (*max >= VIALRGB_BRIGHTNESS_MIN_MAX).then_some(VIALRGB_BRIGHTNESS_STEP);
                    ("Per-key RGB", modes, step, "Effect speed")
                }
            };
            reading.limits.leds.push(LedLimits {
                name: name.to_string(),
                modes,
                brightness_step,
                color: light.color(),
            });
            reading.limits.options.push(Choice { id: light.speed_option(), label, values });
        }
    }

    fn set_channel(io: &mut Io, channel: &Channel, value: u8, data: &[u8]) -> Result<()> {
        let mut request = vec![SET_VALUE, channel.id, value];
        request.extend_from_slice(data);
        let reply = query(io, &request)?;
        if reply[0] != SET_VALUE {
            return Err(Error::Protocol(format!("{} did not take value {value}", channel.name)));
        }
        Ok(())
    }
}

/// The light is off and something else than turning it on changed: QMK ignores colour and
/// brightness while the light is off.
fn changes_while_off(from: &Led, to: &Led, speed_changed: bool) -> bool {
    to.mode == 0 && (to.brightness != from.brightness || to.color != from.color || speed_changed)
}

impl Driver for Qmk {
    fn commands(&self) -> Vec<Command> {
        if self.p.vial { VIAL_COMMANDS.to_vec() } else { VIA_COMMANDS.to_vec() }
    }

    fn source(&self) -> &'static str {
        "the QMK and Vial firmware sources (no code copied)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        true
    }

    fn normalize(&self, wanted: &Settings) -> Settings {
        let mut wanted = wanted.clone();
        for (led, light) in wanted.leds.iter_mut().zip(&self.lights) {
            if light.color() {
                let (hue, sat) = self.hs_of(led.color);
                led.color = hs_to_rgb(hue, sat);
            }
        }
        wanted
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let mut reading = Reading::default();
        let mut problems = Vec::new();
        let mut vial = None;
        if self.p.vial {
            let id = query(io, &[VIAL, VIAL_KEYBOARD_ID])?;
            vial = Some((u32::from_le_bytes([id[0], id[1], id[2], id[3]]), id[12] == 1));
        }
        let version = query(io, &[PROTOCOL_VERSION])?;
        if version[0] != PROTOCOL_VERSION {
            reading.problem =
                Some(format!("the keyboard does not answer VIA's requests ({})", super::hex(&version[..4])));
            self.lights.clear();
            return Ok(reading);
        }
        let via = u16::from_be_bytes([version[1], version[2]]);
        let mut firmware = match vial {
            Some((vial, _)) => format!("Vial protocol {vial}, VIA protocol {via}"),
            None => format!("VIA protocol {via}"),
        };
        if let Some(layers) = count(io, LAYER_COUNT)? {
            firmware += &format!(", {layers} layers");
        }
        if let Some(macros) = count(io, MACRO_COUNT)? {
            firmware += &format!(", {macros} macros");
        }
        reading.firmware = Some(firmware);

        self.lights = match vial {
            Some((_, true)) => self.read_vialrgb(io, &mut problems)?.into_iter().collect(),
            Some((_, false)) => Vec::new(),
            None if CHANNEL_FORMAT.contains(&via) => self.read_channels(io, &mut problems)?,
            None => Vec::new(),
        };
        self.show(&mut reading);
        if !problems.is_empty() {
            reading.problem = Some(problems.join("; "));
        } else if self.lights.is_empty() {
            // Nothing PeriHub knows how to change: the page says read-only.
            io.forbid_changes();
        }
        Ok(reading)
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        if from.leds.len() != self.lights.len() || to.leds.len() != self.lights.len() {
            return Err(Error::Invalid("the keyboard's lighting changed since it was read".into()));
        }
        for (i, light) in self.lights.clone().iter().enumerate() {
            let (old, new) = (&from.leds[i], &to.leds[i]);
            let option = light.speed_option();
            let (old_speed, new_speed) = (from.options.get(option), to.options.get(option));
            if old == new && old_speed == new_speed {
                continue;
            }
            if changes_while_off(old, new, old_speed != new_speed) {
                return Err(Error::Invalid(
                    "with the lighting off, the keyboard ignores colour and brightness: turn it on first".into(),
                ));
            }
            let speed = new_speed.and_then(|&s| u8::try_from(s).ok()).ok_or_else(|| Error::Invalid("speed".into()))?;
            let raw = light.raw();
            let hs = if new.color != old.color { self.hs_of(new.color) } else { (raw.hue, raw.sat) };
            let brightness = match new.brightness {
                Some(percent) if new.brightness != old.brightness => self.raw_brightness(i, percent),
                _ => raw.brightness,
            };
            match light {
                Light::Channel { channel, .. } => {
                    // The effect first: with the light off, nothing else is applied.
                    if new.mode != old.mode {
                        Self::set_channel(io, channel, EFFECT, &[new.mode])?;
                    }
                    if new_speed != old_speed {
                        Self::set_channel(io, channel, SPEED, &[speed])?;
                    }
                    if channel.color && new.color != old.color {
                        Self::set_channel(io, channel, COLOR, &[hs.0, hs.1])?;
                    }
                    if new.brightness != old.brightness {
                        Self::set_channel(io, channel, BRIGHTNESS, &[brightness])?;
                    }
                }
                Light::VialRgb { supported, .. } => {
                    let effect = new.mode as u16;
                    if effect == VIALRGB_DIRECT || !supported.contains(&effect) {
                        return Err(Error::Invalid(format!("effect {effect}")));
                    }
                    let [lo, hi] = effect.to_le_bytes();
                    let reply = query(io, &[SET_VALUE, VIALRGB_MODE, lo, hi, speed, hs.0, hs.1, brightness])?;
                    if reply[0] != SET_VALUE {
                        return Err(Error::Protocol("the keyboard did not take the lighting".into()));
                    }
                }
            }
        }
        Ok(())
    }

    fn save(&mut self, io: &mut Io) -> Result<()> {
        let requests: Vec<[u8; 2]> = if self.p.vial {
            if self.lights.is_empty() { Vec::new() } else { vec![[SAVE, 0]] }
        } else {
            self.lights
                .iter()
                .filter_map(|l| match l {
                    Light::Channel { channel, .. } => Some([SAVE, channel.id]),
                    Light::VialRgb { .. } => None,
                })
                .collect()
        };
        for request in requests {
            let reply = query(io, &request)?;
            if reply[0] != SAVE {
                return Err(Error::Protocol("the keyboard did not store the lighting".into()));
            }
        }
        Ok(())
    }
}

/// HID report descriptors, read on Linux from sysfs before any rule opens a device.
pub mod descriptor {
    use super::{USAGE, USAGE_PAGE};

    /// The descriptor holds QMK's raw HID collection and nothing else: one application collection,
    /// usage page `0xFF60`, usage `0x61`, no report id, and no usage page other than `0xFF60`
    /// anywhere (no keyboard, keypad, consumer or system usage). Only such an interface may be
    /// opened by a rule: nothing typed on the keyboard can arrive there.
    pub fn is_raw_hid_only(descriptor: &[u8]) -> bool {
        let mut page: Option<u32> = None;
        let mut usage: Option<u32> = None;
        let mut depth = 0u32;
        let mut top = Vec::new();
        let mut i = 0;
        while i < descriptor.len() {
            let prefix = descriptor[i];
            if prefix == 0xFE {
                // A long item: never in a raw HID descriptor.
                return false;
            }
            let size = match prefix & 0x03 {
                3 => 4,
                n => n as usize,
            };
            let Some(bytes) = descriptor.get(i + 1..i + 1 + size) else { return false };
            let value = bytes.iter().rev().fold(0u32, |v, &b| (v << 8) | b as u32);
            match prefix & 0xFC {
                // Usage Page
                0x04 => {
                    if value != USAGE_PAGE as u32 {
                        return false;
                    }
                    page = Some(value);
                }
                // Report ID
                0x84 => return false,
                // Usage, with its page in the high bytes when it has four
                0x08 => {
                    if size == 4 && value >> 16 != USAGE_PAGE as u32 {
                        return false;
                    }
                    usage = Some(value & 0xFFFF);
                }
                // Collection
                0xA0 => {
                    if depth == 0 {
                        top.push((page, usage, value));
                    }
                    depth += 1;
                }
                // End Collection
                0xC0 => {
                    let Some(d) = depth.checked_sub(1) else { return false };
                    depth = d;
                }
                _ => {}
            }
            // Local items (the usage) last until the next main item.
            if (prefix >> 2) & 0x03 == 0 {
                usage = None;
            }
            i += 1 + size;
        }
        depth == 0 && top == [(Some(USAGE_PAGE as u32), Some(USAGE as u32), 1)]
    }
}

/// On Linux a hidraw node carries every collection of its interface, whatever rule opened it: a QMK
/// keyboard is opened only if its node's descriptor holds the raw HID collection and nothing else, so
/// that nothing typed can reach PeriHub. On Windows each collection is a device of its own.
pub fn node_carries_only_raw_hid(path: &str) -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    let Some(name) = path.strip_prefix("/dev/").filter(|n| n.starts_with("hidraw") && !n.contains('/')) else {
        return false;
    };
    std::fs::read(format!("/sys/class/hidraw/{name}/device/report_descriptor"))
        .is_ok_and(|d| descriptor::is_raw_hid_only(&d))
}

/// A USB interface that holds QMK's raw HID collection and nothing else, found in sysfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInterface {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: u8,
    pub name: String,
}

/// The raw HID interfaces of the USB devices under `class_dir` (`/sys/class/hidraw` on Linux), read
/// from each node's report descriptor, which every user may read without opening the device.
pub fn raw_interfaces(class_dir: &std::path::Path) -> Vec<RawInterface> {
    usb_interfaces(class_dir, descriptor::is_raw_hid_only)
}

/// The USB interfaces under `class_dir` whose report descriptor `accept` takes.
fn usb_interfaces(class_dir: &std::path::Path, accept: impl Fn(&[u8]) -> bool) -> Vec<RawInterface> {
    let Ok(entries) = std::fs::read_dir(class_dir) else { return Vec::new() };
    let mut found: Vec<RawInterface> = entries
        .flatten()
        .filter_map(|entry| {
            let device = std::fs::canonicalize(entry.path().join("device")).ok()?;
            let descriptor = std::fs::read(device.join("report_descriptor")).ok()?;
            if !accept(&descriptor) {
                return None;
            }
            let uevent = std::fs::read_to_string(device.join("uevent")).ok()?;
            let field = |key: &str| uevent.lines().find_map(|l| l.strip_prefix(key)).map(str::trim);
            // HID_ID=0003:0000VVVV:0000PPPP, bus 3 being USB.
            let mut id = field("HID_ID=")?.split(':');
            if id.next()? != "0003" {
                return None;
            }
            let vendor_id = u32::from_str_radix(id.next()?, 16).ok().and_then(|v| u16::try_from(v).ok())?;
            let product_id = u32::from_str_radix(id.next()?, 16).ok().and_then(|v| u16::try_from(v).ok())?;
            let number = std::fs::read_to_string(device.parent()?.join("bInterfaceNumber")).ok()?;
            let interface = u8::from_str_radix(number.trim(), 16).ok()?;
            let name = field("HID_NAME=").unwrap_or_default().to_string();
            Some(RawInterface { vendor_id, product_id, interface, name })
        })
        .collect();
    found.sort_by_key(|r| (r.vendor_id, r.product_id, r.interface));
    found.dedup();
    found
}

/// The optional rule file in `/etc/udev/rules.d`. Its name must sort before systemd's
/// `73-seat-late.rules`, which turns the `uaccess` tag into the user's access: a tag added by a file
/// read after it gives nothing.
pub const RULE_FILE: &str = "73-perihub-qmk.rules";

/// The optional udev rule for one keyboard: only the interface whose descriptor PeriHub checked.
pub fn rule_line(vendor_id: u16, product_id: u16, interface: u8) -> String {
    format!(
        "KERNEL==\"hidraw*\", ATTRS{{idVendor}}==\"{vendor_id:04x}\", ATTRS{{idProduct}}==\"{product_id:04x}\", \
         ENV{{ID_USB_INTERFACE_NUM}}==\"{interface:02x}\", TAG+=\"uaccess\""
    )
}

/// The whole optional rule file, for the keyboards given as (vendor id, product id, interface, name).
pub fn rules_file(keyboards: &[(u16, u16, u8, String)]) -> String {
    let mut text = String::from(
        "# PeriHub, optional: the raw HID interface of the QMK keyboards (VIA, Vial) you allowed in PeriHub.\n\
         # Each line opens only the interface whose report descriptor PeriHub checked: QMK's raw HID\n\
         # collection (usage page 0xFF60) and nothing else, so nothing typed on the keyboard arrives there.\n\
         # PeriHub adds and removes the lines from its QMK keyboards page.\n\n\
         KERNEL==\"hidraw*\", SUBSYSTEMS==\"usb\", ENV{ID_USB_INTERFACE_NUM}==\"\", IMPORT{builtin}=\"usb_id\"\n",
    );
    for (vendor_id, product_id, interface, name) in keyboards {
        let name: String = name.chars().filter(|c| !c.is_control()).collect();
        text += &format!("# {name}\n{}\n", rule_line(*vendor_id, *product_id, *interface));
    }
    text
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::task::start_for_test;
    use super::*;

    /// A simulated QMK keyboard: what its firmware has, and every request it got.
    #[derive(Clone)]
    struct Keyboard {
        vial: bool,
        via_version: u16,
        vialrgb: bool,
        /// Channel id → brightness (stored on the keyboard's scale), effect, speed, hue, sat.
        channels: Vec<(u8, [u8; 5])>,
        /// The keyboard's highest brightness, as QMK keeps it.
        max: u8,
        vialrgb_supported: Vec<u16>,
        vialrgb_mode: Raw,
        saved: Vec<Vec<u8>>,
        sent: Vec<Vec<u8>>,
        /// Answers every request with "unhandled".
        silent: bool,
    }

    type Shared = Arc<Mutex<Keyboard>>;

    fn via_keyboard() -> Keyboard {
        Keyboard {
            vial: false,
            via_version: 0x000D,
            vialrgb: false,
            channels: vec![(3, [200, 4, 127, 85, 255])],
            max: 255,
            vialrgb_supported: Vec::new(),
            vialrgb_mode: Raw::default(),
            saved: Vec::new(),
            sent: Vec::new(),
            silent: false,
        }
    }

    fn vial_keyboard() -> Keyboard {
        Keyboard {
            vial: true,
            via_version: 0x0009,
            vialrgb: true,
            channels: Vec::new(),
            max: 200,
            vialrgb_supported: vec![1, 2, 6, 13, 29],
            vialrgb_mode: Raw { effect: 2, speed: 100, hue: 170, sat: 255, brightness: 150 },
            saved: Vec::new(),
            sent: Vec::new(),
            silent: false,
        }
    }

    /// Answers as the firmware described in the spec does: the same 32 bytes, filled in.
    fn answer(k: &mut Keyboard, request: &[u8]) -> Vec<u8> {
        k.sent.push(request.to_vec());
        let mut r = request.to_vec();
        if k.silent {
            r[0] = UNHANDLED;
            return r;
        }
        match (r[0], r[1]) {
            (PROTOCOL_VERSION, _) => r[1..3].copy_from_slice(&k.via_version.to_be_bytes()),
            (LAYER_COUNT, _) => r[1] = 4,
            (MACRO_COUNT, _) => r[1] = 16,
            (VIAL, VIAL_KEYBOARD_ID) if k.vial => {
                r.fill(0);
                r[0] = 6;
                r[4..12].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
                r[12] = k.vialrgb as u8;
            }
            (GET_VALUE, VIALRGB_INFO) if k.vialrgb => {
                r[2..5].copy_from_slice(&[1, 0, k.max]);
            }
            (GET_VALUE, VIALRGB_SUPPORTED) if k.vialrgb => {
                let after = u16::from_le_bytes([r[2], r[3]]);
                r[2..].fill(0xFF);
                for (slot, id) in k.vialrgb_supported.iter().filter(|&&id| id > after).take(15).enumerate() {
                    r[2 + slot * 2..4 + slot * 2].copy_from_slice(&id.to_le_bytes());
                }
            }
            (GET_VALUE, VIALRGB_MODE) if k.vialrgb => {
                let m = k.vialrgb_mode;
                r[2..4].copy_from_slice(&m.effect.to_le_bytes());
                r[4..8].copy_from_slice(&[m.speed, m.hue, m.sat, m.brightness]);
            }
            (SET_VALUE, VIALRGB_MODE) if k.vialrgb => {
                let effect = u16::from_le_bytes([r[2], r[3]]);
                k.vialrgb_mode.effect = effect;
                k.vialrgb_mode.speed = r[4];
                // QMK ignores colour and brightness while the light is off.
                if effect != 0 {
                    k.vialrgb_mode.hue = r[5];
                    k.vialrgb_mode.sat = r[6];
                    k.vialrgb_mode.brightness = r[7].min(k.max);
                }
            }
            (SAVE, _) if k.vial => k.saved.push(r.clone()),
            (GET_VALUE | SET_VALUE | SAVE, ch) if !k.vial => {
                let max = k.max;
                let Some((_, v)) = k.channels.iter_mut().find(|(id, _)| *id == ch) else {
                    r[0] = UNHANDLED;
                    return r;
                };
                match (r[0], r[2]) {
                    (GET_VALUE, BRIGHTNESS) => r[3] = (v[0] as u16 * 255 / max as u16) as u8,
                    (GET_VALUE, EFFECT) => r[3] = v[1],
                    (GET_VALUE, SPEED) => r[3] = v[2],
                    (GET_VALUE, COLOR) => r[3..5].copy_from_slice(&v[3..5]),
                    (SET_VALUE, EFFECT) => v[1] = r[3],
                    (SET_VALUE, SPEED) => v[2] = r[3],
                    // QMK's scale8, and nothing applied while the light is off.
                    (SET_VALUE, BRIGHTNESS) if v[1] != 0 => v[0] = ((r[3] as u16 * (max as u16 + 1)) >> 8) as u8,
                    (SET_VALUE, COLOR) if v[1] != 0 => v[3..5].copy_from_slice(&r[3..5]),
                    (SAVE, _) => k.saved.push(r.clone()),
                    _ => {}
                }
            }
            _ => r[0] = UNHANDLED,
        }
        r
    }

    fn fake(state: Shared) -> FakeHid {
        FakeHid::new(move |request: &[u8]| {
            assert_eq!(request.len(), REPORT, "32-byte reports");
            vec![answer(&mut state.lock().unwrap(), request)]
        })
    }

    fn model(k: &Keyboard) -> &'static Model {
        if k.vial { &VIAL_MODEL } else { &VIA_MODEL }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-qmk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn start(k: Keyboard, changes: bool, name: &str) -> (super::super::task::PeripheralTask, Shared) {
        let m = model(&k);
        let state = Arc::new(Mutex::new(k));
        let task = start_for_test(Box::new(fake(state.clone())), m, 0x0001, changes, temp(name)).unwrap();
        (task, state)
    }

    fn only_reads(state: &Shared) -> bool {
        state.lock().unwrap().sent.iter().all(|r| ![SET_VALUE, SAVE].contains(&r[0]))
    }

    #[test]
    fn a_via_keyboard_reads_as_documented_and_only_reads() {
        let (task, state) = start(via_keyboard(), false, "via-read");
        assert_eq!(task.info.firmware.as_deref(), Some("VIA protocol 13, 4 layers, 16 macros"));
        assert_eq!(task.info.kind, Kind::Keyboard);
        let led = task.info.settings.leds[0];
        assert_eq!((led.mode, led.brightness, led.color), (4, Some(80), hs_to_rgb(85, 255)));
        assert_eq!(task.info.settings.options["rgb_matrix_speed"], 127);
        assert_eq!(task.info.limits.leds[0].modes, [(0, "Off"), (4, "Effect 4")]);
        assert!(task.info.problem.is_none() && !task.info.editable);
        assert!(only_reads(&state));
    }

    #[test]
    fn only_via_12_and_13_have_their_lighting_read() {
        for version in [0x0009, 0x000A, 0x000B, 0x000E] {
            let mut k = via_keyboard();
            k.via_version = version;
            let (task, state) = start(k, true, "versions");
            assert!(task.info.settings.leds.is_empty(), "{version:04x}");
            assert!(!task.info.editable, "{version:04x}: nothing to change, so read-only");
            assert!(state.lock().unwrap().sent.iter().all(|r| r[0] != GET_VALUE), "{version:04x}");
        }
    }

    #[test]
    fn a_change_is_sent_effect_first_read_back_and_undone_unless_kept() {
        let (mut task, state) = start(via_keyboard(), true, "via-change");
        let before = state.lock().unwrap().channels.clone();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb::new(255, 0, 0);
        wanted.leds[0].brightness = Some(50);
        wanted.options.insert("rgb_matrix_speed".into(), 64);
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(info.settings.leds[0].color, Rgb::new(255, 0, 0));
        assert_eq!(info.settings.leds[0].brightness, Some(50));
        let sets: Vec<u8> = state.lock().unwrap().sent.iter().filter(|r| r[0] == SET_VALUE).map(|r| r[2]).collect();
        assert_eq!(sets, [SPEED, COLOR, BRIGHTNESS]);
        assert!(state.lock().unwrap().saved.is_empty(), "Apply does not save");
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().channels, before, "every value as it was");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn brightness_that_comes_back_a_step_off_still_reads_as_set() {
        let mut k = via_keyboard();
        k.max = 120;
        k.channels = vec![(3, [60, 4, 127, 0, 0])];
        let (mut task, state) = start(k, true, "via-scale");
        for percent in [0, 10, 30, 70, 100] {
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].brightness = Some(percent);
            let info = task.change(&wanted, false, None).unwrap();
            assert_eq!(info.settings.leds[0].brightness, Some(percent));
            task.keep_changes();
        }
        assert!(state.lock().unwrap().saved.is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn turning_the_light_off_and_on_again() {
        let (mut task, state) = start(via_keyboard(), true, "via-off");
        let mut off = task.info.settings.clone();
        off.leds[0].mode = 0;
        task.change(&off, false, None).unwrap();
        task.keep_changes();
        assert_eq!(state.lock().unwrap().channels[0].1[1], 0);
        // Off, the effect last seen on is still offered.
        assert_eq!(task.info.limits.leds[0].modes, [(0, "Off"), (4, "Effect 4")]);
        let mut changed_while_off = task.info.settings.clone();
        changed_while_off.leds[0].brightness = Some(20);
        assert!(task.change(&changed_while_off, false, None).is_err());
        let mut on = task.info.settings.clone();
        on.leds[0].mode = 4;
        on.leds[0].color = Rgb::new(0, 0, 255);
        task.change(&on, false, None).unwrap();
        assert_eq!(state.lock().unwrap().channels[0].1[1], 4);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn save_to_memory_stores_each_channel_and_its_undo_stores_again() {
        let mut k = via_keyboard();
        k.channels.push((2, [255, 1, 0, 0, 0]));
        let (mut task, state) = start(k, true, "via-save");
        assert_eq!(task.info.settings.leds.len(), 2);
        let mut wanted = task.info.settings.clone();
        wanted.leds[1].mode = 0;
        task.change(&wanted, true, None).unwrap();
        let saved: Vec<Vec<u8>> = state.lock().unwrap().saved.iter().map(|r| r[..2].to_vec()).collect();
        assert_eq!(saved, [vec![SAVE, 3], vec![SAVE, 2]]);
        task.undo_last_change().unwrap();
        assert_eq!(state.lock().unwrap().saved.len(), 4);
        assert_eq!(state.lock().unwrap().channels[1].1[1], 1);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_colour_put_back_is_the_hue_and_saturation_read() {
        let mut k = via_keyboard();
        // White: every hue reads the same, the undo must give back hue 200, not another.
        k.channels = vec![(3, [255, 4, 127, 200, 0])];
        let (mut task, state) = start(k, true, "via-hue");
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb::new(0, 255, 0);
        task.change(&wanted, false, None).unwrap();
        task.undo_last_change().unwrap();
        assert_eq!(state.lock().unwrap().channels[0].1[3..5], [200, 0]);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_vial_keyboard_with_vialrgb() {
        let (mut task, state) = start(vial_keyboard(), true, "vial");
        assert_eq!(task.info.firmware.as_deref(), Some("Vial protocol 6, VIA protocol 9, 4 layers, 16 macros"));
        let led = task.info.settings.leds[0];
        assert_eq!((led.mode, led.brightness), (2, Some(75)));
        let modes: Vec<u8> = task.info.limits.leds[0].modes.iter().map(|(m, _)| *m).collect();
        assert_eq!(modes, [0, 2, 6, 13, 29], "Off, the supported effects, never Direct");
        assert_eq!(task.info.limits.leds[0].modes[3], (13, "Cycle all"));
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 6;
        wanted.leds[0].brightness = Some(40);
        task.change(&wanted, false, None).unwrap();
        let m = state.lock().unwrap().vialrgb_mode;
        assert_eq!(m, Raw { effect: 6, speed: 100, hue: 170, sat: 255, brightness: 80 });
        task.undo_last_change().unwrap();
        assert_eq!(state.lock().unwrap().vialrgb_mode, vial_keyboard().vialrgb_mode, "exactly as read");
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 13;
        task.change(&wanted, true, None).unwrap();
        assert_eq!(state.lock().unwrap().saved.len(), 1);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_vial_keyboard_without_vialrgb_is_read_only() {
        let mut k = vial_keyboard();
        k.vialrgb = false;
        let (task, state) = start(k, true, "vial-plain");
        assert!(task.info.settings.leds.is_empty() && !task.info.editable && task.info.problem.is_none());
        assert!(state.lock().unwrap().sent.iter().all(|r| r[0] != GET_VALUE));
    }

    #[test]
    fn effects_vial_does_not_have_and_direct_are_refused() {
        let (mut task, state) = start(vial_keyboard(), true, "vial-refuse");
        for effect in [1, 3, 44, 45] {
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].mode = effect;
            assert!(task.change(&wanted, false, None).is_err(), "{effect}");
        }
        assert!(only_reads(&state));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_keyboard_that_does_not_answer_via_gets_a_read_only_page() {
        let mut k = via_keyboard();
        k.silent = true;
        let (task, state) = start(k, true, "silent");
        assert!(task.info.problem.as_deref().unwrap().contains("VIA"));
        assert!(!task.info.editable);
        assert_eq!(state.lock().unwrap().sent.len(), 1, "only the version was asked");
    }

    #[test]
    fn commands_outside_the_lists_never_leave() {
        let state = Arc::new(Mutex::new(via_keyboard()));
        let refused: [&[u8]; 16] = [
            &[0x02, 0x03],       // state of the key matrix
            &[0x02, 0x01],       // any keyboard value
            &[0x03, 0x05],       // device indication
            &[0x05, 0, 0, 0],    // keymap: one key
            &[0x06],             // keymap reset
            &[0x0A],             // memory reset
            &[0x0B],             // bootloader jump
            &[0x0F, 0, 0],       // macro write
            &[0x10],             // macro reset
            &[0x13, 0, 0],       // keymap write
            &[0x15],             // encoder write
            &[0x07, 0x00, 0x01], // the keyboard's own channel
            &[0x07, 0x01, 0x02], // backlight breathing
            &[0x09, 0x00],       // the keyboard's own channel
            &[0xFE, 0x06],       // Vial unlock
            &[0xFE, 0x0C],       // Vial settings reset
        ];
        for (commands, vial) in [(VIA_COMMANDS, false), (VIAL_COMMANDS, true)] {
            let mut io = Io::new(Box::new(fake(state.clone())), commands.to_vec(), true);
            for request in refused {
                let mut payload = [0u8; REPORT];
                payload[..request.len()].copy_from_slice(request);
                if vial && request == [0x09, 0x00] {
                    continue; // Vial's lighting save
                }
                let mut reply = [0u8; REPORT];
                assert!(io.output_query(&payload, &mut reply, TIMEOUT).is_err(), "{request:02x?}");
            }
            for request in [[0xFE, 0x07], [0xFE, 0x08]] {
                let mut payload = [0u8; REPORT];
                payload[..2].copy_from_slice(&request);
                let mut reply = [0u8; REPORT];
                assert!(io.output_query(&payload, &mut reply, TIMEOUT).is_err());
            }
        }
        assert!(state.lock().unwrap().sent.is_empty());
    }

    #[test]
    fn serial_numbers_and_collections() {
        assert_eq!(classify(USAGE_PAGE, USAGE, Some("vial:f64c2b3c")), Found::Vial);
        assert_eq!(classify(USAGE_PAGE, USAGE, Some("ABC vial:f64c2b3c")), Found::Vial);
        assert_eq!(classify(USAGE_PAGE, USAGE, Some("1234")), Found::Via);
        assert_eq!(classify(USAGE_PAGE, USAGE, None), Found::Via);
        assert_eq!(classify(USAGE_PAGE, USAGE, Some("vibl:d4f8159c")), Found::Bootloader);
        assert_eq!(classify(0x0001, 0x06, Some("vibl:d4f8159c")), Found::Bootloader);
        assert_eq!(classify(0x0001, 0x06, Some("vial:f64c2b3c")), Found::No, "the keyboard's typing interface");
        assert_eq!(classify(USAGE_PAGE, 0x62, Some("vial:f64c2b3c")), Found::No);
    }

    #[test]
    fn names_from_the_usb_strings() {
        assert_eq!(names(Some("Keychron"), Some("Keychron Q1 Pro"), &VIA_MODEL), ("Keychron".into(), "Q1 Pro".into()));
        assert_eq!(names(Some("foostan"), Some("Corne"), &VIAL_MODEL), ("foostan".into(), "Corne".into()));
        assert_eq!(names(None, Some("Corne"), &VIAL_MODEL), ("Vial".into(), "Corne".into()));
        assert_eq!(names(Some(" "), None, &VIA_MODEL), ("VIA".into(), "keyboard".into()));
        assert_eq!(names(Some("Keychron"), Some("Keychron"), &VIA_MODEL), ("Keychron".into(), "Keychron".into()));
    }

    /// QMK's raw HID descriptor: usage page 0xFF60, usage 0x61, 32 bytes in and out.
    const RAW: [u8; 34] = [
        0x06, 0x60, 0xFF, 0x09, 0x61, 0xA1, 0x01, 0x09, 0x62, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20, 0x75, 0x08,
        0x81, 0x02, 0x09, 0x63, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20, 0x75, 0x08, 0x91, 0x02, 0xC0,
    ];

    #[test]
    fn only_a_raw_hid_descriptor_may_be_opened_by_a_rule() {
        assert!(descriptor::is_raw_hid_only(&RAW));
        // A boot keyboard: what is typed.
        let keyboard = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x81, 0x02, 0xC0];
        assert!(!descriptor::is_raw_hid_only(&keyboard));
        // Raw HID sharing an interface with a keyboard collection.
        let shared: Vec<u8> = RAW.iter().chain(&keyboard).copied().collect();
        assert!(!descriptor::is_raw_hid_only(&shared));
        let shared: Vec<u8> = keyboard.iter().chain(&RAW).copied().collect();
        assert!(!descriptor::is_raw_hid_only(&shared));
        // With a report id, another usage, a usage of another page, unbalanced or cut short.
        let mut with_id = RAW.to_vec();
        with_id.splice(7..7, [0x85, 0x01]);
        assert!(!descriptor::is_raw_hid_only(&with_id));
        let mut other_usage = RAW;
        other_usage[4] = 0x62;
        assert!(!descriptor::is_raw_hid_only(&other_usage));
        let mut long_usage = RAW.to_vec();
        long_usage.splice(7..9, [0x0B, 0xE0, 0x00, 0x07, 0x00]);
        assert!(!descriptor::is_raw_hid_only(&long_usage));
        assert!(!descriptor::is_raw_hid_only(&RAW[..RAW.len() - 1]));
        assert!(!descriptor::is_raw_hid_only(&RAW[..10]));
        assert!(!descriptor::is_raw_hid_only(&[]));
    }

    #[cfg(unix)]
    #[test]
    fn raw_interfaces_are_found_by_their_descriptor_in_sysfs() {
        let root = temp("sysfs");
        let usb = root.join("devices/usb1/1-2");
        let keyboard: &[u8] =
            &[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x81, 0x02, 0xC0];
        for (n, number, descriptor) in [(0, "00", keyboard), (1, "01", &RAW[..])] {
            let interface = usb.join(format!("1-2:1.{n}"));
            let hid = interface.join(format!("0003:3434:0311.000{n}"));
            std::fs::create_dir_all(&hid).unwrap();
            std::fs::write(interface.join("bInterfaceNumber"), format!("{number}\n")).unwrap();
            std::fs::write(hid.join("report_descriptor"), descriptor).unwrap();
            std::fs::write(hid.join("uevent"), "HID_ID=0003:00003434:00000311\nHID_NAME=Keychron Q1 Pro\n").unwrap();
            let node = root.join(format!("class/hidraw{n}"));
            std::fs::create_dir_all(&node).unwrap();
            std::os::unix::fs::symlink(&hid, node.join("device")).unwrap();
        }
        let found = raw_interfaces(&root.join("class"));
        assert_eq!(
            found,
            [RawInterface { vendor_id: 0x3434, product_id: 0x0311, interface: 1, name: "Keychron Q1 Pro".into() }]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_node_is_opened_only_with_a_raw_hid_descriptor() {
        assert!(!node_carries_only_raw_hid("/dev/hidraw../../etc"));
        assert!(!node_carries_only_raw_hid("/tmp/hidraw0"));
        assert!(!node_carries_only_raw_hid("/dev/hidraw999"));
        // The nodes of this machine: none is a QMK keyboard's raw HID interface.
        for entry in std::fs::read_dir("/sys/class/hidraw").into_iter().flatten().flatten() {
            let path = format!("/dev/{}", entry.file_name().to_string_lossy());
            let descriptor = std::fs::read(entry.path().join("device/report_descriptor")).unwrap_or_default();
            assert_eq!(node_carries_only_raw_hid(&path), descriptor::is_raw_hid_only(&descriptor), "{path}");
        }
    }

    #[test]
    fn the_optional_rule_is_read_before_systemd_gives_access() {
        assert!(RULE_FILE < "73-seat-late.rules");
        assert!(RULE_FILE.ends_with(".rules"));
    }

    #[test]
    fn the_optional_rule_pins_the_interface() {
        let file = rules_file(&[(0x3434, 0x0311, 1, "Keychron Q1\nPro".into())]);
        let rules: Vec<&str> = file.lines().filter(|l| l.contains("TAG+=")).collect();
        assert_eq!(
            rules,
            [
                r#"KERNEL=="hidraw*", ATTRS{idVendor}=="3434", ATTRS{idProduct}=="0311", ENV{ID_USB_INTERFACE_NUM}=="01", TAG+="uaccess""#
            ]
        );
        assert!(file.contains("# Keychron Q1Pro\n"), "a name cannot add a line");
        assert!(file.contains(r#"IMPORT{builtin}="usb_id""#));
    }
}

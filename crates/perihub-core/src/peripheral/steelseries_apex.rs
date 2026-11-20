//! SteelSeries Apex Pro (1038:1610), Apex 7 (1612), Apex 7 TKL (1618) and Apex 5 (161c): the first-generation
//! protocol of the vendor collection on USB interface 1 (usage page ffc0). The protocol is in
//! `docs/protocols/steelseries-apex.md`, from facts found in OpenRGB (GPL: no code copied) and, for the OLED
//! display, apex-tux (Unlicense). Nobody has tried one with PeriHub.
//!
//! What it reads: the two firmware versions and the brightness. What it changes, as an experimental feature:
//! the brightness (read back, backed up and undone like every other change), and, for the session only, the
//! colours of the keys, the display and the way back to the keyboard's own lighting. Those cannot be read, so
//! they are sent one at a time under a warning. Nothing is stored in the keyboard: no command that stores
//! anything is listed. The commands of the later generations, and the serial number, are never sent.
//!
//! Only interface 1 is opened: the others carry what is typed.

use std::time::Duration;

use super::task::Driver;
use super::{
    BlindKind, BlindSetting, BlindValue, Command, Effect, Io, Kind, Led, LedLimits, Limits, Model, Protocol, Reading,
    Settings,
};
use crate::color::Rgb;
use crate::error::{Error, Result};

const STEELSERIES: u16 = 0x1038;
const INTERFACE: i32 = 1;
const USAGE_PAGE: u16 = 0xFFC0;

/// An output report is 64 bytes after the report number, and a feature report 642.
const OUTPUT_LEN: usize = 64;
const FEATURE_LEN: usize = 642;
/// OpenRGB waits 2 ms for the firmware and 100 ms for the brightness.
const TIMEOUT: Duration = Duration::from_millis(300);
/// The keyboard needs a moment before the brightness it was given can be read.
const SETTLE: Duration = Duration::from_millis(100);

const READ_VERSION: u8 = 0x90;
const READ_BRIGHTNESS: u8 = 0xA3;
const SET_BRIGHTNESS: u8 = 0x23;
const DIRECT: u8 = 0x3A;
const ONBOARD: u8 = 0x3B;
const DISPLAY: u8 = 0x61;
/// The brightness goes from 0 to 10, and the page shows tens of percent.
const MAX_BRIGHTNESS: u8 = 10;
/// What the answer of a keyboard without the brightness has in its second byte.
const UNSUPPORTED: u8 = 0xFF;

/// Everything PeriHub may send: three reads, and the changes of the module's documentation.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &[READ_VERSION, 0x00], at: 0, effect: Effect::Read, name: "read keyboard firmware" },
    Command { prefix: &[READ_VERSION, 0x01], at: 0, effect: Effect::Read, name: "read LED firmware" },
    Command { prefix: &[READ_BRIGHTNESS], at: 0, effect: Effect::Read, name: "read brightness" },
    Command { prefix: &[SET_BRIGHTNESS], at: 0, effect: Effect::Change, name: "brightness" },
    Command { prefix: &[DIRECT], at: 0, effect: Effect::Change, name: "colours of the keys" },
    Command { prefix: &[ONBOARD], at: 0, effect: Effect::Change, name: "back to the keyboard's own lighting" },
    Command { prefix: &[DISPLAY], at: 0, effect: Effect::Change, name: "picture on the display" },
];

#[derive(Debug, Clone, Copy)]
pub struct Params;

const fn apex(product_ids: &'static [u16], name: &'static str) -> Model {
    Model {
        vendor_id: STEELSERIES,
        product_ids,
        brand: "SteelSeries",
        name,
        kind: Kind::Keyboard,
        interface: Some(INTERFACE),
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::SteelSeriesApex(Params),
    }
}

/// The models OpenRGB gives this protocol and that are not told apart by their firmware: the Apex Pro TKL is, and the
/// Apex Pro 3 has another picture for its display.
pub const MODELS: &[Model] =
    &[apex(&[0x1610], "Apex Pro"), apex(&[0x1612], "Apex 7"), apex(&[0x1618], "Apex 7 TKL"), apex(&[0x161C], "Apex 5")];

/// The keys of a colour frame, as OpenRGB sends them: HID usages, 112 of them. The keyboard picks the ones it has.
#[rustfmt::skip]
pub const KEYS: [u8; 112] = [
    0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x32, 0x33, 0x34,
    0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C,
    0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52, 0x64, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4,
    0xE5, 0xE6, 0xE7, 0xF0, 0x31, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x53, 0x54,
    0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60,
    0x61, 0x62, 0x63, 0xFB,
];

/// Groups of keys that get one colour each: id and label.
pub const GROUPS: [(&str, &str); 7] = [
    ("keys-letters", "Letters"),
    ("keys-numbers", "Number row"),
    ("keys-functions", "Function keys (F1 to F12)"),
    ("keys-navigation", "Navigation keys and arrows"),
    ("keys-numpad", "Number pad"),
    ("keys-modifiers", "Modifiers and the SteelSeries key"),
    ("keys-other", "Other keys (Esc, Enter, Space, punctuation…)"),
];
const ALL_ID: &str = "colour-all";
const ONBOARD_ID: &str = "lighting";
const ONBOARD_NAME: &str = "Back to the keyboard's own lighting";
const DISPLAY_ID: &str = "display";
const DISPLAY_CLEAR: &str = "Clear the display";
const DISPLAY_FILL: &str = "Light every pixel";
const DISPLAY_TEST: &str = "Test pattern (frame, a big square top left, a small one top right)";

/// The index in [GROUPS] of the group a key is in.
pub fn group_of(usage: u8) -> usize {
    match usage {
        0x04..=0x1D => 0,
        0x1E..=0x27 => 1,
        0x3A..=0x45 => 2,
        0x46..=0x52 => 3,
        0x53..=0x63 => 4,
        0xE0..=0xE7 | 0xF0 => 5,
        _ => 6,
    }
}

/// The feature report that gives every key the colour of its group.
fn colour_frame(groups: &[Rgb; GROUPS.len()]) -> Vec<u8> {
    let mut frame = vec![0u8; FEATURE_LEN];
    frame[0] = DIRECT;
    frame[1] = KEYS.len() as u8;
    for (i, &key) in KEYS.iter().enumerate() {
        let at = 2 + 4 * i;
        frame[at] = key;
        frame[at + 1..at + 4].copy_from_slice(&groups[group_of(key)].to_bytes());
    }
    frame
}

const WIDTH: usize = 128;
const HEIGHT: usize = 40;

/// The feature report with a picture of the display: 128 x 40 pixels, one bit each, row by row from the top, the
/// leftmost pixel in the highest bit of a byte, after the command and before a last zero.
fn picture(lit: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    let mut report = vec![0u8; FEATURE_LEN];
    report[0] = DISPLAY;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if lit(x, y) {
                report[1 + (y * WIDTH + x) / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    report
}

/// A frame around the display, a big square in its top left corner and a small one in the top right: shows at a
/// glance whether the picture came out mirrored or upside down.
fn test_pattern(x: usize, y: usize) -> bool {
    let border = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
    let big = (4..20).contains(&x) && (4..20).contains(&y);
    let small = (WIDTH - 12..WIDTH - 4).contains(&x) && (4..12).contains(&y);
    border || big || small
}

/// The text of a version answer: after the echo of the command if the answer has one, up to the first zero. A version
/// has at least two dots, as in OpenRGB's reading of it, and is printable.
fn version(reply: &[u8]) -> Option<String> {
    let body = reply.strip_prefix(&[READ_VERSION]).unwrap_or(reply);
    let body = &body[body.iter().position(|&b| b != 0)?..];
    let text = &body[..body.iter().position(|&b| b == 0).unwrap_or(body.len())];
    let printable = text.iter().all(|b| (0x20..0x7F).contains(b));
    (printable && text.len() <= 32 && text.iter().filter(|&&b| b == b'.').count() >= 2)
        .then(|| String::from_utf8_lossy(text).into_owned())
}

/// What the brightness read answered.
#[derive(Debug, PartialEq, Eq)]
enum Brightness {
    Level(u8),
    /// The keyboard has no brightness to read: OpenRGB then leaves it alone.
    Unsupported,
    Odd,
}

fn brightness(reply: &[u8]) -> Brightness {
    match (reply[1], reply[2]) {
        (UNSUPPORTED, _) => Brightness::Unsupported,
        (0, level) if level <= MAX_BRIGHTNESS => Brightness::Level(level),
        _ => Brightness::Odd,
    }
}

/// A request, padded with zeros to a whole output report.
fn request(bytes: &[u8]) -> [u8; OUTPUT_LEN] {
    let mut report = [0u8; OUTPUT_LEN];
    report[..bytes.len()].copy_from_slice(bytes);
    report
}

pub struct Apex {
    /// The firmware, read once: the keyboard and the LED controller.
    versions: Option<(String, Option<String>)>,
    /// The keyboard answered the brightness read when it connected.
    has_brightness: bool,
    /// The colour of each group as it was sent: the keyboard cannot be asked what it shows. A group never sent is dark.
    groups: [Rgb; GROUPS.len()],
}

impl Apex {
    pub fn new(_: Params) -> Self {
        Self { versions: None, has_brightness: false, groups: [Rgb::BLACK; GROUPS.len()] }
    }

    fn ask(io: &mut Io, bytes: &[u8], answers: impl Fn(&[u8]) -> bool) -> Result<[u8; OUTPUT_LEN]> {
        let mut reply = [0u8; OUTPUT_LEN];
        io.output_exchange(&request(bytes), &mut reply, TIMEOUT, answers)?;
        Ok(reply)
    }

    /// The firmware, asked twice if the first request got no answer. The LED controller's version is a bonus.
    fn read_versions(io: &mut Io) -> Result<Option<(String, Option<String>)>> {
        let is_version = |r: &[u8]| r[0] == READ_VERSION || r[0].is_ascii_digit();
        let mut keyboard = None;
        for _ in 0..2 {
            match Self::ask(io, &[READ_VERSION, 0x00], is_version) {
                Ok(reply) => {
                    keyboard = Some(reply);
                    break;
                }
                Err(Error::Timeout) => {}
                Err(e) => return Err(e),
            }
        }
        let Some(keyboard) = keyboard else { return Ok(None) };
        let Some(text) = version(&keyboard) else {
            return Err(Error::Protocol(format!(
                "the firmware answer {} does not look like a version",
                super::hex(&keyboard[..16])
            )));
        };
        let led = match Self::ask(io, &[READ_VERSION, 0x01], is_version) {
            Ok(reply) => version(&reply),
            Err(Error::Timeout) => None,
            Err(e) => return Err(e),
        };
        Ok(Some((text, led)))
    }

    fn send_output(io: &mut Io, bytes: &[u8]) -> Result<()> {
        io.send(&request(bytes), false)
    }

    fn send_blind_choice(&self, io: &mut Io, id: &str, name: &str) -> Result<()> {
        match (id, name) {
            (ONBOARD_ID, ONBOARD_NAME) => Self::send_output(io, &[ONBOARD]),
            (DISPLAY_ID, DISPLAY_CLEAR) => io.send(&picture(|_, _| false), true),
            (DISPLAY_ID, DISPLAY_FILL) => io.send(&picture(|_, _| true), true),
            (DISPLAY_ID, DISPLAY_TEST) => io.send(&picture(test_pattern), true),
            _ => Err(Error::Invalid(format!("{name} is not a value of {id} on this keyboard"))),
        }
    }
}

impl Driver for Apex {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "OpenRGB (facts only) and apex-tux"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn blind_settings(&self) -> Vec<BlindSetting> {
        let color = |id, label| BlindSetting::new(id, label, BlindKind::Color);
        let mut settings = vec![color(ALL_ID, "Whole keyboard, one colour")];
        settings.extend(GROUPS.iter().map(|&(id, label)| color(id, label)));
        settings.push(BlindSetting::new(
            ONBOARD_ID,
            "Lighting",
            BlindKind::Choice { values: vec![ONBOARD_NAME.to_string()] },
        ));
        settings.push(BlindSetting::new(
            DISPLAY_ID,
            "Display",
            BlindKind::Choice {
                values: vec![DISPLAY_CLEAR.to_string(), DISPLAY_FILL.to_string(), DISPLAY_TEST.to_string()],
            },
        ));
        settings
    }

    fn send_blind(&mut self, io: &mut Io, id: &str, value: &BlindValue) -> Result<()> {
        log::info!("SteelSeries Apex: {id} ← {value:?}");
        match (id, value) {
            (ALL_ID, BlindValue::Color(color)) => {
                let groups = [*color; GROUPS.len()];
                io.send(&colour_frame(&groups), true)?;
                self.groups = groups;
            }
            (group, BlindValue::Color(color)) if GROUPS.iter().any(|(g, _)| *g == group) => {
                let index = GROUPS.iter().position(|(g, _)| *g == group).expect("found above");
                let mut groups = self.groups;
                groups[index] = *color;
                io.send(&colour_frame(&groups), true)?;
                self.groups = groups;
            }
            (id, BlindValue::Choice(name)) => {
                self.send_blind_choice(io, id, name)?;
                // The keyboard's own lighting is back: the colours sent before are gone with it.
                if id == ONBOARD_ID {
                    self.groups = [Rgb::BLACK; GROUPS.len()];
                }
            }
            (id, _) => {
                return Err(Error::Invalid(format!("{id} is not a setting of this keyboard, or not of this kind")));
            }
        }
        Ok(())
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        // For the test report, at every pass.
        let _ = io.report_descriptor();
        let first = self.versions.is_none();
        if first {
            match Self::read_versions(io) {
                Ok(Some(versions)) => self.versions = Some(versions),
                // Off, or busy with another program: the page and its test report show it, and it is read again every few
                // seconds.
                Ok(None) => {
                    io.forbid_changes();
                    return Ok(Reading {
                        details: vec![(
                            "Status".into(),
                            "no answer yet: if the keyboard is plugged in, is SteelSeries GG closed? Please send a test report"
                                .into(),
                        )],
                        ..Default::default()
                    });
                }
                Err(Error::Protocol(problem)) => {
                    return Ok(Reading { problem: Some(problem), ..Default::default() });
                }
                Err(e) => return Err(e),
            }
        }
        let (firmware, led) = self.versions.clone().expect("read above");
        let mut details = vec![("Protocol".to_string(), "first generation (colours in feature report 3a)".to_string())];
        if let Some(led) = led {
            details.push(("LED controller firmware".into(), led));
        }
        let mut reading = Reading { firmware: Some(firmware), ..Default::default() };
        // The brightness is asked every time (the keys of the keyboard change it), once it answered.
        if first || self.has_brightness {
            match Self::ask(io, &[READ_BRIGHTNESS], |r| r[0] == READ_BRIGHTNESS) {
                Ok(reply) => match brightness(&reply) {
                    Brightness::Level(level) => {
                        self.has_brightness = true;
                        reading.settings.leds =
                            vec![Led { mode: 0, brightness: Some(level * 10), color: Rgb::default() }];
                        reading.limits = Limits {
                            leds: vec![LedLimits {
                                name: "Backlight".into(),
                                modes: Vec::new(),
                                brightness_step: Some(10),
                                color: false,
                            }],
                            ..Default::default()
                        };
                    }
                    Brightness::Unsupported => {
                        details.push(("Brightness".into(), "not read: the keyboard says it has none".into()))
                    }
                    Brightness::Odd => {
                        reading.problem = Some(format!(
                            "the brightness answer {} does not look as documented (a3 00 and a level from 0 to 10)",
                            super::hex(&reply[..4])
                        ))
                    }
                },
                // A keyboard that answered before and stops is gone: the manager opens it again when it is back.
                Err(Error::Timeout) if self.has_brightness => return Err(Error::Timeout),
                Err(Error::Timeout) => details.push(("Brightness".into(), "not read: no answer".into())),
                Err(e) => return Err(e),
            }
        }
        reading.details = details;
        Ok(reading)
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let (Some(old), Some(new)) = (from.leds.first(), to.leds.first()) else {
            return Err(Error::Invalid("this keyboard has only its brightness to change".into()));
        };
        if from.leds.len() != 1 || to.leds.len() != 1 || from.dpi_levels != to.dpi_levels || from.options != to.options
        {
            return Err(Error::Invalid("this keyboard has only its brightness to change".into()));
        }
        if new.brightness != old.brightness {
            let percent = new.brightness.ok_or_else(|| Error::Invalid("brightness".into()))?;
            if percent % 10 != 0 || percent / 10 > MAX_BRIGHTNESS {
                return Err(Error::Invalid(format!("brightness {percent} %: in tens, up to 100")));
            }
            Self::send_output(io, &[SET_BRIGHTNESS, percent / 10])?;
            std::thread::sleep(SETTLE);
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("PeriHub does not save anything to the memory of this keyboard".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::task::start_for_test;
    use super::super::{find, opened_on_linux};
    use super::*;

    struct State {
        level: u8,
        /// The second byte of the brightness answer: 0 is an answer, ff says the keyboard has none.
        status: u8,
        /// The version answers start with the echo of the command.
        echo: bool,
        firmware: &'static str,
        led: &'static str,
        /// Does not answer at all.
        silent: bool,
        /// Ignores the brightness it is given.
        deaf: bool,
    }

    type Shared = Arc<Mutex<State>>;

    fn state() -> Shared {
        Arc::new(Mutex::new(State {
            level: 8,
            status: 0,
            echo: true,
            firmware: "1.2.3",
            led: "4.5.6",
            silent: false,
            deaf: false,
        }))
    }

    /// An Apex Pro as OpenRGB describes it: every request is an output report of 64 bytes, every answer an input report
    /// of 64 bytes.
    fn keyboard(state: Shared) -> FakeHid {
        FakeHid::new(move |req: &[u8]| {
            let mut s = state.lock().unwrap();
            assert_eq!(req.len(), OUTPUT_LEN, "{req:02x?}");
            if s.silent {
                return Vec::new();
            }
            let mut answer = vec![0u8; OUTPUT_LEN];
            let text = |answer: &mut Vec<u8>, text: &str| {
                let at = usize::from(s.echo);
                if s.echo {
                    answer[0] = READ_VERSION;
                }
                answer[at..at + text.len()].copy_from_slice(text.as_bytes());
            };
            match (req[0], req[1]) {
                (READ_VERSION, 0) => text(&mut answer, s.firmware),
                (READ_VERSION, 1) => text(&mut answer, s.led),
                (READ_BRIGHTNESS, _) => answer[..3].copy_from_slice(&[READ_BRIGHTNESS, s.status, s.level]),
                (SET_BRIGHTNESS, level) => {
                    if !s.deaf {
                        s.level = level;
                    }
                    return Vec::new();
                }
                (ONBOARD, _) => return Vec::new(),
                other => panic!("unexpected request {other:02x?}"),
            }
            vec![answer]
        })
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-apex-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn open(state: &Shared, name: &str, changes: bool) -> (super::super::task::PeripheralTask, FakeHid) {
        let hid = keyboard(state.clone());
        let model = find(STEELSERIES, 0x1610).unwrap();
        let task = start_for_test(Box::new(hid.clone()), model, 0x1610, changes, temp(name)).unwrap();
        (task, hid)
    }

    /// The reports sent that change something: feature reports, and the output reports that are not reads.
    fn changes(hid: &FakeHid) -> Vec<Vec<u8>> {
        hid.sent()
            .into_iter()
            .filter(|r| COMMANDS.iter().any(|c| r.starts_with(c.prefix) && c.effect != Effect::Read))
            .collect()
    }

    const RED: Rgb = Rgb { r: 0xff, g: 0, b: 0 };
    const BLUE: Rgb = Rgb { r: 0, g: 0, b: 0xff };

    #[test]
    fn the_four_models_are_keyboards_on_interface_1_and_the_others_of_the_family_are_left_out() {
        for (id, name) in [(0x1610, "Apex Pro"), (0x1612, "Apex 7"), (0x1618, "Apex 7 TKL"), (0x161C, "Apex 5")] {
            let m = find(STEELSERIES, id).unwrap();
            assert_eq!((m.name, m.kind, m.interface, m.usage_page), (name, Kind::Keyboard, Some(1), Some(0xFFC0)));
            assert!(matches!(m.protocol, Protocol::SteelSeriesApex(_)));
            assert!(opened_on_linux(m), "{name}: only interface 1 is opened, it carries no typing");
        }
        // The Apex Pro TKL changes protocol with its firmware, the Apex Pro 3 has another display, the Apex 3 TKL
        // has its own driver.
        for id in [0x1614, 0x1640, 0x1622, 0x161A, 0x1628, 0x1630, 0x1634] {
            assert!(find(STEELSERIES, id).is_none(), "{id:04x}");
        }
    }

    #[test]
    fn the_list_of_commands_holds_the_documented_ones_and_nothing_of_the_other_generations() {
        let firsts: Vec<u8> = COMMANDS.iter().map(|c| c.prefix[0]).collect();
        for sent in [0x90, 0xA3, 0x23, 0x3A, 0x3B, 0x61] {
            assert!(firsts.contains(&sent), "{sent:02x}");
        }
        // Gen 2 and 3 (colours, back to the profile, initialisation), the region, the serial number and the profile select.
        for never in [0x40, 0x41, 0x4B, 0xF5, 0xFF, 0x89] {
            assert!(!firsts.contains(&never), "{never:02x}");
        }
        assert_eq!(COMMANDS.iter().filter(|c| c.effect == Effect::Read).count(), 3);
        assert!(COMMANDS.iter().all(|c| c.effect != Effect::Save), "nothing is stored in the keyboard");
    }

    #[test]
    fn reads_the_firmware_and_the_brightness_as_documented_with_or_without_the_echo_and_only_reads() {
        for echo in [true, false] {
            let s = state();
            s.lock().unwrap().echo = echo;
            let (task, hid) = open(&s, "read", true);
            let info = &task.info;
            assert_eq!(info.firmware.as_deref(), Some("1.2.3"), "echo {echo}");
            assert!(info.details.contains(&("LED controller firmware".into(), "4.5.6".into())), "{:?}", info.details);
            assert_eq!(info.settings.leds, [Led { mode: 0, brightness: Some(80), color: Rgb::default() }]);
            assert_eq!(info.limits.leds[0].brightness_step, Some(10));
            assert!(info.limits.leds[0].modes.is_empty() && !info.limits.leds[0].color);
            assert_eq!(info.problem, None);
            assert!(info.editable && info.readable && !info.can_save && !info.changes_stay);
            assert!(changes(&hid).is_empty());
            for req in hid.sent() {
                assert!(COMMANDS.iter().any(|c| c.effect == Effect::Read && req.starts_with(c.prefix)), "{req:02x?}");
            }
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn a_connection_that_may_not_change_anything_sends_only_reads() {
        let s = state();
        let (mut task, hid) = open(&s, "readonly", false);
        assert!(!task.info.editable);
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(30);
        assert!(task.change(&wanted, false, None).is_err());
        for (id, value) in [
            (ALL_ID, BlindValue::Color(RED)),
            (GROUPS[0].0, BlindValue::Color(RED)),
            (ONBOARD_ID, BlindValue::Choice(ONBOARD_NAME.into())),
            (DISPLAY_ID, BlindValue::Choice(DISPLAY_CLEAR.into())),
        ] {
            assert!(task.set_blind(id, &value).is_err(), "{id}");
        }
        assert!(changes(&hid).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_brightness_change_is_read_back_and_undone_unless_kept() {
        let s = state();
        let (mut task, hid) = open(&s, "brightness", true);
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(30);
        task.change(&wanted, false, None).unwrap();
        let sent = changes(&hid);
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][..2], &[SET_BRIGHTNESS, 3]);
        assert!(sent[0][2..].iter().all(|&b| b == 0));
        assert_eq!(task.info.settings, wanted, "read back");
        assert_eq!(s.lock().unwrap().level, 3);
        // Not kept: put back, as it was read.
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(s.lock().unwrap().level, 8);
        assert_eq!(task.info.settings.leds[0].brightness, Some(80));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_brightness_the_keyboard_ignores_is_reported_and_put_back() {
        let s = state();
        let (mut task, _hid) = open(&s, "deaf", true);
        s.lock().unwrap().deaf = true;
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(30);
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("reads back"), "{error}");
        assert_eq!(task.info.settings.leds[0].brightness, Some(80));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn only_tens_of_percent_up_to_a_hundred_are_accepted() {
        let s = state();
        let (mut task, hid) = open(&s, "steps", true);
        for bad in [35, 110] {
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].brightness = Some(bad);
            assert!(task.change(&wanted, false, None).is_err(), "{bad}");
        }
        assert!(changes(&hid).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_keyboard_without_a_brightness_is_shown_without_one_and_keeps_its_colours() {
        let s = state();
        s.lock().unwrap().status = UNSUPPORTED;
        let (mut task, hid) = open(&s, "nobrightness", true);
        assert!(task.info.settings.leds.is_empty() && task.info.limits.leds.is_empty());
        assert!(task.info.details.iter().any(|(k, _)| k == "Brightness"), "{:?}", task.info.details);
        assert_eq!(task.info.problem, None);
        assert!(task.info.editable);
        // Read again by the refresh: no new request for the brightness.
        let before = hid.sent().len();
        task.info().unwrap();
        assert_eq!(hid.sent().len(), before);
        task.set_blind(ALL_ID, &BlindValue::Color(RED)).unwrap();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn answers_that_do_not_look_as_documented_leave_the_keyboard_read_only() {
        let s = state();
        s.lock().unwrap().firmware = "hello";
        let (task, _) = open(&s, "oddversion", true);
        let problem = task.info.problem.clone().unwrap();
        assert!(problem.contains("does not look like a version"), "{problem}");
        assert!(!task.info.editable && task.info.firmware.is_none());
        let _ = std::fs::remove_dir_all(task.backups());

        let s = state();
        s.lock().unwrap().level = 11;
        let (task, _) = open(&s, "oddlevel", true);
        let problem = task.info.problem.clone().unwrap();
        assert!(problem.contains("a3 00"), "{problem}");
        assert!(!task.info.editable);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_keyboard_that_does_not_answer_gets_its_page_and_is_read_only() {
        let s = state();
        s.lock().unwrap().silent = true;
        let (task, hid) = open(&s, "silent", true);
        assert!(task.info.details[0].1.starts_with("no answer yet"), "{:?}", task.info.details);
        assert!(!task.info.editable);
        assert!(changes(&hid).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn versions_are_read_with_or_without_the_echo_and_only_when_they_look_like_one() {
        let mut a = vec![0u8; OUTPUT_LEN];
        a[..6].copy_from_slice(&[READ_VERSION, b'1', b'.', b'2', b'.', b'3']);
        assert_eq!(version(&a).as_deref(), Some("1.2.3"));
        a[..6].copy_from_slice(b"10.2.3");
        assert_eq!(version(&a).as_deref(), Some("10.2.3"));
        // A zero before the text, as another layout of the answer would have.
        a[..7].copy_from_slice(&[READ_VERSION, 0, b'1', b'.', b'2', b'.', b'3']);
        assert_eq!(version(&a).as_deref(), Some("1.2.3"));
        a[..4].copy_from_slice(&[b'1', b'.', b'2', 0]);
        assert_eq!(version(&a), None, "two dots");
        a[..6].copy_from_slice(&[0x01, b'.', b'2', b'.', b'3', 0]);
        assert_eq!(version(&a), None, "not printable");
        assert_eq!(version(&[0u8; OUTPUT_LEN]), None);
        assert_eq!(
            [
                brightness(&[0xA3, 0, 10]),
                brightness(&[0xA3, 0, 11]),
                brightness(&[0xA3, 0xFF, 0]),
                brightness(&[0xA3, 1, 5])
            ],
            [Brightness::Level(10), Brightness::Odd, Brightness::Unsupported, Brightness::Odd]
        );
    }

    #[test]
    fn the_112_keys_are_distinct_usages_and_each_is_in_one_group() {
        let mut sorted = KEYS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 112);
        assert_eq!(&KEYS[..3], &[0x04, 0x05, 0x06], "A first, as OpenRGB sends them");
        assert_eq!(KEYS[111], 0xFB, "media play/pause last");
        assert_eq!([group_of(0x04), group_of(0x27), group_of(0x3A), group_of(0x52), group_of(0x63)], [0, 1, 2, 3, 4]);
        assert_eq!([group_of(0xE0), group_of(0xF0), group_of(0x28), group_of(0x64), group_of(0xFB)], [5, 5, 6, 6, 6]);
        for (g, (_, label)) in GROUPS.iter().enumerate() {
            assert!(KEYS.iter().any(|&k| group_of(k) == g), "{label}");
        }
    }

    #[test]
    fn colours_go_out_as_one_frame_with_all_112_keys_and_a_group_keeps_the_others() {
        let s = state();
        let (mut task, hid) = open(&s, "colours", true);
        task.set_blind(ALL_ID, &BlindValue::Color(RED)).unwrap();
        task.set_blind(GROUPS[0].0, &BlindValue::Color(BLUE)).unwrap();
        let frames = changes(&hid);
        assert_eq!(frames.len(), 2);
        for frame in &frames {
            assert_eq!((frame.len(), frame[0], frame[1]), (FEATURE_LEN, DIRECT, 112));
            assert!(frame[2 + 4 * 112..].iter().all(|&b| b == 0), "zeros after the keys");
        }
        let colour_of = |frame: &Vec<u8>, key: u8| {
            let i = KEYS.iter().position(|&k| k == key).unwrap();
            assert_eq!(frame[2 + 4 * i], key);
            Rgb::from_bytes(&frame[3 + 4 * i..6 + 4 * i])
        };
        assert!(KEYS.iter().all(|&k| colour_of(&frames[0], k) == RED));
        assert_eq!(colour_of(&frames[1], 0x04), BLUE, "letters");
        assert_eq!(colour_of(&frames[1], 0x1D), BLUE);
        assert_eq!(colour_of(&frames[1], 0x1E), RED, "the number row kept its colour");
        assert_eq!(colour_of(&frames[1], 0xF0), RED);
        // Back to the keyboard's own lighting: one output report, and the colours sent before are forgotten.
        task.set_blind(ONBOARD_ID, &BlindValue::Choice(ONBOARD_NAME.into())).unwrap();
        task.set_blind(GROUPS[2].0, &BlindValue::Color(BLUE)).unwrap();
        let sent = changes(&hid);
        assert_eq!((sent[2].len(), sent[2][0]), (OUTPUT_LEN, ONBOARD));
        assert!(sent[2][1..].iter().all(|&b| b == 0));
        assert_eq!(colour_of(&sent[3], 0x3A), BLUE, "F1");
        assert_eq!(colour_of(&sent[3], 0x04), Rgb::BLACK, "a group never sent is dark");
        assert_eq!(task.info.blind.iter().find(|b| b.id == ALL_ID).unwrap().last, Some(BlindValue::Color(RED)));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_display_takes_a_picture_of_128_by_40_pixels_row_by_row_with_the_leftmost_pixel_in_the_highest_bit() {
        let s = state();
        let (mut task, hid) = open(&s, "display", true);
        for name in [DISPLAY_CLEAR, DISPLAY_FILL, DISPLAY_TEST] {
            task.set_blind(DISPLAY_ID, &BlindValue::Choice(name.into())).unwrap();
        }
        let pictures = changes(&hid);
        assert_eq!(pictures.len(), 3);
        for p in &pictures {
            assert_eq!((p.len(), p[0], p[641]), (FEATURE_LEN, DISPLAY, 0));
        }
        assert!(pictures[0][1..].iter().all(|&b| b == 0));
        assert!(pictures[1][1..641].iter().all(|&b| b == 0xFF));
        let pixel = |p: &Vec<u8>, x: usize, y: usize| p[1 + y * 16 + x / 8] & (0x80 >> (x % 8)) != 0;
        let t = &pictures[2];
        assert!(pixel(t, 0, 0) && pixel(t, 127, 0) && pixel(t, 0, 39) && pixel(t, 127, 39), "the frame");
        assert!(pixel(t, 4, 4) && pixel(t, 19, 19) && !pixel(t, 20, 20), "the big square, top left");
        assert!(pixel(t, 116, 4) && pixel(t, 123, 11) && !pixel(t, 124, 4), "the small one, top right");
        assert!(!pixel(t, 4, 30) && !pixel(t, 120, 30), "nothing at the bottom");
        // The first byte of the second row holds pixels 0 to 7 of that row, the first pixel in the highest bit.
        assert_eq!(t[1 + 16], 0x80, "pixel (0, 1) only: the frame's left edge");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_value_that_is_not_of_the_setting_is_refused_before_anything_is_sent() {
        let s = state();
        let (mut task, hid) = open(&s, "refused", true);
        assert!(task.set_blind(DISPLAY_ID, &BlindValue::Color(RED)).is_err());
        assert!(task.set_blind(DISPLAY_ID, &BlindValue::Choice("Something else".into())).is_err());
        assert!(task.set_blind(ONBOARD_ID, &BlindValue::Choice(DISPLAY_CLEAR.into())).is_err());
        assert!(task.set_blind(ALL_ID, &BlindValue::Number(3)).is_err());
        assert!(task.set_blind("nothing", &BlindValue::Color(RED)).is_err());
        assert!(changes(&hid).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }
}

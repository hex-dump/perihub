//! The Logitech G213 Prodigy keyboard. The protocol is in `docs/protocols/logitech-g213.md`, written from facts in
//! OpenRGB and g810-led (GPL: no code was copied) and in G213Colors and gdevd (MIT). Nobody has tried it with
//! PeriHub yet.
//!
//! The keyboard speaks Logitech's HID++ 2.0 on a vendor collection of its own (usage page `ff43`, usage `0602`). It
//! has five lighting areas and a few effects of its own, and no source reads its lighting back, so PeriHub
//! can only send them, one setting at a time, with the warning of every setting that cannot be checked. Nothing is
//! stored in the keyboard: it forgets what was sent when it is unplugged. The startup effect, which the keyboard
//! does store, is not sent (see the protocol page).
//!
//! What PeriHub reads is the root feature of HID++ 2.0: the protocol version, and where the keyboard lists the
//! colour LED effects (`8070`). The lighting commands go to index `0c` in every source, so they are sent only if the
//! keyboard lists `8070` there.
//!
//! On Linux one hidraw node carries every collection of an interface, so the node of interface 1 also carries what
//! is typed: the keyboard is not opened there (see [`super::opened_on_linux`]). On Windows each collection has a path
//! of its own, and only the vendor one is opened.

use std::time::Duration;

use super::task::Driver;
use super::{BlindKind, BlindSetting, BlindValue, Command, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, HidppErrorCode, Result};

const INTERFACE: i32 = 1;
const USAGE_PAGE: u16 = 0xFF43;
/// The collection of HID++ long reports (id 0x11, 20 bytes). The other vendor collection, 0604, has reports of 64
/// bytes with the id 0x12.
const USAGE: u16 = 0x0602;

/// Whether a HID collection of interface 1 is the one that takes the HID++ long reports the lighting commands are.
pub fn is_command_collection(usage_page: u16, usage: u16) -> bool {
    usage_page == USAGE_PAGE && usage == USAGE
}

const REPORT: usize = 20;
const LONG: u8 = 0x11;
/// The device index of a device on its own USB connection.
const DEVICE: u8 = 0xFF;
/// In place of a feature index: the answer is an error.
const ERROR: u8 = 0xFF;
const ROOT: u8 = 0x00;
/// Where every source sends the lighting.
const LIGHTING: u8 = 0x0C;
/// Function 3, software id a.
const SET_EFFECT: u8 = 0x3A;
/// Root functions 1 and 0, software id a.
const PROTOCOL_VERSION: u8 = 0x1A;
const FEATURE_INDEX: u8 = 0x0A;
const PING_VALUE: u8 = 0x5A;
/// How long the keyboard has to answer one request.
const TIMEOUT: Duration = Duration::from_millis(1000);

/// The feature of the colour LED effects, which the lighting commands are expected to belong to.
const COLOR_LED_EFFECTS: u16 = 0x8070;
/// Features whose index is only asked for, for the test report.
const OTHER_FEATURES: [(u16, &str); 6] = [
    (0x0001, "feature set"),
    (0x0003, "device information"),
    (0x0005, "device name"),
    (0x8040, "brightness control"),
    (0x8071, "RGB effects"),
    (0x8081, "per-key lighting"),
];

/// The effect byte (byte 5 of a lighting command).
const OFF: u8 = 0x00;
const FIXED: u8 = 0x01;
const BREATHING: u8 = 0x02;
const CYCLE: u8 = 0x03;
const WAVE: u8 = 0x04;

/// What an effect may be asked for: the time of one cycle in milliseconds (OpenRGB offers 1000 to 20000) and a
/// brightness in percent (gdevd: 0 to 100; PeriHub never sends 0, which may turn the lights off).
const SPEED_MIN: u16 = 1000;
const SPEED_MAX: u16 = 20000;
const SPEED_STEP: u16 = 100;
const SPEED_DEFAULT: u16 = 5000;
const BRIGHTNESS_MIN: u8 = 10;
const BRIGHTNESS_MAX: u8 = 100;
const BRIGHTNESS_STEP: u8 = 10;

/// The areas, by the number that goes in byte 4 (area 0 is the whole keyboard), with OpenRGB's names.
const AREAS: [(&str, &str); 5] = [
    ("area-1", "Left area"),
    ("area-2", "Middle area"),
    ("area-3", "Right area"),
    ("area-4", "Arrows and home keys"),
    ("area-5", "Numpad"),
];
const ALL_ID: &str = "colour";
const BREATHING_ID: &str = "breathing";
const EFFECT_ID: &str = "effect";
const SPEED_ID: &str = "speed";
const BRIGHTNESS_ID: &str = "brightness";

/// The choices of the effect setting. A wave's number is its direction, byte 13 of the command: OpenRGB and gdevd
/// name 1, 6, 3 and 8, g810-led adds 2.
const OFF_NAME: &str = "Off";
const CYCLE_NAME: &str = "Colour cycle";
const WAVES: [(&str, u8); 5] = [
    ("Wave, left to right", 1),
    ("Wave, right to left", 6),
    ("Wave, vertical", 2),
    ("Wave, from the centre outwards", 3),
    ("Wave, from the edges inwards", 8),
];

const fn read(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 0, effect: Effect::Read, name }
}

const fn change(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 0, effect: Effect::Change, name }
}

/// Everything PeriHub may send: two reads of the root feature, and the lighting commands for the whole keyboard and
/// each area, by area and effect. Everything else at feature index 0c, such as the startup effect, the stored byte of
/// an effect and every function other than 3, is left out.
pub const COMMANDS: &[Command] = &[
    read(&[LONG, DEVICE, ROOT, PROTOCOL_VERSION], "read HID++ version"),
    read(&[LONG, DEVICE, ROOT, FEATURE_INDEX], "read feature index"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 0, OFF], "lighting off"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 0, FIXED], "colour, whole keyboard"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 1, FIXED], "colour, left area"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 2, FIXED], "colour, middle area"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 3, FIXED], "colour, right area"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 4, FIXED], "colour, arrows and home keys"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 5, FIXED], "colour, numpad"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 0, BREATHING], "breathing"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 0, CYCLE], "colour cycle"),
    change(&[LONG, DEVICE, LIGHTING, SET_EFFECT, 0, WAVE], "wave"),
];

pub const MODELS: &[Model] = &[Model {
    vendor_id: 0x046D,
    product_ids: &[0xC336],
    brand: "Logitech",
    name: "G213 Prodigy",
    kind: Kind::Keyboard,
    interface: Some(INTERFACE),
    usage_page: Some(USAGE_PAGE),
    protocol: Protocol::LogitechG213,
}];

/// A request: the long report, the feature index, the function and software id, then the parameters.
fn request(feature: u8, address: u8, params: &[u8]) -> [u8; REPORT] {
    let mut r = [0u8; REPORT];
    r[..4].copy_from_slice(&[LONG, DEVICE, feature, address]);
    r[4..4 + params.len()].copy_from_slice(params);
    r
}

/// A lighting command for an area and an effect, with nothing else set yet.
fn lighting(area: u8, effect: u8) -> [u8; REPORT] {
    request(LIGHTING, SET_EFFECT, &[area, effect])
}

fn fixed(area: u8, color: Rgb) -> [u8; REPORT] {
    let mut r = lighting(area, FIXED);
    r[6..9].copy_from_slice(&color.to_bytes());
    r[9] = 0x02;
    r
}

/// The effects that move. A breathing has a colour; the colour cycle and the wave use every colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Animation {
    Breathing(Rgb),
    Cycle,
    /// With its direction, byte 13 of the command.
    Wave(u8),
}

/// The command of an effect that moves: the time of one cycle (two bytes, high first) and the brightness.
fn animation(what: Animation, ms: u16, brightness: u8) -> [u8; REPORT] {
    let [high, low] = ms.to_be_bytes();
    let mut r = lighting(0, 0);
    match what {
        Animation::Breathing(color) => {
            r[5] = BREATHING;
            r[6..9].copy_from_slice(&color.to_bytes());
            r[9] = high;
            r[10] = low;
            r[12] = brightness;
        }
        Animation::Cycle => {
            r[5] = CYCLE;
            r[6..9].copy_from_slice(&[0xFF; 3]);
            r[11] = high;
            r[12] = low;
            r[13] = brightness;
        }
        Animation::Wave(direction) => {
            r[5] = WAVE;
            r[11] = high;
            r[12] = low;
            r[13] = direction;
            r[14] = brightness;
            r[15] = high;
        }
    }
    r
}

/// Sends a request and returns the keyboard's answer to it; an error answer is an error. The answer is read after every
/// command (G213Colors): the keyboard does not carry a command out completely otherwise.
fn exchange(io: &mut Io, packet: &[u8; REPORT]) -> Result<Vec<u8>> {
    let (feature, address) = (packet[2], packet[3]);
    // The device index of the answer is not checked: the only HID++ device on this collection is the keyboard.
    let answer = io.hidpp_exchange(packet, TIMEOUT, false, move |r| {
        r.len() >= 7
            && r[0] == LONG
            && ((r[2] == feature && r[3] == address) || (r[2] == ERROR && r[3] == feature && r[4] == address))
    })?;
    if answer[2] == ERROR {
        return Err(Error::Hidpp(HidppErrorCode(answer[5])));
    }
    Ok(answer)
}

/// The HID++ version, as the root feature says it.
fn version(io: &mut Io) -> Result<(u8, u8)> {
    let answer = exchange(io, &request(ROOT, PROTOCOL_VERSION, &[0, 0, PING_VALUE]))?;
    if answer[6] != PING_VALUE {
        return Err(Error::Protocol(format!("the ping came back as {:02x}", answer[6])));
    }
    Ok((answer[4], answer[5]))
}

/// Where the keyboard lists a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Listed {
    index: u8,
    version: u8,
}

/// Where the keyboard lists the feature with this id. `None`: it does not list it.
fn feature(io: &mut Io, id: u16) -> Result<Option<Listed>> {
    let answer = exchange(io, &request(ROOT, FEATURE_INDEX, &id.to_be_bytes()))?;
    Ok((answer[4] != 0).then_some(Listed { index: answer[4], version: answer[6] }))
}

/// What the keyboard answered the first time, kept: the features do not change while it is plugged in.
struct Probe {
    version: (u8, u8),
    lighting: Option<Listed>,
    others: Vec<(u16, &'static str, Option<Listed>)>,
}

impl Probe {
    /// What is wrong with the answers, if that makes it unsafe to send a lighting command.
    fn problem(&self) -> Option<String> {
        if self.version.0 < 2 {
            return Some(format!(
                "the keyboard says HID++ {}.{}, and the lighting commands are those of HID++ 2.0",
                self.version.0, self.version.1
            ));
        }
        match self.lighting {
            Some(Listed { index: LIGHTING, .. }) => None,
            Some(Listed { index, .. }) => Some(format!(
                "the keyboard lists the colour LED effects ({COLOR_LED_EFFECTS:04x}) at index {index:02x}, not at {LIGHTING:02x} where the lighting commands go"
            )),
            None => Some(format!(
                "the keyboard does not list the colour LED effects ({COLOR_LED_EFFECTS:04x}) that the lighting commands belong to"
            )),
        }
    }

    fn details(&self) -> Vec<(String, String)> {
        let at = |found: Option<Listed>| match found {
            Some(Listed { index, version }) => format!("at index {index:02x}, version {version}"),
            None => "not listed".to_string(),
        };
        let others: Vec<String> =
            self.others.iter().map(|(id, name, found)| format!("{name} ({id:04x}) {}", at(*found))).collect();
        vec![
            ("HID++ version".into(), format!("{}.{}", self.version.0, self.version.1)),
            (format!("Colour LED effects ({COLOR_LED_EFFECTS:04x})"), at(self.lighting)),
            ("Other features".into(), others.join("; ")),
        ]
    }
}

/// A reading of a keyboard whose answers do not look as documented: it is only shown.
fn refused(problem: String, details: Vec<(String, String)>) -> Reading {
    Reading { details, problem: Some(problem), ..Default::default() }
}

pub struct G213 {
    probe: Option<Probe>,
    /// Milliseconds of one cycle, and percent: what the next effect that moves is sent with.
    speed: u16,
    brightness: u8,
    /// The effect that moves that was sent last, if the keyboard shows it still.
    last: Option<Animation>,
}

impl G213 {
    pub fn new() -> Self {
        Self { probe: None, speed: SPEED_DEFAULT, brightness: BRIGHTNESS_MAX, last: None }
    }

    /// Sends an effect that moves, and keeps it with its time and brightness once the keyboard answered.
    fn show(&mut self, io: &mut Io, what: Animation, speed: u16, brightness: u8) -> Result<()> {
        exchange(io, &animation(what, speed, brightness))?;
        (self.last, self.speed, self.brightness) = (Some(what), speed, brightness);
        Ok(())
    }

    /// The effect that was sent last, again, with a new time or brightness.
    fn again(&mut self, io: &mut Io, speed: u16, brightness: u8) -> Result<()> {
        let Some(what) = self.last else {
            return Err(Error::Invalid(
                "send an effect first (breathing, colour cycle or a wave): the time and the brightness belong to it"
                    .into(),
            ));
        };
        self.show(io, what, speed, brightness)
    }
}

impl Default for G213 {
    fn default() -> Self {
        Self::new()
    }
}

/// A whole number the setting takes: in its range, in its steps.
fn number(setting: &str, value: &BlindValue, min: i32, max: i32, step: i32) -> Result<i32> {
    match value {
        BlindValue::Number(n) if (min..=max).contains(n) && (n - min) % step == 0 => Ok(*n),
        BlindValue::Number(_) => Err(Error::Invalid(format!("{setting}: from {min} to {max} in steps of {step}"))),
        _ => Err(Error::Invalid(format!("{setting}: wrong kind of value"))),
    }
}

impl Driver for G213 {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "OpenRGB, g810-led, G213Colors and gdevd"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    /// Nothing about the lighting can be read: see the module's documentation.
    fn readable(&self) -> bool {
        false
    }

    fn blind_settings(&self) -> Vec<BlindSetting> {
        let color = |id, label| BlindSetting { id, label, kind: BlindKind::Color, last: None };
        let mut settings = vec![color(ALL_ID, "Whole keyboard, one colour")];
        settings.extend(AREAS.iter().map(|&(id, label)| color(id, label)));
        settings.push(color(BREATHING_ID, "Breathing, in this colour"));
        let mut effects = vec![OFF_NAME.to_string(), CYCLE_NAME.to_string()];
        effects.extend(WAVES.iter().map(|(name, _)| name.to_string()));
        settings.push(BlindSetting {
            id: EFFECT_ID,
            label: "Effect",
            kind: BlindKind::Choice { values: effects },
            last: None,
        });
        settings.push(BlindSetting {
            id: SPEED_ID,
            label: "Time of one cycle (ms), for the effect sent last",
            kind: BlindKind::Range { min: SPEED_MIN as i32, max: SPEED_MAX as i32, step: SPEED_STEP as i32 },
            last: None,
        });
        settings.push(BlindSetting {
            id: BRIGHTNESS_ID,
            label: "Brightness of the effect (%), for the effect sent last",
            kind: BlindKind::Range {
                min: BRIGHTNESS_MIN as i32,
                max: BRIGHTNESS_MAX as i32,
                step: BRIGHTNESS_STEP as i32,
            },
            last: None,
        });
        settings
    }

    fn send_blind(&mut self, io: &mut Io, id: &str, value: &BlindValue) -> Result<()> {
        log::info!("Logitech G213: {id} ← {value:?}");
        match (id, value) {
            (ALL_ID, BlindValue::Color(color)) => {
                exchange(io, &fixed(0, *color))?;
                self.last = None;
            }
            (area, BlindValue::Color(color)) if AREAS.iter().any(|(id, _)| *id == area) => {
                let number = AREAS.iter().position(|(id, _)| *id == area).expect("found above") as u8 + 1;
                exchange(io, &fixed(number, *color))?;
                self.last = None;
            }
            (BREATHING_ID, BlindValue::Color(color)) => {
                self.show(io, Animation::Breathing(*color), self.speed, self.brightness)?;
            }
            (EFFECT_ID, BlindValue::Choice(name)) if name == OFF_NAME => {
                exchange(io, &lighting(0, OFF))?;
                self.last = None;
            }
            (EFFECT_ID, BlindValue::Choice(name)) if name == CYCLE_NAME => {
                self.show(io, Animation::Cycle, self.speed, self.brightness)?;
            }
            (EFFECT_ID, BlindValue::Choice(name)) => {
                let (_, direction) = WAVES
                    .iter()
                    .find(|(wave, _)| *wave == name.as_str())
                    .ok_or_else(|| Error::Invalid(format!("{name} is not an effect of this keyboard")))?;
                self.show(io, Animation::Wave(*direction), self.speed, self.brightness)?;
            }
            (SPEED_ID, value) => {
                let speed = number("time", value, SPEED_MIN as i32, SPEED_MAX as i32, SPEED_STEP as i32)?;
                self.again(io, speed as u16, self.brightness)?;
            }
            (BRIGHTNESS_ID, value) => {
                let brightness =
                    number("brightness", value, BRIGHTNESS_MIN as i32, BRIGHTNESS_MAX as i32, BRIGHTNESS_STEP as i32)?;
                self.again(io, self.speed, brightness as u8)?;
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
        let version = match version(io) {
            Ok(version) => version,
            // One that answered before and stops is gone: the manager opens it again when it is back.
            Err(Error::Timeout) if self.probe.is_some() => return Err(Error::Timeout),
            // One that never answered: the page and its test report show it, and it is read again every few seconds.
            Err(Error::Timeout) => {
                io.forbid_changes();
                return Ok(Reading {
                    details: vec![(
                        "Status".into(),
                        "no answer yet: if the keyboard is plugged in, please send a test report".into(),
                    )],
                    ..Default::default()
                });
            }
            Err(Error::Hidpp(code)) => {
                return Ok(refused(
                    format!("the keyboard answered the version request with an error: {code}"),
                    Vec::new(),
                ));
            }
            Err(Error::Protocol(text)) => return Ok(refused(text, Vec::new())),
            Err(e) => return Err(e),
        };
        if self.probe.is_none() {
            let lighting = match feature(io, COLOR_LED_EFFECTS) {
                Ok(found) => found,
                Err(Error::Hidpp(code)) => {
                    return Ok(refused(
                        format!("the keyboard answered the request for the colour LED effects with an error: {code}"),
                        vec![("HID++ version".into(), format!("{}.{}", version.0, version.1))],
                    ));
                }
                Err(e) => return Err(e),
            };
            let mut others = Vec::new();
            for (id, name) in OTHER_FEATURES {
                // Only for the test report: a feature that is not answered is left out of it.
                if let Ok(found) = feature(io, id) {
                    others.push((id, name, found));
                }
            }
            self.probe = Some(Probe { version, lighting, others });
        }
        let probe = self.probe.as_ref().expect("probed above");
        Ok(Reading { details: probe.details(), problem: probe.problem(), ..Default::default() })
    }

    fn apply(&mut self, _io: &mut Io, _from: &Settings, _to: &Settings) -> Result<()> {
        Err(Error::Invalid("this keyboard cannot be read back: its lighting is sent one setting at a time".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("PeriHub does not store anything in the memory of this keyboard".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake::FakeHid;
    use super::super::task::{PeripheralTask, start_for_test};
    use super::super::{find, opened_on_linux};
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        let mut report: Vec<u8> =
            (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
        report.resize(REPORT, 0);
        report
    }

    /// A G213 as the sources describe it: it answers the root feature (HID++ 4.2, with the features at the index
    /// given, the colour LED effects at `lighting`) and echoes every lighting command.
    fn keyboard(lighting: u8) -> FakeHid {
        let hid = FakeHid::new(move |req| {
            let mut answer = req.to_vec();
            match (req[2], req[3]) {
                (0x00, PROTOCOL_VERSION) => answer[4..6].copy_from_slice(&[4, 2]),
                (0x00, FEATURE_INDEX) => {
                    let index = match u16::from_be_bytes([req[4], req[5]]) {
                        0x8070 => lighting,
                        0x0001 => 0x01,
                        0x0003 => 0x02,
                        0x0005 => 0x03,
                        _ => 0,
                    };
                    answer[4..7].copy_from_slice(&[index, 0, 0]);
                }
                _ => {}
            }
            vec![answer]
        });
        hid.0.lock().unwrap().numbered_output = true;
        hid
    }

    fn start_with(hid: &FakeHid, name: &str, changes: bool) -> PeripheralTask {
        let model = find(0x046D, 0xC336).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-g213-{name}-{}", std::process::id()));
        start_for_test(Box::new(hid.clone()), model, 0xC336, changes, backups).unwrap()
    }

    fn start(hid: &FakeHid, name: &str) -> PeripheralTask {
        start_with(hid, name, true)
    }

    fn color(hex: &str) -> BlindValue {
        BlindValue::Color(Rgb::from_hex(hex).unwrap())
    }

    /// Everything sent after the reads of the first pass.
    fn commands(hid: &FakeHid) -> Vec<Vec<u8>> {
        hid.sent().into_iter().filter(|r| r[2] == LIGHTING).collect()
    }

    #[test]
    fn the_keyboard_is_read_and_its_lighting_is_sent_one_setting_at_a_time() {
        let hid = keyboard(LIGHTING);
        let task = start(&hid, "read");
        let info = &task.info;
        assert_eq!((info.brand.as_str(), info.model.as_str()), ("Logitech", "G213 Prodigy"));
        assert_eq!(info.problem, None);
        assert!(info.editable && !info.readable && !info.can_save);
        let d = &info.details;
        assert!(d.contains(&("HID++ version".into(), "4.2".into())), "{d:?}");
        assert!(d.contains(&("Colour LED effects (8070)".into(), "at index 0c, version 0".into())), "{d:?}");
        let others = &d.iter().find(|(label, _)| label == "Other features").unwrap().1;
        assert!(others.contains("feature set (0001) at index 01, version 0"), "{others}");
        assert!(others.contains("per-key lighting (8081) not listed"), "{others}");
        let ids: Vec<_> = info.blind.iter().map(|b| b.id).collect();
        assert_eq!(
            ids,
            ["colour", "area-1", "area-2", "area-3", "area-4", "area-5", "breathing", "effect", "speed", "brightness"]
        );
        // Reading only reads: the root feature, nothing at index 0c.
        for report in hid.sent() {
            assert_eq!(report[..3], [LONG, DEVICE, ROOT], "{report:02x?}");
        }
    }

    #[test]
    fn colours_go_out_as_g213colors_captured_them() {
        let hid = keyboard(LIGHTING);
        let mut task = start(&hid, "colours");
        // G213Colors: 11ff0c3a, the field (0 all, 1 to 5 an area), 01, the colour, 02, zeros up to 20 bytes.
        let expected = |field: u8, rgb: &str| bytes(&format!("11ff0c3a{field:02x}01{rgb}02"));
        task.set_blind("colour", &color("#ffb4aa")).unwrap();
        assert_eq!(commands(&hid).last().unwrap(), &expected(0, "ffb4aa"));
        for (n, id) in ["area-1", "area-2", "area-3", "area-4", "area-5"].into_iter().enumerate() {
            task.set_blind(id, &color("#ff8000")).unwrap();
            assert_eq!(commands(&hid).last().unwrap(), &expected(n as u8 + 1, "ff8000"), "{id}");
        }
        assert_eq!(commands(&hid).len(), 6);
        assert_eq!(task.info.blind[0].last, Some(color("#ffb4aa")));
    }

    #[test]
    fn effects_go_out_as_the_sources_document_them() {
        let hid = keyboard(LIGHTING);
        let mut task = start(&hid, "effects");
        let sent = |hid: &FakeHid| commands(hid).last().unwrap().clone();
        let choice = |name: &str| BlindValue::Choice(name.into());

        // OpenRGB: off is the command with nothing else set.
        task.set_blind("effect", &choice("Off")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a0000"));

        // G213Colors: 11ff0c3a0002, the colour, the time (two bytes), 00, 64, zeros. 5000 ms is 0x1388.
        task.set_blind("breathing", &color("#00ff00")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a000200ff0013880064"));
        // G213Colors: 11ff0c3a0003, ffffff, 0000, the time, 64, zeros.
        task.set_blind("effect", &choice("Colour cycle")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a0003ffffff0000138864"));
        // OpenRGB and gdevd: the wave has the time in bytes 11 and 12 (and its high byte in 15), the direction in 13, the
        // brightness in 14.
        task.set_blind("effect", &choice("Wave, left to right")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a00040000000000138801641300"));
        for (name, direction) in [
            ("Wave, right to left", 0x06),
            ("Wave, vertical", 0x02),
            ("Wave, from the centre outwards", 0x03),
            ("Wave, from the edges inwards", 0x08),
        ] {
            task.set_blind("effect", &choice(name)).unwrap();
            let r = sent(&hid);
            assert_eq!((r[5], r[13]), (WAVE, direction), "{name}");
        }
        // The time and the brightness are sent again with the effect sent last.
        task.set_blind("speed", &BlindValue::Number(1000)).unwrap();
        let r = sent(&hid);
        assert_eq!((r[5], r[11], r[12], r[15], r[14]), (WAVE, 0x03, 0xE8, 0x03, 0x64));
        task.set_blind("brightness", &BlindValue::Number(50)).unwrap();
        let r = sent(&hid);
        assert_eq!((r[5], r[11], r[12], r[14]), (WAVE, 0x03, 0xE8, 50));
        task.set_blind("effect", &choice("Colour cycle")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a0003ffffff000003e832"));
        task.set_blind("breathing", &color("#0000ff")).unwrap();
        assert_eq!(sent(&hid), bytes("11ff0c3a00020000ff03e8003200"));
        // No command carries the stored byte (16) or anything past it.
        for r in commands(&hid) {
            assert_eq!(r[16..], [0; 4], "{r:02x?}");
        }
    }

    #[test]
    fn the_time_and_brightness_need_an_effect_that_moves_and_stay_in_range() {
        let hid = keyboard(LIGHTING);
        let mut task = start(&hid, "ranges");
        let number = BlindValue::Number;
        // Nothing moves yet, and after a fixed colour nothing does either.
        assert!(task.set_blind("speed", &number(2000)).is_err());
        task.set_blind("area-2", &color("#123456")).unwrap();
        assert!(task.set_blind("brightness", &number(50)).is_err());
        assert_eq!(commands(&hid).len(), 1);
        task.set_blind("effect", &BlindValue::Choice("Colour cycle".into())).unwrap();
        let sent = commands(&hid).len();
        for (id, value) in [
            ("speed", 999),
            ("speed", 1050),
            ("speed", 20100),
            ("speed", -100),
            ("brightness", 0),
            ("brightness", 5),
            ("brightness", 55),
            ("brightness", 110),
        ] {
            assert!(task.set_blind(id, &number(value)).is_err(), "{id} {value}");
        }
        assert!(task.set_blind("speed", &color("#ffffff")).is_err());
        assert!(task.set_blind("effect", &BlindValue::Choice("Rainbow".into())).is_err());
        assert!(task.set_blind("colour", &number(1)).is_err());
        assert!(task.set_blind("startup", &BlindValue::Choice("Off".into())).is_err());
        assert_eq!(commands(&hid).len(), sent, "refused values never reach the keyboard");
        for (id, value) in [("speed", 1000), ("speed", 20000), ("brightness", 10), ("brightness", 100)] {
            task.set_blind(id, &number(value)).unwrap();
        }
    }

    #[test]
    fn only_listed_commands_are_sent_and_the_startup_effect_is_not_one_of_them() {
        let hid = keyboard(LIGHTING);
        let mut task = start(&hid, "listed");
        let choice = |name: &str| BlindValue::Choice(name.into());
        task.set_blind("colour", &color("#ffffff")).unwrap();
        task.set_blind("effect", &choice("Off")).unwrap();
        task.set_blind("breathing", &color("#ff0000")).unwrap();
        task.set_blind("effect", &choice("Colour cycle")).unwrap();
        task.set_blind("effect", &choice("Wave, vertical")).unwrap();
        for report in hid.sent() {
            assert_eq!(report.len(), REPORT);
            assert!(COMMANDS.iter().any(|c| report.starts_with(c.prefix)), "{report:02x?}");
        }
        // The startup effect of gdevd and of g810-led, a different function at 0c, another feature, a "commit", a
        // function 3 for an effect no source documents, and the same command to another device index.
        let io = Io::new(Box::new(hid.clone()), COMMANDS.to_vec(), true);
        for refused in [
            "11ff0c5d000102",
            "11ff0d5a000101",
            "11ff0c5a",
            "11ff0c3a0006",
            "11ff0c3a0105",
            "11ff0c3a0600",
            "11ff0c3c0001",
            "11000c3a0001",
            "11ff0c0d",
            "11ff003a0001",
        ] {
            assert!(io.check(&bytes(refused)[..refused.len() / 2]).is_err(), "{refused}");
        }
        let change = |hex: &str| io.check(&bytes(hex)).unwrap();
        assert_eq!(change("11ff0c3a0001"), Effect::Change);
        assert_eq!(change("11ff001a0000"), Effect::Read);
        assert_eq!(change("11ff000a8070"), Effect::Read);
    }

    #[test]
    fn a_keyboard_that_lists_the_lighting_elsewhere_is_read_only_with_the_reason() {
        for (lighting, reason) in [(0x0D, "at index 0d, not at 0c"), (0x00, "does not list the colour LED effects")] {
            let hid = keyboard(lighting);
            let mut task = start(&hid, &format!("elsewhere-{lighting}"));
            let problem = task.info.problem.clone().expect("a problem");
            assert!(problem.contains(reason), "{problem}");
            assert!(!task.info.editable);
            assert!(task.set_blind("colour", &color("#ffffff")).is_err());
            assert!(commands(&hid).is_empty(), "nothing is sent to index 0c");
        }
    }

    #[test]
    fn a_keyboard_that_says_hidpp_1_or_answers_with_an_error_is_read_only() {
        let old = FakeHid::new(|req| {
            let mut answer = req.to_vec();
            answer[4..6].copy_from_slice(&[1, 0]);
            vec![answer]
        });
        old.0.lock().unwrap().numbered_output = true;
        let task = start(&old, "hidpp1");
        assert!(task.info.problem.as_deref().unwrap_or("").contains("HID++ 1.0"));
        assert!(!task.info.editable);

        // An error answer: 11 ff ff, the feature index, the function, the code.
        let error = FakeHid::new(|req| vec![bytes(&format!("11ffff{:02x}{:02x}09", req[2], req[3]))]);
        error.0.lock().unwrap().numbered_output = true;
        let task = start(&error, "error");
        assert!(task.info.problem.as_deref().unwrap_or("").contains("unsupported (0x09)"));
        assert!(!task.info.editable);
    }

    #[test]
    fn a_keyboard_that_does_not_answer_gets_its_page_and_test_report() {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().numbered_output = true;
        hid.0.lock().unwrap().descriptor = Some(vec![0x06, 0x43, 0xFF, 0x0A, 0x02, 0x06, 0xA1, 0x01, 0x85, 0x11, 0xC0]);
        let mut task = start(&hid, "silent");
        assert!(task.info.details[0].1.starts_with("no answer yet"));
        assert!(!task.info.editable);
        assert!(task.set_blind("colour", &color("#ffffff")).is_err());
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "kept after a refresh: {raw:?}");
        assert!(hid.sent().iter().all(|r| r[2] == ROOT), "only the version is asked");
    }

    #[test]
    fn an_error_to_a_lighting_command_is_shown_and_not_kept() {
        let hid = FakeHid::new(|req| {
            let mut answer = req.to_vec();
            match (req[2], req[3]) {
                (0x00, PROTOCOL_VERSION) => answer[4..6].copy_from_slice(&[4, 2]),
                (0x00, FEATURE_INDEX) => {
                    let index = if req[4..6] == [0x80, 0x70] { LIGHTING } else { 0 };
                    answer[4..7].copy_from_slice(&[index, 0, 0]);
                }
                _ => return vec![bytes(&format!("11ffff{:02x}{:02x}02", req[2], req[3]))],
            }
            vec![answer]
        });
        hid.0.lock().unwrap().numbered_output = true;
        let mut task = start(&hid, "refused");
        let error = task.set_blind("colour", &color("#ffffff")).unwrap_err();
        assert!(matches!(error, Error::Hidpp(HidppErrorCode(2))), "{error:?}");
        assert!(task.info.blind.iter().all(|b| b.last.is_none()), "an effect that was refused is not remembered");
        // Nor does a refused effect become the one the time is sent for.
        assert!(task.set_blind("effect", &BlindValue::Choice("Colour cycle".into())).is_err());
        assert!(task.set_blind("speed", &BlindValue::Number(2000)).is_err());
        assert_eq!(commands(&hid).len(), 2);
    }

    #[test]
    fn read_only_mode_and_a_read_only_connection_send_nothing_to_the_lighting() {
        let hid = keyboard(LIGHTING);
        let mut task = start(&hid, "read-only-mode");
        let on = crate::writes::test::read_only_mode();
        assert!(matches!(task.set_blind("colour", &color("#ffffff")), Err(Error::ReadOnly)));
        assert!(matches!(task.set_blind("effect", &BlindValue::Choice("Off".into())), Err(Error::ReadOnly)));
        drop(on);
        assert!(commands(&hid).is_empty());

        let hid = keyboard(LIGHTING);
        let mut task = start_with(&hid, "read-only-connection", false);
        assert!(!task.info.editable);
        assert!(task.set_blind("colour", &color("#ffffff")).is_err());
        assert!(commands(&hid).is_empty());
    }

    #[test]
    fn an_answer_with_another_device_index_is_still_the_answer() {
        let hid = FakeHid::new(|req| {
            let mut answer = req.to_vec();
            answer[1] = 0x00;
            match (req[2], req[3]) {
                (0x00, PROTOCOL_VERSION) => answer[4..6].copy_from_slice(&[4, 2]),
                (0x00, FEATURE_INDEX) => {
                    answer[4..7].copy_from_slice(&[if req[4..6] == [0x80, 0x70] { LIGHTING } else { 0 }, 0, 0])
                }
                _ => {}
            }
            vec![answer]
        });
        hid.0.lock().unwrap().numbered_output = true;
        let mut task = start(&hid, "index-0");
        assert!(task.info.editable, "{:?}", task.info.problem);
        task.set_blind("colour", &color("#102030")).unwrap();
        assert_eq!(commands(&hid).len(), 1);
    }

    #[test]
    fn a_keyboard_that_stops_answering_is_gone_and_not_made_read_only() {
        let silent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let quiet = silent.clone();
        let hid = FakeHid::new(move |req| {
            if quiet.load(std::sync::atomic::Ordering::SeqCst) {
                return Vec::new();
            }
            let mut answer = req.to_vec();
            match (req[2], req[3]) {
                (0x00, PROTOCOL_VERSION) => answer[4..6].copy_from_slice(&[4, 2]),
                (0x00, FEATURE_INDEX) => {
                    answer[4..7].copy_from_slice(&[if req[4..6] == [0x80, 0x70] { LIGHTING } else { 0 }, 0, 0])
                }
                _ => {}
            }
            vec![answer]
        });
        hid.0.lock().unwrap().numbered_output = true;
        let mut task = start(&hid, "stops");
        assert!(task.info.editable);
        silent.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(task.info(), Err(Error::Timeout)), "the worker ends, and the manager opens it again");
    }

    #[test]
    fn the_vendor_collection_of_long_reports_is_the_one_that_is_opened_and_only_on_windows() {
        assert!(is_command_collection(0xFF43, 0x0602));
        assert!(!is_command_collection(0xFF43, 0x0604), "the 64-byte reports");
        assert!(!is_command_collection(0xFF00, 0x0602));
        assert!(!is_command_collection(0x0001, 0x0006), "the keyboard");
        let model = find(0x046D, 0xC336).unwrap();
        assert_eq!((model.interface, model.usage_page), (Some(1), Some(0xFF43)));
        assert!(!opened_on_linux(model), "interface 1 also carries what is typed");
    }
}

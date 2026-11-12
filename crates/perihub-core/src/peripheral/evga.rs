//! EVGA X12 and X20 mice on their cable: their lighting. The protocol is in
//! `docs/protocols/evga-mice.md`, from facts found in OpenRGB's EVGA drivers (GPL: no code copied).
//! None has been tried with PeriHub.
//!
//! The X12's lighting is read back after every write, so it can be changed: off or steady, colour
//! and brightness of each zone, every change treated as one that stays (No warranty box, backup,
//! read-back and undo). The X20 is read only: its answers have no documented header to check, and
//! its writes take two reports whose order matters. The Z15 and Z20 keyboards are left out: their
//! lighting collection shares an interface that may carry what is typed.

use super::task::Driver;
use super::{Command, Effect, Io, Kind, Led, LedLimits, Model, Protocol, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, Result};

const EVGA: u16 = 0x3842;

/// The X12: feature report 7, 40 bytes with its number, on interface 1 (LED usage page 08).
mod x12 {
    pub const INTERFACE: i32 = 1;
    pub const USAGE_PAGE: u16 = 0x0008;
    pub const ID: u8 = 0x07;
    pub const LEN: usize = 40;
    pub const HEADER: u8 = 0xEA;
    pub const GET: u8 = 0x01;
    pub const SET: u8 = 0x02;
    pub const MODES: u8 = 0x01;
    pub const COLOURS: u8 = 0x02;
    /// Four slots in the firmware: front left, front right, wheel, logo. OpenRGB drives three LEDs
    /// (front left, wheel, logo); the second slot is written back as read.
    pub const SLOTS: usize = 4;
    pub const ZONES: [(usize, &str); 3] = [(0, "Front"), (2, "Wheel"), (3, "Logo")];
    /// Bytes 20–23 of a colour write: 1, which OpenRGB names "start with profile".
    pub const START_WITH_PROFILE: usize = 20;
    pub const OFF: u8 = 0;
    pub const STEADY: u8 = 1;
    pub const OFFERED: [(u8, &str); 2] = [(STEADY, "Steady"), (OFF, "Off")];
    /// How long the mouse needs before its answer can be read, and how many times it is read.
    pub const WAIT: std::time::Duration = std::time::Duration::from_millis(5);
    pub const READS: usize = 5;
}

/// The X20 on its cable: feature report 0, 64 bytes after the number, on interface 2 (FFFF).
mod x20 {
    pub const INTERFACE: i32 = 2;
    pub const USAGE_PAGE: u16 = 0xFFFF;
    pub const LEN: usize = 64;
    /// What every request starts with on the cable (connection byte 0), then the LED.
    pub const READ: [u8; 7] = [0x00, 0x00, 0x00, 0x1D, 0x02, 0x80, 0x02];
    pub const LED: usize = 7;
    pub const MODE: usize = 8;
    /// 0–100.
    pub const BRIGHTNESS: usize = 9;
    pub const SPEED: usize = 10;
    pub const COLOUR_COUNT: usize = 13;
    pub const COLOURS: usize = 14;
    /// First byte of the answers "not ready yet" and "asleep".
    pub const NOT_READY: u8 = 0xA0;
    pub const ASLEEP: u8 = 0xA4;
    pub const ZONES: [&str; 3] = ["Front", "Wheel", "Logo"];
    pub const MODES: [(u8, &str); 5] =
        [(1, "Steady"), (2, "Breathing"), (3, "Colour cycle"), (4, "Pulse"), (6, "On a click")];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Params {
    X12,
    X20,
}

const fn mouse(product_id: &'static [u16], name: &'static str, interface: i32, page: u16, params: Params) -> Model {
    Model {
        vendor_id: EVGA,
        product_ids: product_id,
        brand: "EVGA",
        name,
        kind: Kind::Mouse,
        interface: Some(interface),
        usage_page: Some(page),
        protocol: Protocol::Evga(params),
    }
}

/// The X20's wireless receiver (`2402`) and the keyboards are left out.
pub const MODELS: &[Model] = &[
    mouse(&[0x2422], "X12", x12::INTERFACE, x12::USAGE_PAGE, Params::X12),
    mouse(&[0x2420], "X20", x20::INTERFACE, x20::USAGE_PAGE, Params::X20),
];

const fn command(prefix: &'static [u8], effect: Effect, name: &'static str) -> Command {
    Command { prefix, at: 0, effect, name }
}

const X12_COMMANDS: [Command; 4] = [
    command(&[x12::ID, x12::HEADER, x12::GET, x12::MODES], Effect::Read, "read lighting modes"),
    command(&[x12::ID, x12::HEADER, x12::GET, x12::COLOURS], Effect::Read, "read lighting colours"),
    command(&[x12::ID, x12::HEADER, x12::SET, x12::MODES], Effect::Save, "store lighting modes"),
    command(&[x12::ID, x12::HEADER, x12::SET, x12::COLOURS], Effect::Save, "store lighting colours"),
];
const X20_COMMANDS: [Command; 1] = [command(&x20::READ, Effect::Read, "read the lighting of one LED")];

fn percent(raw: u8) -> u8 {
    ((raw as u32 * 100 + 127) / 255) as u8
}

fn raw(percent: u8) -> u8 {
    ((percent as u32 * 255 + 50) / 100) as u8
}

pub struct Evga {
    p: Params,
    /// The X12's two reports as last read.
    modes: [u8; x12::LEN],
    colours: [u8; x12::LEN],
    read_once: bool,
}

impl Evga {
    pub fn new(p: Params) -> Self {
        Self { p, modes: [0; x12::LEN], colours: [0; x12::LEN], read_once: false }
    }

    /// Asks the X12 for one of its reports and reads it back until it carries the header and the
    /// command asked for.
    fn x12_get(io: &mut Io, what: u8) -> Result<[u8; x12::LEN]> {
        let mut request = [0u8; x12::LEN];
        request[..4].copy_from_slice(&[x12::ID, x12::HEADER, x12::GET, what]);
        io.send_numbered(&request)?;
        let mut answer = [0u8; x12::LEN];
        for _ in 0..x12::READS {
            std::thread::sleep(x12::WAIT);
            let n = io.read_numbered(x12::ID, &mut answer)?;
            if n == x12::LEN && answer[1] == x12::HEADER && answer[3] == what {
                return Ok(answer);
            }
        }
        Err(Error::Timeout)
    }

    /// Writes one of the X12's reports and reads it back: the bytes it set must come back.
    fn x12_set(io: &mut Io, report: &[u8; x12::LEN], checked: std::ops::Range<usize>) -> Result<()> {
        io.send_numbered(report)?;
        let back = Self::x12_get(io, report[3])?;
        if back[checked.clone()] != report[checked] {
            return Err(Error::Protocol("the mouse did not keep the lighting it was sent".into()));
        }
        Ok(())
    }

    fn read_x12(&mut self, io: &mut Io) -> Result<Reading> {
        let modes = Self::x12_get(io, x12::MODES)?;
        let colours = Self::x12_get(io, x12::COLOURS)?;
        let mut reading = Reading::default();
        let known = |m: u8| x12::OFFERED.iter().any(|(o, _)| *o == m);
        let changeable = x12::ZONES.iter().all(|(slot, _)| known(modes[4 + slot]));
        for (slot, name) in x12::ZONES {
            let at = 4 + slot * 4;
            reading.settings.leds.push(Led {
                mode: modes[4 + slot],
                brightness: Some(percent(colours[at])),
                color: Rgb::from_bytes(&colours[at + 1..at + 4]),
            });
            // A mode set by EVGA's software that PeriHub does not know is shown, not changed.
            if changeable {
                reading.limits.leds.push(LedLimits {
                    name: name.into(),
                    modes: x12::OFFERED.to_vec(),
                    brightness_step: Some(1),
                    color: true,
                });
            }
        }
        if !changeable {
            for (slot, name) in x12::ZONES {
                let mode = modes[4 + slot];
                let label = if known(mode) { "" } else { "Set by EVGA's software" };
                reading.limits.leds.push(LedLimits {
                    name: name.into(),
                    modes: x12::OFFERED.iter().copied().chain((!label.is_empty()).then_some((mode, label))).collect(),
                    brightness_step: None,
                    color: false,
                });
            }
        }
        (self.modes, self.colours) = (modes, colours);
        Ok(reading)
    }

    fn read_x20(&mut self, io: &mut Io) -> Result<Reading> {
        io.forbid_changes();
        let mut reading = Reading::default();
        for (led, name) in x20::ZONES.iter().enumerate() {
            let mut request = [0u8; x20::LEN];
            request[..x20::READ.len()].copy_from_slice(&x20::READ);
            request[x20::LED] = led as u8;
            let a = io.feature_query(&request, x12::WAIT, |a| a[0] != x20::NOT_READY)?;
            let (mode, brightness, speed, count) =
                (a[x20::MODE], a[x20::BRIGHTNESS], a[x20::SPEED], a[x20::COLOUR_COUNT]);
            let known = x20::MODES.iter().any(|(m, _)| *m == mode);
            if a[0] == x20::ASLEEP || !known || brightness > 100 || speed > 100 || !(1..=7).contains(&count) {
                reading.problem = Some(format!(
                    "the {} LED reads mode {mode}, brightness {brightness}, speed {speed}, {count} colours",
                    name.to_lowercase()
                ));
                reading.settings.leds.clear();
                reading.limits.leds.clear();
                return Ok(reading);
            }
            reading.settings.leds.push(Led {
                mode,
                brightness: Some(brightness),
                color: Rgb::from_bytes(&a[x20::COLOURS..x20::COLOURS + 3]),
            });
            reading.limits.leds.push(LedLimits {
                name: (*name).into(),
                modes: x20::MODES.to_vec(),
                brightness_step: None,
                color: false,
            });
        }
        Ok(reading)
    }
}

impl Driver for Evga {
    fn commands(&self) -> Vec<Command> {
        match self.p {
            Params::X12 => X12_COMMANDS.to_vec(),
            Params::X20 => X20_COMMANDS.to_vec(),
        }
    }

    fn source(&self) -> &'static str {
        "facts from OpenRGB's EVGA drivers (no code copied)"
    }

    fn profiles(&self) -> u8 {
        0
    }

    fn can_save(&self) -> bool {
        false
    }

    /// Whether the X12 keeps a change after it is unplugged is not known: it is treated as one that
    /// stays, with the No warranty box, backup and undo.
    fn changes_stay(&self) -> bool {
        self.p == Params::X12
    }

    /// A mouse that does not answer its first reading gets a read-only page with the reason.
    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let result = match self.p {
            Params::X12 => self.read_x12(io),
            Params::X20 => self.read_x20(io),
        };
        match result {
            Ok(reading) => {
                self.read_once = true;
                Ok(reading)
            }
            Err(e @ (Error::Hid(_) | Error::Timeout | Error::Protocol(_))) if !self.read_once => Ok(Reading {
                problem: Some(format!("the mouse did not answer as documented: {e}")),
                ..Default::default()
            }),
            Err(e) => Err(e),
        }
    }

    /// The colour report first, then the modes, as OpenRGB does it; each written only if it
    /// changes, with every byte PeriHub does not change sent back as read.
    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        if self.p != Params::X12 {
            return Err(Error::Invalid("this mouse is read only".into()));
        }
        if from.leds.len() != x12::ZONES.len() || to.leds.len() != x12::ZONES.len() {
            return Err(Error::Invalid("lighting".into()));
        }
        let offered = |m: u8| x12::OFFERED.iter().any(|(o, _)| *o == m);
        let mut modes = self.modes;
        modes[1..3].copy_from_slice(&[x12::HEADER, x12::SET]);
        let mut colours = self.colours;
        colours[1..3].copy_from_slice(&[x12::HEADER, x12::SET]);
        for (((slot, _), old), new) in x12::ZONES.iter().zip(&from.leds).zip(&to.leds) {
            if !offered(old.mode) || !offered(new.mode) || self.modes[4 + slot] != old.mode {
                return Err(Error::Invalid(format!("lighting mode {}", new.mode)));
            }
            modes[4 + slot] = new.mode;
            let at = 4 + slot * 4;
            let brightness = new.brightness.filter(|b| *b <= 100).ok_or_else(|| Error::Invalid("brightness".into()))?;
            if percent(colours[at]) != brightness {
                colours[at] = raw(brightness);
            }
            colours[at + 1..at + 4].copy_from_slice(&new.color.to_bytes());
        }
        colours[x12::START_WITH_PROFILE..x12::START_WITH_PROFILE + x12::SLOTS].fill(1);
        let slots = 4..4 + x12::SLOTS * 4;
        if colours[slots.clone()] != self.colours[slots.clone()] {
            Self::x12_set(io, &colours, slots)?;
            self.colours[4..4 + x12::SLOTS * 4].copy_from_slice(&colours[4..4 + x12::SLOTS * 4]);
        }
        let slots = 4..4 + x12::SLOTS;
        if modes[slots.clone()] != self.modes[slots.clone()] {
            Self::x12_set(io, &modes, slots)?;
            self.modes[4..4 + x12::SLOTS].copy_from_slice(&modes[4..4 + x12::SLOTS]);
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("every change is stored as it is made".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    struct X12 {
        modes: [u8; x12::SLOTS],
        colours: [[u8; 4]; x12::SLOTS],
        /// The report the next read returns.
        next: Option<u8>,
        sent: Vec<Vec<u8>>,
        /// Answers with another header.
        wrong_header: bool,
        /// Drops the colour writes.
        deaf: bool,
    }

    type Shared = Arc<Mutex<X12>>;

    /// Steady on every slot: red front, green wheel, blue logo at full brightness; the second slot
    /// holds something of its own.
    fn x12_mouse() -> Shared {
        Arc::new(Mutex::new(X12 {
            modes: [1, 1, 1, 1],
            colours: [[0xff, 0xff, 0, 0], [0x10, 1, 2, 3], [0xff, 0, 0xff, 0], [0x80, 0, 0, 0xff]],
            next: None,
            sent: Vec::new(),
            wrong_header: false,
            deaf: false,
        }))
    }

    fn x12_fake(state: Shared) -> FakeHid {
        let set_state = state.clone();
        let set = move |r: &[u8]| {
            let mut m = set_state.lock().unwrap();
            assert_eq!((r.len(), r[0], r[1]), (x12::LEN, x12::ID, x12::HEADER));
            m.sent.push(r.to_vec());
            match (r[2], r[3]) {
                (x12::GET, what) => m.next = Some(what),
                (x12::SET, x12::MODES) => m.modes.copy_from_slice(&r[4..8]),
                (x12::SET, x12::COLOURS) if !m.deaf => {
                    for s in 0..x12::SLOTS {
                        m.colours[s].copy_from_slice(&r[4 + s * 4..8 + s * 4]);
                    }
                }
                (x12::SET, x12::COLOURS) => {}
                other => panic!("unexpected {other:02x?}"),
            }
            true
        };
        let get = move |id: u8| {
            let m = state.lock().unwrap();
            assert_eq!(id, x12::ID);
            let what = m.next?;
            let mut a = vec![0u8; x12::LEN];
            a[..4].copy_from_slice(&[x12::ID, if m.wrong_header { 0xEB } else { x12::HEADER }, 0, what]);
            match what {
                x12::MODES => a[4..8].copy_from_slice(&m.modes),
                _ => {
                    for s in 0..x12::SLOTS {
                        a[4 + s * 4..8 + s * 4].copy_from_slice(&m.colours[s]);
                    }
                }
            }
            Some(a)
        };
        let hid = FakeHid::new(|_| panic!("no output reports"));
        hid.0.lock().unwrap().numbered = Some((Box::new(set), Box::new(get)));
        hid
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-evga-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn writes(state: &Shared) -> Vec<Vec<u8>> {
        state.lock().unwrap().sent.iter().filter(|r| r[2] == x12::SET).cloned().collect()
    }

    const RED: Rgb = Rgb { r: 0xff, g: 0, b: 0 };
    const BLUE: Rgb = Rgb { r: 0, g: 0, b: 0xff };

    #[test]
    fn the_x12_reads_its_three_zones() {
        let state = x12_mouse();
        let model = find(EVGA, 0x2422).unwrap();
        let task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("read")).unwrap();
        assert!(task.info.problem.is_none() && task.info.editable && task.info.changes_stay);
        let leds = &task.info.settings.leds;
        assert_eq!(leds[0], Led { mode: 1, brightness: Some(100), color: RED });
        assert_eq!(leds[2], Led { mode: 1, brightness: Some(50), color: BLUE });
        assert_eq!(
            task.info.limits.leds.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            ["Front", "Wheel", "Logo"]
        );
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn a_new_colour_writes_the_colour_report_with_the_rest_as_read_and_is_undone_unless_kept() {
        let state = x12_mouse();
        let before = state.lock().unwrap().colours;
        let model = find(EVGA, 0x2422).unwrap();
        let mut task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("colour")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[1].color = BLUE;
        task.change(&wanted, false, None).unwrap();
        let sent = writes(&state);
        assert_eq!(sent.len(), 1, "the colours only: no mode changed");
        assert_eq!(&sent[0][..4], &[x12::ID, x12::HEADER, x12::SET, x12::COLOURS]);
        assert_eq!(&sent[0][4..20], &[0xff, 0xff, 0, 0, 0x10, 1, 2, 3, 0xff, 0, 0, 0xff, 0x80, 0, 0, 0xff]);
        assert_eq!(&sent[0][20..24], &[1, 1, 1, 1]);
        assert_eq!(task.info.settings, wanted);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().colours, before);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn off_writes_only_the_modes() {
        let state = x12_mouse();
        let model = find(EVGA, 0x2422).unwrap();
        let mut task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("off")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[2].mode = x12::OFF;
        task.change(&wanted, false, None).unwrap();
        let sent = writes(&state);
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][..8], &[x12::ID, x12::HEADER, x12::SET, x12::MODES, 1, 1, 1, 0]);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_write_the_mouse_does_not_keep_is_reported_and_put_back() {
        let state = x12_mouse();
        state.lock().unwrap().deaf = true;
        let model = find(EVGA, 0x2422).unwrap();
        let mut task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("deaf")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = BLUE;
        wanted.leds[1].mode = x12::OFF;
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("did not keep the lighting"), "{error}");
        assert!(writes(&state).iter().all(|r| r[3] == x12::COLOURS), "no mode written after the colours failed");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_mode_from_evgas_software_is_shown_and_left_alone() {
        let state = x12_mouse();
        state.lock().unwrap().modes = [1, 1, 5, 1];
        let model = find(EVGA, 0x2422).unwrap();
        let mut task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("mode")).unwrap();
        assert!(task.info.problem.is_none());
        assert!(task.info.limits.leds.iter().all(|l| !l.color && l.brightness_step.is_none()));
        let mut wanted = task.info.settings.clone();
        // The wheel, in slot 2, is in EVGA's mode 5.
        wanted.leds[1].mode = x12::STEADY;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn answers_without_the_header_leave_it_read_only() {
        let state = x12_mouse();
        state.lock().unwrap().wrong_header = true;
        let model = find(EVGA, 0x2422).unwrap();
        let task = start_for_test(Box::new(x12_fake(state.clone())), model, 0x2422, true, temp("header")).unwrap();
        assert!(task.info.problem.is_some() && !task.info.editable);
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = x12_mouse();
        let mut io = Io::new(Box::new(x12_fake(state.clone())), Evga::new(Params::X12).commands(), true);
        for head in
            [[x12::ID, x12::HEADER, x12::SET, 0x03], [x12::ID, 0xEB, x12::SET, x12::MODES], [0x06, x12::HEADER, 1, 1]]
        {
            let mut report = [0u8; x12::LEN];
            report[..4].copy_from_slice(&head);
            assert!(io.send_numbered(&report).is_err(), "{head:02x?}");
        }
        assert!(state.lock().unwrap().sent.is_empty());
        let mut x20 = Io::new(Box::new(x12_fake(state.clone())), Evga::new(Params::X20).commands(), true);
        let mut write = [0u8; x20::LEN];
        write[..7].copy_from_slice(&[0x00, 0x00, 0x00, 0x1D, 0x02, 0x00, 0x02]);
        assert!(x20.send(&write, true).is_err(), "an X20 write");
    }

    /// The X20: what each LED answers, by LED.
    fn x20_fake(answers: [Vec<u8>; 3], not_ready_first: bool) -> FakeHid {
        let hid = FakeHid::new(|_| panic!("no output reports"));
        let mut not_ready = not_ready_first;
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |r: &[u8]| {
            assert_eq!(&r[..7], &x20::READ, "reads only");
            if std::mem::take(&mut not_ready) {
                let mut a = vec![0u8; x20::LEN];
                a[..7].copy_from_slice(&[x20::NOT_READY, 0, 0, 0x1D, 0x02, 0x80, 0x02]);
                return a;
            }
            answers[r[x20::LED] as usize].clone()
        }));
        hid
    }

    fn x20_led(mode: u8, brightness: u8, colour: [u8; 3]) -> Vec<u8> {
        let mut a = vec![0u8; x20::LEN];
        a[..7].copy_from_slice(&x20::READ);
        a[x20::MODE] = mode;
        a[x20::BRIGHTNESS] = brightness;
        a[x20::SPEED] = 50;
        a[x20::COLOUR_COUNT] = 1;
        a[x20::COLOURS..x20::COLOURS + 3].copy_from_slice(&colour);
        a
    }

    #[test]
    fn the_x20_is_read_only() {
        let leds = [x20_led(1, 100, [0xff, 0, 0]), x20_led(2, 40, [0, 0xff, 0]), x20_led(3, 80, [0, 0, 0xff])];
        let hid = x20_fake(leds, false);
        let model = find(EVGA, 0x2420).unwrap();
        let mut task = start_for_test(Box::new(hid.clone()), model, 0x2420, true, temp("x20")).unwrap();
        assert!(task.info.problem.is_none() && !task.info.editable && !task.info.changes_stay);
        assert_eq!(
            task.info.settings.leds[1],
            Led { mode: 2, brightness: Some(40), color: Rgb { r: 0, g: 0xff, b: 0 } }
        );
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = BLUE;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(hid.sent().iter().all(|r| r[..7] == x20::READ));
    }

    #[test]
    fn an_x20_answer_out_of_range_is_a_problem() {
        let leds = [x20_led(1, 100, [0xff, 0, 0]), x20_led(9, 40, [0, 0xff, 0]), x20_led(3, 80, [0, 0, 0xff])];
        let model = find(EVGA, 0x2420).unwrap();
        let task = start_for_test(Box::new(x20_fake(leds, false)), model, 0x2420, true, temp("x20-odd")).unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("wheel")));
        assert!(task.info.settings.leds.is_empty());
    }

    #[test]
    fn percent_and_back_is_exact() {
        for p in 0..=100 {
            assert_eq!(percent(raw(p)), p);
        }
    }
}

//! Cooler Master MM530, MM711, MM720 and MM730: their lighting. The protocol is in
//! `docs/protocols/coolermaster-mm.md`, from the USB captures of Cooler Master's own app attached
//! to OpenRGB's device requests (facts only, no code). None has been tried with PeriHub.
//!
//! Every change goes through the official app's save sequence, so every change is one that stays
//! (No warranty box, backup, read-back and undo). PeriHub changes the colour and brightness of the
//! mode in use and switches between steady, breathing, colour cycle and off; it never sends the
//! per-zone, multilayer or button commands.

use std::collections::BTreeMap;
use std::time::Duration;

use super::task::Driver;
use super::{Command, Effect, Io, Kind, Led, LedLimits, Model, Protocol, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, Result};

/// Interface 1 holds one vendor collection (usage page FF00): 64-byte reports without numbers.
const INTERFACE: i32 = 1;
const USAGE_PAGE: u16 = 0xFF00;
const REPORT: usize = 64;
const TIMEOUT: Duration = Duration::from_millis(250);

const BEGIN: [u8; 2] = [0x41, 0x80];
const READ_MODE: [u8; 2] = [0x52, 0x28];
const SELECT_MODE: [u8; 2] = [0x51, 0x28];
const SAVE_SEQUENCE: [[u8; 2]; 3] = [[0x41, 0x01], [0x50, 0x55], [0x41, 0x00]];
const READ: u8 = 0x52;
const WRITE: u8 = 0x51;

const STEADY: u8 = 0x00;
const BREATHING: u8 = 0x01;
const CYCLE: u8 = 0x02;
const OFF: u8 = 0xFE;
/// The modes PeriHub switches between; their settings are read every time.
const OFFERED: [(u8, &str); 4] = [(STEADY, "Steady"), (BREATHING, "Breathing"), (CYCLE, "Colour cycle"), (OFF, "Off")];
/// Modes a mouse may be in that PeriHub shows but does not change.
const SHOWN: [u8; 4] = [0x03, 0x04, 0xB0, 0xE0];
const WITH_SETTINGS: [u8; 3] = [STEADY, BREATHING, CYCLE];

/// The family's command byte: `2C` on the MM530, `2B` on the others.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    command: u8,
}

const MM530: Params = Params { command: 0x2C };
const MM7XX: Params = Params { command: 0x2B };

const fn mouse(product_id: &'static [u16], name: &'static str, params: Params) -> Model {
    Model {
        vendor_id: 0x2516,
        product_ids: product_id,
        brand: "Cooler Master",
        name,
        kind: Kind::Mouse,
        interface: Some(INTERFACE),
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::CoolerMaster(params),
    }
}

/// The MM531 is left out (OpenRGB keeps it disabled), and so is the MM712 (another protocol).
pub const MODELS: &[Model] = &[
    mouse(&[0x0065], "MM530", MM530),
    mouse(&[0x0101], "MM711", MM7XX),
    mouse(&[0x0141], "MM720", MM7XX),
    mouse(&[0x0165], "MM730", MM7XX),
];

const fn command(prefix: &'static [u8], effect: Effect, name: &'static str) -> Command {
    Command { prefix, at: 0, effect, name }
}

/// What the official app sends before every command. Its effect is not documented, so it counts
/// as a change: a read-only connection never sends it.
const COMMON: [Command; 6] = [
    command(&BEGIN, Effect::Change, "begin a command"),
    command(&READ_MODE, Effect::Read, "read the mode in use"),
    command(&SELECT_MODE, Effect::Save, "select the mode"),
    command(&SAVE_SEQUENCE[0], Effect::Save, "begin storing"),
    command(&SAVE_SEQUENCE[1], Effect::Save, "store in memory"),
    command(&SAVE_SEQUENCE[2], Effect::Save, "end"),
];

/// The settings one mode keeps, as read after its number: speed, flag, two more bytes, brightness
/// (0–255), red, green, blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stored([u8; 8]);

const BRIGHTNESS: usize = 4;
const COLOR: usize = 5;

impl Stored {
    fn brightness(&self) -> u8 {
        self.0[BRIGHTNESS]
    }
    fn color(&self) -> Rgb {
        Rgb::from_bytes(&self.0[COLOR..COLOR + 3])
    }
    /// These settings with another brightness (in percent) and colour; a brightness that reads the
    /// same keeps its byte.
    fn with(&self, brightness: u8, color: Rgb) -> Stored {
        let mut bytes = self.0;
        if percent(self.brightness()) != brightness {
            bytes[BRIGHTNESS] = raw(brightness);
        }
        bytes[COLOR..COLOR + 3].copy_from_slice(&color.to_bytes());
        Stored(bytes)
    }
}

fn percent(raw: u8) -> u8 {
    ((raw as u32 * 100 + 127) / 255) as u8
}

fn raw(percent: u8) -> u8 {
    ((percent as u32 * 255 + 50) / 100) as u8
}

pub struct CoolerMaster {
    p: Params,
    /// The mode in use, as last read.
    active: Option<u8>,
    stored: BTreeMap<u8, Stored>,
    read_once: bool,
}

impl CoolerMaster {
    pub fn new(p: Params) -> Self {
        Self { p, active: None, stored: BTreeMap::new(), read_once: false }
    }

    /// Sends `bytes` (64 with zeros after them) and returns the answer whose first two bytes are
    /// the request's; the reports the mouse sends by itself are skipped.
    fn exchange(io: &mut Io, bytes: &[u8]) -> Result<[u8; REPORT]> {
        let mut request = [0u8; REPORT];
        request[..bytes.len()].copy_from_slice(bytes);
        let mut answer = [0u8; REPORT];
        io.output_exchange(&request, &mut answer, TIMEOUT, |a| a[..2] == request[..2])?;
        Ok(answer)
    }

    /// `41 80`, then the command, as the official app does it.
    fn command(io: &mut Io, bytes: &[u8]) -> Result<[u8; REPORT]> {
        Self::exchange(io, &BEGIN)?;
        Self::exchange(io, bytes)
    }

    /// A write is taken only when the mouse echoes it.
    fn write(io: &mut Io, bytes: &[u8]) -> Result<()> {
        let answer = Self::command(io, bytes)?;
        if answer[..bytes.len()] != *bytes {
            return Err(Error::Protocol(format!("the mouse did not echo {}", super::hex(&bytes[..2]))));
        }
        Ok(())
    }

    fn read_settings(&self, io: &mut Io, mode: u8) -> Result<Option<Stored>> {
        let answer = Self::command(io, &[READ, self.p.command, 0, 0, mode])?;
        if answer[4] != mode {
            return Ok(None);
        }
        Ok(Some(Stored(answer[5..13].try_into().expect("8 bytes"))))
    }

    fn read_all(&mut self, io: &mut Io) -> Result<Reading> {
        self.active = None;
        self.stored.clear();
        let mut reading = Reading::default();
        let active = Self::command(io, &READ_MODE)?[4];
        if !OFFERED.iter().any(|(m, _)| *m == active) && !SHOWN.contains(&active) {
            reading.problem = Some(format!("lighting mode {active:02x}"));
            return Ok(reading);
        }
        for mode in WITH_SETTINGS {
            match self.read_settings(io, mode)? {
                Some(s) => {
                    self.stored.insert(mode, s);
                }
                None => {
                    reading.problem = Some(format!("the settings of mode {mode:02x} came back for another mode"));
                    return Ok(reading);
                }
            }
        }
        self.active = Some(active);
        let shown = self.stored.get(&active).unwrap_or(&self.stored[&STEADY]);
        reading.settings.leds.push(Led {
            mode: active,
            brightness: Some(percent(shown.brightness())),
            color: shown.color(),
        });
        if OFFERED.iter().any(|(m, _)| *m == active) {
            reading.limits.leds.push(LedLimits {
                name: "Lighting".into(),
                modes: OFFERED.to_vec(),
                brightness_step: Some(1),
                color: true,
            });
        }
        Ok(reading)
    }

    /// The colour and brightness a mode shows: its own, or the steady mode's when it is off.
    fn shown(&self, mode: u8) -> Option<&Stored> {
        self.stored.get(&mode).or_else(|| self.stored.get(&STEADY))
    }
}

impl Driver for CoolerMaster {
    fn commands(&self) -> Vec<Command> {
        let (read, write): (&'static [u8], &'static [u8]) = match self.p.command {
            0x2C => (&[READ, 0x2C], &[WRITE, 0x2C]),
            _ => (&[READ, 0x2B], &[WRITE, 0x2B]),
        };
        let mut commands = COMMON.to_vec();
        commands.push(command(read, Effect::Read, "read a mode's settings"));
        commands.push(command(write, Effect::Save, "write a mode's settings"));
        commands
    }

    fn source(&self) -> &'static str {
        "captures of Cooler Master's app (OpenRGB issues)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        true
    }

    /// A new mode keeps its own stored colour and brightness: the page's are replaced by them.
    fn normalize(&self, wanted: &Settings) -> Settings {
        let mut wanted = wanted.clone();
        if let (Some(led), Some(active)) = (wanted.leds.first_mut(), self.active)
            && led.mode != active
            && let Some(s) = self.shown(led.mode)
        {
            led.brightness = Some(percent(s.brightness()));
            led.color = s.color();
        }
        wanted
    }

    /// A mouse that does not answer its first reading, or a connection that may not send the
    /// command the reads need, gets a read-only page with the reason.
    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        match self.read_all(io) {
            Ok(reading) => {
                self.read_once = true;
                Ok(reading)
            }
            Err(Error::Invalid(why)) if !io.changes_allowed() => Ok(Reading {
                problem: Some(format!(
                    "this mouse answers only after a command whose effect is not documented, which PeriHub sends only from its app ({why})"
                )),
                ..Default::default()
            }),
            Err(e @ (Error::Hid(_) | Error::Timeout | Error::Protocol(_))) if !self.read_once => Ok(Reading {
                problem: Some(format!("the mouse did not answer as documented: {e}")),
                ..Default::default()
            }),
            Err(e) => Err(e),
        }
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let ([old], [new]) = (from.leds.as_slice(), to.leds.as_slice()) else {
            return Err(Error::Invalid("lighting".into()));
        };
        if old == new {
            return Ok(());
        }
        let offered = |m: u8| OFFERED.iter().any(|(o, _)| *o == m);
        if !offered(old.mode) || !offered(new.mode) || self.active != Some(old.mode) {
            return Err(Error::Invalid(format!("lighting mode {:02x}", new.mode)));
        }
        let brightness = new.brightness.filter(|b| *b <= 100).ok_or_else(|| Error::Invalid("brightness".into()))?;
        // Everything is checked before the first write. Off has no settings of its own.
        let settings = match self.stored.get(&new.mode) {
            Some(s) => {
                let mut write = vec![WRITE, self.p.command, 0, 0, new.mode];
                write.extend_from_slice(&s.with(brightness, new.color).0);
                Some(write)
            }
            None if new.mode == OFF => None,
            None => return Err(Error::Invalid(format!("lighting mode {:02x}", new.mode))),
        };
        if let Some(write) = settings {
            Self::write(io, &write)?;
        }
        Self::write(io, &[SELECT_MODE[0], SELECT_MODE[1], 0, 0, new.mode])?;
        for step in SAVE_SEQUENCE {
            let answer = Self::exchange(io, &step)?;
            if answer[..2] != step {
                return Err(Error::Protocol("the mouse did not echo the save".into()));
            }
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("every change is stored as it is made".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// A simulated mouse: the mode in use, each mode's stored settings, what was saved, and every
    /// report it got. With `chatty` it sends a report of its own before each answer.
    struct Mouse {
        command: u8,
        active: u8,
        modes: BTreeMap<u8, [u8; 8]>,
        saved: usize,
        sent: Vec<Vec<u8>>,
        chatty: bool,
        /// Answers a mode's settings with this mode's number instead.
        wrong_mode: Option<u8>,
        /// Answers a write without repeating it.
        no_echo: bool,
    }

    type Shared = Arc<Mutex<Mouse>>;

    /// The MM720 as read in the captures: steady red, breathing at speed 31, colour cycle at 28.
    fn mm720() -> Shared {
        let modes = BTreeMap::from([
            (STEADY, [0x03, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]),
            (BREATHING, [0x31, 0x20, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]),
            (CYCLE, [0x28, 0x00, 0xff, 0xff, 0x7f, 0xff, 0xff, 0xff]),
        ]);
        Arc::new(Mutex::new(Mouse {
            command: 0x2B,
            active: STEADY,
            modes,
            saved: 0,
            sent: Vec::new(),
            chatty: false,
            wrong_mode: None,
            no_echo: false,
        }))
    }

    fn answer(m: &mut Mouse, r: &[u8]) -> Vec<Vec<u8>> {
        assert_eq!(r.len(), REPORT, "64-byte reports without a number");
        m.sent.push(r.to_vec());
        let mut a = r.to_vec();
        match (r[0], r[1]) {
            (0x52, 0x28) => a[4] = m.active,
            (0x52, c) if c == m.command => {
                let mode = r[4];
                a[4] = m.wrong_mode.unwrap_or(mode);
                if let Some(s) = m.modes.get(&mode) {
                    a[5..13].copy_from_slice(s);
                }
            }
            (0x51, 0x28) => m.active = r[4],
            (0x51, c) if c == m.command => {
                m.modes.insert(r[4], r[5..13].try_into().unwrap());
                if m.no_echo {
                    a[5..].fill(0);
                }
            }
            (0x50, 0x55) => m.saved += 1,
            (0x41, _) => {}
            other => panic!("unexpected {other:02x?}"),
        }
        let mut reports = Vec::new();
        if m.chatty {
            let mut own = vec![0u8; REPORT];
            own[..6].copy_from_slice(&[0x43, 0x01, 0, 0, 0, 0xc0]);
            reports.push(own);
        }
        reports.push(a);
        reports
    }

    fn fake(state: Shared) -> FakeHid {
        FakeHid::new(move |r| answer(&mut state.lock().unwrap(), r))
    }

    fn model(id: u16) -> &'static Model {
        find(0x2516, id).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-coolermaster-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// Every report sent, by its first two bytes, after `from`.
    fn heads(state: &Shared, from: usize) -> Vec<[u8; 2]> {
        state.lock().unwrap().sent[from..].iter().map(|r| [r[0], r[1]]).collect()
    }

    fn writes(state: &Shared) -> Vec<Vec<u8>> {
        state.lock().unwrap().sent.iter().filter(|r| r[0] == 0x51 || r[0] == 0x50).cloned().collect()
    }

    #[test]
    fn the_lighting_reads_as_captured() {
        let state = mm720();
        let task = start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("read")).unwrap();
        assert!(task.info.problem.is_none() && task.info.editable && task.info.changes_stay);
        assert_eq!(
            task.info.settings.leds,
            [Led { mode: STEADY, brightness: Some(100), color: Rgb { r: 0xff, g: 0, b: 0 } }]
        );
        assert_eq!(task.info.limits.leds[0].modes, OFFERED);
        // `41 80` before every read, as the official app sends it.
        let sent = heads(&state, 0);
        assert!(sent.chunks(2).all(|pair| pair[0] == BEGIN), "{sent:02x?}");
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn the_command_line_sends_nothing_and_says_why() {
        let state = mm720();
        let task = start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, false, temp("cli")).unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("not documented")));
        assert!(state.lock().unwrap().sent.is_empty());
    }

    #[test]
    fn a_new_colour_follows_the_official_save_sequence_and_is_undone_unless_kept() {
        let state = mm720();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("blue")).unwrap();
        let from = state.lock().unwrap().sent.len();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb { r: 0, g: 0, b: 0xff };
        task.change(&wanted, false, None).unwrap();
        let written = writes(&state);
        // The bytes Cooler Master's app sent for steady blue on the MM720.
        let blue = [0x51, 0x2b, 0, 0, 0, 0x03, 0, 0xff, 0xff, 0xff, 0, 0, 0xff];
        assert_eq!(written[0][..13], blue);
        assert!(written[0][13..].iter().all(|&b| b == 0));
        assert_eq!(heads(&state, from)[..2], [BEGIN, READ_MODE], "the change reads the mouse first");
        let tail: Vec<[u8; 2]> = heads(&state, from).into_iter().skip_while(|h| *h != [0x51, 0x2b]).take(7).collect();
        assert_eq!(tail, [[0x51, 0x2b], BEGIN, [0x51, 0x28], [0x41, 0x01], [0x50, 0x55], [0x41, 0x00], BEGIN]);
        assert_eq!(state.lock().unwrap().saved, 1);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().modes[&STEADY], [0x03, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]);
        assert_eq!(state.lock().unwrap().saved, 2);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_write_the_mouse_does_not_echo_is_put_back() {
        let state = mm720();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("echo")).unwrap();
        state.lock().unwrap().no_echo = true;
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb { r: 0, g: 0, b: 0xff };
        assert!(task.change(&wanted, false, None).is_err());
        let m = state.lock().unwrap();
        assert_eq!(m.modes[&STEADY][COLOR..], [0xff, 0, 0], "the settings from before");
        drop(m);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_new_mode_keeps_its_own_settings() {
        let state = mm720();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("mode")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = BREATHING;
        wanted.leds[0].color = Rgb { r: 0, g: 0xff, b: 0 };
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(
            info.settings.leds[0],
            Led { mode: BREATHING, brightness: Some(100), color: Rgb { r: 0xff, g: 0, b: 0 } }
        );
        let m = state.lock().unwrap();
        assert_eq!(m.active, BREATHING);
        assert_eq!(m.modes[&BREATHING], [0x31, 0x20, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00], "written back as read");
        drop(m);
        task.keep_changes();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn off_sends_only_the_mode_and_the_save() {
        let state = mm720();
        let mut task = start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("off")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = OFF;
        task.change(&wanted, false, None).unwrap();
        // As in the capture of Cooler Master's app turning the MM720 off.
        let written = writes(&state);
        assert_eq!(written.len(), 2);
        assert_eq!(written[0][..5], [0x51, 0x28, 0, 0, 0xfe]);
        assert_eq!(written[1][..2], [0x50, 0x55]);
        assert_eq!(task.info.settings.leds[0].mode, OFF);
        task.keep_changes();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_brightness_left_alone_keeps_its_byte() {
        let state = mm720();
        state.lock().unwrap().modes.get_mut(&STEADY).unwrap()[BRIGHTNESS] = 0x2c;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("byte")).unwrap();
        assert_eq!(task.info.settings.leds[0].brightness, Some(17));
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb { r: 0, g: 0xff, b: 0 };
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().modes[&STEADY][BRIGHTNESS], 0x2c);
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(40);
        task.keep_changes();
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().modes[&STEADY][BRIGHTNESS], 102);
        task.keep_changes();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn reports_the_mouse_sends_by_itself_are_skipped() {
        let state = mm720();
        state.lock().unwrap().chatty = true;
        let task = start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("chatty")).unwrap();
        assert!(task.info.problem.is_none());
        assert_eq!(task.info.settings.leds[0].color, Rgb { r: 0xff, g: 0, b: 0 });
    }

    #[test]
    fn a_mode_it_does_not_change_is_shown_and_left_alone() {
        let state = mm720();
        state.lock().unwrap().active = 0xE0;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("multi")).unwrap();
        assert!(task.info.problem.is_none() && task.info.limits.leds.is_empty());
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = STEADY;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn answers_that_do_not_look_as_documented_leave_it_read_only() {
        for (name, damage) in [("unknown mode", 0x07u8), ("settings of another mode", 0xff)] {
            let state = mm720();
            if damage == 0x07 {
                state.lock().unwrap().active = 0x07;
            } else {
                state.lock().unwrap().wrong_mode = Some(0x03);
            }
            let task = start_for_test(Box::new(fake(state.clone())), model(0x0141), 0x0141, true, temp("odd")).unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
            assert!(writes(&state).is_empty(), "{name}");
        }
    }

    #[test]
    fn the_mm530_uses_its_own_command_byte() {
        let state = mm720();
        state.lock().unwrap().command = 0x2C;
        let mut task = start_for_test(Box::new(fake(state.clone())), model(0x0065), 0x0065, true, temp("530")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].color = Rgb { r: 0, g: 0, b: 0xff };
        task.change(&wanted, false, None).unwrap();
        assert_eq!(writes(&state)[0][..2], [0x51, 0x2c]);
        task.keep_changes();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = mm720();
        let commands = CoolerMaster::new(MM7XX).commands();
        let mut io = Io::new(Box::new(fake(state.clone())), commands, true);
        for bytes in [[0x51, 0xa8], [0x51, 0x30], [0x51, 0xa0], [0x41, 0x03], [0x52, 0x18], [0x51, 0x10]] {
            assert!(CoolerMaster::exchange(&mut io, &bytes).is_err(), "{bytes:02x?}");
        }
        assert!(state.lock().unwrap().sent.is_empty());
        assert!(find(0x2516, 0x0097).is_none() && find(0x2516, 0x0169).is_none(), "MM531 and MM712");
    }

    #[test]
    fn percent_and_back_is_exact() {
        for p in 0..=100u8 {
            assert_eq!(percent(raw(p)), p);
        }
    }
}

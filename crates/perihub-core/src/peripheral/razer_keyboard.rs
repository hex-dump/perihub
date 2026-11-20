//! Razer keyboards, on their cable and through their own HyperSpeed receiver. The protocol is the
//! one of the Razer mice (`docs/protocols/razer-mice.md`); the keyboards' own facts are in
//! `docs/protocols/razer-keyboards.md`, and per model in [`super::razer_keyboard_models`], written by
//! `tools/openrazer_facts.py` from facts found in OpenRazer (GPL-2.0): no code was copied. Nobody has
//! tried one with PeriHub yet.
//!
//! Every keyboard is read: firmware, layout, and where the keyboard has them, battery, charging,
//! brightness and game mode. The BlackWidow V4 Mini HyperSpeed (`Params::change`) is also changed, as an
//! experimental feature: the backlight brightness and the game mode, which OpenRazer sends with
//! "variable storage" and which PeriHub therefore treats as stored (No warranty box, backup, read-back,
//! undo), and one colour on the custom frame, shown without storage.
//!
//! The lighting effects of the keyboard's own memory are never sent: OpenRazer stores every one of them
//! and has no command that reads the effect in use, so one could not be undone.
//!
//! The reports go to the USB interface OpenRazer sends to (1, 2 or 3). On Linux 70-perihub.rules
//! opens it only where a tester showed it carries no typed keys (the BlackWidow V4 Mini HyperSpeed's
//! interface 3), and a keyboard node that carries typed keys is never opened anyway (see
//! [`super::node_carries_typing`]).

use super::razer::{exchange, percent, raw};
use super::task::Driver;
use super::{
    BlindKind, BlindSetting, BlindValue, Choice, Command, Effect, Io, Led, LedLimits, Limits, Reading, Settings,
};
use crate::color::Rgb;
use crate::error::{Error, Result};

const NOSTORE: u8 = 0x00;
const VARSTORE: u8 = 0x01;
/// The LED id of the keyboard's backlight, and of the game mode light.
const BACKLIGHT_LED: u8 = 0x05;
const GAME_LED: u8 = 0x08;

/// (class, id) of the commands, at bytes 6 and 7 of the report.
const GET_FIRMWARE: [u8; 2] = [0x00, 0x81];
const GET_BATTERY: [u8; 2] = [0x07, 0x80];
const GET_CHARGING: [u8; 2] = [0x07, 0x84];
const GET_BRIGHTNESS: [u8; 2] = [0x0F, 0x84];
/// The standard brightness read of the older keyboards.
const GET_STANDARD_BRIGHTNESS: [u8; 2] = [0x03, 0x83];
const GET_LED_STATE: [u8; 2] = [0x03, 0x80];
/// The keyboard's region, so its physical layout (see [`crate::kblayout::razer`]).
const GET_LAYOUT: [u8; 2] = [0x00, 0x86];
/// The commands of a change: the brightness of the backlight, the state of one LED, one row of the
/// custom frame and the effect that shows it.
const SET_BRIGHTNESS: [u8; 2] = [0x0F, 0x04];
const SET_LED_STATE: [u8; 2] = [0x03, 0x00];
const SET_FRAME: [u8; 2] = [0x0F, 0x03];
const SHOW_FRAME: [u8; 2] = [0x0F, 0x02];
/// The custom frame effect number.
const CUSTOM_EFFECT: u8 = 0x08;
/// Packet size of a row of the custom frame.
const FRAME_SIZE: u8 = 0x47;
/// The most rows of a matrix OpenRazer gives a keyboard.
const MAX_ROWS: usize = 6;
/// The ids of the game mode setting and of the blind colour setting.
const GAME_MODE: &str = "game_mode";
const COLOR: &str = "color";

/// Everything PeriHub may send to a Razer keyboard it only reads: reads only, the lights' with the
/// stored values and the one LED each.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &GET_FIRMWARE, at: 6, effect: Effect::Read, name: "read firmware" },
    Command { prefix: &GET_BATTERY, at: 6, effect: Effect::Read, name: "read battery" },
    Command { prefix: &GET_CHARGING, at: 6, effect: Effect::Read, name: "read charging" },
    Command {
        prefix: &[0x0F, 0x84, VARSTORE, BACKLIGHT_LED],
        at: 6,
        effect: Effect::Read,
        name: "read backlight brightness",
    },
    Command {
        prefix: &[0x03, 0x83, VARSTORE, BACKLIGHT_LED],
        at: 6,
        effect: Effect::Read,
        name: "read backlight brightness",
    },
    Command { prefix: &[0x03, 0x80, VARSTORE, GAME_LED], at: 6, effect: Effect::Read, name: "read game mode" },
    Command { prefix: &GET_LAYOUT, at: 6, effect: Effect::Read, name: "read layout" },
];

/// What a keyboard PeriHub changes may also be sent, by class and command, the storage byte and the
/// LED: the brightness and the game mode only with "variable storage", as OpenRazer sends them, and the
/// custom frame only without it. Every other effect, the device mode and the rest are left out.
const BRIGHTNESS_SET: Command = Command {
    prefix: &[0x0F, 0x04, VARSTORE, BACKLIGHT_LED],
    at: 6,
    effect: Effect::Save,
    name: "set backlight brightness (stored)",
};
const GAME_SET: Command =
    Command { prefix: &[0x03, 0x00, VARSTORE, GAME_LED], at: 6, effect: Effect::Save, name: "set game mode (stored)" };
/// Bytes 8 and 9 are unused, byte 10 is the row and byte 11 the first column: always 0.
macro_rules! frame_row {
    ($row:expr) => {
        Command {
            prefix: &[0x0F, 0x03, 0x00, 0x00, $row, 0x00],
            at: 6,
            effect: Effect::Change,
            name: "set custom frame row",
        }
    };
}
const FRAME_ROWS: [Command; MAX_ROWS] =
    [frame_row!(0), frame_row!(1), frame_row!(2), frame_row!(3), frame_row!(4), frame_row!(5)];
const FRAME_SHOW: Command = Command {
    prefix: &[0x0F, 0x02, NOSTORE, 0x00, CUSTOM_EFFECT],
    at: 6,
    effect: Effect::Change,
    name: "show custom frame (not stored)",
};

/// What a keyboard needs, from OpenRazer (written by `tools/openrazer_facts.py --keyboards` into
/// [`super::razer_keyboard_models`]): how long it takes to answer, and the transaction id of each read
/// it takes. The firmware and the layout are read with transaction id `ff` on every keyboard.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub wait_us: u32,
    /// The battery and charging reads, on the keyboards with a battery.
    pub battery: Option<u8>,
    /// The backlight brightness read.
    pub brightness: Option<Brightness>,
    /// The game mode read.
    pub game: Option<u8>,
    /// 70-perihub.rules opens its interface on Linux: only where a tester showed that the interface
    /// carries no typed keys (on most Razer keyboards the one OpenRazer sends to carries them).
    pub linux: bool,
    /// Set on the keyboards PeriHub changes. The brightness and the game mode are then changed with
    /// the transaction id they are read with, which the generator checked in OpenRazer.
    pub change: Option<Change>,
}

/// How a keyboard reads its backlight brightness: with the extended command (`0f 84`) or the
/// standard one (`03 83`), and its transaction id.
#[derive(Debug, Clone, Copy)]
pub struct Brightness {
    pub standard: bool,
    pub tid: u8,
}

/// What a keyboard that PeriHub changes needs besides its reads.
#[derive(Debug, Clone, Copy)]
pub struct Change {
    pub frame: Frame,
}

/// The custom colour frame: `rows` rows of `columns` colours, drawn row by row and then shown.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub tid: u8,
    pub rows: u8,
    pub columns: u8,
}

/// The transaction id of the firmware and layout reads on every keyboard.
const TID_FF: u8 = 0xFF;

/// One row of the custom frame: the same colour on every column. The arguments are two unused bytes,
/// the row, the first and the last column, then three bytes per column.
fn frame_row_args(frame: Frame, row: u8, color: Rgb) -> Vec<u8> {
    let columns = frame.columns.clamp(1, 25);
    let mut args = vec![0, 0, row, 0, columns - 1];
    for _ in 0..columns {
        args.extend([color.r, color.g, color.b]);
    }
    args
}

/// What PeriHub changes on a keyboard, for the device list: `None` on a keyboard that is only read.
pub fn changes(p: &Params) -> Option<Vec<String>> {
    p.change?;
    let mut changes = Vec::new();
    if matches!(p.brightness, Some(Brightness { standard: false, .. })) {
        changes.push("backlight brightness".to_owned());
    }
    if p.game.is_some() {
        changes.push("game mode".into());
    }
    changes.push("one colour, not saved".into());
    Some(changes)
}

pub struct RazerKeyboard {
    p: Params,
    /// The brightness as first read, in 0–255: an undo puts back exactly that, not the nearest percent.
    first_brightness: Option<u8>,
}

impl RazerKeyboard {
    pub fn new(p: Params) -> Self {
        Self { p, first_brightness: None }
    }

    fn query(&self, io: &mut Io, tid: u8, command: [u8; 2], size: u8, args: &[u8]) -> Result<[u8; 80]> {
        exchange(io, self.p.wait_us, tid, command, size, args)
    }

    /// The backlight brightness is changed, on a keyboard PeriHub changes that reads it with the
    /// extended command (the one OpenRazer writes with).
    fn changes_brightness(&self) -> Option<u8> {
        match (self.p.change, self.p.brightness) {
            (Some(_), Some(Brightness { standard: false, tid })) => Some(tid),
            _ => None,
        }
    }

    fn changes_game_mode(&self) -> Option<u8> {
        self.p.change.and(self.p.game)
    }
}

impl Driver for RazerKeyboard {
    fn commands(&self) -> Vec<Command> {
        let mut commands = COMMANDS.to_vec();
        if let Some(change) = self.p.change {
            if self.changes_brightness().is_some() {
                commands.push(BRIGHTNESS_SET);
            }
            if self.changes_game_mode().is_some() {
                commands.push(GAME_SET);
            }
            commands.extend(FRAME_ROWS.iter().take((change.frame.rows as usize).min(MAX_ROWS)));
            commands.push(FRAME_SHOW);
        }
        commands
    }

    fn blind_settings(&self) -> Vec<BlindSetting> {
        match self.p.change {
            Some(_) => {
                vec![BlindSetting { id: COLOR, label: "One colour, not saved", kind: BlindKind::Color, last: None }]
            }
            None => Vec::new(),
        }
    }

    /// One colour on every column of every row of the custom frame, then the custom frame effect, not
    /// stored: the order and bytes of OpenRazer's own drawing.
    fn send_blind(&mut self, io: &mut Io, id: &str, value: &BlindValue) -> Result<()> {
        let (Some(change), COLOR, BlindValue::Color(color)) = (self.p.change, id, value) else {
            return Err(Error::Invalid(format!("{id} is not a setting of this keyboard")));
        };
        let frame = change.frame;
        for row in 0..frame.rows.min(MAX_ROWS as u8) {
            self.query(io, frame.tid, SET_FRAME, FRAME_SIZE, &frame_row_args(frame, row, *color))?;
        }
        self.query(io, frame.tid, SHOW_FRAME, 0x0C, &[NOSTORE, 0x00, CUSTOM_EFFECT])?;
        Ok(())
    }

    fn source(&self) -> &'static str {
        "OpenRazer (facts only)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        self.p.change.is_some()
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        // A keyboard PeriHub does not change is only read: the page shows it read-only.
        if self.p.change.is_none() {
            io.forbid_changes();
        }
        let p = self.p;
        // For the test report: whether the interface declares the 90-byte feature report.
        let _ = io.report_descriptor();
        let fw = match self.query(io, TID_FF, GET_FIRMWARE, 0x02, &[]) {
            Ok(fw) => fw,
            // Off, asleep or away from the receiver, or a status the keyboard was not expected to
            // give: the page and its test report show it, and it is read again every few seconds.
            Err(e @ (Error::Timeout | Error::Protocol(_))) => {
                return Ok(Reading {
                    details: vec![(
                        "Status".into(),
                        format!("no answer yet ({e}): if the keyboard is on, please send a test report"),
                    )],
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };
        let mut problems = Vec::new();
        let mut details = Vec::new();
        let mut settings = Settings::default();
        let mut limits = Limits::default();
        let battery = match p.battery {
            Some(tid) => {
                let level = self.query(io, tid, GET_BATTERY, 0x02, &[])?[1];
                let charging = self.query(io, tid, GET_CHARGING, 0x02, &[])?[1];
                if charging > 1 {
                    problems.push(format!("charging state {charging:02x}"));
                }
                Some((percent(level), charging == 1))
            }
            None => None,
        };
        // A keyboard that does not take one of the light reads is shown without it.
        if let Some(b) = p.brightness {
            let command = if b.standard { GET_STANDARD_BRIGHTNESS } else { GET_BRIGHTNESS };
            match self.query(io, b.tid, command, 0x03, &[VARSTORE, BACKLIGHT_LED]) {
                Ok(a) if a[1] == BACKLIGHT_LED && self.changes_brightness().is_some() => {
                    settings.leds.push(Led { mode: 0, brightness: Some(percent(a[2])), color: Rgb::BLACK });
                    limits.leds.push(LedLimits {
                        name: "Backlight".into(),
                        modes: Vec::new(),
                        brightness_step: Some(1),
                        color: false,
                    });
                    self.first_brightness.get_or_insert(a[2]);
                }
                Ok(a) if a[1] == BACKLIGHT_LED => {
                    details.push(("Backlight brightness".into(), format!("{} %", percent(a[2]))))
                }
                Ok(a) => problems.push(format!("brightness answered for LED {:02x}", a[1])),
                Err(Error::Invalid(_)) => {
                    details.push(("Backlight brightness".into(), "not read: not supported".into()))
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(tid) = p.game {
            match self.query(io, tid, GET_LED_STATE, 0x03, &[VARSTORE, GAME_LED]) {
                Ok(a) if a[1] == GAME_LED && a[2] <= 1 && self.changes_game_mode().is_some() => {
                    settings.options.insert(GAME_MODE.into(), a[2].into());
                    limits.options.push(Choice {
                        id: GAME_MODE,
                        label: "Game mode",
                        values: vec![(0, "Off".into()), (1, "On".into())],
                    });
                }
                Ok(a) if a[1] == GAME_LED && a[2] <= 1 => {
                    details.push(("Game mode".into(), if a[2] == 1 { "on" } else { "off" }.into()))
                }
                Ok(a) => problems.push(format!("game mode answered {:02x} {:02x}", a[1], a[2])),
                Err(Error::Invalid(_)) => details.push(("Game mode".into(), "not read: not supported".into())),
                Err(e) => return Err(e),
            }
        }
        // Its layout, with the firmware's transaction id as OpenRazer sends it. A keyboard that does
        // not take it leaves the layout to the system.
        let layout = match self.query(io, TID_FF, GET_LAYOUT, 0x02, &[]) {
            Ok(a) => Some(crate::kblayout::razer(a[0])),
            Err(Error::Invalid(_)) => None,
            Err(e) => return Err(e),
        };
        Ok(Reading {
            firmware: Some(format!("v{}.{}", fw[0], fw[1])),
            layout,
            battery,
            settings,
            limits,
            details,
            problem: (!problems.is_empty()).then(|| format!("unexpected answer: {}", problems.join(", "))),
            ..Default::default()
        })
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        if self.p.change.is_none() {
            return Err(Error::Invalid("this keyboard is only read for now".into()));
        }
        if to.dpi_levels != from.dpi_levels || to.polling_hz != from.polling_hz {
            return Err(Error::Invalid("a keyboard has no DPI or polling rate to change".into()));
        }
        if to.options != from.options {
            let tid = self.changes_game_mode().ok_or_else(|| Error::Invalid("game mode".into()))?;
            if to.options.keys().ne(from.options.keys()) || to.options.keys().any(|k| k != GAME_MODE) {
                return Err(Error::Invalid("only the game mode is changed".into()));
            }
            let on = match to.options.get(GAME_MODE) {
                Some(0) => 0,
                Some(1) => 1,
                _ => return Err(Error::Invalid("game mode".into())),
            };
            // As OpenRazer sends it: stored.
            self.query(io, tid, SET_LED_STATE, 0x03, &[VARSTORE, GAME_LED, on])?;
        }
        if to.leds != from.leds {
            let tid = self.changes_brightness().ok_or_else(|| Error::Invalid("lighting".into()))?;
            let ([new], [old]) = (&to.leds[..], &from.leds[..]) else {
                return Err(Error::Invalid("this keyboard has one light".into()));
            };
            if new.mode != old.mode || new.color != old.color {
                return Err(Error::Invalid("only the brightness of the backlight is changed".into()));
            }
            let wanted = new.brightness.ok_or_else(|| Error::Invalid("brightness".into()))?;
            // As OpenRazer sends it: stored. The value first read where it is the one wanted.
            let value = match self.first_brightness {
                Some(first) if percent(first) == wanted => first,
                _ => raw(wanted),
            };
            self.query(io, tid, SET_BRIGHTNESS, 0x03, &[VARSTORE, BACKLIGHT_LED, value])?;
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("Razer keyboards store every change by themselves".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::super::fake::FakeHid;
    use super::super::razer::request;
    use super::super::task::start_for_test;
    use super::super::{Kind, Protocol, find, models, opened_on_linux};
    use super::*;

    /// What a simulated V4 Mini holds: brightness 0x80, game mode on; and every request received.
    struct Board {
        brightness: u8,
        game: u8,
        sent: Vec<Vec<u8>>,
        /// The rows of the custom frame as last drawn, and whether it was shown.
        frame: Vec<Vec<u8>>,
        shown: bool,
    }

    type Keyboard = Arc<Mutex<Board>>;

    fn board() -> Keyboard {
        Arc::new(Mutex::new(Board { brightness: 0x80, game: 1, sent: Vec::new(), frame: Vec::new(), shown: false }))
    }

    /// A simulated V4 Mini: battery 0xcc (80 %), charging, and the brightness and game mode of `state`.
    /// `status` is the one it answers with; `wireless`: through its receiver.
    fn fake(state: Keyboard, status: u8, wireless: bool) -> FakeHid {
        fake_with(state, status, if wireless { 0x9F } else { 0x1F })
    }

    /// The same, with the transaction id the keyboard takes for its battery, brightness and frame.
    fn fake_with(state: Keyboard, status: u8, battery_tid: u8) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |req: &[u8]| {
            let mut st = state.lock().unwrap();
            st.sent.push(req.to_vec());
            let tid = req[1];
            let mut a = req.to_vec();
            a[0] = status;
            match ([req[6], req[7]], tid) {
                (GET_FIRMWARE, 0xFF) => a[8..10].copy_from_slice(&[1, 3]),
                (GET_BATTERY, t) if t == battery_tid => a[9] = 0xCC,
                (GET_CHARGING, t) if t == battery_tid => a[9] = 1,
                (GET_BRIGHTNESS, t) if t == battery_tid => a[10] = st.brightness,
                (SET_BRIGHTNESS, t) if t == battery_tid => st.brightness = req[10],
                (GET_LED_STATE, 0xFF) => a[10] = st.game,
                (SET_LED_STATE, 0xFF) => st.game = req[10],
                (GET_LAYOUT, 0xFF) => a[8] = 0x11,
                (SET_FRAME, t) if t == battery_tid => st.frame.push(req[8..8 + 5 + 3 * 14].to_vec()),
                (SHOW_FRAME, t) if t == battery_tid => st.shown = true,
                _ => a[0] = 0x03,
            }
            a
        }));
        hid
    }

    fn params(id: u16) -> Params {
        match find(0x1532, id).unwrap().protocol {
            Protocol::RazerKeyboard(p) => p,
            _ => panic!("{id:04x} is not a Razer keyboard"),
        }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-razer-kb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// Starts a V4 Mini (`id`) on a simulated keyboard; `changes`: on a connection that may change things.
    fn open(state: &Keyboard, id: u16, changes: bool, name: &str) -> super::super::task::PeripheralTask {
        let model = find(0x1532, id).unwrap();
        start_for_test(Box::new(fake(state.clone(), 0x02, id == 0x02BA)), model, id, changes, temp(name)).unwrap()
    }

    #[test]
    fn every_razer_keyboard_is_a_keyboard_on_the_interface_openrazer_names_and_only_the_v4_mini_is_changed() {
        let keyboards: Vec<_> = models().filter(|m| matches!(m.protocol, Protocol::RazerKeyboard(_))).collect();
        assert!(keyboards.len() >= 60, "{}", keyboards.len());
        for m in keyboards {
            assert_eq!(m.kind, Kind::Keyboard, "{}", m.name);
            assert!(matches!(m.interface, Some(1..=3)), "{}", m.name);
            assert_eq!(m.product_ids.len(), 1, "{}", m.name);
            // No mouse, receiver or other device of the tables has the same id.
            assert!(std::ptr::eq(find(0x1532, m.product_ids[0]).unwrap(), m), "{}", m.name);
            // Linux: only where the interface was seen to carry no typed keys.
            let v4_mini = matches!(m.product_ids[0], 0x02B9 | 0x02BA);
            assert_eq!(opened_on_linux(m), v4_mini, "{}", m.name);
            // Changes: the same keyboards, and only commands the driver names.
            let Protocol::RazerKeyboard(p) = m.protocol else { unreachable!() };
            assert_eq!(p.change.is_some(), v4_mini, "{}", m.name);
            let commands = RazerKeyboard::new(p).commands();
            assert_eq!(commands.iter().all(|c| c.effect == Effect::Read), !v4_mini, "{}", m.name);
        }
        assert!(COMMANDS.iter().all(|c| c.effect == Effect::Read));
    }

    #[test]
    fn the_facts_of_a_few_keyboards_are_the_ones_openrazer_gives() {
        // BlackWidow V3: interface 3, extended brightness with 3f, no battery.
        let p = params(0x024E);
        assert_eq!(find(0x1532, 0x024E).unwrap().interface, Some(3));
        assert!(matches!(p.brightness, Some(Brightness { standard: false, tid: 0x3F })));
        assert_eq!((p.battery, p.game, p.wait_us), (None, Some(0xFF), 600));
        // BlackWidow Chroma: interface 1, the standard brightness read.
        assert_eq!(find(0x1532, 0x0203).unwrap().interface, Some(1));
        assert!(matches!(params(0x0203).brightness, Some(Brightness { standard: true, tid: 0xFF })));
        // Huntsman V2: its game mode is read from another storage, which PeriHub does not send.
        assert_eq!(params(0x026C).game, None);
        // DeathStalker V2 Pro through its receiver: interface 2, 4.9 ms, battery with 9f.
        let p = params(0x0290);
        assert_eq!(find(0x1532, 0x0290).unwrap().interface, Some(2));
        assert_eq!((p.battery, p.wait_us), (Some(0x9F), 4900));
        // The BlackWidow V4 Mini HyperSpeed: the same transaction id for the brightness read, its change
        // and the frame; the matrix of 5 rows of 14 columns.
        let (wired, wireless) = (params(0x02B9), params(0x02BA));
        assert_eq!((wired.battery, wired.wait_us, wired.linux), (Some(0x1F), 600, true));
        assert_eq!((wireless.battery, wireless.wait_us, wireless.linux), (Some(0x9F), 4900, true));
        for (p, tid) in [(wired, 0x1F), (wireless, 0x9F)] {
            assert!(matches!(p.brightness, Some(Brightness { standard: false, tid: t }) if t == tid));
            let frame = p.change.unwrap().frame;
            assert_eq!((frame.tid, frame.rows, frame.columns), (tid, 5, 14));
            assert_eq!(p.game, Some(0xFF));
        }
    }

    /// A simulated BlackWidow Chroma: no battery, the standard brightness read (0x40), game mode off.
    fn chroma(sent: Arc<Mutex<Vec<Vec<u8>>>>) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |req: &[u8]| {
            sent.lock().unwrap().push(req.to_vec());
            let mut a = req.to_vec();
            a[0] = 0x02;
            match ([req[6], req[7]], req[1]) {
                (GET_FIRMWARE, 0xFF) => a[8..10].copy_from_slice(&[2, 4]),
                (GET_STANDARD_BRIGHTNESS, 0xFF) => a[10] = 0x40,
                (GET_LED_STATE, 0xFF) => a[10] = 0,
                (GET_LAYOUT, 0xFF) => a[8] = 0x01,
                _ => a[0] = 0x05,
            }
            a
        }));
        hid
    }

    #[test]
    fn a_wired_keyboard_is_not_asked_for_a_battery_and_reads_the_standard_brightness() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let model = find(0x1532, 0x0203).unwrap();
        let task = start_for_test(Box::new(chroma(sent.clone())), model, 0x0203, true, temp("chroma")).unwrap();
        let info = &task.info;
        assert_eq!(info.firmware.as_deref(), Some("v2.4"));
        assert!(info.battery.is_none());
        assert!(info.details.contains(&("Backlight brightness".into(), "25 %".into())), "{:?}", info.details);
        assert!(info.details.contains(&("Game mode".into(), "off".into())));
        assert_eq!(info.problem, None);
        assert!(!info.editable && !info.changes_stay, "a keyboard PeriHub does not change stays read-only");
        assert!(info.blind.is_empty() && info.settings.leds.is_empty() && info.settings.options.is_empty());
        let sent = sent.lock().unwrap();
        assert!(
            !sent
                .iter()
                .any(|r| [r[6], r[7]] == GET_BATTERY || [r[6], r[7]] == GET_CHARGING || [r[6], r[7]] == GET_BRIGHTNESS)
        );
        for req in sent.iter() {
            assert!(COMMANDS.iter().any(|c| req[6..].starts_with(c.prefix)), "{req:02x?}");
        }
    }

    #[test]
    fn a_keyboard_without_a_game_mode_read_is_not_asked_for_it() {
        let state = board();
        let model = find(0x1532, 0x026C).unwrap();
        let task =
            start_for_test(Box::new(fake(state.clone(), 0x02, false)), model, 0x026C, true, temp("huntsman")).unwrap();
        assert!(!task.info.details.iter().any(|(k, _)| k == "Game mode"), "{:?}", task.info.details);
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == GET_LED_STATE));
    }

    #[test]
    fn a_keyboard_that_is_only_read_never_sends_a_change_even_on_a_connection_that_may() {
        // A keyboard with every read but no change: an answer for each, and nothing but reads goes out.
        let state = board();
        let model = find(0x1532, 0x024E).unwrap();
        let mut task =
            start_for_test(Box::new(fake_with(state.clone(), 0x02, 0x3F)), model, 0x024E, true, temp("v3")).unwrap();
        assert!(!task.info.editable && task.info.blind.is_empty());
        assert!(task.set_blind(COLOR, &BlindValue::Color(Rgb::WHITE)).is_err());
        let wanted = task.info.settings.clone();
        assert!(task.change(&wanted, false, None).is_err());
        assert!(state.lock().unwrap().sent.iter().all(|r| r[7] & 0x80 != 0), "reads only");
        let _ = std::fs::remove_dir_all(task.backups());
        assert!(RazerKeyboard::new(params(0x024E)).blind_settings().is_empty());
    }

    #[test]
    fn both_ids_are_keyboards_on_interface_3() {
        for id in [0x02B9, 0x02BA] {
            let m = find(0x1532, id).unwrap();
            assert_eq!(m.kind, Kind::Keyboard);
            assert_eq!(m.interface, Some(3));
            assert!(matches!(m.protocol, Protocol::RazerKeyboard(_)));
        }
    }

    #[test]
    fn the_v4_mini_reads_as_documented() {
        for (id, wireless) in [(0x02B9, false), (0x02BA, true)] {
            let state = board();
            let task = open(&state, id, true, &format!("read-{id:04x}"));
            let info = &task.info;
            assert_eq!(info.firmware.as_deref(), Some("v1.3"));
            let battery = info.battery.unwrap();
            assert_eq!((battery.level, battery.charging), (80, true));
            // The brightness and the game mode are settings now, not only details.
            assert_eq!(info.settings.leds.iter().map(|l| l.brightness).collect::<Vec<_>>(), [Some(50)]);
            assert_eq!(info.limits.leds.len(), 1);
            let backlight = &info.limits.leds[0];
            assert_eq!(backlight.name, "Backlight");
            assert!(backlight.modes.is_empty() && !backlight.color && backlight.brightness_step == Some(1));
            assert_eq!(info.settings.options.get(GAME_MODE), Some(&1));
            assert_eq!(info.limits.options.len(), 1);
            assert_eq!(info.limits.options[0].values, [(0, "Off".to_string()), (1, "On".to_string())]);
            assert!(info.details.is_empty(), "{:?}", info.details);
            assert_eq!(info.blind.iter().map(|b| b.id).collect::<Vec<_>>(), [COLOR]);
            let layout = info.layout.as_ref().unwrap();
            assert_eq!((layout.region, layout.physical), (Some("Italian"), Some(crate::kblayout::Physical::Iso)));
            assert_eq!(info.problem, None);
            assert!(info.editable && info.changes_stay && !info.can_save);
            // Reads only, each as documented, with the transaction id of the cable or the receiver.
            let tid = if wireless { 0x9F } else { 0x1F };
            for req in state.lock().unwrap().sent.iter() {
                assert!(COMMANDS.iter().any(|c| req[6..].starts_with(c.prefix)), "{req:02x?}");
                let expected = request(req[1], [req[6], req[7]], req[5], &req[8..88]);
                assert_eq!(req[..], expected[..], "checksum");
                let on_ff = matches!([req[6], req[7]], GET_FIRMWARE | GET_LED_STATE | GET_LAYOUT);
                assert_eq!(req[1], if on_ff { 0xFF } else { tid }, "{req:02x?}");
            }
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn the_commands_of_a_v4_mini_are_the_reads_the_two_stored_changes_and_the_frame() {
        let commands = RazerKeyboard::new(params(0x02B9)).commands();
        assert_eq!(commands.len(), COMMANDS.len() + 2 + 5 + 1);
        let names: Vec<_> = commands.iter().filter(|c| c.effect != Effect::Read).map(|c| (c.name, c.effect)).collect();
        assert_eq!(
            names,
            [
                ("set backlight brightness (stored)", Effect::Save),
                ("set game mode (stored)", Effect::Save),
                ("set custom frame row", Effect::Change),
                ("set custom frame row", Effect::Change),
                ("set custom frame row", Effect::Change),
                ("set custom frame row", Effect::Change),
                ("set custom frame row", Effect::Change),
                ("show custom frame (not stored)", Effect::Change),
            ]
        );
    }

    #[test]
    fn a_brightness_change_is_stored_read_back_and_undone_to_the_exact_value() {
        for (id, tid) in [(0x02B9, 0x1F), (0x02BA, 0x9F)] {
            let state = board();
            state.lock().unwrap().brightness = 0x7F;
            let mut task = open(&state, id, true, &format!("bright-{id:04x}"));
            assert_eq!(task.info.settings.leds[0].brightness, Some(50));
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].brightness = Some(80);
            task.change(&wanted, false, None).unwrap();
            assert_eq!(state.lock().unwrap().brightness, 204);
            assert_eq!(task.info.settings.leds[0].brightness, Some(80), "read back");
            let set = state.lock().unwrap().sent.iter().find(|r| [r[6], r[7]] == SET_BRIGHTNESS).unwrap().clone();
            assert_eq!(set[..11], [0, tid, 0, 0, 0, 0x03, 0x0F, 0x04, VARSTORE, BACKLIGHT_LED, 204]);
            assert_eq!(set[88], request(tid, SET_BRIGHTNESS, 0x03, &set[8..88])[88], "checksum");
            task.expire_undo();
            crate::worker::DeviceTask::tick(&mut task).unwrap();
            assert_eq!(state.lock().unwrap().brightness, 0x7F, "127 again, not the 128 of 50 %");
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn a_kept_change_stays() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "kept");
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(20);
        task.change(&wanted, false, None).unwrap();
        assert!(task.undo_pending());
        task.keep_changes();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert!(!task.undo_pending());
        assert_eq!(state.lock().unwrap().brightness, 51, "20 %");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_game_mode_is_stored_read_back_and_undone() {
        let state = board();
        let mut task = open(&state, 0x02BA, true, "game");
        assert_eq!(task.info.settings.options.get(GAME_MODE), Some(&1));
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(GAME_MODE.into(), 0);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().game, 0);
        assert_eq!(task.info.settings.options.get(GAME_MODE), Some(&0), "read back");
        let set = state.lock().unwrap().sent.iter().find(|r| [r[6], r[7]] == SET_LED_STATE).unwrap().clone();
        assert_eq!(set[..11], [0, 0xFF, 0, 0, 0, 0x03, 0x03, 0x00, VARSTORE, GAME_LED, 0]);
        // The brightness was not touched.
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == SET_BRIGHTNESS));
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().game, 1, "on again");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_change_the_keyboard_does_not_read_back_is_undone_at_once() {
        // A keyboard that keeps the brightness only in coarse steps: it reads back 0x10 for 0x1a.
        let state = board();
        let hid = fake(state.clone(), 0x02, false);
        {
            let mut st = hid.0.lock().unwrap();
            let mut answer = st.on_feature.take().unwrap();
            st.on_feature = Some(Box::new(move |req: &[u8]| {
                let mut a = answer(req);
                if [req[6], req[7]] == GET_BRIGHTNESS {
                    a[10] &= 0xF0;
                }
                a
            }));
        }
        let model = find(0x1532, 0x02B9).unwrap();
        let mut task = start_for_test(Box::new(hid), model, 0x02B9, true, temp("coarse")).unwrap();
        assert_eq!(task.info.settings.leds[0].brightness, Some(50));
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(10);
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("reads back") && error.contains("put back"), "{error}");
        // The change, then the undo with the exact value first read.
        let sets: Vec<u8> =
            state.lock().unwrap().sent.iter().filter(|r| [r[6], r[7]] == SET_BRIGHTNESS).map(|r| r[10]).collect();
        assert_eq!(sets, [26, 0x80]);
        assert_eq!(state.lock().unwrap().brightness, 0x80);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_keyboard_does_not_take_are_refused() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "refuse");
        let before = state.lock().unwrap().sent.len();
        let changes: [fn(&mut Settings); 6] = [
            |s| s.leds[0].brightness = Some(101),
            |s| s.leds[0].mode = 1,
            |s| s.leds[0].color = Rgb::WHITE,
            |s| {
                s.options.insert(GAME_MODE.into(), 2);
            },
            |s| {
                s.options.insert("other".into(), 1);
            },
            |s| s.dpi_levels = vec![800],
        ];
        for change in changes {
            let mut wanted = task.info.settings.clone();
            change(&mut wanted);
            assert!(task.change(&wanted, false, None).is_err());
        }
        let sent = state.lock().unwrap().sent.clone();
        assert!(sent[before..].iter().all(|r| r[7] & 0x80 != 0 || [r[6], r[7]] == GET_LAYOUT), "nothing but reads");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_connection_that_may_not_change_anything_sends_only_reads() {
        let state = board();
        let mut task = open(&state, 0x02B9, false, "readonly");
        assert!(!task.info.editable);
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(10);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(task.set_blind(COLOR, &BlindValue::Color(Rgb::WHITE)).is_err());
        assert!(state.lock().unwrap().sent.iter().all(|r| COMMANDS.iter().any(|c| r[6..].starts_with(c.prefix))));
        assert!(state.lock().unwrap().frame.is_empty() && !state.lock().unwrap().shown);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_colour_is_a_custom_frame_of_five_rows_shown_without_storage() {
        for (id, tid) in [(0x02B9, 0x1F), (0x02BA, 0x9F)] {
            let state = board();
            let mut task = open(&state, id, true, &format!("color-{id:04x}"));
            let before = state.lock().unwrap().sent.len();
            let color = Rgb::new(0x12, 0x34, 0x56);
            let info = task.set_blind(COLOR, &BlindValue::Color(color)).unwrap();
            assert_eq!(info.blind[0].last, Some(BlindValue::Color(color)));
            let sent = state.lock().unwrap().sent[before..].to_vec();
            assert_eq!(sent.len(), 6, "five rows, then the effect");
            for (row, req) in sent[..5].iter().enumerate() {
                // Unused, unused, the row, the first and the last column, then 14 colours; size 0x47.
                assert_eq!(req[..13], [0, tid, 0, 0, 0, 0x47, 0x0F, 0x03, 0, 0, row as u8, 0, 13]);
                assert_eq!(req[13..55], [0x12, 0x34, 0x56].repeat(14)[..]);
                assert!(req[55..88].iter().all(|&b| b == 0));
                assert_eq!(req[88], request(tid, SET_FRAME, 0x47, &req[8..88])[88], "checksum");
            }
            assert_eq!(sent[5][..11], [0, tid, 0, 0, 0, 0x0C, 0x0F, 0x02, NOSTORE, 0x00, CUSTOM_EFFECT]);
            assert!(state.lock().unwrap().shown);
            let st = state.lock().unwrap();
            assert_eq!((st.brightness, st.game), (0x80, 1), "brightness and game mode untouched");
            drop(st);
            assert!(task.set_blind("dpi", &BlindValue::Number(800)).is_err());
            assert!(task.set_blind(COLOR, &BlindValue::Number(1)).is_err());
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn stored_effects_and_other_commands_never_leave() {
        let state = board();
        let driver = RazerKeyboard::new(params(0x02B9));
        let mut io = Io::new(Box::new(fake(state.clone(), 0x02, false)), driver.commands(), true);
        let refused: [&[u8]; 17] = [
            // Every effect of the keyboard's memory, stored or not.
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x01],
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, 0x01],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x00],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x03],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x04],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x07],
            &[0x0F, 0x02, NOSTORE, 0x00, 0x07],
            // The custom effect stored, or on a light.
            &[0x0F, 0x02, VARSTORE, 0x00, CUSTOM_EFFECT],
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, CUSTOM_EFFECT],
            // The brightness unstored or of another light; the game mode unstored or of another light.
            &[0x0F, 0x04, NOSTORE, BACKLIGHT_LED, 0x80],
            &[0x0F, 0x04, VARSTORE, 0x04, 0x80],
            &[0x03, 0x00, NOSTORE, GAME_LED, 0x01],
            &[0x03, 0x00, VARSTORE, 0x07, 0x01],
            // A frame row the matrix does not have, or a row that starts elsewhere.
            &[0x0F, 0x03, 0x00, 0x00, 0x05, 0x00],
            &[0x0F, 0x03, 0x00, 0x00, 0x00, 0x01],
            // The device mode and the macro light's effect.
            &[0x00, 0x04, 0x03],
            &[0x03, 0x02, NOSTORE, 0x07, 0x01],
        ];
        for bytes in refused {
            let req = request(0x1F, [bytes[0], bytes[1]], 0x0C, &bytes[2..]);
            assert!(io.feature_query(&req, Duration::ZERO, |_| true).is_err(), "{bytes:02x?}");
        }
        assert!(state.lock().unwrap().sent.is_empty());
    }

    #[test]
    fn a_keyboard_that_answers_for_another_light_or_state_is_left_read_only() {
        // The brightness answered for the wrong LED: nothing is changed on this keyboard.
        let state = board();
        let model = find(0x1532, 0x02B9).unwrap();
        let hid = fake(state.clone(), 0x02, false);
        {
            let inner = hid.0.clone();
            let mut st = inner.lock().unwrap();
            let mut answer = st.on_feature.take().unwrap();
            st.on_feature = Some(Box::new(move |req: &[u8]| {
                let mut a = answer(req);
                if [req[6], req[7]] == GET_BRIGHTNESS {
                    a[9] = 0x04;
                }
                a
            }));
        }
        let mut task = start_for_test(Box::new(hid), model, 0x02B9, true, temp("wrong-led")).unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("brightness answered for LED 04")));
        assert!(!task.info.editable);
        assert!(task.set_blind(COLOR, &BlindValue::Color(Rgb::WHITE)).is_err());
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == SHOW_FRAME || [r[6], r[7]] == SET_FRAME));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_keyboard_away_from_its_receiver_gets_its_page_and_test_report() {
        let state = board();
        let model = find(0x1532, 0x02BA).unwrap();
        let hid = fake(state, 0x04, true);
        hid.0.lock().unwrap().descriptor = Some(vec![0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0xC0]);
        let mut task = start_for_test(Box::new(hid), model, 0x02BA, true, temp("away")).unwrap();
        assert!(task.info.details[0].1.starts_with("no answer yet"), "{:?}", task.info.details);
        assert_eq!(task.info.problem, None);
        // Nothing to change while it does not answer: the colour is refused, and not shown as sent.
        assert!(task.set_blind(COLOR, &BlindValue::Color(Rgb::WHITE)).is_err());
        assert!(task.info.blind.iter().all(|b| b.last.is_none()));
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "kept after a refresh: {raw:?}");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_frame_covers_every_column_in_a_packet_of_the_size_openrazer_uses() {
        let frame = Frame { tid: 0x1F, rows: 5, columns: 14 };
        let args = frame_row_args(frame, 3, Rgb::new(1, 2, 3));
        assert_eq!(args[..5], [0, 0, 3, 0, 13]);
        assert_eq!(args[5..], [1, 2, 3].repeat(14)[..]);
        // 5 + 3 x 14 bytes fit in the 80 bytes of arguments, and in the 0x47 of the packet.
        assert!(args.len() <= FRAME_SIZE as usize && args.len() <= 80);
    }
}

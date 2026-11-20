//! Razer keyboards, on their cable and through their own HyperSpeed receiver. The protocol is the
//! one of the Razer mice (`docs/protocols/razer-mice.md`); the keyboards' own facts are in
//! `docs/protocols/razer-keyboards.md`, and per model in [`super::razer_keyboard_models`], written by
//! `tools/openrazer_facts.py` from facts found in OpenRazer (GPL-2.0): no code was copied. Nobody has
//! tried one with PeriHub yet.
//!
//! Every keyboard is read: firmware, layout, and where the keyboard has them, battery, charging,
//! brightness and game mode. The BlackWidow V4 Mini HyperSpeed (`Params::change`) is also changed, as an
//! experimental feature:
//! - the backlight brightness, the game mode and the macro light (its state, and its effect where OpenRazer
//!   stores it), which OpenRazer sends with "variable storage" and reads back, so PeriHub treats them as stored
//!   (No warranty box, backup, read-back, undo);
//! - the lighting effects of the keyboard's own memory (none, static, breathing, spectrum, wave, reactive,
//!   starlight), which OpenRazer also sends with "variable storage" but has no command to read: PeriHub cannot
//!   show or check the effect in use. It remembers what it sent itself (see [`super::memory`]) and can send the
//!   effect before the last one again; what the keyboard had before PeriHub's first change is not known;
//! - one colour on the custom frame, shown without storage.
//!
//! The ripple effect is never sent: OpenRazer draws it from the key presses it reads, and PeriHub never reads
//! what is typed.
//!
//! The reports go to the USB interface OpenRazer sends to (1, 2 or 3). On Linux 70-perihub.rules
//! opens it only where a tester showed it carries no typed keys (the BlackWidow V4 Mini HyperSpeed's
//! interface 3), and a keyboard node that carries typed keys is never opened anyway (see
//! [`super::node_carries_typing`]).

use super::razer::{exchange, percent, raw};
use super::task::Driver;
use super::{
    BlindKind, BlindSetting, BlindValue, Choice, Command, Effect, EffectSpec, EffectValue, Io, Led, LedLimits, Limits,
    Reading, Settings, hex,
};
use crate::color::Rgb;
use crate::error::{Error, Result};

const NOSTORE: u8 = 0x00;
const VARSTORE: u8 = 0x01;
/// The LED id of the keyboard's backlight, of the macro light and of the game mode light.
const BACKLIGHT_LED: u8 = 0x05;
const MACRO_LED: u8 = 0x07;
const GAME_LED: u8 = 0x08;

/// (class, id) of the commands, at bytes 6 and 7 of the report.
const GET_FIRMWARE: [u8; 2] = [0x00, 0x81];
const GET_BATTERY: [u8; 2] = [0x07, 0x80];
const GET_CHARGING: [u8; 2] = [0x07, 0x84];
const GET_BRIGHTNESS: [u8; 2] = [0x0F, 0x84];
/// The standard brightness read of the older keyboards.
const GET_STANDARD_BRIGHTNESS: [u8; 2] = [0x03, 0x83];
const GET_LED_STATE: [u8; 2] = [0x03, 0x80];
/// The effect of an LED (the macro light's: static or blinking).
const GET_LED_EFFECT: [u8; 2] = [0x03, 0x82];
/// The keyboard's region, so its physical layout (see [`crate::kblayout::razer`]).
const GET_LAYOUT: [u8; 2] = [0x00, 0x86];
/// The commands of a change: the brightness of the backlight, the state of one LED, one row of the
/// custom frame and the effect that shows it.
const SET_BRIGHTNESS: [u8; 2] = [0x0F, 0x04];
const SET_LED_STATE: [u8; 2] = [0x03, 0x00];
const SET_LED_EFFECT: [u8; 2] = [0x03, 0x02];
const SET_FRAME: [u8; 2] = [0x0F, 0x03];
/// The matrix effect command: shows the custom frame, and sets each of the lighting effects of the keyboard's own memory.
const SET_EFFECT: [u8; 2] = [0x0F, 0x02];
/// The custom frame effect number.
const CUSTOM_EFFECT: u8 = 0x08;
/// Packet size of a row of the custom frame.
const FRAME_SIZE: u8 = 0x47;
/// The most rows of a matrix OpenRazer gives a keyboard.
const MAX_ROWS: usize = 6;
/// The ids of the game mode setting, the macro light settings and the blind settings.
const GAME_MODE: &str = "game_mode";
const MACRO_LIGHT: &str = "macro_light";
const MACRO_EFFECT: &str = "macro_effect";
const COLOR: &str = "color";
const EFFECT: &str = "effect";
const MACRO_EFFECT_UNSAVED: &str = "macro_effect_unsaved";
/// The macro light's effects as OpenRazer's client names them: static and blinking.
const MACRO_EFFECTS: [(u16, &str); 2] = [(0, "Static"), (1, "Blinking")];
/// The speed OpenRazer's wave effect is always sent with (lower is faster): it takes no speed of its own.
const WAVE_SPEED: u8 = 0x28;
/// Lines of the log of what was sent.
const LOG_MAX: usize = 30;

/// Everything PeriHub may send to every Razer keyboard: reads only, the lights' with the stored values and
/// the one LED each.
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
/// LED: the brightness, the game mode, the macro light and the lighting effects of the keyboard's own memory
/// only with "variable storage", as OpenRazer sends them, and the custom frame only without it. The device
/// mode and the rest are left out.
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

/// The reads of the macro light, on a keyboard PeriHub changes: its state and its effect, stored values, as
/// OpenRazer's driver and daemon read them.
const MACRO_READS: [Command; 2] = [
    Command { prefix: &[0x03, 0x80, VARSTORE, MACRO_LED], at: 6, effect: Effect::Read, name: "read macro light" },
    Command {
        prefix: &[0x03, 0x82, VARSTORE, MACRO_LED],
        at: 6,
        effect: Effect::Read,
        name: "read macro light effect",
    },
];
/// The macro light's state, always sent stored, with transaction id `ff`, as OpenRazer sends it.
const MACRO_SET: Command = Command {
    prefix: &[0x03, 0x00, VARSTORE, MACRO_LED],
    at: 6,
    effect: Effect::Save,
    name: "set macro light (stored)",
};
/// The macro light's effect: stored on some keyboards and not on others, as OpenRazer sends it for each.
const MACRO_EFFECT_STORED: Command = Command {
    prefix: &[0x03, 0x02, VARSTORE, MACRO_LED],
    at: 6,
    effect: Effect::Save,
    name: "set macro light effect (stored)",
};
const MACRO_EFFECT_UNSTORED: Command = Command {
    prefix: &[0x03, 0x02, NOSTORE, MACRO_LED],
    at: 6,
    effect: Effect::Change,
    name: "set macro light effect (not stored)",
};

/// The lighting effects of the keyboard's own memory, by the effect number OpenRazer builds them with. It
/// sends every one of them stored (`01`) on the backlight (`05`), and has no command that reads the effect in use.
const EFFECT_COMMANDS: [Command; 7] = [
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x00], "set lighting effect: none (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x01], "set lighting effect: static (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x02], "set lighting effect: breathing (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x03], "set lighting effect: spectrum (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x04], "set lighting effect: wave (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x05], "set lighting effect: reactive (stored)"),
    effect_command(&[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x07], "set lighting effect: starlight (stored)"),
];

const fn effect_command(prefix: &'static [u8], name: &'static str) -> Command {
    Command { prefix, at: 6, effect: Effect::Save, name }
}

/// One lighting effect of the keyboard's own memory, as OpenRazer's daemon offers it for a V4 Mini, and what it takes.
struct EffectDef {
    name: &'static str,
    /// The effect number, argument 2.
    number: u8,
    /// Colours: none, one or two.
    colors: u8,
    /// The speeds, by the number OpenRazer sends. Empty: the effect has no speed of its own.
    speeds: &'static [(u8, &'static str)],
    /// The wave's direction, as OpenRazer's client names it: 1 to the right (WAVE_RIGHT), 2 to the left.
    direction: Option<u8>,
}

/// The reactive effect's speeds are its times (OpenRazer's client: REACTIVE_500MS to REACTIVE_2000MS).
const REACTIVE_SPEEDS: &[(u8, &str)] = &[(1, "500 ms"), (2, "1000 ms"), (3, "1500 ms"), (4, "2000 ms")];
/// The starlight effect's speeds (STARLIGHT_FAST, STARLIGHT_NORMAL, STARLIGHT_SLOW).
const STARLIGHT_SPEEDS: &[(u8, &str)] = &[(1, "Fast"), (2, "Normal"), (3, "Slow")];

/// What OpenRazer's daemon offers a V4 Mini (`set_none_effect` and the others), except the ripple effect, which it draws
/// itself from the key presses it reads.
const EFFECTS: [EffectDef; 12] = [
    EffectDef { name: "Off (no effect)", number: 0x00, colors: 0, speeds: &[], direction: None },
    EffectDef { name: "Static, one colour", number: 0x01, colors: 1, speeds: &[], direction: None },
    EffectDef { name: "Breathing, random colours", number: 0x02, colors: 0, speeds: &[], direction: None },
    EffectDef { name: "Breathing, one colour", number: 0x02, colors: 1, speeds: &[], direction: None },
    EffectDef { name: "Breathing, two colours", number: 0x02, colors: 2, speeds: &[], direction: None },
    EffectDef { name: "Spectrum cycling", number: 0x03, colors: 0, speeds: &[], direction: None },
    EffectDef { name: "Wave, left to right", number: 0x04, colors: 0, speeds: &[], direction: Some(1) },
    EffectDef { name: "Wave, right to left", number: 0x04, colors: 0, speeds: &[], direction: Some(2) },
    EffectDef { name: "Reactive, one colour", number: 0x05, colors: 1, speeds: REACTIVE_SPEEDS, direction: None },
    EffectDef { name: "Starlight, random colours", number: 0x07, colors: 0, speeds: STARLIGHT_SPEEDS, direction: None },
    EffectDef { name: "Starlight, one colour", number: 0x07, colors: 1, speeds: STARLIGHT_SPEEDS, direction: None },
    EffectDef { name: "Starlight, two colours", number: 0x07, colors: 2, speeds: STARLIGHT_SPEEDS, direction: None },
];

/// The request of an effect: its size byte and arguments, from the bytes of OpenRazer's builders. Arguments: the
/// storage, the LED and the effect number; then, by effect, a direction and a speed (wave: argument 3 and 4),
/// a speed (reactive, starlight: argument 4), the number of colours (argument 3 for breathing, 5 for the rest) and
/// the colours from argument 6. A value the effect does not take is refused.
fn effect_request(value: &EffectValue) -> Result<(u8, Vec<u8>)> {
    let def = EFFECTS
        .iter()
        .find(|d| d.name == value.name)
        .ok_or_else(|| Error::Invalid(format!("{} is not a lighting effect of this keyboard", value.name)))?;
    if value.colors.len() != def.colors as usize {
        return Err(Error::Invalid(format!("{}: {} colour(s), not {}", def.name, def.colors, value.colors.len())));
    }
    let speed = match (def.speeds.is_empty(), value.speed) {
        (true, None) => None,
        (false, Some(s)) if def.speeds.iter().any(|(n, _)| *n == s) => Some(s),
        _ => return Err(Error::Invalid(format!("{}: speed {:?}", def.name, value.speed))),
    };
    let size: u8 = match (def.number, def.colors) {
        (0x01, _) | (0x05, _) | (_, 1) => 0x09,
        (_, 2) => 0x0C,
        _ => 0x06,
    };
    let mut args = vec![VARSTORE, BACKLIGHT_LED, def.number, 0, 0, 0];
    args.resize(size as usize, 0);
    match def.number {
        0x01 => args[5] = 1,
        0x02 if def.colors > 0 => (args[3], args[5]) = (def.colors, def.colors),
        0x04 => (args[3], args[4]) = (def.direction.expect("every wave has a direction"), WAVE_SPEED),
        0x05 => (args[4], args[5]) = (speed.expect("checked above"), 1),
        0x07 => {
            args[4] = speed.expect("checked above");
            if def.colors > 0 {
                args[5] = def.colors;
            }
        }
        _ => {}
    }
    // The colours, from argument 6, for every effect that takes one (static and reactive: one).
    for (i, color) in value.colors.iter().enumerate() {
        args[6 + 3 * i..9 + 3 * i].copy_from_slice(&color.to_bytes());
    }
    Ok((size, args))
}

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
    pub effects: Effects,
    pub macro_light: MacroLight,
}

/// The lighting effects of the keyboard's own memory: OpenRazer sends every one with the same transaction id, stored,
/// on the backlight.
#[derive(Debug, Clone, Copy)]
pub struct Effects {
    pub tid: u8,
}

/// The macro light. Its state is sent and read stored with transaction id `ff` on every keyboard; its effect is
/// sent stored on some keyboards and not on others, each with a transaction id of its own.
#[derive(Debug, Clone, Copy)]
pub struct MacroLight {
    pub effect_stored: bool,
    pub effect_tid: u8,
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
    changes.push("macro light".into());
    changes.push("lighting effects (saved in the keyboard)".into());
    changes.push("one colour, not saved".into());
    Some(changes)
}

pub struct RazerKeyboard {
    p: Params,
    /// The brightness as first read, in 0–255: an undo puts back exactly that, not the nearest percent.
    first_brightness: Option<u8>,
    /// Whether the keyboard took the macro light's state read: `None` until it answered, `Some(false)` when it said it
    /// does not support it. Without it the macro light is not offered.
    macro_light: Option<bool>,
    /// What was sent that changed something, and how the keyboard answered, for the test report.
    log: Vec<String>,
}

impl RazerKeyboard {
    pub fn new(p: Params) -> Self {
        Self { p, first_brightness: None, macro_light: None, log: Vec::new() }
    }

    /// A request that changes something: the same exchange as a read, and a line in the log of what was sent with
    /// the keyboard's answer. A frame row is only logged when it fails: the command that shows the frame follows.
    fn send_change(
        &mut self,
        io: &mut Io,
        what: &str,
        tid: u8,
        command: [u8; 2],
        size: u8,
        args: &[u8],
    ) -> Result<[u8; 80]> {
        let result = self.query(io, tid, command, size, args);
        if command == SET_FRAME && result.is_ok() {
            return result;
        }
        let outcome = match &result {
            Ok(_) => "accepted (status 02)".to_owned(),
            Err(Error::Timeout) => "no answer, or status 04 (timed out)".to_owned(),
            Err(Error::Invalid(text) | Error::Protocol(text)) => format!("not accepted: {text}"),
            Err(e) => format!("not accepted: {e}"),
        };
        let shown = hex(&args[..args.len().min(size as usize)]);
        self.log.push(format!(
            "{what}: {:02x} {:02x} {shown} (transaction id {tid:02x}) → {outcome}",
            command[0], command[1]
        ));
        if self.log.len() > LOG_MAX {
            self.log.remove(0);
        }
        result
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

    /// The macro light of a keyboard PeriHub changes: its state, and its effect where OpenRazer stores it, read
    /// stored with transaction id `ff`. A keyboard may have no such light, so an answer that does not look right only
    /// leaves it out, with the reason on the page and in the test report.
    fn read_macro_light(
        &mut self,
        io: &mut Io,
        change: Change,
        settings: &mut Settings,
        limits: &mut Limits,
        details: &mut Vec<(String, String)>,
    ) -> Result<()> {
        let mut left_out = |label: &str, why: String| details.push((label.to_owned(), format!("not changed: {why}")));
        match self.query(io, TID_FF, GET_LED_STATE, 0x03, &[VARSTORE, MACRO_LED]) {
            Ok(a) if a[1] == MACRO_LED && a[2] <= 1 => {
                self.macro_light = Some(true);
                settings.options.insert(MACRO_LIGHT.into(), a[2].into());
                limits.options.push(Choice {
                    id: MACRO_LIGHT,
                    label: "Macro light",
                    values: vec![(0, "Off".into()), (1, "On".into())],
                });
            }
            Ok(a) => {
                self.macro_light = Some(false);
                left_out("Macro light", format!("it answered {:02x} {:02x}", a[1], a[2]));
            }
            Err(Error::Invalid(_)) => {
                self.macro_light = Some(false);
                left_out("Macro light", "the keyboard does not support it".into());
            }
            Err(Error::Protocol(text)) => {
                self.macro_light = Some(false);
                left_out("Macro light", text);
            }
            Err(e) => return Err(e),
        }
        if !change.macro_light.effect_stored || self.macro_light != Some(true) {
            return Ok(());
        }
        match self.query(io, TID_FF, GET_LED_EFFECT, 0x03, &[VARSTORE, MACRO_LED]) {
            Ok(a) if a[1] == MACRO_LED && a[2] <= 1 => {
                settings.options.insert(MACRO_EFFECT.into(), a[2].into());
                limits.options.push(Choice {
                    id: MACRO_EFFECT,
                    label: "Macro light effect",
                    values: MACRO_EFFECTS.iter().map(|(n, label)| (*n, (*label).into())).collect(),
                });
            }
            Ok(a) => left_out("Macro light effect", format!("it answered {:02x} {:02x}", a[1], a[2])),
            Err(Error::Invalid(_)) => left_out("Macro light effect", "the keyboard does not support it".into()),
            Err(Error::Protocol(text)) => left_out("Macro light effect", text),
            Err(e) => return Err(e),
        }
        Ok(())
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
            commands.extend(EFFECT_COMMANDS);
            commands.extend(MACRO_READS);
            commands.push(MACRO_SET);
            commands.push(if change.macro_light.effect_stored { MACRO_EFFECT_STORED } else { MACRO_EFFECT_UNSTORED });
        }
        commands
    }

    fn blind_settings(&self) -> Vec<BlindSetting> {
        let Some(change) = self.p.change else { return Vec::new() };
        let effects =
            EFFECTS.iter().map(|d| EffectSpec { name: d.name, colors: d.colors, speeds: d.speeds.to_vec() }).collect();
        let mut settings = vec![
            BlindSetting::new(COLOR, "One colour, not saved", BlindKind::Color),
            // The keyboard keeps it, and nothing reads it: PeriHub remembers what it sent.
            BlindSetting::remembered(EFFECT, "Lighting effect, saved in the keyboard", BlindKind::Effect { effects }),
        ];
        // Where OpenRazer does not store the macro light's effect, it cannot be read back or undone either.
        if !change.macro_light.effect_stored && self.macro_light != Some(false) {
            let values = MACRO_EFFECTS.iter().map(|(_, name)| (*name).to_owned()).collect();
            settings.push(BlindSetting::new(
                MACRO_EFFECT_UNSAVED,
                "Macro light effect, not saved",
                BlindKind::Choice { values },
            ));
        }
        settings
    }

    /// One colour on every column of every row of the custom frame, then the custom frame effect, not
    /// stored: the order and bytes of OpenRazer's own drawing. Or one of the lighting effects of the keyboard's own
    /// memory, stored as OpenRazer sends it. Or the macro light's effect where OpenRazer does not store it.
    fn send_blind(&mut self, io: &mut Io, id: &str, value: &BlindValue) -> Result<()> {
        let Some(change) = self.p.change else {
            return Err(Error::Invalid(format!("{id} is not a setting of this keyboard")));
        };
        match (id, value) {
            (COLOR, BlindValue::Color(color)) => {
                let frame = change.frame;
                for row in 0..frame.rows.min(MAX_ROWS as u8) {
                    let what = format!("one colour, row {row}");
                    self.send_change(io, &what, frame.tid, SET_FRAME, FRAME_SIZE, &frame_row_args(frame, row, *color))?;
                }
                let what = format!("one colour {}, shown", color.to_hex());
                self.send_change(io, &what, frame.tid, SET_EFFECT, 0x0C, &[NOSTORE, 0x00, CUSTOM_EFFECT])?;
                Ok(())
            }
            (EFFECT, BlindValue::Effect(effect)) => {
                let (size, args) = effect_request(effect)?;
                let colors: Vec<String> = effect.colors.iter().map(|c| c.to_hex()).collect();
                let speed = effect.speed.map(|s| format!(", speed {s}")).unwrap_or_default();
                let what = format!("lighting effect {} {}{speed}", effect.name, colors.join(" "));
                self.send_change(io, what.trim_end(), change.effects.tid, SET_EFFECT, size, &args)?;
                Ok(())
            }
            (MACRO_EFFECT_UNSAVED, BlindValue::Choice(name)) if !change.macro_light.effect_stored => {
                let (number, _) = MACRO_EFFECTS
                    .iter()
                    .find(|(_, label)| label == name)
                    .ok_or_else(|| Error::Invalid(format!("{name} is not an effect of the macro light")))?;
                let args = [NOSTORE, MACRO_LED, *number as u8];
                let what = format!("macro light effect {name}");
                self.send_change(io, &what, change.macro_light.effect_tid, SET_LED_EFFECT, 0x03, &args)?;
                Ok(())
            }
            _ => Err(Error::Invalid(format!("{id} is not a setting of this keyboard, or not of this kind"))),
        }
    }

    fn sent(&self) -> Vec<String> {
        self.log.clone()
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
        if let Some(change) = p.change {
            self.read_macro_light(io, change, &mut settings, &mut limits, &mut details)?;
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
        let Some(change) = self.p.change else {
            return Err(Error::Invalid("this keyboard is only read for now".into()));
        };
        if to.dpi_levels != from.dpi_levels || to.polling_hz != from.polling_hz {
            return Err(Error::Invalid("a keyboard has no DPI or polling rate to change".into()));
        }
        if to.options != from.options {
            if to.options.keys().ne(from.options.keys()) {
                return Err(Error::Invalid("only the game mode and the macro light are changed".into()));
            }
            for (id, &value) in &to.options {
                if from.options.get(id) == Some(&value) {
                    continue;
                }
                // Stored, as OpenRazer sends them.
                let (what, tid, command, led) = match id.as_str() {
                    GAME_MODE => {
                        let tid = self.changes_game_mode().ok_or_else(|| Error::Invalid("game mode".into()))?;
                        ("game mode", tid, SET_LED_STATE, GAME_LED)
                    }
                    MACRO_LIGHT if self.macro_light == Some(true) => ("macro light", TID_FF, SET_LED_STATE, MACRO_LED),
                    MACRO_EFFECT if self.macro_light == Some(true) && change.macro_light.effect_stored => {
                        ("macro light effect", change.macro_light.effect_tid, SET_LED_EFFECT, MACRO_LED)
                    }
                    _ => return Err(Error::Invalid("only the game mode and the macro light are changed".into())),
                };
                if value > 1 {
                    return Err(Error::Invalid(format!("{what}: value {value}")));
                }
                self.send_change(io, what, tid, command, 0x03, &[VARSTORE, led, value as u8])?;
            }
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
            self.send_change(io, "backlight brightness", tid, SET_BRIGHTNESS, 0x03, &[VARSTORE, BACKLIGHT_LED, value])?;
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

    /// What a simulated V4 Mini holds: brightness 0x80, game mode on, macro light on and static; and every request
    /// received.
    struct Board {
        brightness: u8,
        game: u8,
        macro_light: u8,
        /// The macro light's effect as stored (a stored write, and what is read), and as last written without storage.
        macro_effect: u8,
        macro_effect_unstored: Option<u8>,
        sent: Vec<Vec<u8>>,
        /// The rows of the custom frame as last drawn, and whether it was shown.
        frame: Vec<Vec<u8>>,
        shown: bool,
        /// The lighting effects received, each as its request's arguments (storage to the last colour).
        effects: Vec<Vec<u8>>,
        /// The status the keyboard answers the lighting effects with: 02 accepts them.
        effect_status: u8,
        /// The status it answers the macro light's reads with.
        macro_status: u8,
    }

    type Keyboard = Arc<Mutex<Board>>;

    fn board() -> Keyboard {
        Arc::new(Mutex::new(Board {
            brightness: 0x80,
            game: 1,
            macro_light: 1,
            macro_effect: 0,
            macro_effect_unstored: None,
            sent: Vec::new(),
            frame: Vec::new(),
            shown: false,
            effects: Vec::new(),
            effect_status: 0x02,
            macro_status: 0x02,
        }))
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
                (GET_LED_STATE, 0xFF) if req[9] == GAME_LED => a[10] = st.game,
                (SET_LED_STATE, 0xFF) if req[9] == GAME_LED => st.game = req[10],
                (GET_LED_STATE, 0xFF) if req[9] == MACRO_LED => {
                    a[0] = st.macro_status;
                    a[10] = st.macro_light;
                }
                (SET_LED_STATE, 0xFF) if req[9] == MACRO_LED => st.macro_light = req[10],
                (GET_LED_EFFECT, 0xFF) if req[9] == MACRO_LED => {
                    a[0] = st.macro_status;
                    a[10] = st.macro_effect;
                }
                // The macro light's effect: stored with ff, or not stored with the keyboard's own transaction id.
                (SET_LED_EFFECT, 0xFF) if req[8] == VARSTORE && req[9] == MACRO_LED => st.macro_effect = req[10],
                (SET_LED_EFFECT, t) if t == battery_tid && req[8] == NOSTORE && req[9] == MACRO_LED => {
                    st.macro_effect_unstored = Some(req[10])
                }
                (GET_LAYOUT, 0xFF) => a[8] = 0x11,
                (SET_FRAME, t) if t == battery_tid => st.frame.push(req[8..8 + 5 + 3 * 14].to_vec()),
                (SET_EFFECT, t) if t == battery_tid && req[8] == NOSTORE => st.shown = true,
                (SET_EFFECT, t) if t == battery_tid => {
                    a[0] = st.effect_status;
                    let size = req[5] as usize;
                    st.effects.push(req[8..8 + size].to_vec());
                }
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

    /// Starts a V4 Mini (`id`) on a simulated keyboard; `changes`: on a connection that may change things. What
    /// PeriHub remembers of its settings is forgotten first, so that every test starts as the first time.
    fn open(state: &Keyboard, id: u16, changes: bool, name: &str) -> super::super::task::PeripheralTask {
        let _ = std::fs::remove_file(crate::config::config_dir().join("remembered-settings.json"));
        let model = find(0x1532, id).unwrap();
        start_for_test(Box::new(fake(state.clone(), 0x02, id == 0x02BA)), model, id, changes, temp(name)).unwrap()
    }

    /// The same keyboard opened again by PeriHub, with what it remembered.
    fn reopen(state: &Keyboard, id: u16, name: &str) -> super::super::task::PeripheralTask {
        let model = find(0x1532, id).unwrap();
        start_for_test(Box::new(fake(state.clone(), 0x02, id == 0x02BA)), model, id, true, temp(name)).unwrap()
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
            let change = p.change.unwrap();
            let frame = change.frame;
            assert_eq!((frame.tid, frame.rows, frame.columns), (tid, 5, 14));
            assert_eq!(p.game, Some(0xFF));
            // The effects of the keyboard's own memory go with the same transaction id.
            assert_eq!(change.effects.tid, tid);
        }
        // The macro light's effect: not stored with the cable's transaction id on the cable, stored with ff through
        // the receiver, as OpenRazer's driver sends each.
        let (cable, receiver) = (wired.change.unwrap().macro_light, wireless.change.unwrap().macro_light);
        assert_eq!((cable.effect_stored, cable.effect_tid), (false, 0x1F));
        assert_eq!((receiver.effect_stored, receiver.effect_tid), (true, 0xFF));
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
            // The game mode and the macro light; the macro light's effect too where OpenRazer stores it (through the receiver).
            let ids: Vec<_> = info.limits.options.iter().map(|c| c.id).collect();
            assert_eq!(
                ids,
                if wireless { vec![GAME_MODE, MACRO_LIGHT, MACRO_EFFECT] } else { vec![GAME_MODE, MACRO_LIGHT] }
            );
            for choice in &info.limits.options {
                let values = if choice.id == MACRO_EFFECT { ["Static", "Blinking"] } else { ["Off", "On"] };
                assert_eq!(choice.values.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), values);
            }
            assert_eq!(info.settings.options.get(MACRO_LIGHT), Some(&1));
            assert_eq!(info.settings.options.get(MACRO_EFFECT), wireless.then_some(&0));
            assert!(info.details.is_empty(), "{:?}", info.details);
            // The colour, the keyboard's lighting effects, and where the macro light's effect is not stored, that.
            let blind: Vec<_> = info.blind.iter().map(|b| b.id).collect();
            assert_eq!(blind, if wireless { vec![COLOR, EFFECT] } else { vec![COLOR, EFFECT, MACRO_EFFECT_UNSAVED] });
            // The first time nothing is known: the effect the keyboard has was set by Razer Synapse.
            let effect = info.blind.iter().find(|b| b.id == EFFECT).unwrap();
            assert!(effect.remembered && effect.last.is_none() && effect.previous.is_none());
            assert!(info.blind.iter().filter(|b| b.id != EFFECT).all(|b| !b.remembered));
            assert!(info.sent.is_empty(), "nothing was sent yet");
            let layout = info.layout.as_ref().unwrap();
            assert_eq!((layout.region, layout.physical), (Some("Italian"), Some(crate::kblayout::Physical::Iso)));
            assert_eq!(info.problem, None);
            assert!(info.editable && info.changes_stay && !info.can_save);
            // Reads only, each as documented, with the transaction id of the cable or the receiver.
            let tid = if wireless { 0x9F } else { 0x1F };
            let reads: Vec<_> =
                RazerKeyboard::new(params(id)).commands().into_iter().filter(|c| c.effect == Effect::Read).collect();
            for req in state.lock().unwrap().sent.iter() {
                assert!(reads.iter().any(|c| req[6..].starts_with(c.prefix)), "{req:02x?}");
                let expected = request(req[1], [req[6], req[7]], req[5], &req[8..88]);
                assert_eq!(req[..], expected[..], "checksum");
                let on_ff = matches!([req[6], req[7]], GET_FIRMWARE | GET_LED_STATE | GET_LED_EFFECT | GET_LAYOUT);
                assert_eq!(req[1], if on_ff { 0xFF } else { tid }, "{req:02x?}");
            }
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn the_commands_of_a_v4_mini_are_the_reads_the_stored_changes_the_effects_and_the_frame() {
        for (id, macro_effect) in [
            (0x02B9, ("set macro light effect (not stored)", Effect::Change)),
            (0x02BA, ("set macro light effect (stored)", Effect::Save)),
        ] {
            let commands = RazerKeyboard::new(params(id)).commands();
            // The reads, two more for the macro light, the two stored changes, the macro light's two, the 7 effects, the frame.
            assert_eq!(commands.len(), COMMANDS.len() + 2 + 2 + 2 + 7 + 5 + 1);
            let names: Vec<_> =
                commands.iter().filter(|c| c.effect != Effect::Read).map(|c| (c.name, c.effect)).collect();
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
                    ("set lighting effect: none (stored)", Effect::Save),
                    ("set lighting effect: static (stored)", Effect::Save),
                    ("set lighting effect: breathing (stored)", Effect::Save),
                    ("set lighting effect: spectrum (stored)", Effect::Save),
                    ("set lighting effect: wave (stored)", Effect::Save),
                    ("set lighting effect: reactive (stored)", Effect::Save),
                    ("set lighting effect: starlight (stored)", Effect::Save),
                    ("set macro light (stored)", Effect::Save),
                    macro_effect,
                ]
            );
        }
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
        assert!(task.set_blind(EFFECT, &effect("Off (no effect)", &[], None)).is_err());
        assert!(task.put_back_blind(EFFECT).is_err());
        assert!(task.set_blind(MACRO_EFFECT_UNSAVED, &BlindValue::Choice("Blinking".into())).is_err());
        let reads: Vec<_> =
            RazerKeyboard::new(params(0x02B9)).commands().into_iter().filter(|c| c.effect == Effect::Read).collect();
        assert!(state.lock().unwrap().sent.iter().all(|r| reads.iter().any(|c| r[6..].starts_with(c.prefix))));
        assert!(state.lock().unwrap().frame.is_empty() && !state.lock().unwrap().shown);
        assert!(state.lock().unwrap().effects.is_empty());
        assert!(task.info.sent.is_empty(), "nothing was sent");
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
    fn only_the_effects_openrazer_sends_stored_on_the_backlight_and_other_listed_commands_leave() {
        let state = board();
        let driver = RazerKeyboard::new(params(0x02B9));
        let mut io = Io::new(Box::new(fake(state.clone(), 0x02, false)), driver.commands(), true);
        let refused: [&[u8]; 20] = [
            // Every effect of the keyboard's memory unstored, or on another light, or one OpenRazer does not send.
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, 0x01],
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, 0x00],
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, 0x04],
            &[0x0F, 0x02, NOSTORE, 0x00, 0x07],
            &[0x0F, 0x02, VARSTORE, GAME_LED, 0x01],
            &[0x0F, 0x02, VARSTORE, MACRO_LED, 0x03],
            &[0x0F, 0x02, VARSTORE, 0x00, 0x01],
            // The ripple effect and the wheel are not among them; nor is the number between reactive and starlight.
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x06],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x0A],
            // The custom effect stored, or on a light.
            &[0x0F, 0x02, VARSTORE, 0x00, CUSTOM_EFFECT],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, CUSTOM_EFFECT],
            &[0x0F, 0x02, NOSTORE, BACKLIGHT_LED, CUSTOM_EFFECT],
            // The brightness unstored or of another light; the game mode unstored or of another light.
            &[0x0F, 0x04, NOSTORE, BACKLIGHT_LED, 0x80],
            &[0x0F, 0x04, VARSTORE, 0x04, 0x80],
            &[0x03, 0x00, NOSTORE, GAME_LED, 0x01],
            &[0x03, 0x00, VARSTORE, 0x09, 0x01],
            // The macro light unstored, and the macro light's effect stored on a keyboard that sends it unstored.
            &[0x03, 0x00, NOSTORE, MACRO_LED, 0x01],
            &[0x03, 0x02, VARSTORE, MACRO_LED, 0x01],
            // A frame row the matrix does not have, or a row that starts elsewhere.
            &[0x0F, 0x03, 0x00, 0x00, 0x05, 0x00],
            // The device mode, and the effect of another light.
            &[0x00, 0x04, 0x03],
        ];
        for bytes in refused {
            let req = request(0x1F, [bytes[0], bytes[1]], 0x0C, &bytes[2..]);
            assert!(io.feature_query(&req, Duration::ZERO, |_| true).is_err(), "{bytes:02x?}");
        }
        assert!(state.lock().unwrap().sent.is_empty(), "nothing left");
        // And what OpenRazer sends does leave.
        let allowed: [&[u8]; 11] = [
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x00],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x01],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x02],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x03],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x04],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x05],
            &[0x0F, 0x02, VARSTORE, BACKLIGHT_LED, 0x07],
            &[0x03, 0x00, VARSTORE, MACRO_LED, 0x01],
            &[0x03, 0x02, NOSTORE, MACRO_LED, 0x01],
            &[0x03, 0x80, VARSTORE, MACRO_LED],
            &[0x03, 0x82, VARSTORE, MACRO_LED],
        ];
        for bytes in allowed {
            let req = request(0x1F, [bytes[0], bytes[1]], 0x0C, &bytes[2..]);
            assert!(io.feature_query(&req, Duration::ZERO, |_| true).is_ok(), "{bytes:02x?}");
        }
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
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == SET_EFFECT || [r[6], r[7]] == SET_FRAME));
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

    const RED: Rgb = Rgb::new(0xFF, 0, 0);
    const GREEN: Rgb = Rgb::new(0, 0xFF, 0);

    fn effect(name: &str, colors: &[Rgb], speed: Option<u8>) -> BlindValue {
        BlindValue::Effect(EffectValue { name: name.into(), colors: colors.to_vec(), speed })
    }

    /// The keyboard's lighting effect setting as the page shows it.
    fn effect_setting(task: &super::super::task::PeripheralTask) -> super::super::BlindSetting {
        task.info.blind.iter().find(|b| b.id == EFFECT).unwrap().clone()
    }

    #[test]
    fn the_page_offers_every_effect_the_daemon_has_for_this_keyboard_but_not_the_ripple() {
        let driver = RazerKeyboard::new(params(0x02B9));
        let setting = driver.blind_settings().into_iter().find(|b| b.id == EFFECT).unwrap();
        let BlindKind::Effect { effects } = &setting.kind else { panic!("{:?}", setting.kind) };
        let shown: Vec<_> = effects.iter().map(|e| (e.name, e.colors, e.speeds.len())).collect();
        assert_eq!(
            shown,
            [
                ("Off (no effect)", 0, 0),
                ("Static, one colour", 1, 0),
                ("Breathing, random colours", 0, 0),
                ("Breathing, one colour", 1, 0),
                ("Breathing, two colours", 2, 0),
                ("Spectrum cycling", 0, 0),
                ("Wave, left to right", 0, 0),
                ("Wave, right to left", 0, 0),
                ("Reactive, one colour", 1, 4),
                ("Starlight, random colours", 0, 3),
                ("Starlight, one colour", 1, 3),
                ("Starlight, two colours", 2, 3),
            ]
        );
        assert!(!effects.iter().any(|e| e.name.to_lowercase().contains("ripple")));
        assert!(setting.remembered && setting.label.contains("saved in the keyboard"));
        // Reactive: OpenRazer's four times; starlight: its three speeds.
        let speeds = |name: &str| effects.iter().find(|e| e.name == name).unwrap().speeds.clone();
        assert_eq!(speeds("Reactive, one colour"), [(1, "500 ms"), (2, "1000 ms"), (3, "1500 ms"), (4, "2000 ms")]);
        assert_eq!(speeds("Starlight, one colour"), [(1, "Fast"), (2, "Normal"), (3, "Slow")]);
    }

    #[test]
    fn every_effect_is_sent_with_the_bytes_openrazer_builds_stored_on_the_backlight() {
        // Name, colours, speed, size, and the arguments: storage, LED, effect, then what the effect takes.
        #[allow(clippy::type_complexity)]
        let cases: Vec<(&str, Vec<Rgb>, Option<u8>, u8, Vec<u8>)> = vec![
            ("Off (no effect)", vec![], None, 6, vec![1, 5, 0, 0, 0, 0]),
            ("Static, one colour", vec![RED], None, 9, vec![1, 5, 1, 0, 0, 1, 0xFF, 0, 0]),
            ("Breathing, random colours", vec![], None, 6, vec![1, 5, 2, 0, 0, 0]),
            ("Breathing, one colour", vec![RED], None, 9, vec![1, 5, 2, 1, 0, 1, 0xFF, 0, 0]),
            ("Breathing, two colours", vec![RED, GREEN], None, 12, vec![1, 5, 2, 2, 0, 2, 0xFF, 0, 0, 0, 0xFF, 0]),
            ("Spectrum cycling", vec![], None, 6, vec![1, 5, 3, 0, 0, 0]),
            ("Wave, left to right", vec![], None, 6, vec![1, 5, 4, 1, 0x28, 0]),
            ("Wave, right to left", vec![], None, 6, vec![1, 5, 4, 2, 0x28, 0]),
            ("Reactive, one colour", vec![RED], Some(1), 9, vec![1, 5, 5, 0, 1, 1, 0xFF, 0, 0]),
            ("Reactive, one colour", vec![GREEN], Some(2), 9, vec![1, 5, 5, 0, 2, 1, 0, 0xFF, 0]),
            ("Reactive, one colour", vec![RED], Some(3), 9, vec![1, 5, 5, 0, 3, 1, 0xFF, 0, 0]),
            ("Reactive, one colour", vec![RED], Some(4), 9, vec![1, 5, 5, 0, 4, 1, 0xFF, 0, 0]),
            ("Starlight, random colours", vec![], Some(1), 6, vec![1, 5, 7, 0, 1, 0]),
            ("Starlight, random colours", vec![], Some(3), 6, vec![1, 5, 7, 0, 3, 0]),
            ("Starlight, one colour", vec![RED], Some(2), 9, vec![1, 5, 7, 0, 2, 1, 0xFF, 0, 0]),
            ("Starlight, two colours", vec![RED, GREEN], Some(3), 12, vec![1, 5, 7, 0, 3, 2, 0xFF, 0, 0, 0, 0xFF, 0]),
        ];
        for (id, tid) in [(0x02B9, 0x1F), (0x02BA, 0x9F)] {
            let state = board();
            let mut task = open(&state, id, true, &format!("effects-{id:04x}"));
            for (name, colors, speed, size, args) in &cases {
                let before = state.lock().unwrap().sent.len();
                task.set_blind(EFFECT, &effect(name, colors, *speed)).unwrap_or_else(|e| panic!("{name}: {e}"));
                let sent = state.lock().unwrap().sent[before..].to_vec();
                assert_eq!(sent.len(), 1, "{name}: one request");
                // The whole report, checksum included, is the one the builder of OpenRazer makes.
                assert_eq!(sent[0][..], request(tid, SET_EFFECT, *size, args)[..], "{name} {args:02x?}");
                assert_eq!(state.lock().unwrap().effects.last().unwrap(), args, "{name}");
            }
            // Nothing else was touched: not the brightness, the game mode, the macro light or the frame.
            let st = state.lock().unwrap();
            assert_eq!((st.brightness, st.game, st.macro_light), (0x80, 1, 1));
            assert!(st.frame.is_empty() && !st.shown);
            assert_eq!(st.effects.len(), cases.len());
            drop(st);
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn an_effect_the_keyboard_does_not_have_or_with_values_it_does_not_take_is_refused() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "bad-effect");
        let before = state.lock().unwrap().sent.len();
        let bad = [
            effect("Ripple", &[], None),
            effect("Wheel", &[], None),
            effect("Static, one colour", &[], None),
            effect("Static, one colour", &[RED, GREEN], None),
            effect("Spectrum cycling", &[RED], None),
            effect("Breathing, two colours", &[RED], None),
            effect("Reactive, one colour", &[RED], None),
            effect("Reactive, one colour", &[RED], Some(0)),
            effect("Reactive, one colour", &[RED], Some(5)),
            effect("Starlight, one colour", &[RED], Some(4)),
            effect("Starlight, two colours", &[RED, GREEN], None),
            effect("Wave, left to right", &[], Some(1)),
            effect("Off (no effect)", &[], Some(1)),
            BlindValue::Choice("Off (no effect)".into()),
            BlindValue::Color(RED),
        ];
        for value in bad {
            assert!(task.set_blind(EFFECT, &value).is_err(), "{value:?}");
        }
        assert_eq!(state.lock().unwrap().sent.len(), before, "nothing was sent");
        let setting = effect_setting(&task);
        assert!(setting.last.is_none() && setting.previous.is_none(), "a refused value is not remembered");
        assert!(task.info.sent.is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_effect_before_the_last_one_can_be_put_back_but_not_the_one_razer_synapse_set() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "put-back");
        // Before any change PeriHub made: nothing is known, and nothing is sent.
        let setting = effect_setting(&task);
        assert!(setting.remembered && setting.last.is_none() && setting.previous.is_none());
        assert!(task.put_back_blind(EFFECT).is_err());
        let first = effect("Static, one colour", &[RED], None);
        let second = effect("Starlight, one colour", &[GREEN], Some(2));
        let third = effect("Off (no effect)", &[], None);
        let setting = {
            task.set_blind(EFFECT, &first).unwrap();
            effect_setting(&task)
        };
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&first), None));
        assert!(task.put_back_blind(EFFECT).is_err(), "what the keyboard had before the first change is unknown");
        assert_eq!(state.lock().unwrap().effects.len(), 1, "and nothing was sent for it");
        task.set_blind(EFFECT, &second).unwrap();
        task.set_blind(EFFECT, &third).unwrap();
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&third), Some(&second)));
        // Put back: the second is sent again with its own bytes, then the first.
        task.put_back_blind(EFFECT).unwrap();
        let effects = state.lock().unwrap().effects.clone();
        assert_eq!(effects.len(), 4);
        assert_eq!(effects[3], effects[1], "the second effect again");
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&second), Some(&first)));
        task.put_back_blind(EFFECT).unwrap();
        let effects = state.lock().unwrap().effects.clone();
        assert_eq!(effects[4], effects[0], "the first effect again");
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&first), None));
        assert!(task.put_back_blind(EFFECT).is_err());
        assert_eq!(state.lock().unwrap().effects.len(), 5, "nothing more was sent");
        // A refused put back changes nothing the keyboard holds, and is not counted as a change.
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_put_back_the_keyboard_refuses_leaves_what_was_remembered_alone() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "put-back-refused");
        let (first, second) = (effect("Spectrum cycling", &[], None), effect("Off (no effect)", &[], None));
        task.set_blind(EFFECT, &first).unwrap();
        task.set_blind(EFFECT, &second).unwrap();
        state.lock().unwrap().effect_status = 0x03;
        assert!(task.put_back_blind(EFFECT).is_err());
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&second), Some(&first)));
        // The refusal is in what the test report shows.
        assert!(task.info.sent.last().unwrap().contains("not accepted"), "{:?}", task.info.sent);
        state.lock().unwrap().effect_status = 0x02;
        task.put_back_blind(EFFECT).unwrap();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn what_was_sent_is_remembered_after_a_restart_for_that_connection_only() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "restart-1");
        let (first, second) = (effect("Static, one colour", &[RED], None), effect("Wave, right to left", &[], None));
        task.set_blind(EFFECT, &first).unwrap();
        task.set_blind(EFFECT, &second).unwrap();
        drop(task);
        // PeriHub opened again: what it sent is shown, and the effect before it can be put back.
        let state = board();
        let mut task = reopen(&state, 0x02B9, "restart-2");
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&second), Some(&first)));
        task.put_back_blind(EFFECT).unwrap();
        assert_eq!(state.lock().unwrap().effects.len(), 1);
        drop(task);
        let task = reopen(&board(), 0x02B9, "restart-3");
        let setting = effect_setting(&task);
        assert_eq!((setting.last.as_ref(), setting.previous.as_ref()), (Some(&first), None), "the put back was kept");
        drop(task);
        // Through the receiver (another device id): nothing is known.
        let task = reopen(&board(), 0x02BA, "restart-4");
        let setting = effect_setting(&task);
        assert!(setting.last.is_none() && setting.previous.is_none());
    }

    #[test]
    fn what_was_sent_is_kept_for_the_test_report_with_the_keyboards_answers() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "log");
        task.set_blind(EFFECT, &effect("Breathing, one colour", &[RED], None)).unwrap();
        assert_eq!(
            task.info.sent,
            [
                "lighting effect Breathing, one colour #ff0000: 0f 02 01 05 02 01 00 01 ff 00 00 (transaction id 1f) → accepted (status 02)"
            ]
        );
        // A refusal, with the status the keyboard gave: the report shows whether each effect was accepted.
        state.lock().unwrap().effect_status = 0x05;
        assert!(task.set_blind(EFFECT, &effect("Wave, left to right", &[], None)).is_err());
        assert_eq!(task.info.sent.len(), 2);
        assert_eq!(
            task.info.sent[1],
            "lighting effect Wave, left to right: 0f 02 01 05 04 01 28 00 (transaction id 1f) → not accepted: the device does not support this command"
        );
        let setting = effect_setting(&task);
        assert_eq!(
            setting.last,
            Some(effect("Breathing, one colour", &[RED], None)),
            "a refused effect is not remembered"
        );
        // A reading does not drop them.
        let info = task.info().unwrap();
        assert_eq!(info.sent.len(), 2);
        // The brightness, the game mode, the macro light and the colour are in it too.
        state.lock().unwrap().effect_status = 0x02;
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(80);
        task.change(&wanted, false, None).unwrap();
        task.set_blind(COLOR, &BlindValue::Color(Rgb::new(1, 2, 3))).unwrap();
        let sent = task.info.sent.clone();
        assert!(sent[2].starts_with("backlight brightness: 0f 04 01 05 cc (transaction id 1f) → accepted"), "{sent:?}");
        assert_eq!(sent.len(), 4, "five rows are not logged, only the command that shows the colour: {sent:?}");
        assert_eq!(sent[3], "one colour #010203, shown: 0f 02 00 00 08 (transaction id 1f) → accepted (status 02)");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_log_of_what_was_sent_keeps_the_latest_lines() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "log-max");
        for n in 0..(LOG_MAX + 5) {
            let color = Rgb::new(n as u8, 0, 0);
            task.set_blind(EFFECT, &effect("Static, one colour", &[color], None)).unwrap();
        }
        assert_eq!(task.info.sent.len(), LOG_MAX);
        assert!(task.info.sent.last().unwrap().contains(&format!("#{:02x}0000", LOG_MAX + 4)));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_macro_light_is_stored_read_back_and_undone_like_the_game_mode() {
        for (id, wireless) in [(0x02B9, false), (0x02BA, true)] {
            let state = board();
            let mut task = open(&state, id, true, &format!("macro-{id:04x}"));
            assert_eq!(task.info.settings.options.get(MACRO_LIGHT), Some(&1));
            let mut wanted = task.info.settings.clone();
            wanted.options.insert(MACRO_LIGHT.into(), 0);
            task.change(&wanted, false, None).unwrap();
            assert_eq!(state.lock().unwrap().macro_light, 0);
            assert_eq!(task.info.settings.options.get(MACRO_LIGHT), Some(&0), "read back");
            // As OpenRazer sends it, on either connection: stored, transaction id ff.
            let set = state
                .lock()
                .unwrap()
                .sent
                .iter()
                .find(|r| [r[6], r[7]] == SET_LED_STATE && r[9] == MACRO_LED)
                .unwrap()
                .clone();
            assert_eq!(set[..11], [0, 0xFF, 0, 0, 0, 0x03, 0x03, 0x00, VARSTORE, MACRO_LED, 0], "wireless: {wireless}");
            assert_eq!(set[88], request(0xFF, SET_LED_STATE, 0x03, &set[8..88])[88], "checksum");
            // The game mode and the brightness were not touched.
            let st = state.lock().unwrap();
            assert_eq!((st.game, st.brightness), (1, 0x80));
            assert!(!st.sent.iter().any(|r| [r[6], r[7]] == SET_LED_STATE && r[9] == GAME_LED));
            drop(st);
            task.expire_undo();
            crate::worker::DeviceTask::tick(&mut task).unwrap();
            assert_eq!(state.lock().unwrap().macro_light, 1, "on again");
            let _ = std::fs::remove_dir_all(task.backups());
        }
    }

    #[test]
    fn the_macro_light_effect_is_stored_through_the_receiver_and_not_stored_on_the_cable() {
        // Through the receiver: stored with ff, read back, and undone.
        let state = board();
        let mut task = open(&state, 0x02BA, true, "macro-effect-wireless");
        assert!(task.info.blind.iter().all(|b| b.id != MACRO_EFFECT_UNSAVED));
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(MACRO_EFFECT.into(), 1);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().macro_effect, 1);
        assert_eq!(task.info.settings.options.get(MACRO_EFFECT), Some(&1), "read back");
        let set = state.lock().unwrap().sent.iter().find(|r| [r[6], r[7]] == SET_LED_EFFECT).unwrap().clone();
        assert_eq!(set[..11], [0, 0xFF, 0, 0, 0, 0x03, 0x03, 0x02, VARSTORE, MACRO_LED, 1]);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().macro_effect, 0, "static again");
        let _ = std::fs::remove_dir_all(task.backups());
        // On the cable: OpenRazer sends it without storage, with the keyboard's transaction id; the stored
        // effect it reads is not what that write changes, so it is a setting that cannot be read back.
        let state = board();
        let mut task = open(&state, 0x02B9, true, "macro-effect-wired");
        assert!(!task.info.settings.options.contains_key(MACRO_EFFECT));
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == GET_LED_EFFECT), "not asked for");
        let setting = task.info.blind.iter().find(|b| b.id == MACRO_EFFECT_UNSAVED).unwrap();
        assert!(!setting.remembered);
        assert_eq!(setting.kind, BlindKind::Choice { values: vec!["Static".into(), "Blinking".into()] });
        task.set_blind(MACRO_EFFECT_UNSAVED, &BlindValue::Choice("Blinking".into())).unwrap();
        let set = state.lock().unwrap().sent.iter().find(|r| [r[6], r[7]] == SET_LED_EFFECT).unwrap().clone();
        assert_eq!(set[..11], [0, 0x1F, 0, 0, 0, 0x03, 0x03, 0x02, NOSTORE, MACRO_LED, 1]);
        assert_eq!(state.lock().unwrap().macro_effect_unstored, Some(1));
        assert_eq!(state.lock().unwrap().macro_effect, 0, "the stored one is untouched");
        assert!(task.set_blind(MACRO_EFFECT_UNSAVED, &BlindValue::Choice("Pulsating".into())).is_err());
        assert!(task.set_blind(MACRO_EFFECT_UNSAVED, &BlindValue::Number(1)).is_err());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_keyboard_that_does_not_take_the_macro_light_is_left_without_one_and_otherwise_works() {
        let state = board();
        state.lock().unwrap().macro_status = 0x05;
        let mut task = open(&state, 0x02B9, true, "no-macro");
        assert_eq!(task.info.problem, None);
        assert!(task.info.editable);
        assert!(
            task.info.details.contains(&("Macro light".into(), "not changed: the keyboard does not support it".into()))
        );
        assert!(!task.info.settings.options.contains_key(MACRO_LIGHT));
        assert!(task.info.limits.options.iter().all(|c| c.id == GAME_MODE));
        let blind: Vec<_> = task.info.blind.iter().map(|b| b.id).collect();
        assert_eq!(blind, [COLOR, EFFECT], "no macro light effect either");
        // The rest is still changed.
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].brightness = Some(80);
        task.change(&wanted, false, None).unwrap();
        task.set_blind(EFFECT, &effect("Spectrum cycling", &[], None)).unwrap();
        // The macro light is not sent to.
        assert!(!state.lock().unwrap().sent.iter().any(|r| [r[6], r[7]] == SET_LED_STATE && r[9] == MACRO_LED));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_effect_setting_and_its_value_cross_to_the_interface_in_the_shape_it_reads() {
        let state = board();
        let mut task = open(&state, 0x02B9, true, "json");
        let first = effect("Reactive, one colour", &[Rgb::new(0, 0xFF, 0)], Some(3));
        task.set_blind(EFFECT, &first).unwrap();
        task.set_blind(EFFECT, &effect("Off (no effect)", &[], None)).unwrap();
        let json = serde_json::to_value(&task.info.blind[1]).unwrap();
        assert_eq!(json["id"], "effect");
        assert_eq!(json["remembered"], true);
        assert_eq!(json["kind"]["type"], "effect");
        let reactive =
            json["kind"]["effects"].as_array().unwrap().iter().find(|e| e["name"] == "Reactive, one colour").unwrap();
        assert_eq!(reactive["colors"], 1);
        assert_eq!(reactive["speeds"][0], serde_json::json!([1, "500 ms"]));
        assert_eq!(
            json["last"],
            serde_json::json!({"type": "effect", "value": {"name": "Off (no effect)", "colors": [], "speed": null}})
        );
        assert_eq!(
            json["previous"],
            serde_json::json!({"type": "effect", "value": {"name": "Reactive, one colour", "colors": ["#00ff00"], "speed": 3}})
        );
        // What the interface sends back is read the same way.
        let back: BlindValue = serde_json::from_value(json["previous"].clone()).unwrap();
        assert_eq!(back, first);
        // The settings that are not remembered say so, and have nothing before them.
        let color = serde_json::to_value(&task.info.blind[0]).unwrap();
        assert_eq!((color["remembered"].as_bool(), color["previous"].is_null()), (Some(false), true));
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

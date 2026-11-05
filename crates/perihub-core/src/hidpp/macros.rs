//! Macros stored in the mouse's memory (HID++ 0x8100, macro format 1).
//!
//! A button bound to a macro points to a sector and an offset ([`ButtonAction::Macro`]). From
//! there the mouse runs commands until `0xFF`. The length of a command comes from its code: 1 byte
//! below 0x20, 2 below 0x40, 3 below 0x60, 5 below 0x80. Codes from libratbag (MIT) and
//! cvuchener/hidpp.
//!
//! Checked on a real G502 HERO by pressing the buttons and recording what the mouse sent:
//! - key presses and releases `[modifiers, usage]` (a press of modifiers alone stays held until
//!   released), delays (milliseconds), mouse buttons (bit mask), media keys (Consumer usage) and the
//!   wheel (signed notches, positive scrolls up); 16-bit values are big-endian;
//! - `0x60 [0, sector, offset]` continues in another place, also in another sector;
//! - `0x01` waits until the button is released;
//! - `0x02` starts again from the beginning while the button is held and ends when it is released:
//!   commands after it never run;
//! - `0x03` starts again forever. Pressing the button again restarts it, and other buttons and
//!   macros do not stop it: only changing profile does. PeriHub reads such macros but does not write them.
//!
//! Two macros can run at the same time.
//!
//! [`ButtonAction::Macro`]: super::profile::ButtonAction::Macro

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const OP_NOOP: u8 = 0x00;
const OP_WAIT_RELEASE: u8 = 0x01;
const OP_REPEAT_WHILE_HELD: u8 = 0x02;
const OP_REPEAT_FOREVER: u8 = 0x03;
const OP_WHEEL: u8 = 0x20;
const OP_DELAY: u8 = 0x40;
const OP_MOUSE_DOWN: u8 = 0x41;
const OP_MOUSE_UP: u8 = 0x42;
const OP_KEY_DOWN: u8 = 0x43;
const OP_KEY_UP: u8 = 0x44;
const OP_CONSUMER_DOWN: u8 = 0x45;
const OP_CONSUMER_UP: u8 = 0x46;
const OP_JUMP: u8 = 0x60;
const OP_END: u8 = 0xFF;

const JUMP_LEN: usize = 5;
/// Last key usage the mouse's keyboard report accepts (from its report descriptor).
const MAX_KEY: u8 = 0xA4;
/// Last media key usage the mouse's report accepts.
const MAX_CONSUMER: u16 = 0x028C;
/// Upper bound on the commands of one macro, against corrupt memory.
const MAX_COMMANDS: usize = 4096;

/// One command of a macro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroStep {
    /// Keyboard key (HID usage) pressed, with the standard HID modifiers as in a key binding.
    /// `key` is 0 when only modifiers are pressed.
    KeyDown {
        modifiers: u8,
        key: u8,
    },
    KeyUp {
        modifiers: u8,
        key: u8,
    },
    /// Mouse buttons, bit 0 = left, 1 = right, 2 = middle, 3 = back, 4 = forward.
    MouseDown {
        buttons: u16,
    },
    MouseUp {
        buttons: u16,
    },
    /// Media key (usage from the HID Consumer page, e.g. 0x00E9 = volume up).
    ConsumerDown {
        usage: u16,
    },
    ConsumerUp {
        usage: u16,
    },
    /// Wheel notches: positive scrolls up.
    Wheel {
        amount: i8,
    },
    Delay {
        ms: u16,
    },
    /// Does nothing for the duration of one step.
    Noop,
}

/// When the steps of a macro run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroKind {
    /// Once, when the button is pressed.
    Once,
    /// `press` when the button is pressed, `release` when it is released.
    Sequence,
    /// Again and again while the button is held.
    WhileHeld,
    /// Again and again until the profile changes. Read only: see the module notes.
    Forever,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub kind: MacroKind,
    pub press: Vec<MacroStep>,
    /// Steps run when the button is released (`Sequence` only).
    #[serde(default)]
    pub release: Vec<MacroStep>,
}

fn unsupported(why: &str) -> Error {
    Error::Protocol(format!("unsupported onboard macro: {why}"))
}

fn command_len(op: u8) -> Option<usize> {
    match op {
        OP_END | 0x00..=0x1F => Some(1),
        0x20..=0x3F => Some(2),
        0x40..=0x5F => Some(3),
        0x60..=0x7F => Some(JUMP_LEN),
        _ => None,
    }
}

/// Runs through the commands of the macro at `sector`/`offset`, following jumps, until the end
/// command or until `visit` returns `false`. Returns the sectors the macro uses.
/// `read_sector` returns a sector's bytes; the last two are its CRC and hold no commands.
fn walk(
    mut read_sector: impl FnMut(u16) -> Result<Vec<u8>>,
    sector: u8,
    offset: u16,
    mut visit: impl FnMut(u8, &[u8]) -> Result<bool>,
) -> Result<Vec<u8>> {
    let mut sectors = vec![sector];
    let mut jumps = HashSet::new();
    let (mut sector, mut at) = (sector, offset as usize);
    let mut data = read_sector(sector as u16)?;
    for _ in 0..MAX_COMMANDS {
        let end = data.len().saturating_sub(2);
        if at >= end {
            return Err(unsupported("no end command in its sector"));
        }
        let op = data[at];
        let len = command_len(op).ok_or_else(|| unsupported(&format!("command {op:#04x}")))?;
        if at + len > end {
            return Err(unsupported("a command crosses the end of its sector"));
        }
        let args = &data[at + 1..at + len];
        if op == OP_JUMP {
            // [memory, sector, offset]: memory 0 is the writable one, as in a button binding.
            if args[0] != 0 {
                return Err(unsupported("it continues in another memory"));
            }
            let to = (args[1], u16::from_be_bytes([args[2], args[3]]));
            if !jumps.insert(to) {
                return Err(unsupported("it jumps in a loop"));
            }
            if to.0 != sector {
                data = read_sector(to.0 as u16)?;
                if !sectors.contains(&to.0) {
                    sectors.push(to.0);
                }
            }
            (sector, at) = (to.0, to.1 as usize);
            continue;
        }
        if op == OP_END || !visit(op, args)? {
            return Ok(sectors);
        }
        at += len;
    }
    Err(unsupported("too many commands"))
}

/// Sectors used by the macro at `sector`/`offset`, which no other macro may overwrite. Commands
/// PeriHub does not know are skipped by their length, so this works on unreadable macros too.
pub fn sectors_used(read_sector: impl FnMut(u16) -> Result<Vec<u8>>, sector: u8, offset: u16) -> Result<Vec<u8>> {
    walk(read_sector, sector, offset, |op, _| Ok(op != OP_REPEAT_WHILE_HELD && op != OP_REPEAT_FOREVER))
}

/// Reads the macro that starts at `offset` in `sector`.
pub fn parse(read_sector: impl FnMut(u16) -> Result<Vec<u8>>, sector: u8, offset: u16) -> Result<Macro> {
    let mut m = Macro { kind: MacroKind::Once, press: Vec::new(), release: Vec::new() };
    walk(read_sector, sector, offset, |op, args| match op {
        OP_WAIT_RELEASE | OP_REPEAT_WHILE_HELD | OP_REPEAT_FOREVER if m.kind != MacroKind::Once => {
            Err(unsupported("more than one repeat or release command"))
        }
        OP_WAIT_RELEASE => {
            m.kind = MacroKind::Sequence;
            Ok(true)
        }
        OP_REPEAT_WHILE_HELD => {
            m.kind = MacroKind::WhileHeld;
            Ok(false)
        }
        OP_REPEAT_FOREVER => {
            m.kind = MacroKind::Forever;
            Ok(false)
        }
        _ => {
            let step = decode_step(op, args)?;
            if m.kind == MacroKind::Sequence {
                m.release.push(step)
            } else {
                m.press.push(step)
            }
            Ok(true)
        }
    })?;
    Ok(m)
}

fn decode_step(op: u8, args: &[u8]) -> Result<MacroStep> {
    let be = || u16::from_be_bytes([args[0], args[1]]);
    Ok(match op {
        OP_NOOP => MacroStep::Noop,
        OP_WHEEL => MacroStep::Wheel { amount: args[0] as i8 },
        OP_DELAY => MacroStep::Delay { ms: be() },
        OP_MOUSE_DOWN => MacroStep::MouseDown { buttons: be() },
        OP_MOUSE_UP => MacroStep::MouseUp { buttons: be() },
        OP_KEY_DOWN => MacroStep::KeyDown { modifiers: args[0], key: args[1] },
        OP_KEY_UP => MacroStep::KeyUp { modifiers: args[0], key: args[1] },
        OP_CONSUMER_DOWN => MacroStep::ConsumerDown { usage: be() },
        OP_CONSUMER_UP => MacroStep::ConsumerUp { usage: be() },
        _ => return Err(unsupported(&format!("command {op:#04x}"))),
    })
}

impl MacroStep {
    fn validate(&self) -> Result<()> {
        let ok = match *self {
            Self::KeyDown { modifiers, key } | Self::KeyUp { modifiers, key } => {
                (key == 0 && modifiers != 0) || (0x04..=MAX_KEY).contains(&key)
            }
            Self::MouseDown { buttons } | Self::MouseUp { buttons } => buttons != 0,
            Self::ConsumerDown { usage } | Self::ConsumerUp { usage } => (1..=MAX_CONSUMER).contains(&usage),
            Self::Wheel { amount } => amount != 0,
            Self::Delay { .. } | Self::Noop => true,
        };
        if ok { Ok(()) } else { Err(Error::Invalid(format!("macro step {self:?}"))) }
    }

    fn encode(&self) -> Vec<u8> {
        let be = |op: u8, value: u16| {
            let [hi, lo] = value.to_be_bytes();
            vec![op, hi, lo]
        };
        match *self {
            Self::KeyDown { modifiers, key } => vec![OP_KEY_DOWN, modifiers, key],
            Self::KeyUp { modifiers, key } => vec![OP_KEY_UP, modifiers, key],
            Self::MouseDown { buttons } => be(OP_MOUSE_DOWN, buttons),
            Self::MouseUp { buttons } => be(OP_MOUSE_UP, buttons),
            Self::ConsumerDown { usage } => be(OP_CONSUMER_DOWN, usage),
            Self::ConsumerUp { usage } => be(OP_CONSUMER_UP, usage),
            Self::Wheel { amount } => vec![OP_WHEEL, amount as u8],
            Self::Delay { ms } => be(OP_DELAY, ms),
            Self::Noop => vec![OP_NOOP],
        }
    }
}

impl Macro {
    /// Checks that the macro can be written to the mouse.
    pub fn validate(&self) -> Result<()> {
        match self.kind {
            MacroKind::Forever => {
                return Err(Error::Invalid("macros that repeat until the profile changes cannot be saved".into()));
            }
            MacroKind::Sequence if self.press.is_empty() && self.release.is_empty() => {
                return Err(Error::Invalid("the macro has no steps".into()));
            }
            MacroKind::Once | MacroKind::WhileHeld if self.press.is_empty() => {
                return Err(Error::Invalid("the macro has no steps".into()));
            }
            MacroKind::Once | MacroKind::WhileHeld if !self.release.is_empty() => {
                return Err(Error::Invalid("only a sequence has steps for the release".into()));
            }
            _ => {}
        }
        self.press.iter().chain(&self.release).try_for_each(MacroStep::validate)
    }

    /// The macro's commands, each as its bytes, ending with the command that ends it.
    /// [`Packer`] adds jumps where a macro continues in another sector.
    fn commands(&self) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = self.press.iter().map(MacroStep::encode).collect();
        match self.kind {
            MacroKind::Once => {}
            MacroKind::Sequence => {
                out.push(vec![OP_WAIT_RELEASE]);
                out.extend(self.release.iter().map(MacroStep::encode));
            }
            MacroKind::WhileHeld => out.push(vec![OP_REPEAT_WHILE_HELD]),
            MacroKind::Forever => out.push(vec![OP_REPEAT_FOREVER]),
        }
        // Never reached after a repeat, but keeps the next macro in the sector from running if a
        // firmware went on after it.
        out.push(vec![OP_END]);
        out
    }
}

/// Places macros one after the other in free sectors of the mouse's memory.
pub struct Packer {
    size: usize,
    free: VecDeque<u8>,
    sectors: Vec<(u8, Vec<u8>)>,
    at: usize,
}

impl Packer {
    /// `free` lists the sectors that may be overwritten, in the order they are used.
    pub fn new(free: impl IntoIterator<Item = u8>, sector_size: u16) -> Self {
        Self { size: sector_size as usize, free: free.into_iter().collect(), sectors: Vec::new(), at: 0 }
    }

    /// Bytes left in the current sector, before its CRC.
    fn room(&self) -> usize {
        if self.sectors.is_empty() { 0 } else { self.size - 2 - self.at }
    }

    fn next_free(&self) -> Result<u8> {
        self.free
            .front()
            .copied()
            .ok_or_else(|| Error::Invalid("the mouse's memory is full: make the macros shorter".into()))
    }

    fn open_sector(&mut self) -> Result<()> {
        let sector = self.next_free()?;
        self.free.pop_front();
        self.sectors.push((sector, vec![0xFF; self.size]));
        self.at = 0;
        Ok(())
    }

    fn push(&mut self, bytes: &[u8]) {
        let (_, data) = self.sectors.last_mut().expect("a sector is open");
        data[self.at..self.at + bytes.len()].copy_from_slice(bytes);
        self.at += bytes.len();
    }

    /// Writes a macro and returns where it starts (sector, offset). A macro that does not fit
    /// continues in the next free sector.
    pub fn place(&mut self, m: &Macro) -> Result<(u8, u16)> {
        let commands = m.commands();
        let total: usize = commands.iter().map(Vec::len).sum();
        // Start here only if the whole macro fits, or at least its first command and a jump.
        if self.room() < total.min(commands[0].len() + JUMP_LEN) {
            self.open_sector()?;
        }
        let start = (self.sectors.last().expect("a sector is open").0, self.at as u16);
        for (i, command) in commands.iter().enumerate() {
            let rest: usize = commands[i..].iter().map(Vec::len).sum();
            // Every command placed with more to come leaves room for a jump after it.
            if rest > self.room() && command.len() + JUMP_LEN > self.room() {
                self.push(&[OP_JUMP, 0, self.next_free()?, 0, 0]);
                self.open_sector()?;
            }
            self.push(command);
        }
        Ok(start)
    }

    /// The sectors to write, filled with 0xFF after the macros. Their CRC is not set yet.
    pub fn finish(self) -> Vec<(u8, Vec<u8>)> {
        self.sectors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: usize = 256;

    /// A sector with `bytes` at `offset`, the rest erased.
    fn sector_with(offset: usize, bytes: &[u8]) -> Vec<u8> {
        let mut data = vec![0xFF; SIZE];
        data[offset..offset + bytes.len()].copy_from_slice(bytes);
        data
    }

    /// Sectors smaller than 256 bytes (the G502 X) are read too: a macro must stay inside them.
    #[test]
    fn small_sectors_end_in_an_error_not_a_panic() {
        const SMALL: usize = 240;
        let small = |offset: usize, bytes: &[u8]| {
            let mut data = vec![0xFF; SMALL];
            data[offset..offset + bytes.len()].copy_from_slice(bytes);
            data
        };
        // A key press whose arguments would go past the CRC.
        let sectors = vec![(6, small(SMALL - 3, &[OP_KEY_DOWN, 0x00]))];
        assert!(parse(reader(sectors.clone()), 6, (SMALL - 3) as u16).is_err());
        assert!(sectors_used(reader(sectors), 6, (SMALL - 3) as u16).is_err());
        // A jump to an offset past the end of the next sector.
        let sectors = vec![(6, small(0, &[OP_JUMP, 0, 7, 0x01, 0x00])), (7, small(0, &[OP_END]))];
        assert!(parse(reader(sectors.clone()), 6, 0).is_err());
        assert!(sectors_used(reader(sectors), 6, 0).is_err());
        // A macro that starts past the end.
        assert!(parse(reader(vec![(6, small(0, &[OP_END]))]), 6, 0x0300).is_err());
        // The same macro inside the small sector is read.
        let sectors = vec![(6, small(SMALL - 6, &[OP_KEY_DOWN, 0x00, 0x04, OP_END]))];
        assert!(parse(reader(sectors), 6, (SMALL - 6) as u16).is_ok());
    }

    fn reader(sectors: Vec<(u16, Vec<u8>)>) -> impl FnMut(u16) -> Result<Vec<u8>> {
        move |id| {
            sectors
                .iter()
                .find(|(s, _)| *s == id)
                .map(|(_, d)| d.clone())
                .ok_or_else(|| Error::Invalid(format!("sector {id}")))
        }
    }

    fn tap(key: u8) -> [MacroStep; 3] {
        [MacroStep::KeyDown { modifiers: 0, key }, MacroStep::Delay { ms: 20 }, MacroStep::KeyUp { modifiers: 0, key }]
    }

    fn once(press: Vec<MacroStep>) -> Macro {
        Macro { kind: MacroKind::Once, press, release: Vec::new() }
    }

    /// Packs macros into sectors 7, 8… and returns a reader over them.
    type Reader = Box<dyn FnMut(u16) -> Result<Vec<u8>>>;

    fn packed(macros: &[Macro]) -> (Vec<(u8, u16)>, Reader) {
        let mut packer = Packer::new(7..16, SIZE as u16);
        let addresses = macros.iter().map(|m| packer.place(m).unwrap()).collect();
        let sectors = packer.finish().into_iter().map(|(s, d)| (s as u16, d)).collect();
        (addresses, Box::new(reader(sectors)))
    }

    #[test]
    fn encodes_like_the_mouse() {
        let m = Macro {
            kind: MacroKind::Sequence,
            press: vec![
                MacroStep::KeyDown { modifiers: 0x02, key: 0 },
                MacroStep::MouseDown { buttons: 0x0002 },
                MacroStep::ConsumerDown { usage: 0x00E9 },
                MacroStep::Wheel { amount: -1 },
                MacroStep::Delay { ms: 300 },
                MacroStep::Noop,
            ],
            release: vec![MacroStep::KeyUp { modifiers: 0x02, key: 0 }],
        };
        let bytes: Vec<u8> = m.commands().concat();
        assert_eq!(
            bytes,
            [
                0x43, 0x02, 0x00, 0x41, 0x00, 0x02, 0x45, 0x00, 0xE9, 0x20, 0xFF, 0x40, 0x01, 0x2C, 0x00, 0x01, 0x44,
                0x02, 0x00, 0xFF
            ]
        );
        let (addresses, read) = packed(std::slice::from_ref(&m));
        assert_eq!(addresses, [(7, 0)]);
        assert_eq!(parse(read, 7, 0).unwrap(), m);
    }

    #[test]
    fn repeats_end_the_macro() {
        let held = sector_with(0x10, &[0x43, 0x00, 0x04, 0x02, 0x43, 0x00, 0x05, 0xFF]);
        let m = parse(reader(vec![(6, held.clone())]), 6, 0x10).unwrap();
        assert_eq!(m, Macro { kind: MacroKind::WhileHeld, press: vec![tap(4)[0].clone()], release: vec![] });
        assert_eq!(sectors_used(reader(vec![(6, held)]), 6, 0x10).unwrap(), [6]);
        let forever = sector_with(0, &[0x43, 0x00, 0x04, 0x03]);
        assert_eq!(parse(reader(vec![(6, forever)]), 6, 0).unwrap().kind, MacroKind::Forever);
        assert!(Macro { kind: MacroKind::Forever, press: tap(4).to_vec(), release: vec![] }.validate().is_err());
    }

    #[test]
    fn follows_jumps_to_other_sectors() {
        // The jump checked on the G502: [0x60, memory 0, sector 8, offset 0x0020].
        let first = sector_with(0, &[0x43, 0x00, 0x04, 0x60, 0x00, 0x08, 0x00, 0x20]);
        let second = sector_with(0x20, &[0x44, 0x00, 0x04, 0xFF]);
        let read = || reader(vec![(7, first.clone()), (8, second.clone())]);
        assert_eq!(parse(read(), 7, 0).unwrap().press, [tap(4)[0].clone(), tap(4)[2].clone()]);
        assert_eq!(sectors_used(read(), 7, 0).unwrap(), [7, 8]);
    }

    #[test]
    fn long_macros_continue_in_the_next_sector() {
        let long = once((0..30).flat_map(|i| tap(0x04 + i % 26)).collect()); // 270 bytes
        let short = once(tap(0x05).to_vec());
        let (addresses, mut read) = packed(&[short.clone(), long.clone(), short.clone()]);
        assert_eq!(addresses[0], (7, 0));
        assert_eq!(addresses[1], (7, 10));
        assert_eq!(parse(&mut read, 7, 0).unwrap(), short);
        assert_eq!(parse(&mut read, 7, 10).unwrap(), long);
        assert_eq!(sectors_used(&mut read, 7, 10).unwrap(), [7, 8]);
        let (sector, offset) = addresses[2];
        assert_eq!(sector, 8);
        assert_eq!(parse(&mut read, sector, offset).unwrap(), short);
    }

    #[test]
    fn a_macro_starts_where_its_first_command_and_a_jump_fit() {
        let filler = once((0..26).flat_map(|i| tap(0x04 + i)).collect()); // 235 bytes
        let small = once(tap(0x04).to_vec()); // 10 bytes: fits in the 19 left
        let medium = once((0..3).flat_map(|i| tap(0x04 + i)).collect()); // 28 bytes: does not
        let (addresses, mut read) = packed(&[filler.clone(), small.clone(), medium.clone()]);
        assert_eq!(addresses[1], (7, 235));
        // 9 bytes are left: the first command (3) and a jump (5) fit, so it starts there.
        assert_eq!(addresses[2], (7, 245));
        assert_eq!(parse(&mut read, 7, 245).unwrap(), medium);

        // With 3 bytes left it starts in the next sector.
        let mut longer = filler;
        longer.press.extend([MacroStep::Delay { ms: 1 }, MacroStep::Delay { ms: 1 }]); // 241 bytes
        let (addresses, mut read) = packed(&[longer, small, medium.clone()]);
        assert_eq!(addresses[1], (7, 241));
        assert_eq!(addresses[2], (8, 0));
        assert_eq!(parse(&mut read, 8, 0).unwrap(), medium);
    }

    #[test]
    fn full_memory_is_an_error() {
        let long = once((0..200).flat_map(|i| tap(0x04 + i % 26)).collect()); // 1800 bytes
        let mut packer = Packer::new([7, 8], SIZE as u16);
        assert!(packer.place(&long).is_err());
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        // Unknown command.
        assert!(parse(reader(vec![(6, sector_with(0, &[0x21, 0x01, 0xFF]))]), 6, 0).is_err());
        // Jump back to itself.
        assert!(parse(reader(vec![(6, sector_with(0, &[0x60, 0x00, 0x06, 0x00, 0x00]))]), 6, 0).is_err());
        // Runs into the CRC without an end command.
        let mut no_end = vec![0x00; SIZE];
        no_end[SIZE - 2..].copy_from_slice(&[0xFF, 0xFF]);
        assert!(parse(reader(vec![(6, no_end)]), 6, 0).is_err());
        // A command cut by the CRC.
        assert!(parse(reader(vec![(6, sector_with(SIZE - 4, &[0x40, 0x00]))]), 6, (SIZE - 4) as u16).is_err());
        // An unknown command is still skipped by its length when looking for the sectors in use.
        let unknown = sector_with(0, &[0x21, 0x01, 0x60, 0x00, 0x09, 0x00, 0x00]);
        let read = reader(vec![(6, unknown), (9, sector_with(0, &[0xFF]))]);
        assert_eq!(sectors_used(read, 6, 0).unwrap(), [6, 9]);
    }

    #[test]
    fn validation() {
        assert!(once(vec![]).validate().is_err());
        assert!(once(vec![MacroStep::KeyDown { modifiers: 0, key: 0 }]).validate().is_err());
        assert!(once(vec![MacroStep::KeyDown { modifiers: 0, key: 0xE0 }]).validate().is_err());
        assert!(once(vec![MacroStep::KeyDown { modifiers: 0x01, key: 0 }]).validate().is_ok());
        assert!(Macro { kind: MacroKind::Once, press: tap(4).to_vec(), release: tap(5).to_vec() }.validate().is_err());
        assert!(Macro { kind: MacroKind::Sequence, press: vec![], release: tap(5).to_vec() }.validate().is_ok());
    }
}

//! EVision keyboards (320f and 0c45 ids), reached through their vendor collection (usage page `ff1c`).
//! The protocol is in `docs/protocols/evision.md`, from facts found in OpenRGB (GPL-2.0) and OpenEVision
//! (GPL-2.0), which tried it on a `320f:505b`. No code was copied. A Redragon K580 (`320f:5000`, issue #117)
//! has answered the reads with PeriHub 0.9.7 as documented.
//!
//! Every listed id is read; only `320f:505b` (the one OpenEVision checked on a keyboard) and `320f:5000` (the same
//! controller and writes in OpenRGB, and a keyboard that answered the reads) may have their lighting changed, as an
//! experimental feature that stays in the keyboard's flash. The key table is read once for the test report and
//! never written. On Linux these keyboards are not opened: the interface also carries
//! what is typed (see `peripheral::opened_on_linux`).

use std::collections::BTreeMap;
use std::time::Duration;

use super::task::Driver;
use super::{Choice, Command, Effect, Io, Kind, Led, LedLimits, Limits, Model, Protocol, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, Result};

/// The vendor collection: interface 1, usage page ff1c.
const INTERFACE: i32 = 1;
const USAGE_PAGE: u16 = 0xFF1C;

/// Report number, then 63 bytes, both ways.
const REPORT_ID: u8 = 0x04;
const REPORT: usize = 64;
/// Data bytes a report holds: from byte 8 on.
const DATA_MAX: usize = REPORT - 8;
const TIMEOUT: Duration = Duration::from_millis(500);

const BEGIN: u8 = 0x01;
const END: u8 = 0x02;
const CAPABILITIES: u8 = 0x03;
const READ_CONFIG: u8 = 0x05;
const WRITE_CONFIG: u8 = 0x06;

/// The lighting header: mode, brightness, speed, direction, random colour, red, green, blue.
const HEADER: usize = 8;
/// Layout 2: the profile in use at offset 0, then 64 bytes per profile.
const PROFILE_FIRST: u16 = 0x01;
const PROFILE_SIZE: u16 = 0x40;
const PROFILES: u8 = 3;
/// Layout 1: the key table, 128 entries of 3 bytes.
const KEY_TABLE: u16 = 0xC0;
const KEY_ENTRIES: usize = 128;

const MAX_BRIGHTNESS: u8 = 4;
const MAX_SPEED: u8 = 5;
/// One step of the brightness, in percent.
const STEP: u8 = 25;
const SPEED: &str = "speed";

/// The lighting modes, by the number the header holds.
const MODES: [(u8, &str); 19] = [
    (1, "Wave (short)"),
    (2, "Wave (long)"),
    (3, "Colour wheel"),
    (4, "Spectrum cycle"),
    (5, "Breathing"),
    (6, "Static"),
    (7, "Reactive"),
    (8, "Reactive ripple"),
    (9, "Reactive line"),
    (10, "Starlight (fast)"),
    (11, "Blooming"),
    (12, "Vertical rainbow"),
    (13, "Hurricane"),
    (14, "Accumulate"),
    (15, "Starlight (slow)"),
    (16, "Visor"),
    (17, "Surmount"),
    (18, "Circular rainbow"),
    (20, "Custom (per key)"),
];
/// The modes PeriHub sets: the older controller's list. Starlight (slow) is not on it, Surmount takes its
/// colour from a parameter PeriHub does not write, and Custom shows colours PeriHub never writes.
const OFFERED: [(u8, &str); 16] = [
    MODES[0], MODES[1], MODES[2], MODES[3], MODES[4], MODES[5], MODES[6], MODES[7], MODES[8], MODES[9], MODES[10],
    MODES[11], MODES[12], MODES[13], MODES[15], MODES[17],
];

/// How the configuration memory is laid out (see the protocol page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The lighting header at offset 0, the key table at c0.
    One,
    /// The profile in use at offset 0, a header in each profile.
    Two,
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub layout: Layout,
    /// The lighting may be changed (experimental). Only where a source tried the reads and writes, or lists the id
    /// on the same controller and a keyboard answered the reads as documented.
    pub edit: bool,
}

const READ: Params = Params { layout: Layout::One, edit: false };
const EDIT: Params = Params { layout: Layout::One, edit: true };
const PROFILED: Params = Params { layout: Layout::Two, edit: false };

const fn keyboard(vendor_id: u16, product_ids: &'static [u16], name: &'static str, params: Params) -> Model {
    Model {
        vendor_id,
        product_ids,
        brand: "EVision",
        name,
        kind: Kind::Keyboard,
        interface: Some(INTERFACE),
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::Evision(params),
    }
}

/// OpenRGB's detection lists, one model per id so that a confirmation confirms only that id.
pub const MODELS: &[Model] = &[
    keyboard(0x320F, &[0x505B], "RGB keyboard 320f 505b", EDIT),
    keyboard(0x320F, &[0x5000], "RGB keyboard 320f 5000", EDIT),
    keyboard(0x320F, &[0x502A], "RGB keyboard 320f 502a", READ),
    keyboard(0x320F, &[0x5064], "RGB keyboard 320f 5064", READ),
    keyboard(0x320F, &[0x5078], "RGB keyboard 320f 5078", READ),
    keyboard(0x320F, &[0x5084], "RGB keyboard 320f 5084", READ),
    keyboard(0x320F, &[0x5041], "RGB keyboard 320f 5041", PROFILED),
    keyboard(0x0C45, &[0x5004], "RGB keyboard 0c45 5004", READ),
    keyboard(0x0C45, &[0x5104], "RGB keyboard 0c45 5104", READ),
    keyboard(0x0C45, &[0x5204], "RGB keyboard 0c45 5204", READ),
    keyboard(0x0C45, &[0x652F], "RGB keyboard 0c45 652f", READ),
    keyboard(0x0C45, &[0x7672], "RGB keyboard 0c45 7672", READ),
    keyboard(0x0C45, &[0x7698], "RGB keyboard 0c45 7698", READ),
    keyboard(0x0C45, &[0x8520], "RGB keyboard 0c45 8520", READ),
];

const fn command(prefix: &'static [u8], effect: Effect, name: &'static str) -> Command {
    // Bytes 1 and 2 of a request are its checksum: the command is recognized from byte 3.
    Command { prefix, at: 3, effect, name }
}

/// A request: report number, checksum, command, data size, offset, then the data. Reads give the size
/// they want and no data.
fn request(command: u8, offset: u16, size: u8, data: &[u8]) -> [u8; REPORT] {
    let mut r = [0u8; REPORT];
    r[0] = REPORT_ID;
    r[3] = command;
    r[4] = if data.is_empty() { size } else { data.len() as u8 };
    r[5..7].copy_from_slice(&offset.to_le_bytes());
    r[8..8 + data.len()].copy_from_slice(data);
    let sum: u16 = r[3..].iter().map(|&b| u16::from(b)).sum();
    r[1..3].copy_from_slice(&sum.to_le_bytes());
    r
}

struct Answer {
    status: u8,
    data: Vec<u8>,
}

/// Sends a request and returns the answer with the same command and offset: the keyboard answers every
/// program that has the collection open, so what is waiting is dropped first and other answers skipped.
fn exchange(io: &mut Io, req: &[u8; REPORT]) -> Result<Answer> {
    io.drain();
    let mut reply = [0u8; REPORT];
    io.numbered_output_exchange(req, &mut reply, TIMEOUT, |r| {
        r[0] == REPORT_ID && r[3] == req[3] && r[5..7] == req[5..7]
    })?;
    let size = reply[4] as usize;
    if size > DATA_MAX {
        return Err(Error::Protocol(format!("an answer with {size} data bytes")));
    }
    Ok(Answer { status: reply[7], data: reply[8..8 + size].to_vec() })
}

/// Reads `len` bytes of the configuration memory, as many requests as it takes.
fn read_config(io: &mut Io, offset: u16, len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let want = (len - out.len()).min(DATA_MAX);
        let at = offset + out.len() as u16;
        let answer = exchange(io, &request(READ_CONFIG, at, want as u8, &[]))?;
        if answer.status != 0 {
            return Err(Error::Protocol(format!(
                "the keyboard refused a read at {at:#06x} (status {:02x})",
                answer.status
            )));
        }
        if answer.data.is_empty() || answer.data.len() > want {
            return Err(Error::Protocol(format!("{} bytes came back for {want}", answer.data.len())));
        }
        out.extend(answer.data);
    }
    Ok(out)
}

/// What the header says, and what in it is out of range.
fn decode(h: &[u8; HEADER]) -> (Settings, Vec<String>) {
    let mut problems = Vec::new();
    if !MODES.iter().any(|(m, _)| *m == h[0]) {
        problems.push(format!("lighting mode {}", h[0]));
    }
    if h[1] > MAX_BRIGHTNESS {
        problems.push(format!("brightness {}", h[1]));
    }
    if h[2] > MAX_SPEED {
        problems.push(format!("speed {}", h[2]));
    }
    let led = Led { mode: h[0], brightness: Some(h[1].min(MAX_BRIGHTNESS) * STEP), color: Rgb::from_bytes(&h[5..8]) };
    let settings = Settings {
        options: BTreeMap::from([(SPEED.to_string(), u16::from(h[2]))]),
        leds: vec![led],
        ..Default::default()
    };
    (settings, problems)
}

fn mode_name(mode: u8) -> String {
    match MODES.iter().find(|(m, _)| *m == mode) {
        Some((_, name)) => format!("{name} ({mode})"),
        None => format!("{mode}"),
    }
}

/// Most kinds of not-described entry the summary spells out: a keyboard has a handful of them.
const OTHER_SHOWN: usize = 8;

/// How many entries of the key table are of each kind: key, disabled key, modifier, media, Fn, other.
/// The other entries are no source's to name, so the test report gets their bytes, to tell what they are.
fn key_table_summary(table: &[u8]) -> String {
    let (mut keys, mut disabled, mut modifiers, mut media, mut fn_keys, mut other) = (0, 0, 0, 0, 0, 0);
    let mut unknown: BTreeMap<[u8; 3], usize> = BTreeMap::new();
    for e in table.as_chunks::<3>().0 {
        match (e[0], e[1], e[2]) {
            (0x02, 0x02, 0x00) => disabled += 1,
            (0x02, 0x02, _) => keys += 1,
            (0x02, 0x01, _) => modifiers += 1,
            (0x03, _, _) => media += 1,
            (0x05, _, _) => fn_keys += 1,
            _ => {
                other += 1;
                *unknown.entry(*e).or_default() += 1;
            }
        }
    }
    let mut seen: Vec<String> = unknown
        .iter()
        .take(OTHER_SHOWN)
        .map(|(e, n)| if *n > 1 { format!("{n}× {}", super::hex(e)) } else { super::hex(e) })
        .collect();
    if unknown.len() > OTHER_SHOWN {
        seen.push(format!("{} more kinds", unknown.len() - OTHER_SHOWN));
    }
    let other_text = format!("not described: {}", seen.join(", "));
    let parts: Vec<String> =
        [(keys, "keys"), (disabled, "disabled"), (modifiers, "modifiers"), (media, "media"), (fn_keys, "Fn")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}"))
            .chain((other > 0).then(|| format!("{other} {other_text}")))
            .collect();
    format!("{} entries: {}", table.len() / 3, parts.join(", "))
}

pub struct Evision {
    p: Params,
    /// The header as last read, when it looked as documented: what a change starts from.
    header: Option<[u8; HEADER]>,
    /// What the key table held when it was read (once).
    keys: Option<String>,
    keys_tried: bool,
}

impl Evision {
    pub fn new(p: Params) -> Self {
        Self { p, header: None, keys: None, keys_tried: false }
    }

    fn read_all(&mut self, io: &mut Io) -> Result<Reading> {
        self.header = None;
        let mut reading = Reading::default();
        let mut details: Vec<(String, String)> = Vec::new();
        let mut at = 0u16;
        if self.p.layout == Layout::Two {
            let caps = exchange(io, &request(CAPABILITIES, 0, 7, &[]))?;
            if caps.status != 0 {
                return Err(Error::Protocol(format!(
                    "the keyboard refused the capabilities read (status {:02x})",
                    caps.status
                )));
            }
            if caps.data.len() < 7 || caps.data[..2] != [0xAA, 0x55] {
                reading.problem = Some(format!("unexpected answer: capabilities {}", super::hex(&caps.data)));
                return Ok(reading);
            }
            details.push(("Key slots".into(), caps.data[5].to_string()));
            details.push(("Macro memory".into(), format!("{} bytes", u32::from(caps.data[6]) * 0x80)));
            let profile = read_config(io, 0, 1)?[0];
            if profile >= PROFILES {
                reading.problem = Some(format!("unexpected answer: profile {profile}"));
                reading.details = details;
                return Ok(reading);
            }
            at = PROFILE_FIRST + u16::from(profile) * PROFILE_SIZE;
            reading.active_profile = Some(profile + 1);
        }
        let header: [u8; HEADER] = read_config(io, at, HEADER)?.try_into().expect("8 bytes");
        let (settings, problems) = decode(&header);
        details.push(("Lighting mode".into(), mode_name(header[0])));
        details.push(("Direction (a number)".into(), header[3].to_string()));
        details.push(("Random colour".into(), if header[4] == 0 { "off" } else { "on" }.into()));
        if self.p.layout == Layout::One && !self.keys_tried {
            self.keys_tried = true;
            // For the test report: a keyboard that does not answer this still gets its page.
            match read_config(io, KEY_TABLE, KEY_ENTRIES * 3) {
                Ok(table) => self.keys = Some(key_table_summary(&table)),
                Err(Error::Timeout | Error::Protocol(_)) => {}
                Err(e) => return Err(e),
            }
        }
        if let Some(keys) = &self.keys {
            details.push(("Key table".into(), keys.clone()));
        }
        reading.settings = settings;
        reading.details = details;
        if problems.is_empty() {
            self.header = Some(header);
            if self.p.edit {
                reading.limits = Limits {
                    options: vec![Choice {
                        id: SPEED,
                        label: "Speed",
                        values: [
                            (5, "5 (slowest)"),
                            (4, "4"),
                            (3, "3 (normal)"),
                            (2, "2"),
                            (1, "1"),
                            (0, "0 (fastest)"),
                        ]
                        .map(|(v, l)| (v, l.to_string()))
                        .to_vec(),
                    }],
                    leds: vec![LedLimits {
                        name: "Lighting".into(),
                        modes: OFFERED.to_vec(),
                        brightness_step: Some(STEP),
                        color: true,
                    }],
                    ..Default::default()
                };
            }
        } else {
            reading.problem = Some(format!("unexpected answer: {}", problems.join(", ")));
        }
        Ok(reading)
    }
}

impl Driver for Evision {
    fn commands(&self) -> Vec<Command> {
        let mut commands = vec![command(&[READ_CONFIG], Effect::Read, "read the configuration")];
        if self.p.layout == Layout::Two {
            commands.push(command(&[CAPABILITIES], Effect::Read, "read the capabilities"));
        }
        if self.p.edit {
            commands.push(command(&[BEGIN], Effect::Change, "begin a configuration change"));
            commands.push(command(&[END], Effect::Change, "end a configuration change"));
            // Only the 8 bytes of the lighting header, at offset 0.
            commands.push(command(&[WRITE_CONFIG, HEADER as u8, 0, 0], Effect::Save, "write the lighting"));
        }
        commands
    }

    fn source(&self) -> &'static str {
        "OpenRGB, OpenEVision (GPL-2.0, facts only)"
    }

    fn profiles(&self) -> u8 {
        if self.p.layout == Layout::Two { PROFILES } else { 1 }
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        true
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        if !self.p.edit {
            io.forbid_changes();
        }
        // For the test report, at every pass.
        let _ = io.report_descriptor();
        match self.read_all(io) {
            Ok(reading) => Ok(reading),
            // The page and its test report show it, and it is read again every few seconds.
            Err(Error::Timeout) => Ok(Reading {
                details: vec![(
                    "Status".into(),
                    "no answer yet: if the keyboard is plugged in, please send a test report".into(),
                )],
                ..Default::default()
            }),
            Err(Error::Protocol(why)) => Ok(Reading {
                problem: Some(format!("the keyboard did not answer as documented: {why}")),
                ..Default::default()
            }),
            Err(e) => Err(e),
        }
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        if !self.p.edit {
            return Err(Error::Invalid("this keyboard is only read for now".into()));
        }
        let ([_], [new]) = (from.leds.as_slice(), to.leds.as_slice()) else {
            return Err(Error::Invalid("lighting".into()));
        };
        let header = self.header.ok_or_else(|| Error::Invalid("the lighting was not read as documented".into()))?;
        if decode(&header).0 != *from {
            return Err(Error::Invalid("the keyboard's lighting changed since it was read".into()));
        }
        if !MODES.iter().any(|(m, _)| *m == new.mode) {
            return Err(Error::Invalid(format!("lighting mode {}", new.mode)));
        }
        let brightness = new
            .brightness
            .filter(|b| *b <= 100 && b.is_multiple_of(STEP))
            .ok_or_else(|| Error::Invalid("brightness".into()))?;
        let speed = to
            .options
            .get(SPEED)
            .and_then(|s| u8::try_from(*s).ok())
            .filter(|s| *s <= MAX_SPEED)
            .ok_or_else(|| Error::Invalid("speed".into()))?;
        // Direction and the random colour flag go back as they were read.
        let mut wanted = header;
        wanted[0] = new.mode;
        wanted[1] = brightness / STEP;
        wanted[2] = speed;
        wanted[5..8].copy_from_slice(&new.color.to_bytes());
        if wanted == header {
            return Ok(());
        }
        let begun = exchange(io, &request(BEGIN, 0, 0, &[]))?;
        if begun.status != 0 {
            return Err(Error::Protocol(format!("the keyboard refused to begin (status {:02x})", begun.status)));
        }
        let written = exchange(io, &request(WRITE_CONFIG, 0, 0, &wanted)).and_then(|a| match a.status {
            0 => Ok(()),
            s => Err(Error::Protocol(format!("the keyboard refused the lighting (status {s:02x})"))),
        });
        // The end is sent also after a refused write.
        let ended = exchange(io, &request(END, 0, 0, &[]));
        written?;
        ended.map(|_| ())
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

    /// A simulated keyboard: its configuration memory, what it answers, and every report it got.
    struct Keyboard {
        memory: Vec<u8>,
        capabilities: Vec<u8>,
        sent: Vec<Vec<u8>>,
        writing: bool,
        /// The status it answers with to a write, and to a read.
        write_status: u8,
        read_status: u8,
        /// Sends an answer to another request first, as it does to every program that has it open.
        noisy: bool,
    }

    type Shared = Arc<Mutex<Keyboard>>;

    const HEADER_AT_ONE: usize = 0;
    const HEADER_AT_TWO: usize = 0x41;

    /// A 320f:505b as OpenEVision describes it: Static, brightness 4, speed 3, red, and a key table.
    fn keyboard_505b() -> Shared {
        let mut memory = vec![0u8; 0x1000];
        memory[..8].copy_from_slice(&[6, 4, 3, 0, 0, 0xff, 0x00, 0x00]);
        let table = &mut memory[KEY_TABLE as usize..KEY_TABLE as usize + KEY_ENTRIES * 3];
        for (i, e) in table.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            let entry: [u8; 3] = match i {
                0..=99 => [0x02, 0x02, 0x04 + i as u8],
                100..=106 => [0x02, 0x02, 0x00],
                107..=116 => [0x02, 0x01, 1 << (i % 8)],
                117..=126 => [0x03, 0xe9, 0x00],
                _ => [0x05, 0x00, 0x00],
            };
            e.copy_from_slice(&entry);
        }
        Arc::new(Mutex::new(Keyboard {
            memory,
            capabilities: vec![0xaa, 0x55, 0, 0, 0, 0, 0],
            sent: Vec::new(),
            writing: false,
            write_status: 0,
            read_status: 0,
            noisy: false,
        }))
    }

    /// A keyboard with the newer layout: the second profile in use.
    fn keyboard_with_profiles() -> Shared {
        let k = keyboard_505b();
        {
            let mut m = k.lock().unwrap();
            m.memory[..8].fill(0);
            m.memory[0] = 1;
            m.memory[HEADER_AT_TWO..HEADER_AT_TWO + 8].copy_from_slice(&[5, 2, 1, 1, 1, 0, 0xff, 0]);
            m.capabilities = vec![0xaa, 0x55, 0, 0, 0, 126, 8];
        }
        k
    }

    fn answer(k: &mut Keyboard, r: &[u8]) -> Vec<Vec<u8>> {
        assert_eq!(r.len(), REPORT);
        assert_eq!(r[0], REPORT_ID);
        let sum: u16 = r[3..].iter().map(|&b| u16::from(b)).sum();
        assert_eq!(u16::from_le_bytes([r[1], r[2]]), sum, "checksum of {r:02x?}");
        k.sent.push(r.to_vec());
        let (offset, size) = (u16::from_le_bytes([r[5], r[6]]) as usize, r[4] as usize);
        let mut a = r.to_vec();
        match r[3] {
            BEGIN => k.writing = true,
            END => k.writing = false,
            CAPABILITIES => a[8..15].copy_from_slice(&k.capabilities),
            READ_CONFIG => {
                if k.read_status == 0 {
                    a[8..8 + size].copy_from_slice(&k.memory[offset..offset + size]);
                }
                a[7] = k.read_status;
            }
            WRITE_CONFIG => {
                assert!(k.writing, "a write outside begin and end");
                if k.write_status == 0 {
                    k.memory[offset..offset + size].copy_from_slice(&r[8..8 + size]);
                }
                a[7] = k.write_status;
            }
            other => panic!("unexpected command {other:02x}"),
        }
        let mut reports = Vec::new();
        if k.noisy {
            // The answer to a read of another offset, and a report of another command.
            let mut other = a.clone();
            other[5] ^= 0x01;
            reports.push(other);
            let mut other = a.clone();
            other[3] = 0x13;
            reports.push(other);
        }
        reports.push(a);
        reports
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(move |r| answer(&mut state.lock().unwrap(), r));
        hid.0.lock().unwrap().numbered_output = true;
        hid
    }

    fn model(vendor: u16, id: u16) -> &'static Model {
        find(vendor, id).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-evision-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn commands_sent(state: &Shared) -> Vec<u8> {
        state.lock().unwrap().sent.iter().map(|r| r[3]).collect()
    }

    fn header(state: &Shared, at: usize) -> [u8; 8] {
        state.lock().unwrap().memory[at..at + 8].try_into().unwrap()
    }

    #[test]
    fn requests_have_the_documented_bytes() {
        let begin = request(BEGIN, 0, 0, &[]);
        assert_eq!(begin[..5], [0x04, 0x01, 0x00, 0x01, 0x00]);
        assert!(begin[5..].iter().all(|&b| b == 0));
        assert_eq!(request(END, 0, 0, &[])[..4], [0x04, 0x02, 0x00, 0x02]);
        // A read of 56 bytes at c0: the checksum is 05 + 56 + c0.
        let read = request(READ_CONFIG, 0xC0, 56, &[]);
        assert_eq!(read[3..7], [0x05, 56, 0xC0, 0x00]);
        assert_eq!(u16::from_le_bytes([read[1], read[2]]), 0x05 + 56 + 0xC0);
        let write = request(WRITE_CONFIG, 0, 0, &[6, 4, 3, 0, 0, 255, 0, 0]);
        assert_eq!(write[3..16], [0x06, 8, 0, 0, 0, 6, 4, 3, 0, 0, 255, 0, 0]);
        assert_eq!(u16::from_le_bytes([write[1], write[2]]), 6 + 8 + 6 + 4 + 3 + 255);
    }

    #[test]
    fn a_505b_reads_as_documented_and_only_reads() {
        let state = keyboard_505b();
        let m = model(0x320F, 0x505B);
        let task = start_for_test(Box::new(fake(state.clone())), m, 0x505B, true, temp("read")).unwrap();
        let info = &task.info;
        assert_eq!(info.problem, None);
        assert!(info.editable && info.changes_stay && !info.can_save);
        assert_eq!(info.settings.leds, [Led { mode: 6, brightness: Some(100), color: Rgb { r: 255, g: 0, b: 0 } }]);
        assert_eq!(info.settings.options[SPEED], 3);
        assert!(info.details.contains(&("Lighting mode".into(), "Static (6)".into())), "{:?}", info.details);
        assert!(info.details.contains(&("Random colour".into(), "off".into())));
        assert!(
            info.details.contains(&(
                "Key table".into(),
                "128 entries: 100 keys, 7 disabled, 10 modifiers, 10 media, 1 Fn".into()
            )),
            "{:?}",
            info.details
        );
        assert_eq!(info.limits.leds[0].modes, OFFERED);
        assert_eq!(info.limits.leds[0].brightness_step, Some(25));
        // Only reads, each on the list: the header, then the key table in 56-byte reads.
        let commands = Evision::new(EDIT).commands();
        for req in state.lock().unwrap().sent.iter() {
            assert!(commands.iter().any(|c| req[c.at..].starts_with(c.prefix)), "{req:02x?}");
        }
        assert_eq!(commands_sent(&state), [READ_CONFIG; 8]);
        let reads: Vec<(u16, u8)> =
            state.lock().unwrap().sent.iter().map(|r| (u16::from_le_bytes([r[5], r[6]]), r[4])).collect();
        assert_eq!(reads[0], (0, 8));
        assert_eq!(reads[1], (0xC0, 56));
        assert_eq!(reads[7], (0xC0 + 6 * 56, 48));
    }

    #[test]
    fn answers_meant_for_other_requests_are_skipped() {
        let state = keyboard_505b();
        state.lock().unwrap().noisy = true;
        let task = start_for_test(Box::new(fake(state)), model(0x320F, 0x505B), 0x505B, true, temp("noisy")).unwrap();
        assert_eq!(task.info.problem, None);
        assert_eq!(task.info.settings.leds[0].mode, 6);
        assert!(task.info.details.iter().any(|(l, v)| l == "Key table" && v.starts_with("128 entries")));
    }

    #[test]
    fn a_lighting_change_is_written_between_begin_and_end_and_undone_unless_kept() {
        let state = keyboard_505b();
        // Direction and the random colour flag, which PeriHub does not change.
        state.lock().unwrap().memory[3] = 2;
        state.lock().unwrap().memory[4] = 1;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("change")).unwrap();
        let from = state.lock().unwrap().sent.len();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0] = Led { mode: 5, brightness: Some(50), color: Rgb { r: 0, g: 0, b: 255 } };
        wanted.options.insert(SPEED.into(), 1);
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(header(&state, HEADER_AT_ONE), [5, 2, 1, 2, 1, 0, 0, 255]);
        assert_eq!(info.settings, wanted);
        let sent = commands_sent(&state)[from..].to_vec();
        // Read first, begin, write, end, read back.
        assert_eq!(sent, [READ_CONFIG, BEGIN, WRITE_CONFIG, END, READ_CONFIG]);
        let write = state.lock().unwrap().sent.iter().find(|r| r[3] == WRITE_CONFIG).unwrap().clone();
        assert_eq!(write[4..8], [8, 0, 0, 0], "8 bytes at offset 0");
        assert_eq!(write[8..16], [5, 2, 1, 2, 1, 0, 0, 255]);
        assert!(info.changes_stay);
        // Not kept: put back.
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(header(&state, HEADER_AT_ONE), [6, 4, 3, 2, 1, 255, 0, 0]);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_kept_change_stays_and_an_unchanged_one_sends_nothing() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("keep")).unwrap();
        let wanted = task.info.settings.clone();
        let from = state.lock().unwrap().sent.len();
        task.change(&wanted, false, None).unwrap();
        assert!(!commands_sent(&state)[from..].contains(&WRITE_CONFIG));
        let mut wanted = wanted;
        wanted.leds[0].mode = 4;
        task.change(&wanted, false, None).unwrap();
        task.keep_changes();
        assert_eq!(header(&state, HEADER_AT_ONE)[0], 4);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_refused_write_is_put_back_and_the_end_is_still_sent() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("refused"))
                .unwrap();
        state.lock().unwrap().write_status = 1;
        let from = state.lock().unwrap().sent.len();
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 4;
        assert!(task.change(&wanted, false, None).is_err());
        assert_eq!(header(&state, HEADER_AT_ONE), [6, 4, 3, 0, 0, 255, 0, 0]);
        let sent = commands_sent(&state)[from..].to_vec();
        assert_eq!(sent[..4], [READ_CONFIG, BEGIN, WRITE_CONFIG, END], "{sent:?}");
        assert!(!state.lock().unwrap().writing, "ended");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn modes_that_are_not_offered_are_shown_and_can_be_put_back() {
        let state = keyboard_505b();
        state.lock().unwrap().memory[0] = 20;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("custom")).unwrap();
        assert_eq!(task.info.problem, None);
        assert!(task.info.details.contains(&("Lighting mode".into(), "Custom (per key) (20)".into())));
        assert!(!task.info.limits.leds[0].modes.iter().any(|(m, _)| *m == 20));
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 20;
        wanted.leds[0].color = Rgb { r: 1, g: 2, b: 3 };
        assert!(task.change(&wanted, false, None).is_ok(), "a colour changes while the mode stays");
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(header(&state, HEADER_AT_ONE)[0], 20, "the undo writes a mode that is not offered");
        // A mode that does not exist is refused.
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 19;
        assert!(task.change(&wanted, false, None).is_err());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_other_ids_are_only_read_even_on_a_connection_that_may_change() {
        for (vendor, id) in [(0x320F, 0x502A), (0x0C45, 0x5004), (0x0C45, 0x8520)] {
            let state = keyboard_505b();
            let mut task =
                start_for_test(Box::new(fake(state.clone())), model(vendor, id), id, true, temp("other")).unwrap();
            assert_eq!(task.info.problem, None, "{id:04x}");
            assert!(!task.info.editable && task.info.limits == Limits::default());
            assert_eq!(task.info.settings.leds[0].mode, 6);
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].mode = 4;
            assert!(task.change(&wanted, false, None).is_err());
            assert!(commands_sent(&state).iter().all(|&c| c == READ_CONFIG), "{:?}", commands_sent(&state));
            assert_eq!(header(&state, HEADER_AT_ONE)[0], 6);
        }
    }

    #[test]
    fn the_command_line_never_changes_the_lighting() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, false, temp("cli")).unwrap();
        assert!(!task.info.editable);
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 4;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(commands_sent(&state).iter().all(|&c| c == READ_CONFIG));
    }

    #[test]
    fn the_newer_layout_reads_the_profile_in_use_and_its_header() {
        let state = keyboard_with_profiles();
        let task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5041), 0x5041, true, temp("two")).unwrap();
        let info = &task.info;
        assert_eq!(info.problem, None);
        assert_eq!(info.active_profile, Some(2));
        assert_eq!(info.profiles, 3);
        assert_eq!(info.settings.leds, [Led { mode: 5, brightness: Some(50), color: Rgb { r: 0, g: 255, b: 0 } }]);
        assert!(info.details.contains(&("Key slots".into(), "126".into())));
        assert!(info.details.contains(&("Macro memory".into(), "1024 bytes".into())));
        assert!(!info.editable && info.limits == Limits::default());
        // The capabilities, the profile, its header: and no key table.
        assert_eq!(commands_sent(&state), [CAPABILITIES, READ_CONFIG, READ_CONFIG]);
        assert_eq!(state.lock().unwrap().sent[2][5..7], [HEADER_AT_TWO as u8, 0]);
    }

    #[test]
    fn answers_that_do_not_look_as_documented_leave_it_read_only() {
        // A header out of range.
        for (name, header) in [
            ("brightness", [6u8, 9, 3, 0, 0, 0, 0, 0]),
            ("mode", [19, 4, 3, 0, 0, 0, 0, 0]),
            ("speed", [6, 4, 6, 0, 0, 0, 0, 0]),
        ] {
            let state = keyboard_505b();
            state.lock().unwrap().memory[..8].copy_from_slice(&header);
            let mut task =
                start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("odd"))
                    .unwrap();
            assert!(task.info.problem.as_deref().is_some_and(|p| p.contains(name)), "{name}: {:?}", task.info.problem);
            assert!(!task.info.editable && task.info.limits == Limits::default(), "{name}");
            let mut wanted = task.info.settings.clone();
            wanted.leds[0].mode = 4;
            assert!(task.change(&wanted, false, None).is_err(), "{name}");
            assert!(!commands_sent(&state).contains(&WRITE_CONFIG), "{name}");
        }
        // Capabilities without the magic bytes, and a profile that does not exist.
        let state = keyboard_with_profiles();
        state.lock().unwrap().capabilities[0] = 0;
        let task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5041), 0x5041, true, temp("magic")).unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("capabilities")));
        assert_eq!(commands_sent(&state), [CAPABILITIES]);
        let state = keyboard_with_profiles();
        state.lock().unwrap().memory[0] = 3;
        let task = start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5041), 0x5041, true, temp("profile"))
            .unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("profile 3")));
        // A refused read.
        let state = keyboard_505b();
        state.lock().unwrap().read_status = 1;
        let task = start_for_test(Box::new(fake(state)), model(0x320F, 0x505B), 0x505B, true, temp("status")).unwrap();
        assert!(task.info.problem.as_deref().is_some_and(|p| p.contains("did not answer as documented")));
        assert!(!task.info.editable || task.info.limits == Limits::default());
    }

    #[test]
    fn a_keyboard_that_does_not_answer_gets_its_page_and_test_report() {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().numbered_output = true;
        hid.0.lock().unwrap().descriptor = Some(vec![0x06, 0x1C, 0xFF, 0x09, 0x92, 0xA1, 0x01, 0x85, 0x04, 0xC0]);
        let mut task = start_for_test(Box::new(hid), model(0x320F, 0x505B), 0x505B, true, temp("silent")).unwrap();
        assert!(task.info.details[0].1.starts_with("no answer yet"));
        assert_eq!(task.info.problem, None);
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "kept after a refresh: {raw:?}");
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = keyboard_505b();
        let mut io = Io::new(Box::new(fake(state.clone())), Evision::new(EDIT).commands(), true);
        let refused = |io: &mut Io, r: [u8; REPORT]| assert!(exchange(io, &r).is_err(), "{:02x?}", &r[..8]);
        // The custom colours, live colours, the V2 colour commands, anything unknown.
        for command in [0x10, 0x11, 0x12, 0x13, 0x0A, 0x0B, 0x03, 0x04, 0x07, 0xF0, 0xFF] {
            refused(&mut io, request(command, 0, 8, &[]));
        }
        // A write of the key table, of another size, or at another offset.
        refused(&mut io, request(WRITE_CONFIG, KEY_TABLE, 0, &[2, 2, 4]));
        refused(&mut io, request(WRITE_CONFIG, 0, 0, &[1, 2, 3, 4]));
        refused(&mut io, request(WRITE_CONFIG, 8, 0, &[0; 8]));
        assert!(state.lock().unwrap().sent.is_empty());
        // A connection that may not change anything refuses even the listed writes.
        let mut io = Io::new(Box::new(fake(state.clone())), Evision::new(EDIT).commands(), false);
        refused(&mut io, request(WRITE_CONFIG, 0, 0, &[6, 4, 3, 0, 0, 255, 0, 0]));
        refused(&mut io, request(BEGIN, 0, 0, &[]));
        assert!(state.lock().unwrap().sent.is_empty());
        // The read-only models have no write on their list at all.
        for params in [READ, PROFILED] {
            let commands = Evision::new(params).commands();
            assert!(commands.iter().all(|c| c.effect == Effect::Read), "{params:?}");
        }
    }

    #[test]
    fn the_table_is_what_the_sources_name() {
        let ids: Vec<(u16, u16)> =
            MODELS.iter().flat_map(|m| m.product_ids.iter().map(|&p| (m.vendor_id, p))).collect();
        assert_eq!(ids.len(), 14);
        assert!(MODELS.iter().all(|m| m.product_ids.len() == 1 && m.kind == Kind::Keyboard));
        assert!(MODELS.iter().all(|m| m.interface == Some(1) && m.usage_page == Some(0xFF1C)));
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no id twice");
        // Only the ids OpenEVision tried (505b) or a keyboard answered as documented (5000, issue #117) may have
        // their lighting changed; the newer layout is only read.
        let edits: Vec<_> = MODELS
            .iter()
            .filter(|m| matches!(m.protocol, Protocol::Evision(p) if p.edit))
            .map(|m| (m.vendor_id, m.product_ids[0]))
            .collect();
        assert_eq!(edits, [(0x320F, 0x505B), (0x320F, 0x5000)]);
        assert!(!super::super::opened_on_linux(model(0x320F, 0x5000)), "the interface also carries typing");
        assert!(!super::super::opened_on_linux(model(0x320F, 0x505B)), "the interface also carries typing");
    }

    #[test]
    fn key_tables_are_summarized() {
        let table = [2, 2, 4, 2, 2, 0, 2, 1, 2, 3, 0xe9, 0, 5, 0, 0, 9, 9, 9];
        assert_eq!(
            key_table_summary(&table),
            "6 entries: 1 keys, 1 disabled, 1 modifiers, 1 media, 1 Fn, 1 not described: 09 09 09"
        );
        // The same bytes together are counted, the kinds are in byte order, and a long list is cut.
        let table: Vec<u8> = [[2, 0, 0], [2, 0, 0], [0xd0, 0xa2, 1], [2, 0, 0]].concat();
        assert_eq!(key_table_summary(&table), "4 entries: 4 not described: 3× 02 00 00, d0 a2 01");
        let table: Vec<u8> = (0..10u8).flat_map(|i| [0xd0, 0xa2, i]).collect();
        assert_eq!(
            key_table_summary(&table),
            "10 entries: 10 not described: d0 a2 00, d0 a2 01, d0 a2 02, d0 a2 03, d0 a2 04, d0 a2 05, d0 a2 06, d0 a2 07, 2 more kinds"
        );
    }

    #[test]
    fn the_header_is_decoded_and_out_of_range_values_are_named() {
        let (settings, problems) = decode(&[18, 0, 5, 7, 1, 1, 2, 3]);
        assert!(problems.is_empty());
        assert_eq!(settings.leds[0], Led { mode: 18, brightness: Some(0), color: Rgb { r: 1, g: 2, b: 3 } });
        assert_eq!(settings.options[SPEED], 5);
        let (_, problems) = decode(&[0, 5, 6, 0, 0, 0, 0, 0]);
        assert_eq!(problems.len(), 3, "{problems:?}");
    }
}

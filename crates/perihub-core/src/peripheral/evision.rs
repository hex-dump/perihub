//! EVision keyboards (320f and 0c45 ids), reached through their vendor collection (usage page `ff1c`).
//! The protocol is in `docs/protocols/evision.md`, from facts found in OpenRGB (GPL-2.0) and OpenEVision
//! (GPL-2.0), which tried it on a `320f:505b`. No code was copied. A Redragon K580 (`320f:5000`, issue #117)
//! has answered the reads with PeriHub 0.9.7 as documented.
//!
//! Every listed id is read. Only `320f:505b` (the one OpenEVision checked on a keyboard) and `320f:5000` (the
//! same controller and writes in OpenRGB, and a keyboard that answered the reads) may be changed, as an experimental
//! feature that stays in the keyboard's flash: the lighting header (mode, brightness, speed, colour, direction and
//! random colour), the colours per key of the Custom mode, and the entries of the key table that hold a key, a
//! modifier or a media usage. The Fn key, entries no source describes and every other offset are never written.
//! The key table names its keys by what each entry held when PeriHub first read the keyboard: the task keeps that
//! table in the device's backup folder. On Linux these keyboards are not opened: the interface also carries what is
//! typed (see `peripheral::opened_on_linux`).

use std::collections::BTreeMap;
use std::time::Duration;

use super::task::Driver;
use super::{
    Choice, Command, Effect, Io, KeyAction, KeyColorLimits, KeyLimits, Kind, Led, LedLimits, Limits, Model, Protocol,
    Reading, Settings,
};
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
const READ_COLORS: u8 = 0x10;
const WRITE_COLORS: u8 = 0x11;

/// The lighting header: mode, brightness, speed, direction, random colour, red, green, blue.
const HEADER: usize = 8;
/// Layout 2: the profile in use at offset 0, then 64 bytes per profile.
const PROFILE_FIRST: u16 = 0x01;
const PROFILE_SIZE: u16 = 0x40;
const PROFILES: u8 = 3;
/// Layout 1: the key table, 128 entries of 3 bytes.
const KEY_TABLE: u16 = 0xC0;
const KEY_ENTRIES: usize = 128;
const ENTRY: usize = 3;
/// Layout 1: the colours per key, 3 bytes per LED, LED `row * 21 + column`.
const LEDS: usize = 126;
const COLUMNS: usize = 21;
/// Colours written per request: 18 LEDs (OpenRGB's size), so that no LED is split over two requests.
const COLOR_PACKET: usize = 54;
const _: () = assert!((LEDS * 3).is_multiple_of(COLOR_PACKET) && COLOR_PACKET <= DATA_MAX);

const MAX_BRIGHTNESS: u8 = 4;
const MAX_SPEED: u8 = 5;
const MAX_DIRECTION: u8 = 3;
/// One step of the brightness, in percent.
const STEP: u8 = 25;
const SPEED: &str = "speed";
const DIRECTION: &str = "direction";
const RANDOM: &str = "random";
/// Most lines of what was sent kept for the test report.
const SENT_MAX: usize = 40;

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
const CUSTOM: u8 = 20;

/// The media usages a key may send: OpenEVision's list.
const MEDIA: [u16; 12] = [0xE9, 0xEA, 0xE2, 0xCD, 0xB5, 0xB6, 0xB7, 0x183, 0x192, 0x196, 0x18A, 0x194];

/// The key at each position of the colours per key, by HID usage, 0 where no source names one: OpenEVision's map
/// of a full-size ISO `320f:505b` and OpenRGB's map for the older controller, which agree but for three positions
/// (55, the ANSI backslash, only in OpenRGB's; 75 and 85, the ISO keys, only in OpenEVision's). The modifiers and Fn
/// are below.
const LED_USAGES: [[u8; COLUMNS]; LEDS / COLUMNS] = [
    [0x29, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0, 0x46, 0x47, 0x48, 0, 0, 0, 0],
    [
        0x35, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A, 0x49, 0x4A, 0x4B, 0x53,
        0x54, 0x55, 0x56,
    ],
    [
        0x2B, 0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x31, 0x4C, 0x4D, 0x4E, 0x5F,
        0x60, 0x61, 0x57,
    ],
    [0x39, 0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x32, 0x28, 0, 0, 0, 0x5C, 0x5D, 0x5E, 0],
    [0, 0x64, 0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0, 0, 0, 0x52, 0, 0x59, 0x5A, 0x5B, 0x58],
    [0, 0, 0, 0x2C, 0, 0, 0x65, 0, 0, 0, 0, 0, 0, 0, 0x50, 0x51, 0x4F, 0, 0x62, 0x63, 0],
];
/// The modifiers in that map, by LED and bit: left Shift, right Shift, left Ctrl, left Win, left Alt, right Alt,
/// right Ctrl.
const LED_MODIFIERS: [(usize, u8); 7] =
    [(84, 0x02), (97, 0x20), (105, 0x01), (106, 0x08), (107, 0x04), (109, 0x40), (113, 0x10)];
const LED_FN: usize = 110;
const MAP_NOTE: &str = "Key names from OpenRGB's map for these keyboards and OpenEVision's for a full-size ISO 320f:505b, not checked on every keyboard: on yours a key may sit at another position, and keys no map names (such as macro keys) are at the positions without a name.";

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
    /// The keyboard may be changed (experimental). Only where a source tried the reads and writes, or lists the id
    /// on the same controller and a keyboard answered the reads as documented.
    pub edit: bool,
    /// Starlight (slow) and Surmount may be set too: OpenEVision sets every mode on this id, OpenRGB's older
    /// controller leaves those two out.
    pub all_modes: bool,
}

const READ: Params = Params { layout: Layout::One, edit: false, all_modes: false };
const EDIT: Params = Params { layout: Layout::One, edit: true, all_modes: false };
const OPENEVISION: Params = Params { layout: Layout::One, edit: true, all_modes: true };
const PROFILED: Params = Params { layout: Layout::Two, edit: false, all_modes: false };

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
    keyboard(0x320F, &[0x505B], "RGB keyboard 320f 505b", OPENEVISION),
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

/// The write of each key table entry: command, 3 bytes, the entry's offset.
static KEY_WRITES: [[u8; 4]; KEY_ENTRIES] = {
    let mut all = [[0u8; 4]; KEY_ENTRIES];
    let mut i = 0;
    while i < KEY_ENTRIES {
        let at = KEY_TABLE + (i * ENTRY) as u16;
        all[i] = [WRITE_CONFIG, ENTRY as u8, at as u8, (at >> 8) as u8];
        i += 1;
    }
    all
};

/// The write of each packet of colours per key: command, 54 bytes, the packet's offset.
static COLOR_WRITES: [[u8; 4]; LEDS * 3 / COLOR_PACKET] = {
    let mut all = [[0u8; 4]; LEDS * 3 / COLOR_PACKET];
    let mut i = 0;
    while i < all.len() {
        let at = (i * COLOR_PACKET) as u16;
        all[i] = [WRITE_COLORS, COLOR_PACKET as u8, at as u8, (at >> 8) as u8];
        i += 1;
    }
    all
};

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

/// Reads `len` bytes with `command` (the configuration memory, or the colours per key), as many requests
/// as it takes.
fn read_memory(io: &mut Io, command: u8, offset: u16, len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let want = (len - out.len()).min(DATA_MAX);
        let at = offset + out.len() as u16;
        let answer = exchange(io, &request(command, at, want as u8, &[]))?;
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

fn read_config(io: &mut Io, offset: u16, len: usize) -> Result<Vec<u8>> {
    read_memory(io, READ_CONFIG, offset, len)
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

/// The modes PeriHub sets: OpenRGB's older controller's list, Custom where the colours per key were read, and
/// Starlight (slow) and Surmount where OpenEVision sets them. Surmount's own colour (offset 11) is never written.
fn offered(p: &Params, custom: bool) -> Vec<(u8, &'static str)> {
    MODES
        .iter()
        .copied()
        .filter(|(m, _)| match m {
            15 | 17 => p.all_modes,
            &CUSTOM => custom,
            _ => true,
        })
        .collect()
}

/// An entry of the key table, by its documented forms: `02 02` usage, `02 01` bit, `03` media usage, `05` Fn.
fn key_action(e: &[u8; ENTRY]) -> KeyAction {
    match *e {
        [0x02, 0x02, usage] => KeyAction::Key { usage },
        [0x02, 0x01, bit] => KeyAction::Modifier { bit },
        [0x03, lo, hi] => KeyAction::Media { usage: u16::from_le_bytes([lo, hi]) },
        [0x05, _, _] => KeyAction::Fn,
        _ => KeyAction::Other { bytes: e.to_vec() },
    }
}

/// The bytes of an entry that may be written; `None` for the Fn key and the entries no source describes.
fn key_entry(action: &KeyAction) -> Option<[u8; ENTRY]> {
    match *action {
        KeyAction::Key { usage } => Some([0x02, 0x02, usage]),
        KeyAction::Modifier { bit } => Some([0x02, 0x01, bit]),
        KeyAction::Media { usage } => {
            let [lo, hi] = usage.to_le_bytes();
            Some([0x03, lo, hi])
        }
        KeyAction::Fn | KeyAction::Other { .. } => None,
    }
}

/// What a key may be set to: nothing (usage 0), the keys OpenEVision offers (keyboard page 04 to 65, with the ANSI
/// backslash 31 that its ISO keyboard has not), the international keys the K580's own table holds (87 to 8b, 90,
/// 91), one modifier, or one of OpenEVision's media usages.
fn actions() -> Vec<KeyAction> {
    let keys = std::iter::once(0).chain(0x04..=0x65).chain(0x87..=0x8B).chain(0x90..=0x91);
    keys.map(|usage| KeyAction::Key { usage })
        .chain((0..8).map(|b| KeyAction::Modifier { bit: 1 << b }))
        .chain(MEDIA.iter().map(|&usage| KeyAction::Media { usage }))
        .collect()
}

/// The key at each LED of the colours per key, where a source names it.
fn led_keys() -> Vec<Option<KeyAction>> {
    (0..LEDS)
        .map(|i| {
            if i == LED_FN {
                return Some(KeyAction::Fn);
            }
            if let Some((_, bit)) = LED_MODIFIERS.iter().find(|(led, _)| *led == i) {
                return Some(KeyAction::Modifier { bit: *bit });
            }
            let usage = LED_USAGES[i / COLUMNS][i % COLUMNS];
            (usage != 0).then_some(KeyAction::Key { usage })
        })
        .collect()
}

/// The key each entry is, from the table PeriHub first read on this keyboard: an entry that held a key, a modifier
/// or a media usage then and holds one now may be changed; every other entry must be the same bytes as then. `Err`:
/// the tables do not fit (another keyboard with the same id, or one changed by other software in a way that cannot
/// be followed), with the first entry that does not.
fn factory_keys(first: &[u8], now: &[u8]) -> std::result::Result<Vec<Option<KeyAction>>, String> {
    if first.len() != now.len() {
        return Err(format!("{} bytes were kept for {}", first.len(), now.len()));
    }
    let pairs = first.as_chunks::<ENTRY>().0.iter().zip(now.as_chunks::<ENTRY>().0);
    pairs
        .enumerate()
        .map(|(i, (then, now))| {
            let action = key_action(then);
            if action.changeable() && key_action(now).changeable() {
                Ok(Some(action))
            } else if then == now {
                Ok(None)
            } else {
                Err(format!("entry {i} was {} and is now {}", super::hex(then), super::hex(now)))
            }
        })
        .collect()
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

/// Entries that differ from the table PeriHub first read, for the test report: index, then and now.
fn changed_entries(first: &[u8], now: &[u8]) -> Option<String> {
    let changed: Vec<String> = first
        .as_chunks::<ENTRY>()
        .0
        .iter()
        .zip(now.as_chunks::<ENTRY>().0)
        .enumerate()
        .filter(|(_, (then, now))| then != now)
        .map(|(i, (then, now))| format!("entry {i}: {} → {}", super::hex(then), super::hex(now)))
        .collect();
    (!changed.is_empty()).then(|| changed.join(", "))
}

/// What a change starts from: the header bytes and the settings as last read, when they looked as documented.
struct Last {
    header: [u8; HEADER],
    settings: Settings,
}

pub struct Evision {
    p: Params,
    last: Option<Last>,
    /// What the key table held when it was last read, summarized.
    keys: Option<String>,
    keys_tried: bool,
    /// The key table as PeriHub first read this keyboard (kept by the task in the backup folder): it names the keys.
    first_table: Option<Vec<u8>>,
    /// Why the colours per key are not read, once they failed: they are not asked again.
    colors_failed: Option<String>,
    /// What was sent that changes something, with the keyboard's answer, for the test report.
    sent: Vec<String>,
}

impl Evision {
    pub fn new(p: Params) -> Self {
        Self { p, last: None, keys: None, keys_tried: false, first_table: None, colors_failed: None, sent: Vec::new() }
    }

    /// Layout 1, on an id that may be changed: the key table and the colours per key are read at every reading.
    fn full(&self) -> bool {
        self.p.edit && self.p.layout == Layout::One
    }

    fn read_all(&mut self, io: &mut Io) -> Result<Reading> {
        self.last = None;
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
        let (mut settings, problems) = decode(&header);
        // Direction and random colour are changed where they read as the sources write them.
        let direction = (self.full() && header[3] <= MAX_DIRECTION).then_some(header[3]);
        let random = (self.full() && header[4] <= 1).then_some(header[4]);
        details.push(("Lighting mode".into(), mode_name(header[0])));
        match direction {
            Some(d) => _ = settings.options.insert(DIRECTION.into(), u16::from(d)),
            None => details.push(("Direction (a number)".into(), header[3].to_string())),
        }
        match random {
            Some(r) => _ = settings.options.insert(RANDOM.into(), u16::from(r)),
            None => details.push(("Random colour".into(), if header[4] == 0 { "off" } else { "on" }.into())),
        }

        let mut table = None;
        if self.p.layout == Layout::One && (self.full() || !self.keys_tried) {
            self.keys_tried = true;
            // For the test report: a keyboard that does not answer this still gets its page.
            match read_config(io, KEY_TABLE, KEY_ENTRIES * ENTRY) {
                Ok(t) => {
                    self.keys = Some(key_table_summary(&t));
                    table = Some(t);
                }
                Err(Error::Timeout | Error::Protocol(_)) => {}
                Err(e) => return Err(e),
            }
        }
        if let Some(keys) = &self.keys {
            details.push(("Key table".into(), keys.clone()));
        }

        let mut key_limits = None;
        if let (true, Some(table)) = (self.full(), &table) {
            let first = self.first_table.get_or_insert_with(|| table.clone());
            settings.keys = table.as_chunks::<ENTRY>().0.iter().map(key_action).collect();
            match factory_keys(first, table) {
                Ok(factory) => {
                    if let Some(changed) = changed_entries(first, table) {
                        details.push(("Keys changed since PeriHub first read the keyboard".into(), changed));
                    }
                    key_limits = Some(KeyLimits { factory, actions: actions() });
                }
                Err(why) => details.push((
                    "Key remapping".into(),
                    format!("not offered: the key table does not fit the one PeriHub first read on a keyboard with this id ({why})"),
                )),
            }
        }

        let mut color_limits = None;
        if self.full() {
            match &self.colors_failed {
                Some(why) => details.push(("Colours per key".into(), format!("not read: {why}"))),
                None => match read_memory(io, READ_COLORS, 0, LEDS * 3) {
                    Ok(bytes) => {
                        settings.key_colors = bytes.as_chunks::<3>().0.iter().map(|c| Rgb::from_bytes(c)).collect();
                        details.push(("Colours per key".into(), format!("{LEDS} LEDs read")));
                        color_limits =
                            Some(KeyColorLimits { columns: COLUMNS as u8, keys: led_keys(), note: MAP_NOTE });
                    }
                    Err(e @ (Error::Timeout | Error::Protocol(_))) => {
                        let why = match e {
                            Error::Timeout => "no answer".to_string(),
                            other => other.to_string(),
                        };
                        details.push(("Colours per key".into(), format!("not read: {why}")));
                        self.colors_failed = Some(why);
                    }
                    Err(e) => return Err(e),
                },
            }
        }

        reading.details = details;
        if problems.is_empty() {
            if self.p.edit {
                let mut options = vec![Choice {
                    id: SPEED,
                    label: "Speed",
                    values: [(5, "5 (slowest)"), (4, "4"), (3, "3 (normal)"), (2, "2"), (1, "1"), (0, "0 (fastest)")]
                        .map(|(v, l)| (v, l.to_string()))
                        .to_vec(),
                }];
                if direction.is_some() {
                    options.push(Choice {
                        id: DIRECTION,
                        label: "Direction",
                        values: [(0, "0 (left)"), (1, "1 (right)"), (2, "2 (up)"), (3, "3 (down)")]
                            .map(|(v, l)| (v, l.to_string()))
                            .to_vec(),
                    });
                }
                if random.is_some() {
                    options.push(Choice {
                        id: RANDOM,
                        label: "Random colour",
                        values: vec![(0, "Off".into()), (1, "On".into())],
                    });
                }
                reading.limits = Limits {
                    options,
                    leds: vec![LedLimits {
                        name: "Lighting".into(),
                        modes: offered(&self.p, color_limits.is_some()),
                        brightness_step: Some(STEP),
                        color: true,
                    }],
                    key_colors: color_limits,
                    keys: key_limits,
                    ..Default::default()
                };
            }
            self.last = Some(Last { header, settings: settings.clone() });
        } else {
            reading.problem = Some(format!("unexpected answer: {}", problems.join(", ")));
        }
        reading.settings = settings;
        Ok(reading)
    }

    /// The requests that turn `from` into `to`: the colours per key that differ (whole packets), the key
    /// table entries that differ, then the header, so that a keyboard switched to Custom shows its new colours.
    fn writes(&self, from: &Settings, to: &Settings) -> Result<Vec<[u8; REPORT]>> {
        let last =
            self.last.as_ref().ok_or_else(|| Error::Invalid("the keyboard was not read as documented".into()))?;
        if last.settings != *from {
            return Err(Error::Invalid("the keyboard's settings changed since they were read".into()));
        }
        let ([_], [new]) = (from.leds.as_slice(), to.leds.as_slice()) else {
            return Err(Error::Invalid("lighting".into()));
        };
        if !MODES.iter().any(|(m, _)| *m == new.mode) {
            return Err(Error::Invalid(format!("lighting mode {}", new.mode)));
        }
        let brightness = new
            .brightness
            .filter(|b| *b <= 100 && b.is_multiple_of(STEP))
            .ok_or_else(|| Error::Invalid("brightness".into()))?;
        let option = |id: &str, max: u8| -> Result<Option<u8>> {
            match (from.options.get(id), to.options.get(id)) {
                (None, None) => Ok(None),
                (Some(_), Some(&v)) if v <= u16::from(max) => Ok(Some(v as u8)),
                _ => Err(Error::Invalid(id.to_string())),
            }
        };
        let speed = option(SPEED, MAX_SPEED)?.ok_or_else(|| Error::Invalid("speed".into()))?;
        // Where the direction or the random colour flag are not offered, they go back as they were read.
        let mut header = last.header;
        header[0] = new.mode;
        header[1] = brightness / STEP;
        header[2] = speed;
        if let Some(direction) = option(DIRECTION, MAX_DIRECTION)? {
            header[3] = direction;
        }
        if let Some(random) = option(RANDOM, 1)? {
            header[4] = random;
        }
        header[5..8].copy_from_slice(&new.color.to_bytes());

        let mut writes = Vec::new();
        if to.key_colors != from.key_colors {
            if from.key_colors.len() != LEDS || to.key_colors.len() != LEDS {
                return Err(Error::Invalid(format!("this keyboard has {LEDS} colours per key")));
            }
            let bytes = |colors: &[Rgb]| -> Vec<u8> { colors.iter().flat_map(|c| c.to_bytes()).collect() };
            let (old, new) = (bytes(&from.key_colors), bytes(&to.key_colors));
            for (i, (new, old)) in new.chunks(COLOR_PACKET).zip(old.chunks(COLOR_PACKET)).enumerate() {
                if new != old {
                    writes.push(request(WRITE_COLORS, (i * COLOR_PACKET) as u16, 0, new));
                }
            }
        }
        if to.keys != from.keys {
            if from.keys.len() != KEY_ENTRIES || to.keys.len() != KEY_ENTRIES {
                return Err(Error::Invalid(format!("this keyboard has {KEY_ENTRIES} key table entries")));
            }
            for (i, (new, old)) in to.keys.iter().zip(&from.keys).enumerate() {
                if new == old {
                    continue;
                }
                // Only an entry that holds a documented form now, and only to another documented form.
                let (Some(_), Some(entry)) = (key_entry(old), key_entry(new)) else {
                    return Err(Error::Invalid(format!("key table entry {i} is not changed")));
                };
                writes.push(request(WRITE_CONFIG, KEY_TABLE + (i * ENTRY) as u16, 0, &entry));
            }
        }
        if header != last.header {
            writes.push(request(WRITE_CONFIG, 0, 0, &header));
        }
        Ok(writes)
    }

    fn note_sent(&mut self, req: &[u8; REPORT], answer: &Result<Answer>) {
        let what = match req[3] {
            WRITE_COLORS => "colours per key",
            WRITE_CONFIG if req[5..7] == [0, 0] => "lighting header",
            WRITE_CONFIG => "key table entry",
            BEGIN => "begin",
            END => "end",
            _ => "other",
        };
        let offset = u16::from_le_bytes([req[5], req[6]]);
        let result = match answer {
            Ok(a) => format!("status {:02x}", a.status),
            Err(e) => e.to_string(),
        };
        let size = req[4] as usize;
        let data = if size > 0 { format!(" {}", super::hex(&req[8..8 + size.min(9)])) } else { String::new() };
        let more = if size > 9 { " …" } else { "" };
        self.sent.push(format!("{what} at {offset:#06x}, {size} bytes{data}{more} → {result}"));
        if self.sent.len() > SENT_MAX {
            self.sent.remove(0);
        }
    }

    /// Sends one request of a change, kept for the test report; a status other than 0 is an error.
    fn send(&mut self, io: &mut Io, req: &[u8; REPORT]) -> Result<()> {
        let answer = exchange(io, req);
        self.note_sent(req, &answer);
        match answer?.status {
            0 => Ok(()),
            s => Err(Error::Protocol(format!(
                "the keyboard refused a change at {:#06x} (status {s:02x})",
                u16::from_le_bytes([req[5], req[6]])
            ))),
        }
    }
}

impl Driver for Evision {
    fn commands(&self) -> Vec<Command> {
        let mut commands = vec![command(&[READ_CONFIG], Effect::Read, "read the configuration")];
        if self.p.layout == Layout::Two {
            commands.push(command(&[CAPABILITIES], Effect::Read, "read the capabilities"));
        }
        if self.p.edit {
            commands.push(command(&[READ_COLORS], Effect::Read, "read the colours per key"));
            commands.push(command(&[BEGIN], Effect::Change, "begin a configuration change"));
            commands.push(command(&[END], Effect::Change, "end a configuration change"));
            // Only the 8 bytes of the lighting header at offset 0, one key table entry of 3 bytes at its own
            // offset, and the colours per key in packets of 54 bytes at theirs: nothing else is ever written.
            commands.push(command(&[WRITE_CONFIG, HEADER as u8, 0, 0], Effect::Save, "write the lighting"));
            commands.extend(KEY_WRITES.iter().map(|p| command(p, Effect::Save, "write a key")));
            commands.extend(COLOR_WRITES.iter().map(|p| command(p, Effect::Save, "write the colours per key")));
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

    fn sent(&self) -> Vec<String> {
        self.sent.clone()
    }

    fn first_reading(&self) -> Option<Vec<u8>> {
        if self.full() { self.first_table.clone() } else { None }
    }

    fn recall_first_reading(&mut self, bytes: &[u8]) {
        if bytes.len() == KEY_ENTRIES * ENTRY {
            self.first_table = Some(bytes.to_vec());
        }
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
        let writes = self.writes(from, to)?;
        if writes.is_empty() {
            return Ok(());
        }
        self.send(io, &request(BEGIN, 0, 0, &[]))?;
        let mut written = Ok(());
        for req in &writes {
            written = self.send(io, req);
            if written.is_err() {
                break;
            }
        }
        // The end is sent also after a refused write.
        let ended = self.send(io, &request(END, 0, 0, &[]));
        written.and(ended)
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
        /// The colours per key (command 10 and 11), 0x200 bytes as OpenEVision reads for a backup.
        colors: Vec<u8>,
        capabilities: Vec<u8>,
        sent: Vec<Vec<u8>>,
        writing: bool,
        /// The status it answers with to a write, and to a read.
        write_status: u8,
        read_status: u8,
        /// Sends an answer to another request first, as it does to every program that has it open.
        noisy: bool,
        /// Does not answer the colours per key.
        no_colors: bool,
        /// Answers the colours per key written with success, but keeps the old ones.
        drops_colors: bool,
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
        // A colour per LED from its index, and bytes after the 126 LEDs that must never change.
        let mut colors: Vec<u8> = (0..LEDS * 3).map(|i| (i % 251) as u8).collect();
        colors.resize(0x200, 0xEE);
        Arc::new(Mutex::new(Keyboard {
            memory,
            colors,
            capabilities: vec![0xaa, 0x55, 0, 0, 0, 0, 0],
            sent: Vec::new(),
            writing: false,
            write_status: 0,
            read_status: 0,
            noisy: false,
            no_colors: false,
            drops_colors: false,
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
            READ_COLORS if k.no_colors => return Vec::new(),
            READ_COLORS => {
                a[8..8 + size].copy_from_slice(&k.colors[offset..offset + size]);
            }
            WRITE_COLORS => {
                assert!(k.writing, "a write outside begin and end");
                if k.write_status == 0 && !k.drops_colors {
                    k.colors[offset..offset + size].copy_from_slice(&r[8..8 + size]);
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

    /// The commands sent from `from` on that change something, with their offset and size.
    fn changes_sent(state: &Shared, from: usize) -> Vec<(u8, u16, u8)> {
        state.lock().unwrap().sent[from..]
            .iter()
            .filter(|r| ![READ_CONFIG, READ_COLORS].contains(&r[3]))
            .map(|r| (r[3], u16::from_le_bytes([r[5], r[6]]), r[4]))
            .collect()
    }

    fn sent_count(state: &Shared) -> usize {
        state.lock().unwrap().sent.len()
    }

    /// The 128 entries of the key table as the fake holds them.
    fn table(state: &Shared) -> Vec<u8> {
        state.lock().unwrap().memory[KEY_TABLE as usize..KEY_TABLE as usize + KEY_ENTRIES * ENTRY].to_vec()
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
        // One key table entry, entry 2 (offset c6): command, 3 bytes, offset, status 0, the entry.
        let key = request(WRITE_CONFIG, KEY_TABLE + 6, 0, &[2, 2, 0x29]);
        assert_eq!(key[3..11], [0x06, 3, 0xC6, 0x00, 0, 2, 2, 0x29]);
        // The last packet of colours per key: 54 bytes at 0x144.
        let colors = request(WRITE_COLORS, 6 * 54, 0, &[1; 54]);
        assert_eq!(colors[3..7], [0x11, 54, 0x44, 0x01]);
        assert!(colors[8..62].iter().all(|&b| b == 1) && colors[62..].iter().all(|&b| b == 0));
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
        assert!(info.details.contains(&("Colours per key".into(), "126 LEDs read".into())));
        // Direction and random colour are settings here, not lines of text.
        assert_eq!((info.settings.options[DIRECTION], info.settings.options[RANDOM]), (0, 0));
        assert!(!info.details.iter().any(|(l, _)| l == "Random colour" || l.starts_with("Direction")));
        assert_eq!(info.settings.key_colors.len(), LEDS);
        assert_eq!(info.settings.key_colors[1], Rgb { r: 3, g: 4, b: 5 });
        assert_eq!(info.settings.keys.len(), KEY_ENTRIES);
        assert_eq!(info.settings.keys[0], KeyAction::Key { usage: 0x04 });
        assert_eq!(info.settings.keys[127], KeyAction::Fn);
        assert!(
            info.details.contains(&(
                "Key table".into(),
                "128 entries: 100 keys, 7 disabled, 10 modifiers, 10 media, 1 Fn".into()
            )),
            "{:?}",
            info.details
        );
        // Every mode on the id OpenEVision sets them all on, Custom included once the colours were read.
        assert_eq!(info.limits.leds[0].modes, MODES);
        assert_eq!(info.limits.leds[0].brightness_step, Some(25));
        let colors = info.limits.key_colors.as_ref().unwrap();
        assert_eq!((colors.columns, colors.keys.len()), (21, LEDS));
        assert_eq!(colors.keys[0], Some(KeyAction::Key { usage: 0x29 }), "Esc");
        assert_eq!(colors.keys[110], Some(KeyAction::Fn));
        assert_eq!(colors.keys[105], Some(KeyAction::Modifier { bit: 0x01 }), "left Ctrl");
        assert_eq!(colors.keys[13], None);
        let keys = info.limits.keys.as_ref().unwrap();
        assert_eq!(keys.factory[0], Some(KeyAction::Key { usage: 0x04 }));
        assert_eq!(keys.factory[127], None, "the Fn key");
        // Only reads, each on the list: the header, the key table in 56-byte reads, the colours per key.
        let commands = Evision::new(OPENEVISION).commands();
        for req in state.lock().unwrap().sent.iter() {
            assert!(commands.iter().any(|c| req[c.at..].starts_with(c.prefix)), "{req:02x?}");
        }
        assert_eq!(commands_sent(&state), [[READ_CONFIG; 8].as_slice(), &[READ_COLORS; 7]].concat());
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
        assert_eq!(sent[0], READ_CONFIG);
        assert_eq!(changes_sent(&state, from), [(BEGIN, 0, 0), (WRITE_CONFIG, 0, 8), (END, 0, 0)]);
        assert_eq!(*sent.last().unwrap(), READ_COLORS, "read back");
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
        let sent = changes_sent(&state, from);
        assert_eq!(sent, [(BEGIN, 0, 0), (WRITE_CONFIG, 0, 8), (END, 0, 0)], "{sent:?}");
        assert!(!state.lock().unwrap().writing, "ended");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn modes_that_are_not_offered_are_shown_and_can_be_put_back() {
        // A 5000 whose colours per key do not answer: Custom is not offered, and neither are the two modes
        // OpenRGB's older controller leaves out.
        let state = keyboard_505b();
        state.lock().unwrap().memory[0] = 20;
        state.lock().unwrap().no_colors = true;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5000), 0x5000, true, temp("custom")).unwrap();
        assert_eq!(task.info.problem, None);
        assert!(task.info.details.contains(&("Lighting mode".into(), "Custom (per key) (20)".into())));
        assert!(task.info.details.contains(&("Colours per key".into(), "not read: no answer".into())));
        assert!(!task.info.limits.leds[0].modes.iter().any(|(m, _)| [15, 17, 20].contains(m)));
        assert!(task.info.settings.key_colors.is_empty() && task.info.limits.key_colors.is_none());
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
        wanted.key_colors[0] = Rgb::WHITE;
        wanted.keys[0] = KeyAction::Key { usage: 0x29 };
        assert!(task.change(&wanted, false, None).is_err());
        assert!(commands_sent(&state).iter().all(|&c| c == READ_CONFIG || c == READ_COLORS));
        // Nothing written to PeriHub's folder either.
        assert!(!task.backups().exists());
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
        // The live colours, the V2 colour commands, anything unknown.
        for command in [0x12, 0x13, 0x0A, 0x0B, 0x03, 0x04, 0x07, 0xF0, 0xFF] {
            refused(&mut io, request(command, 0, 8, &[]));
        }
        // Writes of the configuration other than the header and one key table entry: the table's copies, an entry
        // past the table, two entries at once, an entry not on its own offset, offset 0f (polling rate in OpenRGB),
        // 11 (Surmount's colour), 700, a header of another size or at another offset.
        for (offset, data) in [
            (0x2C0, vec![2, 2, 4]),
            (0x4C0, vec![2, 2, 4]),
            (KEY_TABLE + (KEY_ENTRIES * ENTRY) as u16, vec![2, 2, 4]),
            (KEY_TABLE, vec![2, 2, 4, 2, 2, 5]),
            (KEY_TABLE + 1, vec![2, 2, 4]),
            (0x0F, vec![3]),
            (0x11, vec![1]),
            (0x700, vec![0]),
            (0, vec![1, 2, 3, 4]),
            (8, vec![0; 8]),
        ] {
            refused(&mut io, request(WRITE_CONFIG, offset, 0, &data));
        }
        // Colours per key in another packet size, or past the 126 LEDs.
        refused(&mut io, request(WRITE_COLORS, 0, 0, &[0; 56]));
        refused(&mut io, request(WRITE_COLORS, 3, 0, &[0; 54]));
        refused(&mut io, request(WRITE_COLORS, (LEDS * 3) as u16, 0, &[0; 54]));
        refused(&mut io, request(WRITE_COLORS, 0, 0, &[0; 3]));
        assert!(state.lock().unwrap().sent.is_empty());
        // The listed writes: the header, every entry of the table, every packet of colours.
        let commands = Evision::new(EDIT).commands();
        let listed = |r: &[u8; REPORT]| commands.iter().any(|c| r[c.at..].starts_with(c.prefix));
        assert!(listed(&request(WRITE_CONFIG, 0, 0, &[0; 8])));
        assert!((0..KEY_ENTRIES).all(|i| listed(&request(WRITE_CONFIG, KEY_TABLE + (i * 3) as u16, 0, &[2, 2, 4]))));
        assert!((0..7).all(|i| listed(&request(WRITE_COLORS, i * 54, 0, &[0; 54]))));
        // A connection that may not change anything refuses even the listed writes.
        let mut io = Io::new(Box::new(fake(state.clone())), Evision::new(EDIT).commands(), false);
        refused(&mut io, request(WRITE_CONFIG, 0, 0, &[6, 4, 3, 0, 0, 255, 0, 0]));
        refused(&mut io, request(WRITE_CONFIG, KEY_TABLE, 0, &[2, 2, 4]));
        refused(&mut io, request(WRITE_COLORS, 0, 0, &[0; 54]));
        refused(&mut io, request(BEGIN, 0, 0, &[]));
        assert!(state.lock().unwrap().sent.is_empty());
        // The read-only models have no write on their list at all, and do not read the colours per key.
        for params in [READ, PROFILED] {
            let commands = Evision::new(params).commands();
            assert!(commands.iter().all(|c| c.effect == Effect::Read && c.prefix != [READ_COLORS]), "{params:?}");
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
        // Every mode only where OpenEVision sets them all.
        let all: Vec<_> = MODELS.iter().filter(|m| matches!(m.protocol, Protocol::Evision(p) if p.all_modes)).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].product_ids, [0x505B]);
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
    /// A key table as a K580 has it: keys, modifiers, an Fn key, media, and entries no source describes.
    fn keyboard_with_unknown_entries() -> Shared {
        let state = keyboard_505b();
        {
            let mut k = state.lock().unwrap();
            let at = |i: usize| KEY_TABLE as usize + i * ENTRY;
            for i in (7..KEY_ENTRIES).step_by(8) {
                k.memory[at(i)..at(i) + 3].copy_from_slice(&[0x02, 0x00, 0x00]);
            }
            k.memory[at(54)..at(54) + 3].copy_from_slice(&[0xD0, 0xA2, 0x01]);
            k.memory[at(45)..at(45) + 3].copy_from_slice(&[0x05, 0x02, 0x02]);
        }
        state
    }

    #[test]
    fn colours_per_key_are_written_in_whole_packets_between_begin_and_end_and_undone_unless_kept() {
        let state = keyboard_505b();
        let before = state.lock().unwrap().colors.clone();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("colors")).unwrap();
        let from = sent_count(&state);
        let mut wanted = task.info.settings.clone();
        wanted.key_colors[0] = Rgb { r: 1, g: 2, b: 3 };
        wanted.key_colors[LEDS - 1] = Rgb { r: 9, g: 8, b: 7 };
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(info.settings, wanted, "read back");
        // Only the first and the last packet, 54 bytes each, inside begin and end; no header.
        assert_eq!(
            changes_sent(&state, from),
            [(BEGIN, 0, 0), (WRITE_COLORS, 0, 54), (WRITE_COLORS, 6 * 54, 54), (END, 0, 0)]
        );
        let colors = state.lock().unwrap().colors.clone();
        assert_eq!(colors[..3], [1, 2, 3]);
        assert_eq!(colors[LEDS * 3 - 3..LEDS * 3], [9, 8, 7]);
        assert_eq!(colors[LEDS * 3..], before[LEDS * 3..], "nothing after the 126 LEDs");
        assert_eq!(colors[3..LEDS * 3 - 3], before[3..LEDS * 3 - 3]);
        assert!(
            info.sent.iter().any(|l| l.starts_with("colours per key at 0x0000, 54 bytes 01 02 03")),
            "{:?}",
            info.sent
        );
        // Not kept: put back.
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().colors, before);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn every_led_set_and_custom_chosen_writes_the_colours_before_the_mode() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5000), 0x5000, true, temp("all")).unwrap();
        assert!(task.info.limits.leds[0].modes.iter().any(|(m, _)| *m == CUSTOM), "Custom once the colours were read");
        assert!(!task.info.limits.leds[0].modes.iter().any(|(m, _)| [15, 17].contains(m)), "not on 5000");
        let from = sent_count(&state);
        let mut wanted = task.info.settings.clone();
        wanted.key_colors.fill(Rgb { r: 0, g: 0, b: 255 });
        wanted.leds[0].mode = CUSTOM;
        task.change(&wanted, false, None).unwrap();
        task.keep_changes();
        let sent = changes_sent(&state, from);
        let expected: Vec<(u8, u16, u8)> = std::iter::once((BEGIN, 0, 0))
            .chain((0..7).map(|i| (WRITE_COLORS, i * 54, 54)))
            .chain([(WRITE_CONFIG, 0, 8), (END, 0, 0)])
            .collect();
        assert_eq!(sent, expected);
        assert_eq!(header(&state, HEADER_AT_ONE)[0], CUSTOM);
        assert!(state.lock().unwrap().colors[..LEDS * 3].chunks(3).all(|c| c == [0, 0, 255]));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn colours_that_do_not_read_back_are_put_back() {
        let state = keyboard_505b();
        state.lock().unwrap().drops_colors = true;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("dropped"))
                .unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.key_colors[5] = Rgb::WHITE;
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("instead of what was sent") && error.contains("put back"), "{error}");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_key_is_named_by_what_it_held_first_changed_alone_and_put_back() {
        let state = keyboard_505b();
        let backups = temp("remap");
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, backups.clone())
                .unwrap();
        let before = table(&state);
        // The table as first read is kept in the backup folder before anything is changed.
        assert_eq!(std::fs::read(backups.join("first-reading.bin")).unwrap(), before);
        let from = sent_count(&state);
        let mut wanted = task.info.settings.clone();
        wanted.keys[2] = KeyAction::Key { usage: 0x29 };
        wanted.keys[110] = KeyAction::Modifier { bit: 0x10 };
        wanted.keys[118] = KeyAction::Media { usage: 0x0183 };
        let info = task.change(&wanted, false, None).unwrap();
        assert_eq!(info.settings, wanted);
        assert_eq!(
            changes_sent(&state, from),
            [
                (BEGIN, 0, 0),
                (WRITE_CONFIG, KEY_TABLE + 6, 3),
                (WRITE_CONFIG, KEY_TABLE + 330, 3),
                (WRITE_CONFIG, KEY_TABLE + 354, 3),
                (END, 0, 0)
            ]
        );
        let now = table(&state);
        assert_eq!(now[6..9], [0x02, 0x02, 0x29]);
        assert_eq!(now[330..333], [0x02, 0x01, 0x10]);
        assert_eq!(now[354..357], [0x03, 0x83, 0x01]);
        let changed: Vec<usize> =
            (0..KEY_ENTRIES).filter(|&i| now[i * 3..i * 3 + 3] != before[i * 3..i * 3 + 3]).collect();
        assert_eq!(changed, [2, 110, 118], "nothing else in the table");
        task.keep_changes();
        drop(task);
        // Opened again: the entry is still named by what it held when PeriHub first read the keyboard.
        let task = start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, backups.clone())
            .unwrap();
        let keys = task.info.limits.keys.as_ref().unwrap();
        assert_eq!(keys.factory[2], Some(KeyAction::Key { usage: 0x06 }));
        assert_eq!(task.info.settings.keys[2], KeyAction::Key { usage: 0x29 });
        let changed = task.info.details.iter().find(|(l, _)| l.starts_with("Keys changed")).unwrap();
        assert!(changed.1.starts_with("entry 2: 02 02 06 → 02 02 29"), "{changed:?}");
        let _ = std::fs::remove_dir_all(backups);
    }

    #[test]
    fn fn_and_entries_no_source_describes_are_never_changed() {
        let state = keyboard_with_unknown_entries();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x5000), 0x5000, true, temp("fixed")).unwrap();
        let keys = task.info.limits.keys.clone().unwrap();
        for i in [7, 15, 45, 54, 127] {
            assert_eq!(keys.factory[i], None, "entry {i}");
        }
        assert_eq!(task.info.settings.keys[54], KeyAction::Other { bytes: vec![0xD0, 0xA2, 0x01] });
        assert_eq!(task.info.settings.keys[45], KeyAction::Fn);
        let before = table(&state);
        let from = sent_count(&state);
        for (i, action) in [
            (45, KeyAction::Key { usage: 0x04 }),
            (54, KeyAction::Key { usage: 0x04 }),
            (7, KeyAction::Key { usage: 0x04 }),
            (0, KeyAction::Fn),
            (0, KeyAction::Other { bytes: vec![0x02, 0x02, 0x04] }),
        ] {
            let mut wanted = task.info.settings.clone();
            wanted.keys[i] = action.clone();
            assert!(task.change(&wanted, false, None).is_err(), "{i}: {action:?}");
        }
        assert!(changes_sent(&state, from).is_empty());
        assert_eq!(table(&state), before);
        // Below the page's checks too: the driver writes no entry that is not a key, a modifier or a media usage.
        let mut io = Io::new(Box::new(fake(state.clone())), Evision::new(EDIT).commands(), true);
        let mut driver = Evision::new(EDIT);
        let now = driver.read(&mut io).unwrap().settings;
        for i in [45, 54, 7] {
            let mut to = now.clone();
            to.keys[i] = KeyAction::Key { usage: 0x04 };
            assert!(driver.apply(&mut io, &now, &to).is_err(), "{i}");
        }
        assert_eq!(table(&state), before);
        assert!(!commands_sent(&state).iter().any(|&c| c == WRITE_CONFIG || c == BEGIN));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn actions_that_are_not_offered_are_refused() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("offer")).unwrap();
        let offered = &task.info.limits.keys.as_ref().unwrap().actions;
        assert!(offered.contains(&KeyAction::Key { usage: 0 }), "disabled");
        assert!(offered.contains(&KeyAction::Key { usage: 0x31 }) && offered.contains(&KeyAction::Key { usage: 0x8B }));
        assert!(offered.iter().all(|a| a.changeable()));
        let from = sent_count(&state);
        for action in [
            KeyAction::Modifier { bit: 0x03 },
            KeyAction::Key { usage: 0xE0 },
            KeyAction::Key { usage: 0x66 },
            KeyAction::Media { usage: 0x0223 },
        ] {
            let mut wanted = task.info.settings.clone();
            wanted.keys[0] = action.clone();
            assert!(task.change(&wanted, false, None).is_err(), "{action:?}");
        }
        assert!(changes_sent(&state, from).is_empty());
    }

    #[test]
    fn a_refused_key_is_put_back_and_the_end_is_still_sent() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("refusedkey"))
                .unwrap();
        let before = table(&state);
        state.lock().unwrap().write_status = 1;
        let from = sent_count(&state);
        let mut wanted = task.info.settings.clone();
        wanted.keys[0] = KeyAction::Key { usage: 0x29 };
        wanted.keys[1] = KeyAction::Key { usage: 0x2A };
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("refused"), "{error}");
        // The first refused write stops the change, and the end follows it.
        assert_eq!(changes_sent(&state, from), [(BEGIN, 0, 0), (WRITE_CONFIG, KEY_TABLE, 3), (END, 0, 0)]);
        assert_eq!(table(&state), before);
        assert!(!state.lock().unwrap().writing);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_key_table_that_does_not_fit_the_first_one_is_not_remapped() {
        let state = keyboard_505b();
        let backups = temp("otherfirst");
        // What a keyboard with another table left there: its Fn key elsewhere.
        let mut first = table(&state);
        first[..3].copy_from_slice(&[0x05, 0x00, 0x00]);
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(backups.join("first-reading.bin"), &first).unwrap();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, backups.clone())
                .unwrap();
        assert_eq!(task.info.problem, None, "the rest stays changeable");
        assert!(task.info.limits.keys.is_none());
        let why = task.info.details.iter().find(|(l, _)| l == "Key remapping").unwrap();
        assert!(why.1.contains("entry 0 was 05 00 00 and is now 02 02 04"), "{why:?}");
        let mut wanted = task.info.settings.clone();
        wanted.keys[1] = KeyAction::Key { usage: 0x29 };
        assert!(task.change(&wanted, false, None).is_err());
        // The kept table is left as it was.
        assert_eq!(std::fs::read(backups.join("first-reading.bin")).unwrap(), first);
        let _ = std::fs::remove_dir_all(backups);
    }

    #[test]
    fn colours_that_do_not_answer_are_not_asked_again() {
        let state = keyboard_505b();
        state.lock().unwrap().no_colors = true;
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("nocolors"))
                .unwrap();
        task.info().unwrap();
        assert_eq!(commands_sent(&state).iter().filter(|&&c| c == READ_COLORS).count(), 1);
        assert!(task.info.details.contains(&("Colours per key".into(), "not read: no answer".into())));
        // The keys and the lighting are still changeable.
        assert!(task.info.limits.keys.is_some() && !task.info.limits.leds.is_empty());
        assert!(!task.info.limits.leds[0].modes.iter().any(|(m, _)| *m == CUSTOM));
    }

    #[test]
    fn direction_and_random_colour_are_written_in_the_header() {
        let state = keyboard_505b();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), model(0x320F, 0x505B), 0x505B, true, temp("direction"))
                .unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(DIRECTION.into(), 3);
        wanted.options.insert(RANDOM.into(), 1);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(header(&state, HEADER_AT_ONE), [6, 4, 3, 3, 1, 255, 0, 0]);
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(DIRECTION.into(), 4);
        assert!(task.change(&wanted, false, None).is_err(), "no direction 4");
        // A direction out of the sources' range is shown, kept as read, and not offered.
        task.keep_changes();
        state.lock().unwrap().memory[3] = 7;
        task.info().unwrap();
        assert!(!task.info.settings.options.contains_key(DIRECTION));
        assert!(task.info.details.contains(&("Direction (a number)".into(), "7".into())));
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 5;
        task.change(&wanted, false, None).unwrap();
        assert_eq!(header(&state, HEADER_AT_ONE)[..4], [5, 4, 3, 7]);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_led_map_names_each_key_once() {
        let keys = led_keys();
        assert_eq!(keys.len(), LEDS);
        let named: Vec<&KeyAction> = keys.iter().flatten().collect();
        let mut sorted: Vec<String> = named.iter().map(|k| format!("{k:?}")).collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), named.len(), "no key at two positions");
        // A full-size board: 104 ANSI keys plus the two ISO ones.
        assert_eq!(named.len(), 106);
        // Rows as OpenRGB's and OpenEVision's maps have them: Esc at 0, Tab at 42, Caps Lock at 63, left Shift
        // at 84, left Ctrl at 105, keypad 0 at 123.
        assert_eq!(keys[42], Some(KeyAction::Key { usage: 0x2B }));
        assert_eq!(keys[63], Some(KeyAction::Key { usage: 0x39 }));
        assert_eq!(keys[84], Some(KeyAction::Modifier { bit: 0x02 }));
        assert_eq!(keys[123], Some(KeyAction::Key { usage: 0x62 }));
    }

    #[test]
    fn entries_go_both_ways() {
        for e in [[0x02, 0x02, 0x04], [0x02, 0x02, 0x00], [0x02, 0x01, 0x40], [0x03, 0x83, 0x01]] {
            assert_eq!(key_entry(&key_action(&e)), Some(e), "{e:02x?}");
        }
        for e in [[0x05, 0x00, 0x00], [0x05, 0x1B, 0x00], [0x02, 0x00, 0x00], [0xD0, 0xA2, 0x01]] {
            assert_eq!(key_entry(&key_action(&e)), None, "{e:02x?}");
            assert!(!key_action(&e).changeable());
        }
    }

    /// The K580 of issue #117, answering from its transcript: everything it was asked then, and nothing for the
    /// colours per key, which PeriHub 0.9.7 did not read (the replay test leaves this transcript out until a report
    /// from a version that reads them).
    #[test]
    fn the_k580_reads_its_keys_from_its_own_answers() {
        use crate::replay::{Classifier, MAIN, Replay, ReplayHid, Transcript};
        struct NoColors(ReplayHid);
        impl super::super::Hid for NoColors {
            fn write(&self, data: &[u8]) -> Result<usize> {
                if data.get(3) == Some(&READ_COLORS) { Ok(data.len()) } else { super::super::Hid::write(&self.0, data) }
            }
            fn read_timeout(&self, buf: &mut [u8], ms: i32) -> Result<usize> {
                super::super::Hid::read_timeout(&self.0, buf, ms)
            }
            fn send_feature(&self, data: &[u8]) -> Result<()> {
                self.0.send_feature(data)
            }
            fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
                self.0.get_feature(buf)
            }
            fn report_descriptor(&self, buf: &mut [u8]) -> Result<usize> {
                self.0.report_descriptor(buf)
            }
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/transcripts/evision-k580.json");
        let t = Transcript::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        let m = model(0x320F, 0x5000);
        let replay = Replay::new(&t, Classifier::Commands(Evision::new(EDIT).commands())).unwrap();
        let task = start_for_test(Box::new(NoColors(replay.collection(MAIN))), m, 0x5000, true, temp("k580")).unwrap();
        let info = &task.info;
        assert_eq!(info.problem, None);
        assert!(info.editable);
        assert!(info.details.contains(&(
            "Key table".into(),
            "128 entries: 94 keys, 7 modifiers, 3 media, 3 Fn, 21 not described: 16× 02 00 00, d0 a2 01, d0 a2 02, d0 a2 03, d0 a2 04, d0 a2 05".into()
        )));
        assert!(info.details.contains(&("Colours per key".into(), "not read: no answer".into())));
        assert_eq!(info.settings.leds[0].mode, CUSTOM);
        assert_eq!(info.settings.keys.len(), KEY_ENTRIES);
        let keys = info.limits.keys.as_ref().unwrap();
        // Esc, left Shift, the media keys: named and changeable. The Fn entries, the entries ending each group of
        // eight and the five numbered ones: never.
        assert_eq!(keys.factory[0], Some(KeyAction::Key { usage: 0x29 }));
        assert_eq!(keys.factory[4], Some(KeyAction::Modifier { bit: 0x02 }));
        assert_eq!(keys.factory[118], Some(KeyAction::Media { usage: 0xCD }));
        let fixed: Vec<usize> = (0..KEY_ENTRIES).filter(|&i| keys.factory[i].is_none()).collect();
        let expected: Vec<usize> = (7..KEY_ENTRIES)
            .step_by(8)
            .chain([45, 54, 62, 70, 78, 86, 94, 102])
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(fixed, expected);
        assert!(info.settings.key_colors.is_empty() && info.limits.key_colors.is_none());
        assert!(!replay.sent().is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }
}

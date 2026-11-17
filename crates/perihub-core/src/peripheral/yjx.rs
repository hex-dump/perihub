//! Mice on the YJX platform: a 2.4 GHz receiver `a8a5:2255` (`a8a4:2255` on the cable) that many
//! brands use (MAMBASNAKE, Rajin, K-Snake, EWEADN, Noir…). The protocol is in
//! `docs/protocols/yjx-mice.md`, from claws-mouse-linux (MIT), EWEADN-GS02-Pro-Control (MIT) and the
//! facts of OpenMouse's K-Snake driver (AGPL-3.0, facts only). No code was copied. Nobody has tried
//! it with PeriHub yet.
//!
//! The same product id stands for different mice, and the sources wrap the packets in two ways (a
//! `55` byte in front and no report number, or report number `f0`). So PeriHub first reads the
//! collection's report descriptor, which asks the system and sends nothing to the mouse, and sends
//! its reads only where the descriptor has no report numbers: the format two sources tried on real
//! mice. Anything else is left alone. For now it only reads.

use std::time::Duration;

use super::task::Driver;
use super::{Command, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

const REPORT: usize = 64;
/// First byte of a request, and of an answer.
const REQUEST: u8 = 0x55;
const ANSWER: u8 = 0xAA;

const GET_VERSION: u8 = 0x03;
const GET_CONFIG: u8 = 0x0E;
const GET_BATTERY: u8 = 0x30;
/// What follows the battery command, as K-Snake's vendor panel sends it.
const BATTERY_TAIL: [u8; 8] = [0xA5, 0x0B, 0x2E, 0x01, 0x01, 0x00, 0x00, 0x00];

const TIMEOUT: Duration = Duration::from_millis(1000);
const POLLING: [u16; 4] = [125, 250, 500, 1000];
const DPI_MIN: u16 = 50;
const DPI_MAX: u16 = 26000;

/// Everything PeriHub may send: three reads. Writing the settings (`55 0f`) is left out until a
/// tester's mouse confirms the format; a malformed one can stop the sensor until the mouse is turned
/// off.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &[REQUEST, GET_VERSION, 0x00], at: 0, effect: Effect::Read, name: "read version" },
    Command { prefix: &[REQUEST, GET_CONFIG, 0x00], at: 0, effect: Effect::Read, name: "read settings" },
    Command { prefix: &[REQUEST, GET_BATTERY, 0xA5, 0x0B, 0x2E], at: 0, effect: Effect::Read, name: "read battery" },
];

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Through the receiver: the mouse may be asleep.
    pub receiver: bool,
}

pub const MODELS: &[Model] = &[
    Model {
        vendor_id: 0xA8A5,
        product_ids: &[0x2255],
        brand: "YJX",
        name: "mouse (YJX receiver)",
        kind: Kind::Mouse,
        interface: Some(2),
        usage_page: Some(0xFF01),
        protocol: Protocol::Yjx(Params { receiver: true }),
    },
    Model {
        vendor_id: 0xA8A4,
        product_ids: &[0x2255],
        brand: "YJX",
        name: "mouse (YJX, on its cable)",
        kind: Kind::Mouse,
        interface: Some(2),
        usage_page: Some(0xFF01),
        protocol: Protocol::Yjx(Params { receiver: false }),
    },
];

/// How the collection's reports are numbered, from its report descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Framing {
    /// No report numbers: `55` in front of each request, the answer starts with `aa`.
    Plain,
    /// Report numbers, listed: a format PeriHub does not send yet.
    Numbered(Vec<u8>),
}

/// The report numbers a descriptor declares (its Report ID items), or `None` if it cannot be read.
pub fn framing(descriptor: &[u8]) -> Option<Framing> {
    let mut ids = Vec::new();
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xFE {
            return None;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let bytes = descriptor.get(i + 1..i + 1 + size)?;
        if prefix & 0xFC == 0x84 {
            let id = *bytes.first()?;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        i += 1 + size;
    }
    if descriptor.is_empty() {
        return None;
    }
    Some(if ids.is_empty() { Framing::Plain } else { Framing::Numbered(ids) })
}

/// A 64-byte request: `55`, the command, then `tail`.
fn request(command: u8, tail: &[u8]) -> [u8; REPORT] {
    let mut r = [0u8; REPORT];
    r[0] = REQUEST;
    r[1] = command;
    r[2..2 + tail.len()].copy_from_slice(tail);
    r
}

/// The text the version answer holds from byte 8, up to the first zero, if it is printable.
fn version_text(answer: &[u8]) -> Option<String> {
    let text: Vec<u8> = answer[8..].iter().copied().take_while(|&b| b != 0).collect();
    (!text.is_empty() && text.iter().all(|b| (0x20..0x7F).contains(b))).then(|| String::from_utf8(text).unwrap())
}

/// The settings block, as the sources read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub light_mode: u8,
    pub polling_hz: Option<u16>,
    pub levels: Vec<u16>,
    /// From 1.
    pub active: u8,
    pub lift_off: u8,
}

/// Reads the settings answer; the problems found are returned with it.
pub fn decode_config(a: &[u8; REPORT]) -> (Config, Vec<String>) {
    let mut problems = Vec::new();
    let polling_hz = a[10].checked_sub(1).and_then(|i| POLLING.get(i as usize)).copied();
    if polling_hz.is_none() {
        problems.push(format!("polling rate code {:02x}", a[10]));
    }
    let count = a[11];
    if !(1..=6).contains(&count) {
        problems.push(format!("{count} DPI levels"));
    }
    let levels: Vec<u16> =
        (0..count.min(6) as usize).map(|i| u16::from_le_bytes([a[13 + 2 * i], a[14 + 2 * i]])).collect();
    if let Some(bad) = levels.iter().find(|&&d| !(DPI_MIN..=DPI_MAX).contains(&d)) {
        problems.push(format!("DPI {bad}"));
    }
    if a[12] == 0 || a[12] > count {
        problems.push(format!("active DPI level {}", a[12]));
    }
    (Config { light_mode: a[9], polling_hz, levels, active: a[12], lift_off: a[49] }, problems)
}

pub struct Yjx {
    p: Params,
}

impl Yjx {
    pub fn new(p: Params) -> Self {
        Self { p }
    }

    fn query(&self, io: &mut Io, req: &[u8; REPORT]) -> Result<[u8; REPORT]> {
        let mut reply = [0u8; REPORT];
        let command = req[1];
        io.output_exchange(req, &mut reply, TIMEOUT, |r| r[0] == ANSWER && r[1] == command)?;
        Ok(reply)
    }

    /// A mouse that does not answer still gets its page and test report, with the descriptor: the
    /// most likely way an untried format fails. Read again every few seconds, so it fills in once the
    /// mouse answers.
    fn no_answer(&self) -> Reading {
        let hint =
            if self.p.receiver { "if the mouse is on and near its receiver" } else { "if the mouse is plugged in" };
        Reading {
            details: vec![("Status".into(), format!("no answer yet: {hint}, please send a test report"))],
            ..Default::default()
        }
    }
}

impl Driver for Yjx {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "claws-mouse-linux, EWEADN-GS02-Pro-Control (MIT); OpenMouse (facts only)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        // Only read for now: the page shows it read-only.
        io.forbid_changes();
        // Read at every pass, so that the test report always holds it.
        match io.report_descriptor().ok().and_then(|d| framing(&d)) {
            Some(Framing::Plain) => {}
            other => {
                let why = match other {
                    Some(Framing::Numbered(ids)) => format!(
                        "this mouse numbers its reports ({}), a format PeriHub does not send yet",
                        ids.iter().map(|i| format!("{i:02x}")).collect::<Vec<_>>().join(", ")
                    ),
                    _ => "its report descriptor could not be read".into(),
                };
                return Ok(Reading {
                    details: vec![("Status".into(), "nothing was sent to this mouse".into())],
                    problem: Some(why),
                    ..Default::default()
                });
            }
        }
        let version = match self.query(io, &request(GET_VERSION, &[])) {
            Ok(answer) => version_text(&answer),
            Err(Error::Timeout) => return Ok(self.no_answer()),
            Err(e) => return Err(e),
        };
        let config = match self.query(io, &request(GET_CONFIG, &[])) {
            Ok(answer) => answer,
            Err(Error::Timeout) => return Ok(self.no_answer()),
            Err(e) => return Err(e),
        };
        let (c, mut problems) = decode_config(&config);
        let battery = match self.query(io, &request(GET_BATTERY, &BATTERY_TAIL)) {
            Ok(a) if a[8] <= 100 && a[9] <= 1 => Some((a[8], a[9] == 1)),
            Ok(a) => {
                problems.push(format!("battery {:02x} {:02x}", a[8], a[9]));
                None
            }
            // A mouse on its cable may not answer this.
            Err(Error::Timeout) => None,
            Err(e) => return Err(e),
        };
        let mut details = Vec::new();
        if let Some(text) = version {
            details.push(("Model".into(), text));
        }
        details.push(("Lighting mode".into(), format!("{}", c.light_mode)));
        details.push(("Lift-off distance".into(), format!("{}", c.lift_off)));
        Ok(Reading {
            battery,
            active_dpi: Some(c.active),
            settings: Settings { dpi_levels: c.levels, polling_hz: c.polling_hz, ..Default::default() },
            details,
            problem: (!problems.is_empty()).then(|| format!("unexpected answer: {}", problems.join(", "))),
            ..Default::default()
        })
    }

    fn apply(&mut self, _io: &mut Io, _from: &Settings, _to: &Settings) -> Result<()> {
        Err(Error::Invalid("this mouse is only read for now".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("this mouse is only read for now".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// A vendor collection without report numbers: page ff01, usage 10, 64 bytes in and out.
    const PLAIN: &[u8] = &[
        0x06, 0x01, 0xFF, 0x09, 0x10, 0xA1, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x09, 0x01,
        0x81, 0x02, 0x95, 0x40, 0x09, 0x01, 0x91, 0x02, 0xC0,
    ];
    /// The same with report number f0.
    const NUMBERED: &[u8] = &[
        0x06, 0x01, 0xFF, 0x09, 0x10, 0xA1, 0x01, 0x85, 0xF0, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x3F,
        0x09, 0x01, 0x81, 0x02, 0x95, 0x3F, 0x09, 0x01, 0x91, 0x02, 0xC0,
    ];

    /// The tester's M4 as the vendor panel showed it: 800, 1200 ×4, 1600, the second level in use,
    /// 1000 Hz. Battery 76 %, not charging.
    fn m4(descriptor: &[u8]) -> FakeHid {
        let hid = FakeHid::new(|req| {
            let mut a = vec![0u8; REPORT];
            a[0] = ANSWER;
            a[1] = req[1];
            match req[1] {
                GET_VERSION => a[8..23].copy_from_slice(b"M4 Gaming Mouse"),
                GET_CONFIG => {
                    a[9] = 1;
                    a[10] = 4;
                    a[11] = 6;
                    a[12] = 2;
                    for (i, d) in [800u16, 1200, 1200, 1200, 1200, 1600].iter().enumerate() {
                        a[13 + 2 * i..15 + 2 * i].copy_from_slice(&d.to_le_bytes());
                    }
                    a[49] = 1;
                }
                GET_BATTERY => {
                    a[8] = 76;
                    a[9] = 0;
                }
                _ => return Vec::new(),
            }
            // A DPI button press the mouse reports by itself comes first.
            let mut unasked = vec![0u8; REPORT];
            unasked[..3].copy_from_slice(&[ANSWER, 0xFA, 0xAE]);
            vec![unasked, a]
        });
        hid.0.lock().unwrap().descriptor = Some(descriptor.to_vec());
        hid
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("perihub-test-yjx-{name}-{}", std::process::id()))
    }

    #[test]
    fn the_format_comes_from_the_descriptor() {
        assert_eq!(framing(PLAIN), Some(Framing::Plain));
        assert_eq!(framing(NUMBERED), Some(Framing::Numbered(vec![0xF0])));
        assert_eq!(framing(&[]), None);
        assert_eq!(framing(&[0x06, 0x01]), None, "cut short");
    }

    #[test]
    fn an_m4_reads_as_its_vendor_panel_shows_it_and_only_reads() {
        let hid = m4(PLAIN);
        let model = find(0xA8A5, 0x2255).unwrap();
        let task = start_for_test(Box::new(hid.clone()), model, 0x2255, true, temp("m4")).unwrap();
        let info = &task.info;
        assert_eq!(info.problem, None);
        assert_eq!(info.settings.dpi_levels, [800, 1200, 1200, 1200, 1200, 1600]);
        assert_eq!(info.active_dpi, Some(2));
        assert_eq!(info.settings.polling_hz, Some(1000));
        let battery = info.battery.unwrap();
        assert_eq!((battery.level, battery.charging), (76, false));
        assert!(info.details.contains(&("Model".into(), "M4 Gaming Mouse".into())));
        assert!(info.limits == Default::default(), "nothing may be changed");
        assert!(!info.editable);
        for req in hid.sent() {
            assert_eq!(req.len(), REPORT);
            assert!(COMMANDS.iter().any(|c| req.starts_with(c.prefix)), "{req:02x?}");
        }
        assert!(info.raw.iter().any(|l| l.starts_with("report descriptor")), "kept for the test report");
    }

    #[test]
    fn a_mouse_that_numbers_its_reports_gets_nothing() {
        let hid = m4(NUMBERED);
        let model = find(0xA8A5, 0x2255).unwrap();
        let task = start_for_test(Box::new(hid.clone()), model, 0x2255, true, temp("numbered")).unwrap();
        assert!(hid.sent().is_empty());
        assert!(task.info.problem.as_deref().unwrap().contains("f0"));
        assert!(!task.info.editable);
    }

    #[test]
    fn without_a_descriptor_nothing_is_sent() {
        let hid = FakeHid::new(|_| vec![vec![ANSWER; REPORT]]);
        let model = find(0xA8A5, 0x2255).unwrap();
        let task = start_for_test(Box::new(hid.clone()), model, 0x2255, true, temp("none")).unwrap();
        assert!(hid.sent().is_empty());
        assert!(task.info.problem.is_some());
    }

    #[test]
    fn a_mouse_that_is_off_gets_its_page_and_test_report() {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().descriptor = Some(PLAIN.to_vec());
        let model = find(0xA8A5, 0x2255).unwrap();
        let task = start_for_test(Box::new(hid), model, 0x2255, true, temp("off")).unwrap();
        assert!(task.info.details[0].1.starts_with("no answer yet"), "{:?}", task.info.details);
        assert_eq!(task.info.problem, None, "no problem kept once it answers");
        assert!(task.info.raw.iter().any(|l| l.starts_with("report descriptor")));
    }

    #[test]
    fn the_test_report_keeps_the_descriptor_and_version_after_a_refresh() {
        let model = find(0xA8A5, 0x2255).unwrap();
        let mut task = start_for_test(Box::new(m4(PLAIN)), model, 0x2255, true, temp("refresh")).unwrap();
        task.info().unwrap();
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "{raw:?}");
        assert!(raw.iter().any(|l| l.starts_with("55 03")), "{raw:?}");
    }

    #[test]
    fn odd_settings_are_reported() {
        let mut a = [0u8; REPORT];
        a[10] = 9;
        a[11] = 7;
        a[12] = 0;
        let (_, problems) = decode_config(&a);
        // Polling code, level count, a DPI of 0 and the active level.
        assert_eq!(problems.len(), 4, "{problems:?}");
    }
}

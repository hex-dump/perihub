//! The Wraith W60, a magnetic (Hall effect) keyboard on the platform of the AULA WIN60 HE. The
//! protocol is in `docs/protocols/wraith-he.md`, from AulaControl (Apache-2.0), which documents the
//! WIN60 HE (`2e3c:c365`); the W60 (`2e3c:c368`) has the same configuration collection. No code was
//! copied. Nobody has tried it with PeriHub yet: for now it only reads the model and the lighting.
//!
//! The configuration collection can also send how far each key is pressed while the vendor's travel
//! test runs. PeriHub never asks for that, skips such reports, and on Linux the keyboard needs a rule
//! the user turns on (not in this build).

use std::time::Duration;

use super::task::Driver;
use super::{Command, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

/// Report number, then 63 bytes.
const REPORT_ID: u8 = 0x01;
const REPORT: usize = 64;

const DEVICE_INFO: u8 = 0x0D;
const KEY_LIGHT: u8 = 0x07;
const SIDE_LIGHT: u8 = 0x08;
/// Second byte of a lighting read.
const READ: u8 = 0x01;

const TIMEOUT: Duration = Duration::from_millis(500);
/// The months a build date starts with: the device-info answer that is not the model.
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Everything PeriHub may send, after the report number: three reads.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &[REPORT_ID, DEVICE_INFO, 0x00, 0x00, 0x00], at: 0, effect: Effect::Read, name: "read model" },
    Command { prefix: &[REPORT_ID, KEY_LIGHT, READ], at: 0, effect: Effect::Read, name: "read key lighting" },
    Command { prefix: &[REPORT_ID, SIDE_LIGHT, READ], at: 0, effect: Effect::Read, name: "read side lighting" },
];

#[derive(Debug, Clone, Copy)]
pub struct Params;

pub const MODELS: &[Model] = &[Model {
    vendor_id: 0x2E3C,
    product_ids: &[0xC368],
    brand: "Wraith",
    name: "W60",
    kind: Kind::Keyboard,
    interface: Some(2),
    usage_page: Some(0xFF1B),
    protocol: Protocol::Wraith(Params),
}];

/// A request: the report number, the command and its second byte, zeros up to 64 bytes.
fn request(command: u8, second: u8) -> [u8; REPORT] {
    let mut r = [0u8; REPORT];
    r[0] = REPORT_ID;
    r[1] = command;
    r[2] = second;
    r
}

/// The text of a device-info answer: its length is at byte 5 (after the report number), the text
/// follows.
fn info_text(answer: &[u8; REPORT]) -> Option<String> {
    let len = answer[5] as usize;
    let text = answer.get(6..6 + len)?;
    let text: Vec<u8> = text.iter().copied().take_while(|&b| b != 0).collect();
    (!text.is_empty() && text.iter().all(|b| (0x20..0x7F).contains(b))).then(|| String::from_utf8(text).unwrap())
}

/// A lighting answer as text: mode, speed and brightness (0–4) and the colour.
fn light_text(a: &[u8; REPORT]) -> Option<String> {
    // Report number, command, second byte, 0, index, length, then the values.
    if (a[5] as usize) < 12 {
        return None;
    }
    let v = &a[6..];
    Some(format!(
        "mode {}, speed {}/4, brightness {}/4, colour #{:02x}{:02x}{:02x}",
        v[0], v[1], v[2], v[3], v[4], v[5]
    ))
}

pub struct Wraith;

impl Wraith {
    pub fn new(_p: Params) -> Self {
        Self
    }
}

/// An answer to `command`: the reports the keyboard sends by itself (key travel) are skipped.
fn answers(command: u8) -> impl Fn(&[u8]) -> bool {
    move |r: &[u8]| r[0] == REPORT_ID && r[1] == command
}

impl Driver for Wraith {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "AulaControl (Apache-2.0)"
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
        // For the test report, at every pass.
        let _ = io.report_descriptor();
        // One request, two answers: the model and the build date, in either order.
        let mut first = [0u8; REPORT];
        match io.numbered_output_exchange(&request(DEVICE_INFO, 0), &mut first, TIMEOUT, answers(DEVICE_INFO)) {
            Ok(()) => {}
            // The page and its test report show it, and it is read again every few seconds.
            Err(Error::Timeout) => {
                return Ok(Reading {
                    details: vec![(
                        "Status".into(),
                        "no answer yet: if the keyboard is plugged in, please send a test report".into(),
                    )],
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        }
        let mut second = [0u8; REPORT];
        let second = io.read_matching(&mut second, TIMEOUT, answers(DEVICE_INFO)).ok().map(|_| second);
        let mut details = Vec::new();
        for text in std::iter::once(first).chain(second).filter_map(|a| info_text(&a)) {
            let label = if MONTHS.iter().any(|m| text.starts_with(m)) { "Firmware date" } else { "Model" };
            details.push((label.to_string(), text));
        }
        let mut problems = Vec::new();
        for (command, label) in [(KEY_LIGHT, "Key lighting"), (SIDE_LIGHT, "Side lighting")] {
            let mut a = [0u8; REPORT];
            match io.numbered_output_exchange(&request(command, READ), &mut a, TIMEOUT, answers(command)) {
                Ok(()) => match light_text(&a) {
                    Some(text) => details.push((label.into(), text)),
                    None => problems.push(format!("{label}: length {}", a[5])),
                },
                // A keyboard without side lights may not answer.
                Err(Error::Timeout) if command == SIDE_LIGHT => {}
                Err(e) => return Err(e),
            }
        }
        Ok(Reading {
            details,
            problem: (!problems.is_empty()).then(|| format!("unexpected answer: {}", problems.join(", "))),
            ..Default::default()
        })
    }

    fn apply(&mut self, _io: &mut Io, _from: &Settings, _to: &Settings) -> Result<()> {
        Err(Error::Invalid("this keyboard is only read for now".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("this keyboard is only read for now".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    fn answer(command: u8, second: u8, payload: &[u8]) -> Vec<u8> {
        let mut a = vec![0u8; REPORT];
        a[..3].copy_from_slice(&[REPORT_ID, command, second]);
        a[5] = payload.len() as u8;
        a[6..6 + payload.len()].copy_from_slice(payload);
        a
    }

    /// A simulated W60: the build date comes first, key travel reports arrive in between.
    fn w60() -> FakeHid {
        let hid = FakeHid::new(|req| {
            let travel = answer(0x21, 0x00, &[0x01, 0x03, 0x02, 0x66, 0x00]);
            match req[1] {
                DEVICE_INFO => vec![
                    travel.clone(),
                    answer(DEVICE_INFO, 0, b"Apr 15 2026,11:20:35"),
                    travel,
                    answer(DEVICE_INFO, 0, b"W60,34,KB,V3.22.19"),
                ],
                KEY_LIGHT => vec![travel, answer(KEY_LIGHT, READ, &[0, 4, 3, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0])],
                SIDE_LIGHT => vec![answer(SIDE_LIGHT, READ, &[1, 2, 4, 0, 0, 0xFF, 0, 0, 0, 0, 0, 0])],
                _ => Vec::new(),
            }
        });
        hid.0.lock().unwrap().numbered_output = true;
        hid
    }

    #[test]
    fn a_keyboard_that_does_not_answer_gets_its_page_and_test_report() {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().numbered_output = true;
        hid.0.lock().unwrap().descriptor = Some(vec![0x06, 0x1B, 0xFF, 0x09, 0x91, 0xA1, 0x01, 0x85, 0x01, 0xC0]);
        let model = find(0x2E3C, 0xC368).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-wraith-off-{}", std::process::id()));
        let mut task = start_for_test(Box::new(hid), model, 0xC368, true, backups).unwrap();
        assert!(task.info.details[0].1.starts_with("no answer yet"));
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "kept after a refresh: {raw:?}");
    }

    #[test]
    fn a_w60_reads_its_model_and_lighting_and_only_reads() {
        let hid = w60();
        let model = find(0x2E3C, 0xC368).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-wraith-{}", std::process::id()));
        let task = start_for_test(Box::new(hid.clone()), model, 0xC368, true, backups).unwrap();
        let d = &task.info.details;
        assert!(d.contains(&("Model".into(), "W60,34,KB,V3.22.19".into())), "{d:?}");
        assert!(d.contains(&("Firmware date".into(), "Apr 15 2026,11:20:35".into())));
        assert!(d.contains(&("Key lighting".into(), "mode 0, speed 4/4, brightness 3/4, colour #ff0000".into())));
        assert!(d.contains(&("Side lighting".into(), "mode 1, speed 2/4, brightness 4/4, colour #0000ff".into())));
        assert_eq!(task.info.problem, None);
        assert!(!task.info.editable);
        for req in hid.sent() {
            assert_eq!(req.len(), REPORT);
            assert!(COMMANDS.iter().any(|c| req.starts_with(c.prefix)), "{req:02x?}");
        }
    }
}

//! SteelSeries mice, from rivalcfg's device profiles (WTFPL, see `THIRD-PARTY-NOTICES.md`). The
//! table is generated into `steelseries_models.rs` by `tools/rivalcfg_to_rust.py`; the protocol is
//! in `docs/protocols/steelseries-mice.md`. None has been tried with PeriHub.
//!
//! These mice cannot report their settings, so PeriHub can neither show nor check nor undo them.
//! It only sends a setting the user picks, and never the command that saves to the mouse's memory:
//! rivalcfg documents that without it the settings last until the mouse is unplugged or turned off.

use std::time::Duration;

use super::task::Driver;
use super::{BlindKind, BlindSetting, BlindValue, Command, Effect, Io, Reading, Settings};
use crate::color::Rgb;
use crate::error::{Error, Result};

/// rivalcfg waits this long after every command, so that the mouse is not sent them too fast.
const COMMAND_DELAY: Duration = Duration::from_millis(50);
/// Firmware and battery answers (rivalcfg reads them with a 200 ms timeout).
const READ_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy)]
pub struct Cmd {
    /// A feature report instead of an output report.
    pub feature: bool,
    pub bytes: &'static [u8],
}

#[derive(Debug, Clone, Copy)]
pub enum DpiEncoding {
    /// The sensor's value for each DPI step.
    Choices(&'static [(u32, u32)]),
    /// An output range matched step by step to the input range.
    Range(i32, i32, i32),
}

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// One of a few named values, each with its bytes.
    Choice(&'static [(&'static str, &'static [u8])]),
    /// Red, green, blue.
    Color,
    /// A number in `input` (first, last, step), sent as the matching step of `output`.
    Range { input: (i32, i32, i32), output: (i32, i32, i32), bytes: u8 },
    /// A number in `input`, sent as its value in `choices`.
    RangeChoice { input: (i32, i32, i32), choices: &'static [(u32, u32)], bytes: u8 },
    /// DPI presets: a count, the selected preset, then each DPI.
    Dpi {
        input: (i32, i32, i32),
        encoding: DpiEncoding,
        dpi_bytes: u8,
        first_preset: u8,
        max_presets: u8,
        count_flag: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Setting {
    pub id: &'static str,
    pub label: &'static str,
    pub cmd: Cmd,
    /// Bytes sent after the value.
    pub suffix: &'static [u8],
    /// The command is padded with zeros to this length (0: not padded).
    pub packet: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy)]
pub enum BatteryFormat {
    /// Byte 1: bit 7 charging, the rest (level + 1) in fifths of 100.
    FlagAndFifths,
    /// Byte 0 the level in percent, byte 2 charging.
    PercentFirst,
}

#[derive(Debug, Clone, Copy)]
pub struct Battery {
    pub cmd: Cmd,
    pub reply: usize,
    pub format: BatteryFormat,
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// The command that saves to the mouse's memory: never sent until a model is tested.
    pub save: Option<Cmd>,
    /// Command and answer length.
    pub firmware: Option<(Cmd, usize)>,
    pub battery: Option<Battery>,
    /// Bytes to read after every command (the answers of wireless mice through their receiver).
    pub readback: usize,
    pub settings: &'static [Setting],
}

fn steps((first, last, step): (i32, i32, i32)) -> impl Iterator<Item = i32> {
    (first..=last).step_by(step.max(1) as usize)
}

fn le(value: u32, bytes: u8) -> Result<Vec<u8>> {
    if bytes < 4 && value >> (8 * bytes as u32) != 0 {
        return Err(Error::Invalid(format!("{value} does not fit in {bytes} bytes")));
    }
    Ok((0..bytes).map(|i| (value >> (8 * i as u32)) as u8).collect())
}

/// The step of `input` that `value` is, refused if it is not one.
fn step_index(input: (i32, i32, i32), value: i32) -> Result<usize> {
    steps(input)
        .position(|v| v == value)
        .ok_or_else(|| Error::Invalid(format!("{value}: from {} to {} in steps of {}", input.0, input.1, input.2)))
}

fn range_value(input: (i32, i32, i32), output: (i32, i32, i32), value: i32) -> Result<u32> {
    let i = step_index(input, value)?;
    let out: Vec<i32> = steps(output).collect();
    if out.len() != steps(input).count() {
        return Err(Error::Invalid("input and output ranges differ".into()));
    }
    u32::try_from(out[i]).map_err(|_| Error::Invalid(format!("{}", out[i])))
}

fn choice_value(input: (i32, i32, i32), choices: &[(u32, u32)], value: i32) -> Result<u32> {
    step_index(input, value)?;
    choices
        .iter()
        .find(|(k, _)| *k as i32 == value)
        .map(|(_, v)| *v)
        .ok_or_else(|| Error::Invalid(format!("{value} has no value on this mouse")))
}

/// The bytes that set `setting` to `value`, as rivalcfg builds them.
pub fn encode(setting: &Setting, value: &BlindValue) -> Result<Vec<u8>> {
    let mut out = setting.cmd.bytes.to_vec();
    match (setting.kind, value) {
        (Kind::Choice(values), BlindValue::Choice(name)) => {
            let (_, bytes) = values
                .iter()
                .find(|(n, _)| n == name)
                .ok_or_else(|| Error::Invalid(format!("{}: {name}", setting.label)))?;
            out.extend_from_slice(bytes);
        }
        (Kind::Color, BlindValue::Color(Rgb { r, g, b })) => out.extend([*r, *g, *b]),
        (Kind::Range { input, output, bytes }, BlindValue::Number(n)) => {
            out.extend(le(range_value(input, output, *n)?, bytes)?);
        }
        (Kind::RangeChoice { input, choices, bytes }, BlindValue::Number(n)) => {
            out.extend(le(choice_value(input, choices, *n)?, bytes)?);
        }
        (Kind::Dpi { input, encoding, dpi_bytes, first_preset, max_presets, count_flag }, BlindValue::Dpi(dpis)) => {
            if dpis.is_empty() || dpis.len() > max_presets as usize {
                return Err(Error::Invalid(format!("this mouse takes 1 to {max_presets} DPI presets")));
            }
            let count = dpis.len() as u8;
            out.push(if count_flag { 0xFFu8 >> (8 - count) } else { count });
            out.push(first_preset);
            for &dpi in dpis {
                let value = match encoding {
                    DpiEncoding::Choices(choices) => choice_value(input, choices, dpi as i32)?,
                    DpiEncoding::Range(a, b, c) => range_value(input, (a, b, c), dpi as i32)?,
                };
                out.extend(le(value, dpi_bytes)?);
            }
        }
        _ => return Err(Error::Invalid(format!("{}: wrong kind of value", setting.label))),
    }
    out.extend_from_slice(setting.suffix);
    if setting.packet > out.len() {
        out.resize(setting.packet, 0);
    }
    Ok(out)
}

fn blind_kind(kind: Kind) -> BlindKind {
    match kind {
        Kind::Choice(values) => BlindKind::Choice { values: values.iter().map(|(n, _)| n.to_string()).collect() },
        Kind::Color => BlindKind::Color,
        Kind::Range { input, .. } | Kind::RangeChoice { input, .. } => {
            BlindKind::Range { min: input.0, max: input.1, step: input.2 }
        }
        Kind::Dpi { input, max_presets, .. } => {
            BlindKind::Dpi { min: input.0, max: input.1, step: input.2, max_presets }
        }
    }
}

pub struct SteelSeries {
    p: Params,
}

impl SteelSeries {
    pub fn new(p: Params) -> Self {
        Self { p }
    }

    fn write(&self, io: &mut Io, cmd: Cmd, payload: &[u8]) -> Result<()> {
        io.send(payload, cmd.feature)?;
        if self.p.readback > 0 {
            let mut reply = vec![0u8; self.p.readback];
            // The receiver's answer: only to keep the next answers in order.
            let _ = io.read_at_least(&mut reply, READ_TIMEOUT);
        }
        std::thread::sleep(COMMAND_DELAY);
        Ok(())
    }

    fn query(&self, io: &mut Io, cmd: Cmd, len: usize) -> Option<Vec<u8>> {
        io.drain();
        io.send(cmd.bytes, cmd.feature).ok()?;
        let mut reply = vec![0u8; len];
        io.read_at_least(&mut reply, READ_TIMEOUT).ok()?;
        std::thread::sleep(COMMAND_DELAY);
        Some(reply)
    }
}

impl Driver for SteelSeries {
    fn commands(&self) -> Vec<Command> {
        let mut list: Vec<Command> = self
            .p
            .settings
            .iter()
            .map(|s| Command { prefix: s.cmd.bytes, at: 0, effect: Effect::Change, name: s.label })
            .collect();
        if let Some((cmd, _)) = self.p.firmware {
            list.push(Command { prefix: cmd.bytes, at: 0, effect: Effect::Read, name: "read firmware" });
        }
        if let Some(b) = self.p.battery {
            list.push(Command { prefix: b.cmd.bytes, at: 0, effect: Effect::Read, name: "read battery" });
        }
        // The save command is left out on purpose: see the module's documentation.
        list
    }

    fn source(&self) -> &'static str {
        "rivalcfg (WTFPL)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn readable(&self) -> bool {
        false
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let firmware = self
            .p
            .firmware
            .and_then(|(cmd, len)| self.query(io, cmd, len))
            .map(|v| v.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("."));
        let battery = self.p.battery.and_then(|b| {
            let r = self.query(io, b.cmd, b.reply)?;
            let (level, charging) = match b.format {
                BatteryFormat::FlagAndFifths => (((r[1] & 0x7F) as i32 - 1) * 5, r[1] & 0x80 != 0),
                BatteryFormat::PercentFirst => (r[0] as i32, r[2] != 0),
            };
            (0..=100).contains(&level).then_some((level as u8, charging))
        });
        Ok(Reading { firmware, battery, settings: Settings::default(), ..Default::default() })
    }

    fn apply(&mut self, _io: &mut Io, _from: &Settings, _to: &Settings) -> Result<()> {
        Err(Error::Invalid("this mouse cannot be read back: its settings are sent one at a time".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("PeriHub does not save to the memory of this mouse until someone has tested it".into()))
    }

    fn blind_settings(&self) -> Vec<BlindSetting> {
        self.p
            .settings
            .iter()
            .map(|s| BlindSetting { id: s.id, label: s.label, kind: blind_kind(s.kind), last: None })
            .collect()
    }

    fn send_blind(&mut self, io: &mut Io, id: &str, value: &BlindValue) -> Result<()> {
        let setting = self
            .p
            .settings
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::Invalid(format!("{id} is not a setting of this mouse")))?;
        let payload = encode(setting, value)?;
        log::info!("SteelSeries: {} ← {}", setting.label, super::hex(&payload));
        self.write(io, setting.cmd, &payload)
    }
}

/// A test vector from rivalcfg's own tests: a value and the report it sends.
#[cfg(test)]
#[derive(Debug, serde::Deserialize)]
struct Vector {
    product_id: u16,
    setting: String,
    value: serde_json::Value,
    /// Report type (2 output, 3 feature), report id, then the payload.
    report: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::super::fake::FakeHid;
    use super::super::task::start_for_test;
    use super::super::{Protocol, find};
    use super::*;

    const VECTORS: &str = include_str!("../../tests/fixtures/rivalcfg_vectors.json");

    fn params(product_id: u16) -> Params {
        match find(0x1038, product_id).unwrap().protocol {
            Protocol::SteelSeries(p) => p,
            _ => unreachable!(),
        }
    }

    fn value_for(kind: Kind, v: &serde_json::Value) -> Option<BlindValue> {
        Some(match kind {
            Kind::Choice(_) => BlindValue::Choice(match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }),
            Kind::Color => {
                let s = v.as_str()?;
                let hex = s.strip_prefix('#')?;
                if hex.len() != 6 {
                    return None;
                }
                let n = u32::from_str_radix(hex, 16).ok()?;
                BlindValue::Color(Rgb::new((n >> 16) as u8, (n >> 8) as u8, n as u8))
            }
            Kind::Range { .. } | Kind::RangeChoice { .. } => BlindValue::Number(match v {
                serde_json::Value::Number(n) => n.as_i64()? as i32,
                serde_json::Value::String(s) => s.trim().parse().ok()?,
                _ => return None,
            }),
            Kind::Dpi { .. } => BlindValue::Dpi(match v {
                serde_json::Value::Number(n) => vec![n.as_u64()? as u16],
                serde_json::Value::String(s) => s.split(',').map(|d| d.trim().parse().ok()).collect::<Option<_>>()?,
                serde_json::Value::Array(a) => a.iter().map(|d| d.as_u64().map(|d| d as u16)).collect::<Option<_>>()?,
                _ => return None,
            }),
        })
    }

    #[test]
    fn every_report_matches_rivalcfg_own_tests() {
        let vectors: Vec<Vector> = serde_json::from_str(VECTORS).unwrap();
        let (mut checked, mut rounded) = (0, 0);
        for v in &vectors {
            let p = params(v.product_id);
            let Some(setting) = p.settings.iter().find(|s| s.id == v.setting) else { continue };
            let Some(value) = value_for(setting.kind, &v.value) else { continue };
            let expected = &v.report[2..];
            let payload = match encode(setting, &value) {
                Ok(payload) => payload,
                // rivalcfg rounds a value between two steps to the nearest one; PeriHub refuses it.
                Err(_) if off_step(setting.kind, &value) => {
                    rounded += 1;
                    continue;
                }
                Err(e) => panic!("{v:?}: {e}"),
            };
            assert_eq!(setting.cmd.feature, v.report[0] == 3, "{v:?}");
            if setting.packet > 0 {
                // rivalcfg's test checks the start of the padded report, and its length.
                assert_eq!(payload.len(), setting.packet, "{v:?}");
                assert!(payload.starts_with(expected), "{v:?}: {payload:02x?}");
            } else {
                assert_eq!(payload, expected, "{v:?}");
            }
            checked += 1;
        }
        eprintln!("{checked} reports identical to rivalcfg's tests, {rounded} values rivalcfg rounds refused");
        assert!(checked >= 300, "only {checked} vectors checked");
        assert!(rounded < checked / 4, "{rounded} values off the steps");
    }

    /// A value rivalcfg would round to a step.
    fn off_step(kind: Kind, value: &BlindValue) -> bool {
        let on = |input: (i32, i32, i32), n: i32| step_index(input, n).is_ok();
        match (kind, value) {
            (Kind::Range { input, .. } | Kind::RangeChoice { input, .. }, BlindValue::Number(n)) => !on(input, *n),
            (Kind::Dpi { input, .. }, BlindValue::Dpi(d)) => d.iter().any(|&n| !on(input, n as i32)),
            _ => false,
        }
    }

    #[test]
    fn values_outside_the_documented_steps_are_refused() {
        let p = params(0x1824); // Rival 3
        let dpi = p.settings.iter().find(|s| s.id == "sensitivity").unwrap();
        assert!(encode(dpi, &BlindValue::Dpi(vec![250])).is_err(), "not a step of 100");
        assert!(encode(dpi, &BlindValue::Dpi(vec![100])).is_err(), "below 200");
        assert!(encode(dpi, &BlindValue::Dpi(vec![800; 6])).is_err(), "more than five presets");
        let polling = p.settings.iter().find(|s| s.id == "polling_rate").unwrap();
        assert!(encode(polling, &BlindValue::Choice("333".into())).is_err());
        assert!(encode(polling, &BlindValue::Number(1000)).is_err(), "wrong kind of value");
    }

    #[test]
    fn nothing_but_listed_settings_and_reads_leaves_and_never_the_save() {
        for model in crate::peripheral::models().filter(|m| m.vendor_id == 0x1038) {
            let Protocol::SteelSeries(p) = model.protocol else { continue };
            let driver = SteelSeries::new(p);
            let commands = driver.commands();
            if let Some(save) = p.save {
                assert!(!commands.iter().any(|c| save.bytes.starts_with(c.prefix)), "{}: save is sendable", model.name);
            }
            assert!(commands.iter().all(|c| !c.prefix.is_empty()), "{}", model.name);
        }
    }

    #[test]
    fn a_rival_3_is_only_sent_what_the_user_picks() {
        let hid = FakeHid::new(|p| match p.first() {
            Some(0x10) => vec![vec![0x01, 0x25]],
            _ => Vec::new(),
        });
        let model = find(0x1038, 0x1824).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-ss-{}", std::process::id()));
        let mut task = start_for_test(Box::new(hid.clone()), model, 0x1824, true, backups.clone()).unwrap();
        assert_eq!(task.info.firmware.as_deref(), Some("1.37"));
        assert!(!task.info.readable && !task.info.can_save);
        assert_eq!(hid.sent(), [vec![0x10, 0x00]], "only the firmware was asked");
        let info = task.set_blind("polling_rate", &BlindValue::Choice("500".into())).unwrap();
        assert_eq!(hid.sent()[1], [0x04, 0x00, 0x02]);
        assert!(
            matches!(&info.blind.iter().find(|b| b.id == "polling_rate").unwrap().last, Some(BlindValue::Choice(c)) if c == "500")
        );
        assert!(task.set_blind("unknown", &BlindValue::Number(1)).is_err());
        assert_eq!(hid.sent().len(), 2);
        let mut ro = start_for_test(Box::new(hid.clone()), model, 0x1824, false, backups).unwrap();
        assert!(ro.set_blind("polling_rate", &BlindValue::Choice("125".into())).is_err(), "read-only");
    }
}

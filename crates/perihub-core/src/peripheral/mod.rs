//! Mice and keyboards of other brands, supported from published documentation of their protocols
//! (`docs/protocols`, credits in `THIRD-PARTY-NOTICES.md`). Nobody has tried them with PeriHub yet:
//! they are read, and changed only as an experimental feature.
//!
//! The guardrails are here, under every driver, not only in the interface:
//! - every command goes through [`Io`], which refuses any command missing from the driver's list,
//!   and any command that changes something on a connection that may not change anything;
//! - nothing is changed without a backup of the settings first, every change is read back, and a
//!   change the user does not keep is undone (see [`task::PeripheralTask`]);
//! - an answer that does not look as documented leaves the device read-only.

pub mod asus;
pub mod steelseries;
#[rustfmt::skip]
pub mod steelseries_models;
pub mod task;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::color::Rgb;
use crate::error::{Error, Result};

/// What kind of device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Mouse,
    Keyboard,
}

/// What a command does to the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Only asks for something.
    Read,
    /// Changes a setting. It may last only until the device is turned off, or not: the drivers
    /// treat it as one that stays.
    Change,
    /// Stores the settings in the device's memory.
    Save,
}

/// A command a driver may send, recognized by its first bytes.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub prefix: &'static [u8],
    pub effect: Effect,
    pub name: &'static str,
}

/// Which protocol a model speaks, with what its driver needs to know about it.
#[derive(Debug, Clone, Copy)]
pub enum Protocol {
    Asus(asus::Params),
    SteelSeries(steelseries::Params),
}

/// A device PeriHub recognizes, from a driver's table.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub vendor_id: u16,
    /// USB product ids: a wireless mouse has one on its cable and one through its receiver.
    pub product_ids: &'static [u16],
    pub brand: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    /// The USB interface of the configuration collection, where a source says which one.
    pub interface: Option<i32>,
    /// The usage page of the configuration collection, where a source says which one.
    pub usage_page: Option<u16>,
    pub protocol: Protocol,
}

/// Every model of every driver.
pub fn models() -> impl Iterator<Item = &'static Model> {
    asus::MODELS.iter().chain(steelseries_models::MODELS)
}

pub fn find(vendor_id: u16, product_id: u16) -> Option<&'static Model> {
    models().find(|m| m.vendor_id == vendor_id && m.product_ids.contains(&product_id))
}

/// The vendors with a driver: only their devices are looked at.
pub fn vendors() -> impl Iterator<Item = u16> {
    let mut ids: Vec<u16> = models().map(|m| m.vendor_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
}

/// The settings PeriHub can show and change, the same for every driver. A driver fills what its
/// devices have and leaves the rest empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub dpi_levels: Vec<u16>,
    pub polling_hz: Option<u16>,
    /// Other settings with a few values each, such as button response, by id (see [`Choice`]).
    pub options: BTreeMap<String, u16>,
    pub leds: Vec<Led>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Led {
    /// The device's own mode number (see [`LedLimits::modes`]).
    pub mode: u8,
    /// Percent, where the device has a brightness.
    pub brightness: Option<u8>,
    pub color: Rgb,
}

/// What may be changed, and to what. Something missing here is shown but not changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Limits {
    pub dpi: Option<DpiLimits>,
    /// Empty: the polling rate is not changed.
    pub polling_hz: Vec<u16>,
    pub options: Vec<Choice>,
    pub leds: Vec<LedLimits>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DpiLimits {
    pub min: u16,
    pub max: u16,
    pub step: u16,
}

impl DpiLimits {
    pub fn accepts(&self, dpi: u16) -> bool {
        (self.min..=self.max).contains(&dpi) && (dpi - self.min).is_multiple_of(self.step)
    }
}

/// A setting with a few values, such as "Button response: 4, 8 … 32 ms".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
    /// Value as stored, and how it reads.
    pub values: Vec<(u16, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedLimits {
    pub name: String,
    /// Modes that may be set: the device's number and a name.
    pub modes: Vec<(u8, &'static str)>,
    /// The steps the brightness takes, in percent (25 on a 0–4 scale); `None`: not changed.
    pub brightness_step: Option<u8>,
    pub color: bool,
}

/// A physical button and what it does now, as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ButtonView {
    pub name: String,
    pub action: String,
}

/// A setting of a device that cannot report it: PeriHub can send it but not read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlindSetting {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: BlindKind,
    /// What PeriHub last sent since the device was connected, if anything.
    pub last: Option<BlindValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlindKind {
    Choice {
        values: Vec<String>,
    },
    Color,
    Range {
        min: i32,
        max: i32,
        step: i32,
    },
    /// DPI presets, from one to `max_presets`.
    Dpi {
        min: i32,
        max: i32,
        step: i32,
        max_presets: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BlindValue {
    Choice(String),
    Color(Rgb),
    Number(i32),
    Dpi(Vec<u16>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Battery {
    /// Percent.
    pub level: u8,
    pub charging: bool,
}

/// What a driver reads from a device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    pub firmware: Option<String>,
    /// Level in percent and charging.
    pub battery: Option<(u8, bool)>,
    /// From 1.
    pub active_profile: Option<u8>,
    /// From 1.
    pub active_dpi: Option<u8>,
    pub settings: Settings,
    pub limits: Limits,
    pub buttons: Vec<ButtonView>,
    /// Something did not look as documented: nothing is changed on this device.
    pub problem: Option<String>,
}

/// What the interface shows of a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeripheralInfo {
    /// "vvvv:pppp", how the interface names the device.
    pub key: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub brand: &'static str,
    pub model: &'static str,
    pub kind: Kind,
    /// Where PeriHub knows the protocol from, for the page and the test report.
    pub source: &'static str,
    pub profiles: u8,
    pub firmware: Option<String>,
    pub battery: Option<Battery>,
    pub active_profile: Option<u8>,
    pub active_dpi: Option<u8>,
    /// The device can report its settings. If not, PeriHub shows `blind` instead.
    pub readable: bool,
    pub settings: Settings,
    pub limits: Limits,
    pub buttons: Vec<ButtonView>,
    /// Settings PeriHub can send to a device that cannot report them, one at a time.
    pub blind: Vec<BlindSetting>,
    /// Changes may be tried: this is PeriHub's app (not the command line) and everything read looked
    /// as documented.
    pub editable: bool,
    /// The device has a command that stores the settings in its memory.
    pub can_save: bool,
    pub problem: Option<String>,
    /// A change was neither kept nor undone, and this connection could not undo it.
    pub pending_undo: bool,
    /// Every request and answer of the last reading, for test reports.
    pub raw: Vec<String>,
}

/// The HID calls the drivers use: a device, or a simulated one in the tests.
pub trait Hid: Send {
    fn write(&self, data: &[u8]) -> Result<usize>;
    /// Returns 0 if nothing arrived in time.
    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize>;
    fn send_feature(&self, data: &[u8]) -> Result<()>;
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize>;
}

impl Hid for hidapi::HidDevice {
    fn write(&self, data: &[u8]) -> Result<usize> {
        Ok(hidapi::HidDevice::write(self, data)?)
    }
    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        Ok(hidapi::HidDevice::read_timeout(self, buf, timeout_ms)?)
    }
    fn send_feature(&self, data: &[u8]) -> Result<()> {
        Ok(self.send_feature_report(data)?)
    }
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(self.get_feature_report(buf)?)
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Most requests and answers kept for a test report.
const RAW_MAX: usize = 64;

/// The only way a driver reaches its device. Every command is checked against the driver's list
/// before it is sent.
pub struct Io {
    hid: Box<dyn Hid>,
    commands: Vec<Command>,
    /// Commands that change something may be sent.
    changes: bool,
    /// The connection was opened to change things (PeriHub's app, never the command line).
    granted: bool,
    raw: Vec<String>,
}

impl Io {
    pub fn new(hid: Box<dyn Hid>, commands: Vec<Command>, changes: bool) -> Self {
        Self { hid, commands, changes, granted: changes, raw: Vec::new() }
    }

    pub fn granted(&self) -> bool {
        self.granted
    }

    /// Changes allowed again, only on a connection opened for them: an undo sends back values the
    /// device itself gave, also after something else looked wrong.
    pub fn set_changes(&mut self, on: bool) {
        self.changes = on && self.granted;
    }

    pub fn changes_allowed(&self) -> bool {
        self.changes
    }

    /// No more changes on this connection (an answer did not look as documented).
    pub fn forbid_changes(&mut self) {
        self.changes = false;
    }

    /// What may be sent: a command of the list, and on a read-only connection only a read.
    pub fn check(&self, payload: &[u8]) -> Result<Effect> {
        let command = self.commands.iter().find(|c| payload.starts_with(c.prefix)).ok_or_else(|| {
            Error::Invalid(format!("{} is not a command PeriHub may send", hex(&payload[..payload.len().min(4)])))
        })?;
        if command.effect != Effect::Read && !self.changes {
            return Err(Error::Invalid(format!("{}: this connection may not change anything", command.name)));
        }
        Ok(command.effect)
    }

    /// The requests and answers since the last call, for test reports.
    pub fn take_raw(&mut self) -> Vec<String> {
        std::mem::take(&mut self.raw)
    }

    fn note(&mut self, line: String) {
        if self.raw.len() < RAW_MAX {
            self.raw.push(line);
        }
    }

    /// Drops the input reports left waiting.
    pub fn drain(&mut self) {
        let mut scratch = [0u8; 256];
        for _ in 0..64 {
            if !matches!(self.hid.read_timeout(&mut scratch, 0), Ok(n) if n > 0) {
                break;
            }
        }
    }

    /// Sends `payload`, after report number 0, as an output report or a feature report.
    pub fn send(&mut self, payload: &[u8], feature: bool) -> Result<()> {
        self.check(payload)?;
        let mut report = Vec::with_capacity(payload.len() + 1);
        report.push(0);
        report.extend_from_slice(payload);
        if feature {
            self.hid.send_feature(&report)?;
        } else {
            self.hid.write(&report)?;
        }
        self.note(format!("{} {}", if feature { "feature" } else { "out" }, hex(payload)));
        Ok(())
    }

    /// Fills `reply` with the first input report of at least its length, within `timeout`.
    pub fn read_at_least(&mut self, reply: &mut [u8], timeout: Duration) -> Result<()> {
        let mut scratch = [0u8; 256];
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout);
            }
            let n = self.hid.read_timeout(&mut scratch, left.as_millis().clamp(1, 1000) as i32)?;
            if n >= reply.len() && n > 0 {
                reply.copy_from_slice(&scratch[..reply.len()]);
                self.note(format!("  ← {}", hex(&scratch[..n.min(16)])));
                return Ok(());
            }
        }
    }

    /// Sends `payload` as an output report without a report number, and returns the first input
    /// report of exactly `reply.len()` bytes that arrives within `timeout`. Shorter or longer
    /// reports (a mouse's movements, on the same interface) are skipped. Answers left waiting from
    /// before are dropped first.
    pub fn output_query(&mut self, payload: &[u8], reply: &mut [u8], timeout: Duration) -> Result<()> {
        self.check(payload)?;
        let mut scratch = [0u8; 256];
        for _ in 0..64 {
            if self.hid.read_timeout(&mut scratch, 0)? == 0 {
                break;
            }
        }
        let mut report = Vec::with_capacity(payload.len() + 1);
        report.push(0);
        report.extend_from_slice(payload);
        self.hid.write(&report)?;
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.note(format!("{} → no answer", hex(&payload[..payload.len().min(4)])));
                return Err(Error::Timeout);
            }
            let n = self.hid.read_timeout(&mut scratch, left.as_millis().clamp(1, 1000) as i32)?;
            if n == reply.len() {
                reply.copy_from_slice(&scratch[..n]);
                self.note(format!("{} → {}", hex(&payload[..payload.len().min(4)]), hex(reply)));
                return Ok(());
            }
        }
    }
}

/// Looks for a value in a list of (value, label) pairs.
pub fn label_of(values: &[(u16, String)], value: u16) -> Option<&str> {
    values.iter().find(|(v, _)| *v == value).map(|(_, l)| l.as_str())
}

/// The settings in `wanted` the device does not accept, as an error.
pub fn check_settings(limits: &Limits, current: &Settings, wanted: &Settings) -> Result<()> {
    if wanted.dpi_levels != current.dpi_levels {
        let dpi = limits.dpi.ok_or_else(|| Error::Invalid("the DPI of this device is not changed".into()))?;
        if wanted.dpi_levels.len() != current.dpi_levels.len() {
            return Err(Error::Invalid(format!("this device has {} DPI levels", current.dpi_levels.len())));
        }
        if let Some(bad) = wanted.dpi_levels.iter().find(|&&d| !dpi.accepts(d)) {
            return Err(Error::Invalid(format!(
                "{bad} DPI: this device takes {} to {} DPI in steps of {}",
                dpi.min, dpi.max, dpi.step
            )));
        }
    }
    if wanted.polling_hz != current.polling_hz && !wanted.polling_hz.is_some_and(|hz| limits.polling_hz.contains(&hz)) {
        return Err(Error::Invalid(format!("polling rate {:?} Hz", wanted.polling_hz)));
    }
    if wanted.options.keys().ne(current.options.keys()) {
        return Err(Error::Invalid("unknown setting".into()));
    }
    for (id, value) in &wanted.options {
        if current.options.get(id) == Some(value) {
            continue;
        }
        let choice = limits
            .options
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| Error::Invalid(format!("{id} is not changed on this device")))?;
        if label_of(&choice.values, *value).is_none() {
            return Err(Error::Invalid(format!("{}: value {value}", choice.label)));
        }
    }
    if wanted.leds.len() != current.leds.len() {
        return Err(Error::Invalid(format!("this device has {} LEDs", current.leds.len())));
    }
    for (i, (new, old)) in wanted.leds.iter().zip(&current.leds).enumerate() {
        if new == old {
            continue;
        }
        let led = limits.leds.get(i).ok_or_else(|| Error::Invalid(format!("LED {} is not changed", i + 1)))?;
        if new.mode != old.mode && !led.modes.iter().any(|(m, _)| *m == new.mode) {
            return Err(Error::Invalid(format!("{}: mode {}", led.name, new.mode)));
        }
        let step_ok = |b: u8| led.brightness_step.is_some_and(|step| b <= 100 && b.is_multiple_of(step));
        if new.brightness != old.brightness && !new.brightness.is_some_and(step_ok) {
            return Err(Error::Invalid(format!("{}: brightness", led.name)));
        }
        if new.color != old.color && !led.color {
            return Err(Error::Invalid(format!("{}: colour", led.name)));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fake {
    //! A simulated device for the drivers' tests: answers through a closure, records what is sent.
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    type Answer = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>> + Send>;

    #[derive(Clone)]
    pub struct FakeHid(pub Arc<Mutex<FakeState>>);

    pub struct FakeState {
        pub sent: Vec<Vec<u8>>,
        pub pending: VecDeque<Vec<u8>>,
        answer: Answer,
    }

    impl FakeHid {
        /// `answer` gets every output report (without the report number) and returns the input
        /// reports the device sends back.
        pub fn new(answer: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) -> Self {
            FakeHid(Arc::new(Mutex::new(FakeState {
                sent: Vec::new(),
                pending: VecDeque::new(),
                answer: Box::new(answer),
            })))
        }

        pub fn sent(&self) -> Vec<Vec<u8>> {
            self.0.lock().unwrap().sent.clone()
        }
    }

    impl Hid for FakeHid {
        fn write(&self, data: &[u8]) -> Result<usize> {
            let mut state = self.0.lock().unwrap();
            assert_eq!(data[0], 0, "output reports are sent with report number 0");
            let payload = data[1..].to_vec();
            let answers = (state.answer)(&payload);
            state.sent.push(payload);
            state.pending.extend(answers);
            Ok(data.len())
        }
        fn read_timeout(&self, buf: &mut [u8], _timeout_ms: i32) -> Result<usize> {
            let mut state = self.0.lock().unwrap();
            match state.pending.pop_front() {
                Some(r) => {
                    buf[..r.len()].copy_from_slice(&r);
                    Ok(r.len())
                }
                None => Ok(0),
            }
        }
        fn send_feature(&self, data: &[u8]) -> Result<()> {
            self.0.lock().unwrap().sent.push(data.to_vec());
            Ok(())
        }
        fn get_feature(&self, _buf: &mut [u8]) -> Result<usize> {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeHid;
    use super::*;

    const COMMANDS: &[Command] = &[
        Command { prefix: &[0x12, 0x04], effect: Effect::Read, name: "read" },
        Command { prefix: &[0x51, 0x31], effect: Effect::Change, name: "change" },
    ];

    fn echo() -> FakeHid {
        FakeHid::new(|p| vec![vec![0x01; 8], p.iter().copied().chain(std::iter::repeat(0)).take(64).collect()])
    }

    #[test]
    fn only_listed_commands_are_sent_and_reads_only_when_read_only() {
        let hid = echo();
        let mut io = Io::new(Box::new(hid.clone()), COMMANDS.to_vec(), false);
        let mut reply = [0u8; 64];
        io.output_query(&[0x12, 0x04, 0], &mut reply, Duration::from_millis(50)).unwrap();
        assert_eq!(reply[..2], [0x12, 0x04], "the short report in front was skipped");
        assert!(io.output_query(&[0x51, 0x31, 0], &mut reply, Duration::from_millis(50)).is_err());
        assert!(io.output_query(&[0x50, 0x03], &mut reply, Duration::from_millis(50)).is_err());
        assert_eq!(hid.sent().len(), 1, "refused commands never reach the device");

        let mut io = Io::new(Box::new(hid.clone()), COMMANDS.to_vec(), true);
        io.output_query(&[0x51, 0x31, 0], &mut reply, Duration::from_millis(50)).unwrap();
        assert!(io.output_query(&[0x50, 0x03], &mut reply, Duration::from_millis(50)).is_err(), "not listed");
        assert_eq!(hid.sent().len(), 2);
    }

    #[test]
    fn no_answer_is_a_timeout() {
        let hid = FakeHid::new(|_| Vec::new());
        let mut io = Io::new(Box::new(hid), COMMANDS.to_vec(), false);
        let mut reply = [0u8; 64];
        assert!(matches!(io.output_query(&[0x12, 0x04], &mut reply, Duration::from_millis(20)), Err(Error::Timeout)));
    }

    #[test]
    fn settings_outside_the_limits_are_refused() {
        let current = Settings {
            dpi_levels: vec![800, 1600],
            polling_hz: Some(1000),
            options: [("debounce".to_string(), 1)].into(),
            leds: vec![Led { mode: 0, brightness: Some(100), color: Rgb::WHITE }],
        };
        let limits = Limits {
            dpi: Some(DpiLimits { min: 100, max: 12000, step: 100 }),
            polling_hz: vec![125, 250, 500, 1000],
            options: vec![Choice {
                id: "debounce",
                label: "Button response",
                values: vec![(0, "4 ms".into()), (1, "8 ms".into())],
            }],
            leds: vec![LedLimits {
                name: "Logo".into(),
                modes: vec![(0, "Static")],
                brightness_step: Some(25),
                color: true,
            }],
        };
        let ok = |change: fn(&mut Settings)| {
            let mut s = current.clone();
            change(&mut s);
            check_settings(&limits, &current, &s).is_ok()
        };
        assert!(ok(|_| {}));
        assert!(ok(|s| s.dpi_levels[0] = 12000));
        assert!(!ok(|s| s.dpi_levels[0] = 12100));
        assert!(!ok(|s| s.dpi_levels[0] = 850));
        assert!(!ok(|s| s.dpi_levels.push(400)));
        assert!(!ok(|s| s.polling_hz = Some(2000)));
        assert!(!ok(|s| {
            s.options.insert("debounce".into(), 7);
        }));
        assert!(!ok(|s| {
            s.options.insert("other".into(), 0);
        }));
        assert!(!ok(|s| s.leds[0].mode = 2));
        assert!(!ok(|s| s.leds[0].brightness = Some(101)));
        assert!(!ok(|s| s.leds[0].brightness = Some(60)), "not a step of 25");
        assert!(ok(|s| s.leds[0].brightness = Some(75)));
        let mut no_dpi = limits.clone();
        no_dpi.dpi = None;
        let mut s = current.clone();
        s.dpi_levels[0] = 900;
        assert!(check_settings(&no_dpi, &current, &s).is_err(), "DPI shown but not changed");
    }
}

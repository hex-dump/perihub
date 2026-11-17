//! Transcripts: every request PeriHub sent to a device and every answer, as a versioned JSON file,
//! and [`Replay`], a simulated device that answers from one, so that a driver can run in the tests
//! without the device (see `docs/replay.md`).
//!
//! - Recording ([`Recording`]) only looks at what a driver already sends and reads: it adds nothing
//!   on the way to the device.
//! - A transcript holds only bytes and the device's USB ids. Anything that tells one unit apart (a
//!   serial number, a HID++ unit id) is written as `xx`, and stays out of the file.
//! - In a replay, a request the transcript does not have, and a command that changes something when
//!   the transcript only read the device, stop the test with a panic: nothing passes by accident.

mod record;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::peripheral::{Command, Effect, Hid};

pub use record::{Private, Recording, anonymize};

/// The `format` of every transcript.
pub const FORMAT: &str = "perihub-transcript";
/// The newest version of the format this build reads and writes.
pub const VERSION: u32 = 1;
/// The collection a driver talks to; Windows gives some devices a second one (`short`, HID++).
pub const MAIN: &str = "main";

/// Whether the device was only read while the transcript was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    /// Only reads: a replay fails on any command that changes something.
    ReadOnly,
    /// Changes were sent too (each still has to be in the transcript to be answered).
    ReadWrite,
}

/// Which code reads the device in a replay test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// A driver of [`crate::peripheral`], through [`crate::peripheral::Io`].
    Peripheral,
    /// A HID++ 2.0 device in a slot of a Logitech receiver, read as for its test report.
    HidppReceiver,
    /// A Corsair device (`docs/protocols/corsair-bragi.md`).
    CorsairBragi,
}

/// What happened on a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// An output report sent, its report number first (0 for a device without numbers).
    Write,
    /// An input report that arrived.
    Read,
    /// A feature report sent, report number first.
    FeatureSet,
    /// A feature report read: `report_id` is what was asked, `data` what came, number included.
    FeatureGet,
    /// The collection's report descriptor, which the system gives without asking the device.
    Descriptor,
    /// A note, such as where a reading starts.
    Mark,
}

/// One step of a transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub op: Op,
    #[serde(default = "main_collection", skip_serializing_if = "is_main")]
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<u8>,
    /// The bytes in hex, "xx" for one left out. In a request "xx" matches any byte; in an answer it
    /// reads as 0.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
    /// The length, when `data` leaves out the zeros at its end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<usize>,
    /// A mark's text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

fn main_collection() -> String {
    MAIN.to_string()
}

fn is_main(collection: &str) -> bool {
    collection == MAIN
}

/// Zeros at the end of a report from which they are left out of `data`.
const TRIM_FROM: usize = 8;

impl Event {
    /// The event for `bytes` as they went or came, with the zeros at the end left out of the text.
    pub fn new(op: Op, collection: &str, report_id: Option<u8>, bytes: &[u8]) -> Self {
        let zeros = bytes.iter().rev().take_while(|&&b| b == 0).count();
        let (shown, len) =
            if zeros >= TRIM_FROM { (&bytes[..bytes.len() - zeros], Some(bytes.len())) } else { (bytes, None) };
        Self {
            op,
            collection: collection.to_string(),
            report_id,
            data: crate::peripheral::hex(shown),
            len,
            text: String::new(),
        }
    }

    pub fn mark(text: &str) -> Self {
        Self {
            op: Op::Mark,
            collection: MAIN.to_string(),
            report_id: None,
            data: String::new(),
            len: None,
            text: text.to_string(),
        }
    }

    /// The bytes, `None` for one left out.
    pub fn pattern(&self) -> Result<Vec<Option<u8>>> {
        let mut bytes = Vec::new();
        for word in self.data.split_whitespace() {
            if word.eq_ignore_ascii_case("xx") {
                bytes.push(None);
            } else {
                let byte = u8::from_str_radix(word, 16)
                    .ok()
                    .filter(|_| word.len() == 2)
                    .ok_or_else(|| Error::Invalid(format!("transcript: {word:?} is not a byte in hex")))?;
                bytes.push(Some(byte));
            }
        }
        if let Some(len) = self.len {
            if len < bytes.len() {
                return Err(Error::Invalid(format!("transcript: length {len} is shorter than the data")));
            }
            bytes.resize(len, Some(0));
        }
        Ok(bytes)
    }

    /// The bytes, with 0 for one left out.
    pub fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.pattern()?.into_iter().map(|b| b.unwrap_or(0)).collect())
    }
}

/// The device a transcript is from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// In hex, such as "2e3c".
    pub vendor_id: String,
    pub product_id: String,
    pub name: String,
}

/// A transcript, as it is kept in a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub format: String,
    pub version: u32,
    pub device: Device,
    pub family: Family,
    pub access: Access,
    /// The receiver slot of a [`Family::HidppReceiver`] device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    /// "PeriHub 0.9.3 on windows x86_64".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_by: Option<String>,
    /// Where it comes from, such as an issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// What the replay test checks in what the code read: every value given here must be the same
    /// there (an object may leave fields out, a list is compared whole).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<serde_json::Value>,
    pub events: Vec<Event>,
}

impl Transcript {
    pub fn new(device: Device, family: Family, events: Vec<Event>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            device,
            family,
            access: Access::ReadOnly,
            slot: None,
            recorded_by: Some(format!(
                "PeriHub {} on {} {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
            source: None,
            notes: Vec::new(),
            expect: None,
            events,
        }
    }

    /// Reads a transcript, refusing another format or a newer version.
    pub fn from_json(text: &str) -> Result<Self> {
        let t: Self = serde_json::from_str(text).map_err(|e| Error::Invalid(format!("transcript: {e}")))?;
        if t.format != FORMAT {
            return Err(Error::Invalid(format!("not a transcript: format {:?}", t.format)));
        }
        if t.version == 0 || t.version > VERSION {
            return Err(Error::Invalid(format!("transcript version {} (this build reads up to {VERSION})", t.version)));
        }
        for event in &t.events {
            event.pattern()?;
        }
        Ok(t)
    }

    /// The transcript as JSON, one event per line so that it reads well in an issue.
    pub fn to_json(&self) -> String {
        let mut head = self.clone();
        head.events.clear();
        let head = serde_json::to_string_pretty(&head).expect("a transcript serializes");
        let head = head.trim_end().trim_end_matches('}').trim_end().trim_end_matches("\"events\": []");
        let events: Vec<String> = self
            .events
            .iter()
            .map(|e| format!("    {}", serde_json::to_string(e).expect("an event serializes")))
            .collect();
        format!("{}\n  \"events\": [\n{}\n  ]\n}}\n", head.trim_end(), events.join(",\n"))
    }

    /// The device's USB ids.
    pub fn ids(&self) -> Result<(u16, u16)> {
        let id = |s: &str| u16::from_str_radix(s, 16).map_err(|_| Error::Invalid(format!("transcript: USB id {s:?}")));
        Ok((id(&self.device.vendor_id)?, id(&self.device.product_id)?))
    }
}

/// How a replay tells a command that only reads from one that changes something.
pub enum Classifier {
    /// A driver's list, as [`crate::peripheral::Io`] checks it.
    Commands(Vec<Command>),
    /// HID++: root reads, the read calls of [`crate::hidpp`], receiver register reads. A feature
    /// index is known from the transcript's own answers to the root feature.
    Hidpp,
    /// Corsair: only "get property" reads.
    Bragi,
}

/// The effect of a command of a driver's list, from the bytes on the wire: the report number 0 in
/// front is not part of the command.
pub fn command_effect(commands: &[Command], data: &[u8]) -> Option<Effect> {
    let payload = if data.first() == Some(&0) { &data[1..] } else { data };
    commands.iter().find(|c| payload.get(c.at..).is_some_and(|p| p.starts_with(c.prefix))).map(|c| c.effect)
}

/// A request of the transcript and the input reports that came after it.
struct Exchange {
    op: Op,
    collection: String,
    request: Vec<Option<u8>>,
    answers: Vec<(String, Vec<u8>)>,
    used: usize,
}

struct FeatureAnswer {
    collection: String,
    report_id: u8,
    data: Vec<u8>,
    used: usize,
}

struct State {
    name: String,
    access: Access,
    classifier: Classifier,
    exchanges: Vec<Exchange>,
    features: Vec<FeatureAnswer>,
    descriptors: BTreeMap<String, Vec<u8>>,
    pending: BTreeMap<String, VecDeque<Vec<u8>>>,
    /// HID++ feature indexes seen in the root feature's answers.
    hidpp_index: BTreeMap<u8, u16>,
    /// Every request that reached the simulated device, with its collection.
    sent: Vec<(String, Vec<u8>)>,
}

/// A device that answers from a transcript. [`Replay::collection`] gives a handle for each of its
/// collections, which implements both [`Hid`] and [`crate::hidpp::Transport`].
#[derive(Clone)]
pub struct Replay(Arc<Mutex<State>>);

/// One collection of a [`Replay`].
#[derive(Clone)]
pub struct ReplayHid {
    state: Arc<Mutex<State>>,
    collection: String,
}

fn fits(pattern: &[Option<u8>], data: &[u8]) -> bool {
    pattern.len() == data.len() && pattern.iter().zip(data).all(|(p, d)| p.is_none_or(|p| p == *d))
}

impl Replay {
    pub fn new(transcript: &Transcript, classifier: Classifier) -> Result<Self> {
        let mut exchanges: Vec<Exchange> = Vec::new();
        let mut features = Vec::new();
        let mut descriptors = BTreeMap::new();
        let mut pending: BTreeMap<String, VecDeque<Vec<u8>>> = BTreeMap::new();
        for event in &transcript.events {
            match event.op {
                Op::Write | Op::FeatureSet => exchanges.push(Exchange {
                    op: event.op,
                    collection: event.collection.clone(),
                    request: event.pattern()?,
                    answers: Vec::new(),
                    used: 0,
                }),
                Op::Read => {
                    let answer = (event.collection.clone(), event.bytes()?);
                    match exchanges.last_mut() {
                        Some(exchange) => exchange.answers.push(answer),
                        // Reports waiting before the first request.
                        None => pending.entry(answer.0).or_default().push_back(answer.1),
                    }
                }
                Op::FeatureGet => features.push(FeatureAnswer {
                    collection: event.collection.clone(),
                    report_id: event
                        .report_id
                        .ok_or_else(|| Error::Invalid("transcript: feature_get without report_id".into()))?,
                    data: event.bytes()?,
                    used: 0,
                }),
                Op::Descriptor => {
                    let bytes = event.bytes()?;
                    descriptors.entry(event.collection.clone()).or_insert(bytes);
                }
                Op::Mark => {}
            }
        }
        Ok(Self(Arc::new(Mutex::new(State {
            name: transcript.device.name.clone(),
            access: transcript.access,
            classifier,
            exchanges,
            features,
            descriptors,
            pending,
            hidpp_index: BTreeMap::new(),
            sent: Vec::new(),
        }))))
    }

    pub fn collection(&self, name: &str) -> ReplayHid {
        ReplayHid { state: self.0.clone(), collection: name.to_string() }
    }

    /// Whether the transcript has anything on the collection `name`.
    pub fn has_collection(&self, name: &str) -> bool {
        let s = self.0.lock().unwrap();
        s.exchanges.iter().any(|e| e.collection == name || e.answers.iter().any(|(c, _)| c == name))
            || s.descriptors.contains_key(name)
    }

    /// Every request that reached the simulated device, with its collection.
    pub fn sent(&self) -> Vec<(String, Vec<u8>)> {
        self.0.lock().unwrap().sent.clone()
    }

    /// The requests of the transcript the code never sent, in hex.
    pub fn unused(&self) -> Vec<String> {
        let s = self.0.lock().unwrap();
        s.exchanges
            .iter()
            .filter(|e| e.used == 0)
            .map(|e| {
                let bytes: Vec<String> =
                    e.request.iter().map(|b| b.map_or("xx".to_string(), |b| format!("{b:02x}"))).collect();
                bytes.join(" ")
            })
            .collect()
    }
}

/// Stops the test. The lock is let go first, so that this message is the one that shows.
fn fail(state: MutexGuard<'_, State>, message: String) -> ! {
    let name = state.name.clone();
    drop(state);
    panic!("replay of {name}: {message}");
}

impl ReplayHid {
    fn effect(state: &State, data: &[u8]) -> Option<Effect> {
        match &state.classifier {
            Classifier::Commands(commands) => command_effect(commands, data),
            Classifier::Bragi => {
                let packet = data.get(1..)?;
                if crate::corsair::allowed(packet, false) {
                    Some(Effect::Read)
                } else {
                    crate::corsair::allowed(packet, true).then_some(Effect::Change)
                }
            }
            Classifier::Hidpp => {
                if !matches!(data.first(), Some(0x10 | 0x11)) || data.len() < 7 {
                    return None;
                }
                // A register of the receiver itself (HID++ 1.0): 0x81 and 0x83 read, 0x80 and 0x82 write.
                if data[1] == 0xFF {
                    return Some(if matches!(data[2], 0x81 | 0x83) { Effect::Read } else { Effect::Change });
                }
                let (index, function) = (data[2], data[3] >> 4);
                if index == 0 {
                    return Some(if function <= 1 { Effect::Read } else { Effect::Change });
                }
                let feature = *state.hidpp_index.get(&index)?;
                Some(if crate::hidpp::is_read_request(feature, function) { Effect::Read } else { Effect::Change })
            }
        }
    }

    /// Answers a request: checks it, finds it in the transcript and queues what came after it.
    fn request(&self, op: Op, data: &[u8]) {
        let mut state = self.state.lock().unwrap();
        let shown = crate::peripheral::hex(data);
        match Self::effect(&state, data) {
            None => fail(state, format!("{shown} on {} is not a command the code may send", self.collection)),
            Some(Effect::Read) => {}
            Some(effect) if state.access == Access::ReadOnly => fail(
                state,
                format!(
                    "{shown} on {} changes the device ({effect:?}), and the transcript is read-only",
                    self.collection
                ),
            ),
            Some(_) => {}
        }
        let fits_here = |e: &Exchange| e.op == op && e.collection == self.collection && fits(&e.request, data);
        // The first copy in order; once every copy was answered, the last one again (a reading
        // repeated, as the app does every few seconds).
        let found = state
            .exchanges
            .iter()
            .position(|e| e.used == 0 && fits_here(e))
            .or_else(|| state.exchanges.iter().rposition(fits_here));
        let Some(at) = found else {
            fail(state, format!("{op:?} {shown} on {} is not in the transcript", self.collection))
        };
        state.sent.push((self.collection.clone(), data.to_vec()));
        state.exchanges[at].used += 1;
        let answers = state.exchanges[at].answers.clone();
        if matches!(state.classifier, Classifier::Hidpp) {
            hidpp_learn(state, data, &answers);
            state = self.state.lock().unwrap();
        }
        for (collection, answer) in answers {
            state.pending.entry(collection).or_default().push_back(answer);
        }
    }

    fn next(&self, buf: &mut [u8], timeout_ms: i32) -> usize {
        let answer = self.state.lock().unwrap().pending.get_mut(&self.collection).and_then(VecDeque::pop_front);
        match answer {
            Some(a) => {
                let n = a.len().min(buf.len());
                buf[..n].copy_from_slice(&a[..n]);
                n
            }
            None => {
                // Nothing waits: like a device that stays quiet, without spinning.
                std::thread::sleep(Duration::from_millis(timeout_ms.clamp(0, 2) as u64));
                0
            }
        }
    }
}

/// HID++: the feature index in each answer of the root feature, and a check that a device info
/// answer (0x0003 function 0) has its unit id left out.
fn hidpp_learn(mut state: MutexGuard<'_, State>, request: &[u8], answers: &[(String, Vec<u8>)]) {
    let (slot, index, address) = (request[1], request[2], request[3]);
    let answer =
        answers.iter().map(|(_, a)| a).find(|a| a.len() >= 9 && a[1] == slot && a[2] == index && a[3] == address);
    let Some(answer) = answer else { return };
    if index == 0 && address >> 4 == 0 && answer[4] != 0 {
        state.hidpp_index.insert(answer[4], u16::from_be_bytes([request[4], request[5]]));
    }
    let device_info = state.hidpp_index.get(&index) == Some(&crate::hidpp::feature::DEVICE_INFO);
    if device_info && address >> 4 == 0 && answer[5..9] != [0; 4] {
        fail(state, "the device info answer shows a unit id: write its bytes 1 to 4 as xx".into());
    }
}

impl Hid for ReplayHid {
    fn write(&self, data: &[u8]) -> Result<usize> {
        self.request(Op::Write, data);
        Ok(data.len())
    }
    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        Ok(self.next(buf, timeout_ms))
    }
    fn send_feature(&self, data: &[u8]) -> Result<()> {
        self.request(Op::FeatureSet, data);
        Ok(())
    }
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        let mut state = self.state.lock().unwrap();
        let id = buf[0];
        let fits_here = |f: &FeatureAnswer| f.collection == self.collection && f.report_id == id;
        let found = state
            .features
            .iter()
            .position(|f| f.used == 0 && fits_here(f))
            .or_else(|| state.features.iter().rposition(fits_here));
        let Some(at) = found else {
            fail(state, format!("feature report {id} on {} is not in the transcript", self.collection))
        };
        state.features[at].used += 1;
        let data = &state.features[at].data;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }
    fn report_descriptor(&self, buf: &mut [u8]) -> Result<usize> {
        let state = self.state.lock().unwrap();
        let d = state.descriptors.get(&self.collection).ok_or_else(|| Error::Invalid("no report descriptor".into()))?;
        let n = d.len().min(buf.len());
        buf[..n].copy_from_slice(&d[..n]);
        Ok(n)
    }
}

impl crate::hidpp::Transport for ReplayHid {
    fn write(&self, data: &[u8]) -> Result<()> {
        self.request(Op::Write, data);
        Ok(())
    }
    fn read_timeout(&self, buf: &mut [u8], millis: i32) -> Result<usize> {
        Ok(self.next(buf, millis))
    }
}

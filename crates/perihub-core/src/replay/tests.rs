//! The replay tests: every transcript in `tests/fixtures/transcripts` is played to the code that
//! reads its device, which must send only requests the transcript has, change nothing when the
//! transcript is read-only, and read what the transcript's `expect` says.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::*;
use crate::corsair::BragiDevice;
use crate::corsair::device::CorsairTask;
use crate::hidpp::receiver::UnsupportedDevice;
use crate::hidpp::{HidppDevice, Transport};
use crate::peripheral::{self, task};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcripts")
}

fn fixtures() -> Vec<(String, Transcript)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("tests/fixtures/transcripts")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&p).unwrap();
            let t = Transcript::from_json(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, t)
        })
        .collect()
}

fn fixture(name: &str) -> Transcript {
    Transcript::from_json(&std::fs::read_to_string(fixtures_dir().join(name)).unwrap()).unwrap()
}

fn backups(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("perihub-test-replay-{name}-{}", std::process::id()))
}

/// Plays `t` to the code that reads its device: what it read, as the app gets it, and the replay.
pub(crate) fn replay(t: &Transcript, name: &str) -> (Value, Replay) {
    let (vendor_id, product_id) = t.ids().unwrap();
    match t.family {
        Family::Peripheral => {
            let model = peripheral::find(vendor_id, product_id).expect("a model of a driver's table");
            let replay = Replay::new(t, Classifier::Commands(task::driver(model).commands())).unwrap();
            let short = replay.has_collection("short").then(|| Box::new(replay.collection("short")) as Box<dyn Hid>);
            // Opened as the app opens it, allowed to change things: a read-only transcript still
            // fails on the first command that would.
            let task =
                task::start_for_replay(Box::new(replay.collection(MAIN)), short, model, product_id, backups(name))
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            (serde_json::to_value(&task.info).unwrap(), replay)
        }
        Family::HidppReceiver => {
            let slot = t.slot.expect("the receiver slot");
            let replay = Replay::new(t, Classifier::Hidpp).unwrap();
            let short =
                replay.has_collection("short").then(|| Box::new(replay.collection("short")) as Box<dyn Transport>);
            let mut dev = HidppDevice::behind_receiver(replay.collection(MAIN), short, slot, true).unwrap();
            let device = UnsupportedDevice::read(&mut dev, slot).unwrap_or_else(|e| panic!("{name}: {e}"));
            (serde_json::to_value(&device).unwrap(), replay)
        }
        Family::HidppMouse => {
            let model = crate::hidpp::models::MODELS
                .iter()
                .find(|m| m.product_id == product_id)
                .expect("a model of the HID++ mouse table");
            let replay = Replay::new(t, Classifier::Hidpp).unwrap();
            let short =
                replay.has_collection("short").then(|| Box::new(replay.collection("short")) as Box<dyn Transport>);
            let main = replay.collection(MAIN);
            // Read-only until the memory format says more, as the manager opens it; the interface
            // allows experimental saves.
            let dev = match t.slot {
                Some(slot) => HidppDevice::behind_receiver(main, short, slot, true),
                None => HidppDevice::new(main, model.device_index, true),
            };
            let task = dev
                .and_then(|dev| crate::mouse::MouseTask::open(dev, model, true))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            (serde_json::to_value(&task.info).unwrap(), replay)
        }
        Family::CorsairBragi => {
            let model = crate::corsair::models::MODELS
                .iter()
                .find(|m| m.product_id == product_id)
                .expect("a model of the Corsair table");
            let replay = Replay::new(t, Classifier::Bragi).unwrap();
            let dev = BragiDevice::new(replay.collection(MAIN), model.packet);
            let task = match t.slot {
                // Behind a receiver, on that channel (the receiver's id is not in the transcript).
                Some(channel) => CorsairTask::open_behind(dev, model, 0, channel),
                None => CorsairTask::open(dev, model),
            };
            let task = task.unwrap_or_else(|e| panic!("{name}: {e}"));
            (serde_json::to_value(task.info()).unwrap(), replay)
        }
    }
}

/// Checks that `recorded`, what PeriHub records of a simulated device, is the fixture `name`, or
/// writes it there when PERIHUB_WRITE_TRANSCRIPTS is set. The device, family, access, slot and
/// events come from the recording; the rest (source, notes, expect) stays as written by hand.
pub(crate) fn recorded_fixture(name: &str, recorded: &Transcript) {
    let path = fixtures_dir().join(name);
    let fixture = std::fs::read_to_string(&path).ok().map(|text| Transcript::from_json(&text).unwrap());
    if std::env::var_os("PERIHUB_WRITE_TRANSCRIPTS").is_some() {
        let mut t = fixture.unwrap_or_else(|| recorded.clone());
        t.device = recorded.device.clone();
        t.family = recorded.family;
        t.access = recorded.access;
        t.slot = recorded.slot;
        t.events = recorded.events.clone();
        t.recorded_by = Some("PeriHub's tests, from a simulated device".into());
        std::fs::write(&path, t.to_json()).unwrap();
        return;
    }
    let fixture = fixture.unwrap_or_else(|| panic!("{name}: run with PERIHUB_WRITE_TRANSCRIPTS=1 to write it"));
    let head = |t: &Transcript| (t.device.clone(), t.family, t.access, t.slot);
    assert_eq!(head(&fixture), head(recorded), "{name}");
    let (old, new) = (fixture.to_json(), Transcript { expect: None, ..recorded.clone() }.to_json());
    assert!(
        fixture.events == recorded.events,
        "{name}: PeriHub records something else now. If that is meant, write it again with \
         PERIHUB_WRITE_TRANSCRIPTS=1 and check the difference.\nfixture: {old}\nrecorded: {new}"
    );
}

/// A Corsair device that answers every "get property" from a table, as
/// `docs/protocols/corsair-bragi.md` lays the answers out: its packets have no report number.
struct SimCorsair {
    properties: Vec<(u8, Vec<u8>)>,
    pending: Mutex<VecDeque<Vec<u8>>>,
    /// Every packet that reached it.
    received: std::sync::Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Transport for SimCorsair {
    fn write(&self, data: &[u8]) -> crate::error::Result<()> {
        // The report number 0, the marker, the command, the property.
        self.received.lock().unwrap().push(data.to_vec());
        let (command, property) = (data[2], data[3]);
        let mut reply = vec![0u8; data.len() - 1];
        reply[1] = command;
        match self.properties.iter().find(|(p, _)| *p == property) {
            Some((_, value)) if command == 0x02 => reply[3..3 + value.len()].copy_from_slice(value),
            _ => reply[2] = 0x05,
        }
        self.pending.lock().unwrap().push_back(reply);
        Ok(())
    }
    fn read_timeout(&self, buf: &mut [u8], _millis: i32) -> crate::error::Result<usize> {
        let Some(reply) = self.pending.lock().unwrap().pop_front() else { return Ok(0) };
        buf[..reply.len()].copy_from_slice(&reply);
        Ok(reply.len())
    }
}

/// The KATAR PRO replay holds what PeriHub's Corsair code records when it opens a KATAR PRO: the
/// device is simulated (no Corsair device has sent a report yet), the path is the real one.
#[test]
fn the_katar_pro_replay_is_what_the_corsair_code_records() {
    let model = crate::corsair::models::find(0x1B93).unwrap();
    let sim = SimCorsair {
        properties: vec![
            (crate::corsair::property::FIRMWARE, vec![5, 2, 0x41, 0x00]),
            (crate::corsair::property::MODE, vec![1]),
            (crate::corsair::property::POLLING, vec![4]),
            (crate::corsair::property::DPI_X, vec![0x20, 0x03]),
        ],
        pending: Mutex::new(VecDeque::new()),
        received: Default::default(),
    };
    let received = sim.received.clone();
    let task = CorsairTask::open(BragiDevice::new(sim, model.packet), model).unwrap();
    let info = task.info();
    assert_eq!(
        (info.firmware.as_deref(), info.mode, info.polling_hz, info.dpi),
        (Some("5.2.65"), Some(1), Some(1000), Some(800))
    );
    let recorded = Transcript::from_json(&info.transcript).unwrap();
    assert_eq!(recorded.access, Access::ReadOnly);
    let writes: Vec<Vec<u8>> =
        recorded.events.iter().filter(|e| e.op == Op::Write).map(|e| e.bytes().unwrap()).collect();
    assert_eq!(writes, *received.lock().unwrap(), "the recording sent nothing of its own");
    recorded_fixture("corsair-katar-pro.json", &recorded);
}

/// A HID++ request: report id, device index, feature index, function and software id, parameters.
fn hidpp(device: u8, index: u8, function: u8) -> Vec<u8> {
    let mut request = vec![0u8; 20];
    request[..4].copy_from_slice(&[0x11, device, index, function << 4 | 0x0B]);
    request
}

#[test]
fn hidpp_calls_are_told_apart_by_feature_and_function_on_a_cable_too() {
    let index: BTreeMap<u8, u16> = [(0x05, crate::hidpp::feature::ONBOARD_PROFILES)].into();
    for device in [0xFF, 0x01] {
        // Onboard profiles: memory reads read, a mode switch and memory writes change.
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x05, 5)), Some(Effect::Read));
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x05, 1)), Some(Effect::Change));
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x05, 7)), Some(Effect::Change));
        // The root feature only has its two reads.
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x00, 1)), Some(Effect::Read));
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x00, 2)), Some(Effect::Change));
        // An index the root feature never gave: not a request the code may send.
        assert_eq!(hidpp_effect(&index, &hidpp(device, 0x06, 0)), None);
    }
    // Receiver registers: 0x81 and 0x83 read, 0x80 and 0x82 write.
    let register = |sub_id: u8| [0x10, 0xFF, sub_id, 0xB5, 0x30, 0, 0];
    assert_eq!(hidpp_effect(&index, &register(0x83)), Some(Effect::Read));
    assert_eq!(hidpp_effect(&index, &register(0x80)), Some(Effect::Change));
    assert_eq!(hidpp_effect(&index, &register(0x82)), Some(Effect::Change));
}

#[test]
fn a_recording_with_a_call_nobody_can_tell_counts_as_a_change() {
    let root = |index: u8, feature: u16| {
        let mut request = hidpp(0xFF, 0, 0);
        request[4..6].copy_from_slice(&feature.to_be_bytes());
        let mut answer = request.clone();
        answer[4..6].copy_from_slice(&[index, 0]);
        [Event::new(Op::Write, MAIN, None, &request), Event::new(Op::Read, MAIN, None, &answer)]
    };
    let mut events: Vec<Event> = root(0x05, crate::hidpp::feature::ONBOARD_PROFILES).into();
    events.push(Event::new(Op::Write, MAIN, None, &hidpp(0xFF, 0x05, 4)));
    assert_eq!(access_of(&events, &Classifier::Hidpp), Access::ReadOnly);
    let mut set_mode = events.clone();
    set_mode.push(Event::new(Op::Write, MAIN, None, &hidpp(0xFF, 0x05, 1)));
    assert_eq!(access_of(&set_mode, &Classifier::Hidpp), Access::ReadWrite);
    // A feature index the recording never saw looked up.
    events.push(Event::new(Op::Write, MAIN, None, &hidpp(0xFF, 0x09, 0)));
    assert_eq!(access_of(&events, &Classifier::Hidpp), Access::ReadWrite);
    // Corsair: anything but "get property".
    let mut set = vec![0u8; 65];
    set[1..6].copy_from_slice(&[0x08, 0x01, 0x03, 0x00, 0x02]);
    assert_eq!(access_of(&[Event::new(Op::Write, MAIN, None, &set)], &Classifier::Bragi), Access::ReadWrite);
    set[2] = 0x02;
    assert_eq!(access_of(&[Event::new(Op::Write, MAIN, None, &set)], &Classifier::Bragi), Access::ReadOnly);
}

#[test]
fn hidpp_unit_ids_and_serial_numbers_are_written_as_xx() {
    let info = 0x02;
    let mut events = vec![
        Event::new(Op::Write, MAIN, None, &hidpp(0x01, info, 0)),
        Event::new(Op::Read, MAIN, None, &[0x11, 0x01, info, 0x0B, 0x02, 0xA1, 0xB2, 0xC3, 0xD4, 0x00, 0x0C]),
        Event::new(Op::Write, MAIN, None, &hidpp(0x01, info, 2)),
        Event::new(Op::Read, MAIN, None, &[0x11, 0x01, info, 0x2B, 0x31, 0x32, 0x33]),
        Event::new(Op::Write, MAIN, None, &hidpp(0x01, info, 1)),
        Event::new(Op::Read, MAIN, None, &[0x11, 0x01, info, 0x1B, 0x00, 0x42]),
    ];
    // The index was looked up before these events.
    anonymize_hidpp(&mut events, &[(info, crate::hidpp::feature::DEVICE_INFO)]);
    assert_eq!(events[1].data, "11 01 02 0b 02 xx xx xx xx 00 0c");
    assert_eq!(events[3].data, "11 01 02 2b xx xx xx");
    assert_eq!(events[5].data, "11 01 02 1b 00 42", "the firmware stays");
}

/// Where `actual` is not what `expected` says: an object may leave fields out, the rest is equal.
fn differences(at: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (key, value) in e {
                match a.get(key) {
                    Some(got) => differences(&format!("{at}.{key}"), value, got, out),
                    None => out.push(format!("{at}.{key}: not in what was read")),
                }
            }
        }
        _ if expected == actual => {}
        _ => out.push(format!("{at}: expected {expected}, read {actual}")),
    }
}

/// Transcripts that cannot replay the current code until a real capture replaces them. The G303
/// transcript was rebuilt from values, with no bytes, and has no answer for the battery read of
/// the receiver page (the G303 does list 0x1004, so the read is right). The K580 transcript has no
/// answer for the colours per key (command 10), which PeriHub 0.9.7 did not read; its other answers
/// are still checked by `peripheral::evision`'s tests. No answer is invented.
const NEEDS_CAPTURE: &[(&str, &str)] = &[
    ("logitech-g303-receiver.json", "issue #34: needs a test report from a version with the battery read"),
    ("evision-k580.json", "issue #117: needs a test report from a version that reads the colours per key"),
];

#[test]
fn every_transcript_replays_and_reads_what_it_expects() {
    let all = fixtures();
    assert!(all.len() >= 3, "the fixtures are in {}", fixtures_dir().display());
    for (name, t) in &all {
        if let Some((_, why)) = NEEDS_CAPTURE.iter().find(|(n, _)| n == name) {
            // Not skipped silently: it must still fail, so the entry goes when a real capture arrives.
            let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| replay(t, name))).is_err();
            assert!(failed, "{name} replays now: remove it from NEEDS_CAPTURE ({why})");
            continue;
        }
        let expect = t.expect.as_ref().unwrap_or_else(|| panic!("{name}: no expect, the replay would check nothing"));
        assert!(t.source.is_some(), "{name}: say where the transcript comes from");
        let (read, replay) = replay(t, name);
        let mut wrong = Vec::new();
        differences("", expect, &read, &mut wrong);
        assert!(wrong.is_empty(), "{name}:\n{}\nread: {read:#}", wrong.join("\n"));
        assert!(!replay.sent().is_empty(), "{name}: nothing was sent");
    }
}

#[test]
fn the_fixtures_hold_nothing_that_tells_a_unit_apart() {
    for (name, t) in fixtures() {
        let text = t.to_json().to_lowercase();
        for word in ["serial", "/home/", "\\users\\", "@"] {
            assert!(!text.contains(word), "{name}: {word:?}");
        }
    }
}

#[test]
fn a_transcript_survives_writing_and_reading_back() {
    for (name, t) in fixtures() {
        assert_eq!(Transcript::from_json(&t.to_json()).unwrap(), t, "{name}");
    }
}

#[test]
fn another_format_or_a_newer_version_is_refused() {
    let t = fixture("wraith-w60.json");
    let mut newer = t.clone();
    newer.version = VERSION + 1;
    assert!(Transcript::from_json(&newer.to_json()).unwrap_err().to_string().contains("version"));
    let mut other = t.clone();
    other.format = "pcapng".into();
    assert!(Transcript::from_json(&other.to_json()).is_err());
    let mut broken = t;
    broken.events[1].data = "01 0d zz".into();
    assert!(Transcript::from_json(&broken.to_json()).is_err());
}

#[test]
#[should_panic(expected = "is not in the transcript")]
fn a_request_the_transcript_does_not_have_fails_the_test() {
    let mut t = fixture("wraith-w60.json");
    // Without the side lighting's request and answer.
    let at = t.events.iter().position(|e| e.op == Op::Write && e.data.starts_with("01 08")).unwrap();
    t.events.drain(at..at + 2);
    replay(&t, "missing");
}

/// A command of a driver's list that changes something, as a test device has it.
const CHANGE: &[Command] = &[
    Command { prefix: &[0x01, 0x0D], at: 0, effect: Effect::Read, name: "read" },
    Command { prefix: &[0x01, 0x07, 0x02], at: 0, effect: Effect::Change, name: "set key lighting" },
];

#[test]
#[should_panic(expected = "the transcript is read-only")]
fn a_change_in_a_read_only_transcript_fails_the_test() {
    let mut t = fixture("wraith-w60.json");
    t.events.push(Event::new(Op::Write, MAIN, None, &[0x01, 0x07, 0x02, 0x10]));
    let replay = Replay::new(&t, Classifier::Commands(CHANGE.to_vec())).unwrap();
    Hid::write(&replay.collection(MAIN), &[0x01, 0x07, 0x02, 0x10]).unwrap();
}

#[test]
fn a_change_in_a_read_write_transcript_is_answered_like_a_read() {
    let mut t = fixture("wraith-w60.json");
    t.access = Access::ReadWrite;
    t.events.push(Event::new(Op::Write, MAIN, None, &[0x01, 0x07, 0x02, 0x10]));
    t.events.push(Event::new(Op::Read, MAIN, None, &[0x01, 0x07, 0x00]));
    let replay = Replay::new(&t, Classifier::Commands(CHANGE.to_vec())).unwrap();
    let hid = replay.collection(MAIN);
    Hid::write(&hid, &[0x01, 0x07, 0x02, 0x10]).unwrap();
    let mut buf = [0u8; 8];
    assert_eq!(Hid::read_timeout(&hid, &mut buf, 0).unwrap(), 3);
}

#[test]
#[should_panic(expected = "not a command the code may send")]
fn a_command_missing_from_the_drivers_list_fails_the_test() {
    let t = fixture("wraith-w60.json");
    let replay = Replay::new(&t, Classifier::Commands(CHANGE.to_vec())).unwrap();
    Hid::write(&replay.collection(MAIN), &[0x01, 0x55]).unwrap();
}

#[test]
#[should_panic(expected = "the transcript is read-only")]
fn a_hidpp_change_in_a_read_only_transcript_fails_the_test() {
    let t = fixture("logitech-g303-receiver.json");
    let replay = Replay::new(&t, Classifier::Hidpp).unwrap();
    let main = replay.collection(MAIN);
    let mut dev = HidppDevice::behind_receiver(main.clone(), None, 1, true).unwrap();
    // Learns where 0x8100 is, then asks for a change of onboard mode (function 1) by hand: the
    // device's own guard would never send it.
    dev.onboard_info().unwrap();
    let index = replay.sent().last().unwrap().1[2];
    let mut set_mode = [0u8; 20];
    set_mode[..5].copy_from_slice(&[0x11, 0x01, index, 0x1B, 0x02]);
    Transport::write(&main, &set_mode).unwrap();
}

#[test]
#[should_panic(expected = "shows a unit id")]
fn a_hidpp_unit_id_left_in_a_transcript_fails_the_test() {
    let mut t = fixture("logitech-g303-receiver.json");
    for e in t.events.iter_mut().filter(|e| e.data.contains(" xx xx xx xx")) {
        e.data = e.data.replace(" xx xx xx xx", " a1 b2 c3 d4");
    }
    let (_, _) = replay(&t, "unit-id");
}

#[test]
#[should_panic(expected = "the transcript is read-only")]
fn a_corsair_change_in_a_read_only_transcript_fails_the_test() {
    let t = fixture("corsair-katar-pro.json");
    let replay = Replay::new(&t, Classifier::Bragi).unwrap();
    let mut set_software = vec![0u8; 65];
    set_software[1..6].copy_from_slice(&[0x08, 0x01, 0x03, 0x00, 0x02]);
    Transport::write(&replay.collection(MAIN), &set_software).unwrap();
}

/// What the app records of a reading (here the W60's, replayed) replays the same, and the recording
/// sent nothing of its own.
#[test]
fn what_the_app_records_replays_the_same_and_the_recording_adds_nothing() {
    let t = fixture("wraith-w60.json");
    let (first, replay) = replay(&t, "record");
    let recorded = Transcript::from_json(first["transcript"].as_str().unwrap()).unwrap();
    assert_eq!(recorded.access, Access::ReadOnly);
    assert_eq!(recorded.family, Family::Peripheral);
    assert_eq!((recorded.device.vendor_id.as_str(), recorded.device.product_id.as_str()), ("2e3c", "c368"));
    let writes: Vec<Vec<u8>> =
        recorded.events.iter().filter(|e| e.op == Op::Write).map(|e| e.bytes().unwrap()).collect();
    let sent: Vec<Vec<u8>> = replay.sent().into_iter().map(|(_, d)| d).collect();
    assert_eq!(writes, sent, "the transcript has exactly what reached the device");
    let mut recorded = recorded;
    recorded.expect = t.expect.clone();
    recorded.source = Some("recorded in a test".into());
    let (second, _) = super::tests::replay(&recorded, "record-again");
    let mut wrong = Vec::new();
    differences("", t.expect.as_ref().unwrap(), &second, &mut wrong);
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn private_bytes_are_written_as_xx_in_the_answers_to_their_request() {
    let mut events = vec![
        Event::new(Op::Write, MAIN, None, &[0x00, 0x05, 0x82, 0x00]),
        Event::new(Op::Read, MAIN, None, &[0x05, 0x82, 0x41, 0x42, 0x43, 0x44, 0x00]),
        Event::new(Op::Write, MAIN, None, &[0x00, 0x05, 0x81, 0x00]),
        Event::new(Op::Read, MAIN, None, &[0x05, 0x81, 0x01, 0x02]),
    ];
    anonymize(&mut events, &[Private { request: &[0x05, 0x82], at: 0, answer: 2..6 }]);
    assert_eq!(events[1].data, "05 82 xx xx xx xx 00");
    assert_eq!(events[3].data, "05 81 01 02", "another request's answer stays");
    assert_eq!(events[1].bytes().unwrap(), [0x05, 0x82, 0, 0, 0, 0, 0]);
}

#[test]
fn zeros_at_the_end_are_left_out_of_the_text_and_come_back() {
    let mut report = vec![0x01, 0x0D];
    report.resize(64, 0);
    let e = Event::new(Op::Write, MAIN, None, &report);
    assert_eq!((e.data.as_str(), e.len), ("01 0d", Some(64)));
    assert_eq!(e.bytes().unwrap(), report);
    let short = Event::new(Op::Read, MAIN, None, &[0x01, 0x00, 0x00]);
    assert_eq!((short.data.as_str(), short.len), ("01 00 00", None));
}

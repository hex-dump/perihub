//! The replay tests: every transcript in `tests/fixtures/transcripts` is played to the code that
//! reads its device, which must send only requests the transcript has, change nothing when the
//! transcript is read-only, and read what the transcript's `expect` says.

use std::path::PathBuf;

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
fn replay(t: &Transcript, name: &str) -> (Value, Replay) {
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
        Family::CorsairBragi => {
            let model = crate::corsair::models::MODELS
                .iter()
                .find(|m| m.product_id == product_id)
                .expect("a model of the Corsair table");
            let replay = Replay::new(t, Classifier::Bragi).unwrap();
            let task = CorsairTask::open(BragiDevice::new(replay.collection(MAIN), model.packet), model)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            (serde_json::to_value(task.info()).unwrap(), replay)
        }
    }
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

#[test]
fn every_transcript_replays_and_reads_what_it_expects() {
    let all = fixtures();
    assert!(all.len() >= 3, "the fixtures are in {}", fixtures_dir().display());
    for (name, t) in &all {
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

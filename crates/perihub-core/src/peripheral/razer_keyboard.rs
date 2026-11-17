//! Razer keyboards, on their cable and through their own HyperSpeed receiver. The protocol is the
//! one of the Razer mice (`docs/protocols/razer-mice.md`); the keyboards' own facts are in
//! `docs/protocols/razer-keyboards.md`, and per model in [`super::razer_keyboard_models`], written by
//! `tools/openrazer_facts.py` from facts found in OpenRazer (GPL-2.0): no code was copied. Nobody has
//! tried one with PeriHub yet.
//!
//! Only read for now: firmware, layout, and where the keyboard has them, battery, charging,
//! brightness and game mode. The reports go to the USB interface OpenRazer sends to (1, 2 or 3). On
//! Linux 70-perihub.rules opens it only where a tester showed it carries no typed keys (the
//! BlackWidow V4 Mini HyperSpeed's interface 3), and a keyboard node that carries typed keys is never
//! opened anyway (see [`super::node_carries_typing`]).

use super::razer::exchange;
use super::task::Driver;
use super::{Command, Effect, Io, Reading, Settings};
use crate::error::{Error, Result};

const VARSTORE: u8 = 0x01;
/// The LED id of the keyboard's backlight, and of the game mode light.
const BACKLIGHT_LED: u8 = 0x05;
const GAME_LED: u8 = 0x08;

/// (class, id) of the commands, at bytes 6 and 7 of the report.
const GET_FIRMWARE: [u8; 2] = [0x00, 0x81];
const GET_BATTERY: [u8; 2] = [0x07, 0x80];
const GET_CHARGING: [u8; 2] = [0x07, 0x84];
const GET_BRIGHTNESS: [u8; 2] = [0x0F, 0x84];
/// The standard brightness read of the older keyboards.
const GET_STANDARD_BRIGHTNESS: [u8; 2] = [0x03, 0x83];
const GET_LED_STATE: [u8; 2] = [0x03, 0x80];
/// The keyboard's region, so its physical layout (see [`crate::kblayout::razer`]).
const GET_LAYOUT: [u8; 2] = [0x00, 0x86];

/// Everything PeriHub may send to a Razer keyboard: reads only, the lights' with the stored
/// values and the one LED each.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &GET_FIRMWARE, at: 6, effect: Effect::Read, name: "read firmware" },
    Command { prefix: &GET_BATTERY, at: 6, effect: Effect::Read, name: "read battery" },
    Command { prefix: &GET_CHARGING, at: 6, effect: Effect::Read, name: "read charging" },
    Command {
        prefix: &[0x0F, 0x84, VARSTORE, BACKLIGHT_LED],
        at: 6,
        effect: Effect::Read,
        name: "read backlight brightness",
    },
    Command {
        prefix: &[0x03, 0x83, VARSTORE, BACKLIGHT_LED],
        at: 6,
        effect: Effect::Read,
        name: "read backlight brightness",
    },
    Command { prefix: &[0x03, 0x80, VARSTORE, GAME_LED], at: 6, effect: Effect::Read, name: "read game mode" },
    Command { prefix: &GET_LAYOUT, at: 6, effect: Effect::Read, name: "read layout" },
];

/// What a keyboard needs, from OpenRazer (written by `tools/openrazer_facts.py --keyboards` into
/// [`super::razer_keyboard_models`]): how long it takes to answer, and the transaction id of each read
/// it takes. The firmware and the layout are read with transaction id `ff` on every keyboard.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub wait_us: u32,
    /// The battery and charging reads, on the keyboards with a battery.
    pub battery: Option<u8>,
    /// The backlight brightness read.
    pub brightness: Option<Brightness>,
    /// The game mode read.
    pub game: Option<u8>,
    /// 70-perihub.rules opens its interface on Linux: only where a tester showed that the interface
    /// carries no typed keys (on most Razer keyboards the one OpenRazer sends to carries them).
    pub linux: bool,
}

/// How a keyboard reads its backlight brightness: with the extended command (`0f 84`) or the
/// standard one (`03 83`), and its transaction id.
#[derive(Debug, Clone, Copy)]
pub struct Brightness {
    pub standard: bool,
    pub tid: u8,
}

/// The transaction id of the firmware and layout reads on every keyboard.
const TID_FF: u8 = 0xFF;

/// 0–255 in percent.
fn percent(raw: u8) -> u8 {
    ((raw as u32 * 100 + 127) / 255) as u8
}

pub struct RazerKeyboard {
    p: Params,
}

impl RazerKeyboard {
    pub fn new(p: Params) -> Self {
        Self { p }
    }

    fn query(&self, io: &mut Io, tid: u8, command: [u8; 2], size: u8, args: &[u8]) -> Result<[u8; 80]> {
        exchange(io, self.p.wait_us, tid, command, size, args)
    }
}

impl Driver for RazerKeyboard {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "OpenRazer (facts only)"
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
        let p = self.p;
        // For the test report: whether the interface declares the 90-byte feature report.
        let _ = io.report_descriptor();
        let fw = match self.query(io, TID_FF, GET_FIRMWARE, 0x02, &[]) {
            Ok(fw) => fw,
            // Off, asleep or away from the receiver, or a status the keyboard was not expected to
            // give: the page and its test report show it, and it is read again every few seconds.
            Err(e @ (Error::Timeout | Error::Protocol(_))) => {
                return Ok(Reading {
                    details: vec![(
                        "Status".into(),
                        format!("no answer yet ({e}): if the keyboard is on, please send a test report"),
                    )],
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };
        let mut problems = Vec::new();
        let mut details = Vec::new();
        let battery = match p.battery {
            Some(tid) => {
                let level = self.query(io, tid, GET_BATTERY, 0x02, &[])?[1];
                let charging = self.query(io, tid, GET_CHARGING, 0x02, &[])?[1];
                if charging > 1 {
                    problems.push(format!("charging state {charging:02x}"));
                }
                Some((percent(level), charging == 1))
            }
            None => None,
        };
        // A keyboard that does not take one of the light reads is shown without it.
        if let Some(b) = p.brightness {
            let command = if b.standard { GET_STANDARD_BRIGHTNESS } else { GET_BRIGHTNESS };
            match self.query(io, b.tid, command, 0x03, &[VARSTORE, BACKLIGHT_LED]) {
                Ok(a) if a[1] == BACKLIGHT_LED => {
                    details.push(("Backlight brightness".into(), format!("{} %", percent(a[2]))))
                }
                Ok(a) => problems.push(format!("brightness answered for LED {:02x}", a[1])),
                Err(Error::Invalid(_)) => {
                    details.push(("Backlight brightness".into(), "not read: not supported".into()))
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(tid) = p.game {
            match self.query(io, tid, GET_LED_STATE, 0x03, &[VARSTORE, GAME_LED]) {
                Ok(a) if a[1] == GAME_LED && a[2] <= 1 => {
                    details.push(("Game mode".into(), if a[2] == 1 { "on" } else { "off" }.into()))
                }
                Ok(a) => problems.push(format!("game mode answered {:02x} {:02x}", a[1], a[2])),
                Err(Error::Invalid(_)) => details.push(("Game mode".into(), "not read: not supported".into())),
                Err(e) => return Err(e),
            }
        }
        // Its layout, with the firmware's transaction id as OpenRazer sends it. A keyboard that does
        // not take it leaves the layout to the system.
        let layout = match self.query(io, TID_FF, GET_LAYOUT, 0x02, &[]) {
            Ok(a) => Some(crate::kblayout::razer(a[0])),
            Err(Error::Invalid(_)) => None,
            Err(e) => return Err(e),
        };
        Ok(Reading {
            firmware: Some(format!("v{}.{}", fw[0], fw[1])),
            layout,
            battery,
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
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::razer::request;
    use super::super::task::start_for_test;
    use super::super::{Kind, Protocol, find, models, opened_on_linux};
    use super::*;

    /// A simulated V4 Mini: battery 0xcc (80 %), charging, brightness 0x80, game mode on. Records
    /// every request; `status` is the one it answers with.
    fn fake(sent: Arc<Mutex<Vec<Vec<u8>>>>, status: u8, wireless: bool) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |req: &[u8]| {
            sent.lock().unwrap().push(req.to_vec());
            let tid = req[1];
            let mut a = req.to_vec();
            a[0] = status;
            let battery_tid = if wireless { 0x9F } else { 0x1F };
            match ([req[6], req[7]], tid) {
                (GET_FIRMWARE, 0xFF) => a[8..10].copy_from_slice(&[1, 3]),
                (GET_BATTERY, t) if t == battery_tid => a[9] = 0xCC,
                (GET_CHARGING, t) if t == battery_tid => a[9] = 1,
                (GET_BRIGHTNESS, t) if t == battery_tid => a[10] = 0x80,
                (GET_LED_STATE, 0xFF) => a[10] = 1,
                (GET_LAYOUT, 0xFF) => a[8] = 0x11,
                _ => a[0] = 0x03,
            }
            a
        }));
        hid
    }

    fn params(id: u16) -> Params {
        match find(0x1532, id).unwrap().protocol {
            Protocol::RazerKeyboard(p) => p,
            _ => panic!("{id:04x} is not a Razer keyboard"),
        }
    }

    #[test]
    fn every_razer_keyboard_is_a_keyboard_on_the_interface_openrazer_names_and_only_read() {
        let keyboards: Vec<_> = models().filter(|m| matches!(m.protocol, Protocol::RazerKeyboard(_))).collect();
        assert!(keyboards.len() >= 60, "{}", keyboards.len());
        for m in keyboards {
            assert_eq!(m.kind, Kind::Keyboard, "{}", m.name);
            assert!(matches!(m.interface, Some(1..=3)), "{}", m.name);
            assert_eq!(m.product_ids.len(), 1, "{}", m.name);
            // No mouse, receiver or other device of the tables has the same id.
            assert!(std::ptr::eq(find(0x1532, m.product_ids[0]).unwrap(), m), "{}", m.name);
            // Linux: only where the interface was seen to carry no typed keys.
            assert_eq!(opened_on_linux(m), matches!(m.product_ids[0], 0x02B9 | 0x02BA), "{}", m.name);
        }
        assert!(COMMANDS.iter().all(|c| c.effect == Effect::Read));
    }

    #[test]
    fn the_facts_of_a_few_keyboards_are_the_ones_openrazer_gives() {
        // BlackWidow V3: interface 3, extended brightness with 3f, no battery.
        let p = params(0x024E);
        assert_eq!(find(0x1532, 0x024E).unwrap().interface, Some(3));
        assert!(matches!(p.brightness, Some(Brightness { standard: false, tid: 0x3F })));
        assert_eq!((p.battery, p.game, p.wait_us), (None, Some(0xFF), 600));
        // BlackWidow Chroma: interface 1, the standard brightness read.
        assert_eq!(find(0x1532, 0x0203).unwrap().interface, Some(1));
        assert!(matches!(params(0x0203).brightness, Some(Brightness { standard: true, tid: 0xFF })));
        // Huntsman V2: its game mode is read from another storage, which PeriHub does not send.
        assert_eq!(params(0x026C).game, None);
        // DeathStalker V2 Pro through its receiver: interface 2, 4.9 ms, battery with 9f.
        let p = params(0x0290);
        assert_eq!(find(0x1532, 0x0290).unwrap().interface, Some(2));
        assert_eq!((p.battery, p.wait_us), (Some(0x9F), 4900));
        // The BlackWidow V4 Mini HyperSpeed as before.
        let (wired, wireless) = (params(0x02B9), params(0x02BA));
        assert_eq!((wired.battery, wired.wait_us, wired.linux), (Some(0x1F), 600, true));
        assert_eq!((wireless.battery, wireless.wait_us, wireless.linux), (Some(0x9F), 4900, true));
    }

    /// A simulated BlackWidow Chroma: no battery, the standard brightness read (0x40), game mode off.
    fn chroma(sent: Arc<Mutex<Vec<Vec<u8>>>>) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |req: &[u8]| {
            sent.lock().unwrap().push(req.to_vec());
            let mut a = req.to_vec();
            a[0] = 0x02;
            match ([req[6], req[7]], req[1]) {
                (GET_FIRMWARE, 0xFF) => a[8..10].copy_from_slice(&[2, 4]),
                (GET_STANDARD_BRIGHTNESS, 0xFF) => a[10] = 0x40,
                (GET_LED_STATE, 0xFF) => a[10] = 0,
                (GET_LAYOUT, 0xFF) => a[8] = 0x01,
                _ => a[0] = 0x05,
            }
            a
        }));
        hid
    }

    #[test]
    fn a_wired_keyboard_is_not_asked_for_a_battery_and_reads_the_standard_brightness() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let model = find(0x1532, 0x0203).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-razer-kb-chroma-{}", std::process::id()));
        let task = start_for_test(Box::new(chroma(sent.clone())), model, 0x0203, true, backups).unwrap();
        let info = &task.info;
        assert_eq!(info.firmware.as_deref(), Some("v2.4"));
        assert!(info.battery.is_none());
        assert!(info.details.contains(&("Backlight brightness".into(), "25 %".into())), "{:?}", info.details);
        assert!(info.details.contains(&("Game mode".into(), "off".into())));
        assert_eq!(info.problem, None);
        assert!(!info.editable);
        let sent = sent.lock().unwrap();
        assert!(
            !sent
                .iter()
                .any(|r| [r[6], r[7]] == GET_BATTERY || [r[6], r[7]] == GET_CHARGING || [r[6], r[7]] == GET_BRIGHTNESS)
        );
        for req in sent.iter() {
            assert!(COMMANDS.iter().any(|c| req[6..].starts_with(c.prefix)), "{req:02x?}");
        }
    }

    #[test]
    fn a_keyboard_without_a_game_mode_read_is_not_asked_for_it() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let model = find(0x1532, 0x026C).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-razer-kb-huntsman-{}", std::process::id()));
        let task = start_for_test(Box::new(fake(sent.clone(), 0x02, false)), model, 0x026C, true, backups).unwrap();
        assert!(!task.info.details.iter().any(|(k, _)| k == "Game mode"), "{:?}", task.info.details);
        assert!(!sent.lock().unwrap().iter().any(|r| [r[6], r[7]] == GET_LED_STATE));
    }

    #[test]
    fn both_ids_are_keyboards_on_interface_3() {
        for id in [0x02B9, 0x02BA] {
            let m = find(0x1532, id).unwrap();
            assert_eq!(m.kind, Kind::Keyboard);
            assert_eq!(m.interface, Some(3));
            assert!(matches!(m.protocol, Protocol::RazerKeyboard(_)));
        }
    }

    #[test]
    fn the_wireless_keyboard_reads_as_documented_and_only_reads() {
        for (id, wireless) in [(0x02B9, false), (0x02BA, true)] {
            let sent = Arc::new(Mutex::new(Vec::new()));
            let model = find(0x1532, id).unwrap();
            let backups = std::env::temp_dir().join(format!("perihub-test-razer-kb-{id:04x}-{}", std::process::id()));
            let task = start_for_test(Box::new(fake(sent.clone(), 0x02, wireless)), model, id, true, backups).unwrap();
            let info = &task.info;
            assert_eq!(info.firmware.as_deref(), Some("v1.3"));
            let battery = info.battery.unwrap();
            assert_eq!((battery.level, battery.charging), (80, true));
            assert!(info.details.contains(&("Backlight brightness".into(), "50 %".into())), "{:?}", info.details);
            assert!(info.details.contains(&("Game mode".into(), "on".into())));
            let layout = info.layout.as_ref().unwrap();
            assert_eq!((layout.region, layout.physical), (Some("Italian"), Some(crate::kblayout::Physical::Iso)));
            assert_eq!(info.problem, None);
            assert!(info.limits == Default::default(), "nothing may be changed");
            assert!(!info.editable);
            for req in sent.lock().unwrap().iter() {
                assert!(COMMANDS.iter().any(|c| req[6..].starts_with(c.prefix)), "{req:02x?}");
                let expected = request(req[1], [req[6], req[7]], req[5], &req[8..88]);
                assert_eq!(req[..], expected[..], "checksum");
            }
        }
    }

    #[test]
    fn a_keyboard_away_from_its_receiver_gets_its_page_and_test_report() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let model = find(0x1532, 0x02BA).unwrap();
        let backups = std::env::temp_dir().join(format!("perihub-test-razer-kb-away-{}", std::process::id()));
        let hid = fake(sent, 0x04, true);
        hid.0.lock().unwrap().descriptor = Some(vec![0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0xC0]);
        let mut task = start_for_test(Box::new(hid), model, 0x02BA, true, backups).unwrap();
        assert!(task.info.details[0].1.starts_with("no answer yet"), "{:?}", task.info.details);
        assert_eq!(task.info.problem, None);
        let raw = task.info().unwrap().raw;
        assert!(raw.iter().any(|l| l.starts_with("report descriptor")), "kept after a refresh: {raw:?}");
    }
}

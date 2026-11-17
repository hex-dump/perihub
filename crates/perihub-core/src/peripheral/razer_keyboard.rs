//! Razer keyboards: for now the BlackWidow V4 Mini HyperSpeed, on its cable and through its
//! HyperSpeed receiver. The protocol is the one of the Razer mice (`docs/protocols/razer-mice.md`);
//! the keyboard's own facts are in `docs/protocols/razer-keyboards.md`, written from facts found in
//! OpenRazer (GPL-2.0): no code was copied. Nobody has tried it with PeriHub yet.
//!
//! Only read for now: firmware, battery, charging, brightness and game mode. The reports go to USB
//! interface 3, which through the receiver holds only the media keys. On Linux a keyboard node that
//! carries typed keys is never opened (see [`super::node_carries_typing`]).

use super::razer::exchange;
use super::task::Driver;
use super::{Command, Effect, Io, Kind, Model, Protocol, Reading, Settings};
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
    Command { prefix: &[0x03, 0x80, VARSTORE, GAME_LED], at: 6, effect: Effect::Read, name: "read game mode" },
    Command { prefix: &GET_LAYOUT, at: 6, effect: Effect::Read, name: "read layout" },
];

/// What a keyboard needs, from OpenRazer: how long it takes to answer and the transaction id of
/// each command.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub wait_us: u32,
    pub tid_firmware: u8,
    pub tid_battery: u8,
    pub tid_brightness: u8,
    pub tid_game: u8,
}

/// On its cable: OpenRazer waits 600 µs and sends battery and brightness with 0x1f.
const WIRED: Params =
    Params { wait_us: 600, tid_firmware: 0xFF, tid_battery: 0x1F, tid_brightness: 0x1F, tid_game: 0xFF };
/// Through the HyperSpeed receiver: 4.9 ms, and 0x9f.
const WIRELESS: Params =
    Params { wait_us: 4900, tid_firmware: 0xFF, tid_battery: 0x9F, tid_brightness: 0x9F, tid_game: 0xFF };

pub const MODELS: &[Model] = &[
    Model {
        vendor_id: 0x1532,
        product_ids: &[0x02B9],
        brand: "Razer",
        name: "BlackWidow V4 Mini HyperSpeed",
        kind: Kind::Keyboard,
        interface: Some(3),
        usage_page: None,
        protocol: Protocol::RazerKeyboard(WIRED),
    },
    Model {
        vendor_id: 0x1532,
        product_ids: &[0x02BA],
        brand: "Razer",
        name: "BlackWidow V4 Mini HyperSpeed (wireless)",
        kind: Kind::Keyboard,
        interface: Some(3),
        usage_page: None,
        protocol: Protocol::RazerKeyboard(WIRELESS),
    },
];

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
        // For the test report: whether interface 3 declares the 90-byte feature report.
        let _ = io.report_descriptor();
        let fw = match self.query(io, p.tid_firmware, GET_FIRMWARE, 0x02, &[]) {
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
        let level = self.query(io, p.tid_battery, GET_BATTERY, 0x02, &[])?[1];
        let charging = self.query(io, p.tid_battery, GET_CHARGING, 0x02, &[])?[1];
        if charging > 1 {
            problems.push(format!("charging state {charging:02x}"));
        }
        // A keyboard that does not take one of the light reads is shown without it.
        match self.query(io, p.tid_brightness, GET_BRIGHTNESS, 0x03, &[VARSTORE, BACKLIGHT_LED]) {
            Ok(a) if a[1] == BACKLIGHT_LED => {
                details.push(("Backlight brightness".into(), format!("{} %", percent(a[2]))))
            }
            Ok(a) => problems.push(format!("brightness answered for LED {:02x}", a[1])),
            Err(Error::Invalid(_)) => details.push(("Backlight brightness".into(), "not read: not supported".into())),
            Err(e) => return Err(e),
        }
        match self.query(io, p.tid_game, GET_LED_STATE, 0x03, &[VARSTORE, GAME_LED]) {
            Ok(a) if a[1] == GAME_LED && a[2] <= 1 => {
                details.push(("Game mode".into(), if a[2] == 1 { "on" } else { "off" }.into()))
            }
            Ok(a) => problems.push(format!("game mode answered {:02x} {:02x}", a[1], a[2])),
            Err(Error::Invalid(_)) => details.push(("Game mode".into(), "not read: not supported".into())),
            Err(e) => return Err(e),
        }
        // Its layout, with the firmware's transaction id as OpenRazer sends it. A keyboard that does
        // not take it leaves the layout to the system.
        let layout = match self.query(io, p.tid_firmware, GET_LAYOUT, 0x02, &[]) {
            Ok(a) => Some(crate::kblayout::razer(a[0])),
            Err(Error::Invalid(_)) => None,
            Err(e) => return Err(e),
        };
        Ok(Reading {
            firmware: Some(format!("v{}.{}", fw[0], fw[1])),
            layout,
            battery: Some((percent(level), charging == 1)),
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
    use super::super::{Kind, find};
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

//! Roccat Kone Pure, Kone XTD and Kone EMP, read only. The protocol is in
//! `docs/protocols/roccat-kone.md`, from libratbag's Roccat drivers (MIT, see
//! `THIRD-PARTY-NOTICES.md`); the interface from the Linux kernel and `lsusb` dumps. None has been
//! tried with PeriHub, and the sources disagree on what a write needs, so PeriHub writes nothing:
//! the only report it sends chooses which profile the next read returns.

use std::time::Duration;

use super::task::Driver;
use super::{Command, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

/// The boot mouse interface, where the configuration is (kernel, lsusb dumps).
const INTERFACE: i32 = 0;

const CONFIGURE: u8 = 0x04;
const PROFILE: u8 = 0x05;
const SETTINGS: u8 = 0x06;
/// `04 profile 80`: the next read of report 6 gives that profile's settings.
const SELECT_SETTINGS: u8 = 0x80;
const PROFILES: u8 = 5;
const DPI_SLOTS: usize = 5;
const POLLING: [u16; 4] = [125, 250, 500, 1000];
const READ_BUF: usize = 256;

const READY: u8 = 0x01;
const BUSY: u8 = 0x03;
const READY_TRIES: usize = 10;
const READY_WAIT: Duration = Duration::from_millis(10);
const BUSY_WAIT: Duration = Duration::from_millis(100);

/// The only report PeriHub sends: which profile's settings the next read returns. It changes no
/// setting.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &[CONFIGURE, 0, SELECT_SETTINGS], at: 0, effect: Effect::Read, name: "choose profile 1" },
    Command { prefix: &[CONFIGURE, 1, SELECT_SETTINGS], at: 0, effect: Effect::Read, name: "choose profile 2" },
    Command { prefix: &[CONFIGURE, 2, SELECT_SETTINGS], at: 0, effect: Effect::Read, name: "choose profile 3" },
    Command { prefix: &[CONFIGURE, 3, SELECT_SETTINGS], at: 0, effect: Effect::Read, name: "choose profile 4" },
    Command { prefix: &[CONFIGURE, 4, SELECT_SETTINGS], at: 0, effect: Effect::Read, name: "choose profile 5" },
];

/// Where a model keeps its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// Length of the settings report, report number included.
    len: usize,
    /// Byte 1 must hold this, where a source says what it holds.
    tag: Option<u8>,
    /// The last two bytes are a checksum.
    checksum: bool,
    xres: usize,
    current: usize,
    rate: usize,
    /// DPI = raw × `scale` + `offset`.
    scale: u16,
    offset: u16,
}

const KONE_PURE: Params =
    Params { len: 31, tag: None, checksum: false, xres: 7, current: 12, rate: 19, scale: 50, offset: 0 };
const KONE_XTD: Params =
    Params { len: 43, tag: Some(43), checksum: true, xres: 7, current: 12, rate: 19, scale: 50, offset: 0 };
const KONE_EMP: Params =
    Params { len: 41, tag: Some(0x29), checksum: true, xres: 7, current: 17, rate: 18, scale: 100, offset: 100 };

const DPI_MASK: usize = 6;

const fn roccat(product_ids: &'static [u16], name: &'static str, params: Params) -> Model {
    Model {
        vendor_id: 0x1E7D,
        product_ids,
        brand: "Roccat",
        name,
        kind: Kind::Mouse,
        interface: Some(INTERFACE),
        usage_page: None,
        protocol: Protocol::Roccat(params),
    }
}

/// From libratbag's `data/devices/roccat-kone-*.device`, without `2dcb` (its interfaces differ, and no
/// source says where its configuration is).
pub const MODELS: &[Model] = &[
    roccat(&[0x2DBE, 0x2DC2], "Kone Pure", KONE_PURE),
    roccat(&[0x2E22], "Kone XTD", KONE_XTD),
    roccat(&[0x2E24], "Kone EMP", KONE_EMP),
];

fn checksum_ok(report: &[u8]) -> bool {
    let n = report.len();
    let sum = report[..n - 2].iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
    sum == u16::from_le_bytes([report[n - 2], report[n - 1]])
}

pub struct Roccat {
    p: Params,
}

impl Roccat {
    pub fn new(p: Params) -> Self {
        Self { p }
    }

    fn read_report(io: &mut Io, id: u8, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; READ_BUF];
        let n = io.read_numbered(id, &mut buf)?;
        if n < len || buf[0] != id {
            return Err(Error::Protocol(format!("report {id:02x}: {n} bytes")));
        }
        buf.truncate(len);
        Ok(buf)
    }

    /// Waits until the mouse says it is ready after a report was sent.
    fn wait_ready(io: &mut Io) -> Result<()> {
        std::thread::sleep(READY_WAIT);
        for _ in 0..READY_TRIES {
            let status = Self::read_report(io, CONFIGURE, 3)?;
            match status[1] {
                READY => return Ok(()),
                BUSY => std::thread::sleep(BUSY_WAIT),
                0x02 => return Err(Error::Protocol("the mouse reports an error".into())),
                _ => std::thread::sleep(READY_WAIT),
            }
        }
        Err(Error::Timeout)
    }

    fn settings(&self, io: &mut Io, profile: u8) -> Result<Vec<u8>> {
        io.send_numbered(&[CONFIGURE, profile, SELECT_SETTINGS])?;
        Self::wait_ready(io)?;
        let report = Self::read_report(io, SETTINGS, self.p.len)?;
        if report[2] != profile {
            return Err(Error::Protocol(format!("settings of profile {} instead of {}", report[2] + 1, profile + 1)));
        }
        if self.p.tag.is_some_and(|tag| report[1] != tag) {
            return Err(Error::Protocol(format!("settings tagged {:02x}", report[1])));
        }
        if self.p.checksum && !checksum_ok(&report) {
            return Err(Error::Protocol("settings with a wrong checksum".into()));
        }
        Ok(report)
    }
}

impl Driver for Roccat {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "libratbag (MIT)"
    }

    fn profiles(&self) -> u8 {
        PROFILES
    }

    fn can_save(&self) -> bool {
        false
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        // Read only: nothing is changed, whatever was read.
        io.forbid_changes();
        let problem =
            |e: Error| Reading { problem: Some(format!("not read as documented: {e}")), ..Default::default() };
        let profile = match Self::read_report(io, PROFILE, 3) {
            Ok(r) if r[2] < PROFILES => r[2],
            Ok(r) => return Ok(problem(Error::Protocol(format!("profile {} in use", r[2] as u16 + 1)))),
            Err(e @ (Error::Hid(_) | Error::Protocol(_))) => return Ok(problem(e)),
            Err(e) => return Err(e),
        };
        let mut reading = Reading { active_profile: Some(profile + 1), ..Default::default() };
        let report = match self.settings(io, profile) {
            Ok(report) => report,
            Err(e @ (Error::Hid(_) | Error::Protocol(_) | Error::Timeout)) => {
                reading.problem = Some(format!("the settings were not read as documented: {e}"));
                return Ok(reading);
            }
            Err(e) => return Err(e),
        };
        let p = self.p;
        let mut problems = Vec::new();
        let on: Vec<usize> = (0..DPI_SLOTS).filter(|i| report[DPI_MASK] & (1 << i) != 0).collect();
        let mut settings = Settings::default();
        for &slot in &on {
            settings.dpi_levels.push(report[p.xres + slot] as u16 * p.scale + p.offset);
        }
        if on.is_empty() {
            problems.push("no DPI slot on".to_string());
        }
        let current = report[p.current] as usize;
        reading.active_dpi = on.iter().position(|&s| s == current).map(|i| i as u8 + 1);
        match POLLING.get(report[p.rate] as usize) {
            Some(&hz) => settings.polling_hz = Some(hz),
            None => problems.push(format!("polling rate code {:02x}", report[p.rate])),
        }
        reading.settings = settings;
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected settings: {}", problems.join(", ")));
        }
        Ok(reading)
    }

    fn apply(&mut self, _io: &mut Io, _from: &Settings, _to: &Settings) -> Result<()> {
        Err(Error::Invalid("PeriHub only reads this mouse".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("PeriHub only reads this mouse".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// A simulated Roccat mouse: its settings per profile, the profile in use, the one chosen for
    /// the next read, and every report it got.
    struct Mouse {
        settings: Vec<Vec<u8>>,
        profile: u8,
        chosen: Option<u8>,
        busy: usize,
        sent: Vec<Vec<u8>>,
    }

    type Shared = Arc<Mutex<Mouse>>;

    fn with_checksum(mut r: Vec<u8>) -> Vec<u8> {
        let n = r.len();
        let sum = r[..n - 2].iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
        r[n - 2..].copy_from_slice(&sum.to_le_bytes());
        r
    }

    /// Settings of `profile`: slots 1, 2 and 4 on, slot 2 in use, 1000 Hz.
    fn settings(p: Params, profile: u8, raw: [u8; 5]) -> Vec<u8> {
        let mut r = vec![0u8; p.len];
        r[0] = SETTINGS;
        r[1] = p.tag.unwrap_or(0x2B);
        r[2] = profile;
        r[DPI_MASK] = 0b01011;
        r[p.xres..p.xres + 5].copy_from_slice(&raw);
        r[p.current] = 1;
        r[p.rate] = 3;
        if p.checksum { with_checksum(r) } else { r }
    }

    fn mouse(p: Params, raw: [u8; 5]) -> Shared {
        Arc::new(Mutex::new(Mouse {
            settings: (0..PROFILES).map(|n| settings(p, n, raw)).collect(),
            profile: 2,
            chosen: None,
            busy: 1,
            sent: Vec::new(),
        }))
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        let (set, get) = (state.clone(), state);
        hid.0.lock().unwrap().numbered = Some((
            Box::new(move |data: &[u8]| {
                let mut m = set.lock().unwrap();
                m.sent.push(data.to_vec());
                assert_eq!((data[0], data.len(), data[2]), (CONFIGURE, 3, SELECT_SETTINGS));
                m.chosen = Some(data[1]);
                true
            }),
            Box::new(move |id: u8| {
                let mut m = get.lock().unwrap();
                match id {
                    PROFILE => Some(vec![PROFILE, 3, m.profile]),
                    CONFIGURE => {
                        let status = if m.busy > 0 { BUSY } else { READY };
                        m.busy = m.busy.saturating_sub(1);
                        Some(vec![CONFIGURE, status, 0])
                    }
                    SETTINGS => m.chosen.map(|p| m.settings[p as usize].clone()),
                    _ => None,
                }
            }),
        ));
        hid
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-roccat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn each_model_reads_the_profile_in_use_and_only_chooses_it() {
        for (id, p, raw, dpi) in [
            (0x2DBE, KONE_PURE, [8, 16, 0, 64, 0], [400, 800, 3200]),
            (0x2E22, KONE_XTD, [8, 16, 0, 64, 0], [400, 800, 3200]),
            (0x2E24, KONE_EMP, [3, 7, 0, 31, 0], [400, 800, 3200]),
        ] {
            let state = mouse(p, raw);
            let model = find(0x1E7D, id).unwrap();
            let task = start_for_test(Box::new(fake(state.clone())), model, id, true, temp("read")).unwrap();
            assert_eq!(task.info.active_profile, Some(3), "{id:04x}");
            assert_eq!(task.info.settings.dpi_levels, dpi, "{id:04x}");
            assert_eq!(task.info.active_dpi, Some(2), "{id:04x}");
            assert_eq!(task.info.settings.polling_hz, Some(1000), "{id:04x}");
            assert!(task.info.problem.is_none(), "{id:04x}: {:?}", task.info.problem);
            assert!(!task.info.editable, "{id:04x}: read only");
            assert!(state.lock().unwrap().sent.iter().all(|r| *r == [CONFIGURE, 2, SELECT_SETTINGS]));
        }
    }

    #[test]
    fn answers_that_do_not_look_as_documented_are_shown_as_problems() {
        type Damage = fn(&mut Mouse);
        let cases: [(&str, Damage); 4] = [
            ("wrong checksum", |m| m.settings[2][20] ^= 1),
            ("another profile's settings", |m| m.settings[2] = m.settings[1].clone()),
            ("profile 6 in use", |m| m.profile = 5),
            ("polling code 4", |m| {
                m.settings[2][KONE_XTD.rate] = 4;
                m.settings[2] = with_checksum(m.settings[2].clone());
            }),
        ];
        for (name, damage) in cases {
            let state = mouse(KONE_XTD, [8, 16, 0, 64, 0]);
            damage(&mut state.lock().unwrap());
            let task =
                start_for_test(Box::new(fake(state.clone())), find(0x1E7D, 0x2E22).unwrap(), 0x2E22, true, temp("odd"))
                    .unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
        }
    }

    #[test]
    fn nothing_is_ever_changed() {
        let state = mouse(KONE_EMP, [3, 7, 0, 31, 0]);
        let mut task =
            start_for_test(Box::new(fake(state.clone())), find(0x1E7D, 0x2E24).unwrap(), 0x2E24, true, temp("change"))
                .unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(500);
        assert!(task.change(&wanted, false, None).is_err());
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), true);
        for report in [&[CONFIGURE, 0, 0x90][..], &[SETTINGS, 0x29, 0], &[0x07, 0x47, 0], &[PROFILE, 3, 1]] {
            assert!(io.send_numbered(report).is_err(), "{report:02x?}");
        }
        assert!(state.lock().unwrap().sent.iter().all(|r| r[0] == CONFIGURE && r[2] == SELECT_SETTINGS));
    }

    #[test]
    fn the_2dcb_is_left_out() {
        assert!(find(0x1E7D, 0x2DCB).is_none());
    }
}

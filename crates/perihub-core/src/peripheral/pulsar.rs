//! Pulsar mice on their USB cable that speak the Sonix protocol: Xlite, X2, X2H, X2A, Xlite v4, X3
//! and X3 LHD. The protocol is in `docs/protocols/pulsar-sonix.md`, from pulsar-mouse-linux (MIT, see
//! `THIRD-PARTY-NOTICES.md`), a recent project built from USB captures. None has been tried with
//! PeriHub.
//!
//! Every write is stored by the mouse: like the Razer mice, every change is one that stays (No
//! warranty box, backup, read-back and undo). PeriHub changes the global settings and the DPI of
//! the stages of the profile in use, never the profile itself, the buttons or a reset.

use std::cell::Cell;
use std::time::Duration;

use super::task::Driver;
use super::{Choice, Command, DpiLimits, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

/// Interface 3 holds the configuration (feature report 0).
const INTERFACE: i32 = 3;
const REPORT: usize = 64;
/// The mouse answers a write after about 28 ms; feature report 0 is read this often, five times.
const POLL: Duration = Duration::from_millis(30);
/// Byte 0 of an answer: the request was not taken, or it was answered.
const NOT_TAKEN: u8 = 0x00;
const ANSWERED: u8 = 0x01;
/// Bit 7 of the register asks to read it.
const READ: u8 = 0x80;
/// Right after its cable shows up a mouse can drop requests (byte 0 stays `00`) for about a second:
/// until the first answer, a dropped read is sent again, this many times, this far apart. A write
/// is never sent twice.
const RESENDS: u8 = 6;
const RESEND_WAIT: Duration = Duration::from_millis(100);

type Register = (u8, u8, u8);
const FIRMWARE: Register = (0x01, 0x07, 0x04);
const ACTIVE_PROFILE: Register = (0x02, 0x06, 0x01);
const POLLING: Register = (0x01, 0x09, 0x02);
const DEBOUNCE: Register = (0x04, 0x03, 0x03);
const ANGLE_SNAPPING: Register = (0x07, 0x04, 0x02);
const RIPPLE: Register = (0x07, 0x03, 0x02);
const MOTION_SYNC: Register = (0x07, 0x05, 0x02);
const DPI_READ: Register = (0x05, 0x04, 0x15);
const DPI_WRITE: Register = (0x05, 0x04, 0x21);

const POLLING_CODES: [(u8, u16); 4] = [(1, 125), (2, 250), (4, 500), (8, 1000)];
const DEBOUNCE_MAX: u8 = 20;
const MAX_STAGES: usize = 6;
const DPI_MIN: u16 = 100;

/// The switches, by option id: register and label.
const SWITCHES: [(&str, Register, &str); 3] = [
    ("angle_snapping", ANGLE_SNAPPING, "Angle snapping"),
    ("ripple_control", RIPPLE, "Ripple control"),
    ("motion_sync", MOTION_SYNC, "Motion sync"),
];
const DEBOUNCE_OPTION: &str = "debounce";

const fn command(prefix: &'static [u8], effect: Effect, name: &'static str) -> Command {
    Command { prefix, at: 0, effect, name }
}

/// Everything PeriHub may send: byte 0 (a request), the category and the register, with bit 7 set
/// for a read, and the sub-register. Switching profile (`02 06 01` as a write), resetting a profile
/// (`02 05 01`), buttons, lighting, lift-off and power are left out.
pub const COMMANDS: &[Command] = &[
    command(&[0x00, 0x01, 0x87, 0x04], Effect::Read, "read firmware"),
    command(&[0x00, 0x02, 0x86, 0x01], Effect::Read, "read the profile in use"),
    command(&[0x00, 0x01, 0x89, 0x02], Effect::Read, "read polling rate"),
    command(&[0x00, 0x04, 0x83, 0x03], Effect::Read, "read debounce time"),
    command(&[0x00, 0x07, 0x84, 0x02], Effect::Read, "read angle snapping"),
    command(&[0x00, 0x07, 0x83, 0x02], Effect::Read, "read ripple control"),
    command(&[0x00, 0x07, 0x85, 0x02], Effect::Read, "read motion sync"),
    command(&[0x00, 0x05, 0x84, 0x15], Effect::Read, "read DPI stages"),
    command(&[0x00, 0x01, 0x09, 0x02], Effect::Save, "store polling rate"),
    command(&[0x00, 0x04, 0x03, 0x03], Effect::Save, "store debounce time"),
    command(&[0x00, 0x07, 0x04, 0x02], Effect::Save, "store angle snapping"),
    command(&[0x00, 0x07, 0x03, 0x02], Effect::Save, "store ripple control"),
    command(&[0x00, 0x07, 0x05, 0x02], Effect::Save, "store motion sync"),
    command(&[0x00, 0x05, 0x04, 0x21], Effect::Save, "store DPI stages"),
];

#[derive(Debug, Clone, Copy)]
pub struct Params {
    profiles: u8,
    dpi_max: u16,
    dpi_step: u16,
}

const SONIX_5: Params = Params { profiles: 5, dpi_max: 26000, dpi_step: 100 };
const XLITE_WIRED: Params = Params { profiles: 5, dpi_max: 26000, dpi_step: 50 };
const FUSION2: Params = Params { profiles: 6, dpi_max: 32000, dpi_step: 10 };

const fn pulsar(product_id: &'static [u16], name: &'static str, params: Params) -> Model {
    Model {
        vendor_id: 0x3710,
        product_ids: product_id,
        brand: "Pulsar",
        name,
        kind: Kind::Mouse,
        interface: Some(INTERFACE),
        usage_page: None,
        protocol: Protocol::Pulsar(params),
    }
}

/// From pulsar-mouse-linux's drivers: the cabled mice of the Sonix protocol.
pub const MODELS: &[Model] = &[
    pulsar(&[0x1401], "Xlite Wired", XLITE_WIRED),
    pulsar(&[0x1402], "X2 Wired", SONIX_5),
    pulsar(&[0x1403], "X2H Wired Medium", SONIX_5),
    pulsar(&[0x1404], "X2A Medium Wired", SONIX_5),
    pulsar(&[0x3401], "Xlite v4", FUSION2),
    pulsar(&[0x3409], "X3", FUSION2),
    pulsar(&[0x3410], "X3 LHD", FUSION2),
];

/// A request: direction 0, the register, the profile in byte 6, the data from byte 7, the sum of
/// bytes 0–61 in bytes 62–63.
fn request(register: Register, read: bool, profile: u8, data: &[u8]) -> [u8; REPORT] {
    let (cat, reg, sub) = register;
    let mut r = [0u8; REPORT];
    r[1] = cat;
    r[2] = if read { reg | READ } else { reg };
    r[3] = sub;
    r[6] = profile;
    r[7..7 + data.len()].copy_from_slice(data);
    let sum = r[..62].iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
    r[62..].copy_from_slice(&sum.to_le_bytes());
    r
}

/// The answer to `request`: answered, with its category and register (bit 7 included), and its
/// profile when it was asked for one; the sub-register can differ (the DPI stages are asked with
/// `15` and answered with `21`). An answer left from another request, by PeriHub or another program,
/// is not taken for this one.
fn answers(answer: &[u8; REPORT], request: &[u8; REPORT]) -> bool {
    answer[0] == ANSWERED && answer[1..3] == request[1..3] && (request[6] == 0 || answer[6] == request[6])
}

/// The DPI stages as read: the stage in use, and each stage's number, X and Y.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stages {
    active: u8,
    stages: Vec<(u8, u16, u16)>,
}

pub struct Pulsar {
    p: Params,
    profile: u8,
    stages: Option<Stages>,
    /// Reads that may still be sent again: until the first answer of this connection.
    resends: u8,
}

impl Pulsar {
    pub fn new(p: Params) -> Self {
        Self { p, profile: 0, stages: None, resends: RESENDS }
    }

    /// Sends a request and waits until the mouse answers it. A read the mouse did not take is sent
    /// again while the connection has had no answer yet; one it took and is still on is not.
    fn query(&mut self, io: &mut Io, register: Register, read: bool, profile: u8, data: &[u8]) -> Result<[u8; REPORT]> {
        let r = request(register, read, profile, data);
        loop {
            let last = Cell::new(NOT_TAKEN);
            match io.feature_query(&r, POLL, |a| {
                last.set(a[0]);
                answers(a, &r)
            }) {
                Ok(answer) => {
                    self.resends = 0;
                    return Ok(answer);
                }
                Err(Error::Timeout) if read && self.resends > 0 && last.get() == NOT_TAKEN => {
                    self.resends -= 1;
                    std::thread::sleep(RESEND_WAIT);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn read_stages(&mut self, io: &mut Io, profile: u8, problems: &mut Vec<String>) -> Result<Option<Stages>> {
        let r = self.query(io, DPI_READ, true, profile, &[])?;
        let (active, count) = (r[7], r[8] as usize);
        if !(1..=MAX_STAGES).contains(&count) || active == 0 || active as usize > count {
            problems.push(format!("{count} DPI stages, stage {active} in use"));
            return Ok(None);
        }
        let mut stages = Vec::new();
        for i in 0..count {
            let at = 9 + 5 * i;
            let (n, x, y) =
                (r[at], u16::from_le_bytes([r[at + 1], r[at + 2]]), u16::from_le_bytes([r[at + 3], r[at + 4]]));
            if n as usize != i + 1
                || !(DPI_MIN..=self.p.dpi_max).contains(&x)
                || !(DPI_MIN..=self.p.dpi_max).contains(&y)
            {
                problems.push(format!("DPI stage {} reads {n}: {x}×{y}", i + 1));
                return Ok(None);
            }
            stages.push((n, x, y));
        }
        Ok(Some(Stages { active, stages }))
    }

    fn write(&mut self, io: &mut Io, register: Register, profile: u8, data: &[u8]) -> Result<()> {
        self.query(io, register, false, profile, data).map(|_| ())
    }
}

impl Driver for Pulsar {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "pulsar-mouse-linux (MIT)"
    }

    fn profiles(&self) -> u8 {
        self.p.profiles
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        true
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        self.stages = None;
        let firmware = match self.query(io, FIRMWARE, true, 0, &[]) {
            Ok(r) => r[6..10].iter().rev().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("."),
            // A page with the test report rather than no page at all.
            Err(e @ (Error::Hid(_) | Error::Timeout)) => {
                return Ok(Reading {
                    problem: Some(format!("the mouse did not answer as documented: {e}")),
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };
        let mut reading = Reading { firmware: Some(firmware), ..Default::default() };
        let mut problems = Vec::new();
        // One source reads it on the X3 and the X2A, another says the X3 does not answer it: without
        // it the DPI stages are left out, and the global settings still show.
        let profile = match self.query(io, ACTIVE_PROFILE, true, 0, &[]) {
            Ok(r) => Some(r[6]),
            Err(Error::Timeout) => None,
            Err(e) => return Err(e),
        };
        if let Some(profile) = profile {
            if !(1..=self.p.profiles).contains(&profile) {
                reading.problem = Some(format!("profile {profile} in use"));
                return Ok(reading);
            }
            self.profile = profile;
            reading.active_profile = Some(profile);
        }
        let mut settings = Settings::default();
        let code = self.query(io, POLLING, true, 0, &[])?[7];
        match POLLING_CODES.iter().find(|(c, _)| *c == code) {
            Some((_, hz)) => settings.polling_hz = Some(*hz),
            None => problems.push(format!("polling rate code {code:02x}")),
        }
        let debounce = self.query(io, DEBOUNCE, true, 0, &[])?[7];
        if debounce > DEBOUNCE_MAX {
            problems.push(format!("debounce {debounce} ms"));
        }
        settings.options.insert(DEBOUNCE_OPTION.into(), debounce as u16);
        for (id, register, label) in SWITCHES {
            let value = self.query(io, register, true, 0, &[])?[7];
            if value > 1 {
                problems.push(format!("{label} {value}"));
            }
            settings.options.insert(id.into(), value as u16);
        }
        if let Some(profile) = profile {
            self.stages = self.read_stages(io, profile, &mut problems)?;
        }
        let mut separate_xy = false;
        if let Some(s) = &self.stages {
            settings.dpi_levels = s.stages.iter().map(|(_, x, _)| *x).collect();
            separate_xy = s.stages.iter().any(|(_, x, y)| x != y);
            reading.active_dpi = Some(s.active);
        }
        reading.settings = settings;
        let p = self.p;
        reading.limits.dpi = (!separate_xy && self.stages.is_some()).then_some(DpiLimits {
            min: DPI_MIN,
            max: p.dpi_max,
            step: p.dpi_step,
        });
        reading.limits.polling_hz = POLLING_CODES.iter().map(|(_, hz)| *hz).collect();
        let on_off = || vec![(0, "Off".to_string()), (1, "On".to_string())];
        reading.limits.options.push(Choice {
            id: DEBOUNCE_OPTION,
            label: "Debounce time",
            values: (0..=DEBOUNCE_MAX as u16).map(|ms| (ms, format!("{ms} ms"))).collect(),
        });
        for (id, _, label) in SWITCHES {
            reading.limits.options.push(Choice { id, label, values: on_off() });
        }
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected settings: {}", problems.join(", ")));
        }
        Ok(reading)
    }

    /// Everything is checked before the first write. The task reads the mouse again right before
    /// this, so the stage in use and the profile are the mouse's own of a moment ago.
    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let byte = |s: &Settings, id: &str| -> Result<u8> {
            s.options.get(id).and_then(|&v| u8::try_from(v).ok()).ok_or_else(|| Error::Invalid(id.into()))
        };
        let mut writes: Vec<(Register, u8, Vec<u8>)> = Vec::new();
        if to.polling_hz != from.polling_hz {
            let hz = to.polling_hz.ok_or_else(|| Error::Invalid("polling rate".into()))?;
            let (code, _) =
                POLLING_CODES.iter().find(|(_, h)| *h == hz).ok_or_else(|| Error::Invalid(format!("{hz} Hz")))?;
            writes.push((POLLING, 0, vec![*code]));
        }
        let ms = byte(to, DEBOUNCE_OPTION)?;
        if ms != byte(from, DEBOUNCE_OPTION)? {
            if ms > DEBOUNCE_MAX {
                return Err(Error::Invalid(format!("debounce {ms} ms")));
            }
            writes.push((DEBOUNCE, 0, vec![ms]));
        }
        for (id, register, _) in SWITCHES {
            let value = byte(to, id)?;
            if value != byte(from, id)? {
                if value > 1 {
                    return Err(Error::Invalid(id.into()));
                }
                writes.push((register, 0, vec![value]));
            }
        }
        if to.dpi_levels != from.dpi_levels {
            let s = self.stages.as_ref().ok_or_else(|| Error::Invalid("DPI".into()))?;
            if to.dpi_levels.len() != s.stages.len() || s.stages.iter().any(|(_, x, y)| x != y) {
                return Err(Error::Invalid("DPI".into()));
            }
            let mut data = vec![s.active, s.stages.len() as u8];
            for ((n, _, _), dpi) in s.stages.iter().zip(&to.dpi_levels) {
                if !(DPI_MIN..=self.p.dpi_max).contains(dpi) || !(dpi - DPI_MIN).is_multiple_of(self.p.dpi_step) {
                    return Err(Error::Invalid(format!("{dpi} DPI")));
                }
                let [lo, hi] = dpi.to_le_bytes();
                data.extend_from_slice(&[*n, lo, hi, lo, hi]);
            }
            writes.push((DPI_WRITE, self.profile, data));
        }
        for (register, profile, data) in writes {
            self.write(io, register, profile, &data)?;
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("these mice store every change by themselves".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// A simulated Pulsar mouse: the global registers, the DPI stages of each profile, the profile in
    /// use, and every request it got. `dropped` requests are not taken (a mouse whose cable just
    /// showed up), registers in `silent` are never answered, nor are writes with `deaf_to_writes`,
    /// and a request for `foreign.0` is answered as if it were for `foreign.1`.
    struct Mouse {
        globals: HashMap<Register, u8>,
        stages: Vec<Vec<u8>>,
        profile: u8,
        requests: Vec<[u8; REPORT]>,
        dropped: usize,
        silent: Vec<Register>,
        deaf_to_writes: bool,
        foreign: Option<(Register, Register)>,
    }

    type Shared = Arc<Mutex<Mouse>>;

    /// Stage in use 2 of 3: 800, 1600, 3200.
    fn stages() -> Vec<u8> {
        let mut s = vec![2, 3];
        for (n, dpi) in [(1u8, 800u16), (2, 1600), (3, 3200)] {
            let [lo, hi] = dpi.to_le_bytes();
            s.extend_from_slice(&[n, lo, hi, lo, hi]);
        }
        s
    }

    fn mouse() -> Shared {
        let globals = HashMap::from([(POLLING, 8), (DEBOUNCE, 4), (ANGLE_SNAPPING, 0), (RIPPLE, 0), (MOTION_SYNC, 1)]);
        Arc::new(Mutex::new(Mouse {
            globals,
            stages: vec![stages(); 6],
            profile: 2,
            requests: Vec::new(),
            dropped: 0,
            silent: Vec::new(),
            deaf_to_writes: false,
            foreign: None,
        }))
    }

    fn answer(m: &mut Mouse, r: &[u8]) -> Vec<u8> {
        let r: [u8; REPORT] = r.try_into().expect("64-byte requests");
        let sum = r[..62].iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
        assert_eq!(r[62..], sum.to_le_bytes(), "checksum");
        assert_eq!(r[0], 0x00, "a command");
        m.requests.push(r);
        let reading = r[2] & READ != 0;
        let register = (r[1], r[2] & !READ, r[3]);
        if m.dropped > 0 || m.silent.contains(&register) || (m.deaf_to_writes && !reading) {
            m.dropped = m.dropped.saturating_sub(1);
            return vec![NOT_TAKEN; REPORT];
        }
        let mut a = r.to_vec();
        a[0] = ANSWERED;
        if let Some((asked, (cat, reg, _))) = m.foreign.filter(|(asked, _)| *asked == register) {
            a[1] = cat;
            a[2] = reg | (r[2] & READ);
            a[7] = m.globals[&asked];
            return a;
        }
        match (register, reading) {
            (FIRMWARE, true) => a[6..10].copy_from_slice(&[0x16, 0x10, 0, 0]),
            (ACTIVE_PROFILE, true) => a[6] = m.profile,
            (DPI_READ, true) => {
                let s = &m.stages[r[6] as usize - 1];
                a[7..7 + s.len()].copy_from_slice(s);
            }
            (DPI_WRITE, false) => {
                let count = r[8] as usize;
                m.stages[r[6] as usize - 1] = r[7..9 + 5 * count].to_vec();
            }
            (reg, true) => a[7] = m.globals[&reg],
            (reg, false) => {
                *m.globals.get_mut(&reg).expect("a known register") = r[7];
            }
        }
        a
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(move |r: &[u8]| answer(&mut state.lock().unwrap(), r)));
        hid
    }

    fn x3() -> &'static Model {
        find(0x3710, 0x3409).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-pulsar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn writes(state: &Shared) -> Vec<[u8; REPORT]> {
        state.lock().unwrap().requests.iter().filter(|r| r[2] & READ == 0).copied().collect()
    }

    fn sent_for(state: &Shared, register: Register, read: bool) -> usize {
        let reg = if read { register.1 | READ } else { register.1 };
        state.lock().unwrap().requests.iter().filter(|r| (r[1], r[2], r[3]) == (register.0, reg, register.2)).count()
    }

    #[test]
    fn a_mouse_reads_as_documented_and_only_reads() {
        let state = mouse();
        let task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, false, temp("read")).unwrap();
        assert_eq!(task.info.firmware.as_deref(), Some("00.00.10.16"));
        assert_eq!(task.info.active_profile, Some(2));
        assert_eq!(task.info.profiles, 6);
        assert_eq!(task.info.settings.dpi_levels, [800, 1600, 3200]);
        assert_eq!(task.info.active_dpi, Some(2));
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        assert_eq!(task.info.settings.options[DEBOUNCE_OPTION], 4);
        assert_eq!(task.info.settings.options["motion_sync"], 1);
        assert_eq!(task.info.limits.dpi, Some(DpiLimits { min: 100, max: 32000, step: 10 }));
        assert!(task.info.changes_stay && task.info.problem.is_none());
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn a_change_writes_only_what_changed_and_is_undone_unless_kept() {
        let state = mouse();
        let mut task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("change")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels = vec![800, 1650, 3200];
        wanted.polling_hz = Some(500);
        wanted.options.insert("angle_snapping".into(), 1);
        task.change(&wanted, false, Some(2)).unwrap();
        let written: Vec<Register> = writes(&state).iter().map(|r| (r[1], r[2], r[3])).collect();
        assert_eq!(written, [POLLING, ANGLE_SNAPPING, DPI_WRITE]);
        let m = state.lock().unwrap();
        assert_eq!((m.globals[&POLLING], m.globals[&ANGLE_SNAPPING]), (4, 1));
        let mut expected = stages();
        // Stage i is at 2 + 5i: its number, X, Y.
        expected[8..10].copy_from_slice(&1650u16.to_le_bytes());
        expected[10..12].copy_from_slice(&1650u16.to_le_bytes());
        assert_eq!(m.stages[1], expected, "stage in use and count as read, profile 2 only");
        assert_eq!(m.stages[0], stages());
        drop(m);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        let m = state.lock().unwrap();
        assert_eq!((m.globals[&POLLING], m.globals[&ANGLE_SNAPPING], m.stages[1].clone()), (8, 0, stages()));
        drop(m);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_mouse_does_not_take_are_refused() {
        let state = mouse();
        let mut task =
            start_for_test(Box::new(fake(state.clone())), find(0x3710, 0x1404).unwrap(), 0x1404, true, temp("refuse"))
                .unwrap();
        for dpi in [50, 26100, 850] {
            let mut wanted = task.info.settings.clone();
            wanted.dpi_levels[0] = dpi;
            assert!(task.change(&wanted, false, None).is_err(), "{dpi}");
        }
        for (id, value) in [(DEBOUNCE_OPTION, 21), ("motion_sync", 2)] {
            let mut wanted = task.info.settings.clone();
            wanted.options.insert(id.into(), value);
            assert!(task.change(&wanted, false, None).is_err(), "{id}");
        }
        assert!(writes(&state).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn separate_x_and_y_dpi_are_shown_but_not_changed() {
        let state = mouse();
        state.lock().unwrap().stages[1][5..7].copy_from_slice(&900u16.to_le_bytes());
        let mut task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("xy")).unwrap();
        assert!(task.info.limits.dpi.is_none());
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 900;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn answers_that_do_not_look_as_documented_leave_the_mouse_read_only() {
        type Damage = fn(&mut Mouse);
        let cases: [(&str, Damage); 4] = [
            ("profile 7 in use", |m| m.profile = 7),
            ("polling code 3", |m| *m.globals.get_mut(&POLLING).unwrap() = 3),
            ("seven stages", |m| m.stages[1][1] = 7),
            ("a stage of 50 DPI", |m| m.stages[1][3..5].copy_from_slice(&50u16.to_le_bytes())),
        ];
        for (name, damage) in cases {
            let state = mouse();
            damage(&mut state.lock().unwrap());
            let task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("odd")).unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
            assert!(writes(&state).is_empty(), "{name}");
        }
    }

    #[test]
    fn a_mouse_whose_cable_just_showed_up_is_asked_again_until_it_answers() {
        let state = mouse();
        state.lock().unwrap().dropped = 3;
        let task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, false, temp("dropped")).unwrap();
        assert!(task.info.problem.is_none());
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        assert_eq!(sent_for(&state, FIRMWARE, true), 4, "the same read, sent again");
        assert_eq!(sent_for(&state, POLLING, true), 1, "once answered, never again");
    }

    #[test]
    fn a_write_is_never_sent_twice() {
        let state = mouse();
        let mut task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("write-once")).unwrap();
        state.lock().unwrap().deaf_to_writes = true;
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(500);
        assert!(task.change(&wanted, false, Some(2)).is_err());
        assert_eq!(sent_for(&state, POLLING, false), 1);
        assert_eq!(state.lock().unwrap().globals[&POLLING], 8);
        let _ = std::fs::remove_dir_all(task.backups());

        // Not even before the connection's first answer, when a dropped read would be sent again.
        let state = mouse();
        state.lock().unwrap().dropped = 10;
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), true);
        assert!(Pulsar::new(FUSION2).write(&mut io, POLLING, 0, &[4]).is_err());
        assert_eq!(sent_for(&state, POLLING, false), 1);
    }

    #[test]
    fn an_answer_to_another_request_is_not_taken_for_this_one() {
        let state = mouse();
        // Debounce 4 ms would read as polling code 4, 500 Hz.
        state.lock().unwrap().foreign = Some((POLLING, DEBOUNCE));
        *state.lock().unwrap().globals.get_mut(&POLLING).unwrap() = 4;
        *state.lock().unwrap().globals.get_mut(&DEBOUNCE).unwrap() = 4;
        assert!(start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("foreign")).is_err());
        assert_eq!(sent_for(&state, POLLING, true), 1, "an answered read is not a dropped one");
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn without_the_profile_in_use_the_dpi_is_left_out_and_the_rest_still_changes() {
        let state = mouse();
        state.lock().unwrap().silent.push(ACTIVE_PROFILE);
        let mut task = start_for_test(Box::new(fake(state.clone())), x3(), 0x3409, true, temp("no-profile")).unwrap();
        assert!(task.info.problem.is_none() && task.info.editable);
        assert_eq!((task.info.active_profile, task.info.active_dpi, task.info.limits.dpi), (None, None, None));
        assert!(task.info.settings.dpi_levels.is_empty());
        assert_eq!(sent_for(&state, DPI_READ, true), 0, "no profile is guessed");
        let mut wanted = task.info.settings.clone();
        wanted.options.insert("ripple_control".into(), 1);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().globals[&RIPPLE], 1);
        task.keep_changes();
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = mouse();
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), true);
        let ready = |a: &[u8; REPORT]| a[0] == ANSWERED;
        for (register, profile) in [((0x02, 0x06, 0x01), 3), ((0x02, 0x05, 0x01), 1), ((0x04, 0x01, 0x06), 1)] {
            let r = request(register, false, profile, &[]);
            assert!(io.feature_query(&r, Duration::ZERO, ready).is_err(), "{register:02x?}");
        }
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), false);
        assert!(io.feature_query(&request(POLLING, false, 0, &[4]), Duration::ZERO, ready).is_err());
        assert!(state.lock().unwrap().requests.is_empty());
    }
}

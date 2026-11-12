//! The Pulsar X2 CrazyLight and X3 LHD CrazyLight, and the Lamzu Atlantis (read only), on their
//! cable: the 17-byte "Nordic" protocol. Described in `docs/protocols/nordic-mice.md`, from
//! pulsar-mouse-linux and lamzu-cfg (MIT, see `THIRD-PARTY-NOTICES.md`). None has been tried with
//! PeriHub.
//!
//! Every write is stored by the mouse: only Save to memory, with backup, read-back and undo, on
//! the profile in use. Each writable address is its own command, so nothing else can be written;
//! the factory reset, profile switches and the dongles' commands are never sent.

use std::time::Duration;

use super::task::Driver;
use super::{Choice, Command, DpiLimits, Effect, Io, Kind, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

const INTERFACE: i32 = 1;
const ID: u8 = 0x08;
const FRAME: usize = 17;
const TIMEOUT: Duration = Duration::from_millis(500);

const MEM_SET: u8 = 0x07;
const MEM_GET: u8 = 0x08;
const PROFILE: u8 = 0x0E;
const FIRMWARE: u8 = 0x12;

const POLLING: u8 = 0x00;
const STAGE_COUNT: u8 = 0x02;
const ACTIVE_STAGE: u8 = 0x04;
const LIFT_OFF: u8 = 0x0A;
const DPI_BASE: u8 = 0x0C;
const DEBOUNCE: u8 = 0xA9;
const MOTION_SYNC: u8 = 0xAB;
const ANGLE_SNAPPING: u8 = 0xAF;
const RIPPLE: u8 = 0xB1;
/// The memory read every time: everything up to ripple control's check byte, 10 bytes at a time.
const READ_END: usize = 0xB4;

const POLLING_CODES: [(u8, u16); 7] =
    [(0x01, 1000), (0x02, 500), (0x04, 250), (0x08, 125), (0x10, 2000), (0x20, 4000), (0x40, 8000)];
/// On the cable the mouse runs at 1000 Hz at most.
const CABLE_MAX_HZ: u16 = 1000;

const DEBOUNCE_OPTION: &str = "debounce";
const LIFT_OFF_OPTION: &str = "lift_off";
const SWITCHES: [(&str, u8, &str); 3] = [
    ("motion_sync", MOTION_SYNC, "Motion sync"),
    ("angle_snapping", ANGLE_SNAPPING, "Angle snapping"),
    ("ripple_control", RIPPLE, "Ripple control"),
];

/// One DPI granularity: the mode bits, the step, the base added to the stored index, and the
/// highest DPI Pulsar's app writes in it.
#[derive(Debug, Clone, Copy)]
struct DpiMode {
    mode: u8,
    step: u16,
    base: u16,
    up_to: u16,
}

const CRAZYLIGHT_DPI: &[DpiMode] = &[
    DpiMode { mode: 0, step: 10, base: 1, up_to: 10240 },
    DpiMode { mode: 2, step: 50, base: 201, up_to: 25600 },
    DpiMode { mode: 3, step: 100, base: 201, up_to: 32000 },
];
const LAMZU_DPI: &[DpiMode] = &[DpiMode { mode: 0, step: 50, base: 1, up_to: 26000 }];

#[derive(Debug, Clone, Copy)]
pub struct Params {
    profiles: u8,
    max_stages: usize,
    dpi: &'static [DpiMode],
    debounce_max: u8,
    lift_off: &'static [(u8, &'static str)],
    /// Writes were confirmed on this mouse by the source: Save to memory is offered.
    writable: bool,
}

const CRAZYLIGHT: Params = Params {
    profiles: 4,
    max_stages: 6,
    dpi: CRAZYLIGHT_DPI,
    debounce_max: 20,
    lift_off: &[(0x03, "0.7 mm"), (0x01, "1 mm"), (0x02, "2 mm")],
    writable: true,
};
const LAMZU: Params = Params {
    profiles: 4,
    max_stages: 8,
    dpi: LAMZU_DPI,
    debounce_max: 15,
    lift_off: &[(0x01, "1 mm"), (0x02, "2 mm")],
    writable: false,
};

const fn mouse(
    vendor_id: u16,
    product_id: &'static [u16],
    brand: &'static str,
    name: &'static str,
    params: Params,
) -> Model {
    Model {
        vendor_id,
        product_ids: product_id,
        brand,
        name,
        kind: Kind::Mouse,
        interface: Some(INTERFACE),
        usage_page: None,
        protocol: Protocol::Nordic(params),
    }
}

/// The dongles are left out, and so are the Feinmann mice (their DPI encoding is not documented).
pub const MODELS: &[Model] = &[
    mouse(0x3710, &[0x3414], "Pulsar", "X2 CrazyLight", CRAZYLIGHT),
    mouse(0x3710, &[0x3527], "Pulsar", "X3 LHD CrazyLight", CRAZYLIGHT),
    mouse(0x3554, &[0xF50F], "Lamzu", "Atlantis", LAMZU),
];

const fn write(address: u8, len: u8) -> [u8; 6] {
    [ID, MEM_SET, 0, 0, address, len]
}
const W_POLLING: [u8; 6] = write(POLLING, 2);
const W_LIFT_OFF: [u8; 6] = write(LIFT_OFF, 2);
const W_DEBOUNCE: [u8; 6] = write(DEBOUNCE, 2);
const W_MOTION_SYNC: [u8; 6] = write(MOTION_SYNC, 2);
const W_ANGLE: [u8; 6] = write(ANGLE_SNAPPING, 2);
const W_RIPPLE: [u8; 6] = write(RIPPLE, 2);
const W_DPI: [[u8; 6]; 6] = [
    write(DPI_BASE, 4),
    write(DPI_BASE + 4, 4),
    write(DPI_BASE + 8, 4),
    write(DPI_BASE + 12, 4),
    write(DPI_BASE + 16, 4),
    write(DPI_BASE + 20, 4),
];

const fn command(prefix: &'static [u8], effect: Effect, name: &'static str) -> Command {
    Command { prefix, at: 0, effect, name }
}

const READS: [Command; 3] = [
    command(&[ID, MEM_GET, 0, 0], Effect::Read, "read memory"),
    command(&[ID, PROFILE], Effect::Read, "read the profile in use"),
    command(&[ID, FIRMWARE], Effect::Read, "read firmware"),
];

/// Every address PeriHub may write, one command each.
const WRITES: [Command; 12] = [
    command(&W_POLLING, Effect::Save, "store polling rate"),
    command(&W_LIFT_OFF, Effect::Save, "store lift-off distance"),
    command(&W_DEBOUNCE, Effect::Save, "store debounce time"),
    command(&W_MOTION_SYNC, Effect::Save, "store motion sync"),
    command(&W_ANGLE, Effect::Save, "store angle snapping"),
    command(&W_RIPPLE, Effect::Save, "store ripple control"),
    command(&W_DPI[0], Effect::Save, "store DPI stage 1"),
    command(&W_DPI[1], Effect::Save, "store DPI stage 2"),
    command(&W_DPI[2], Effect::Save, "store DPI stage 3"),
    command(&W_DPI[3], Effect::Save, "store DPI stage 4"),
    command(&W_DPI[4], Effect::Save, "store DPI stage 5"),
    command(&W_DPI[5], Effect::Save, "store DPI stage 6"),
];

fn check(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0x55u8, |c, &b| c.wrapping_sub(b))
}

fn frame(command: u8, address: u8, data: &[u8]) -> [u8; FRAME] {
    let mut f = [0u8; FRAME];
    f[0] = ID;
    f[1] = command;
    f[4] = address;
    f[5] = data.len() as u8;
    f[6..6 + data.len()].copy_from_slice(data);
    f[16] = check(&f[..16]);
    f
}

/// An answer to `request`: our report id, a right checksum, the same command, and for a memory read
/// the same address and length. The mouse's own events (`0A`) are not answers.
fn answers(request: &[u8; FRAME], answer: &[u8]) -> bool {
    answer[0] == ID
        && answer[16] == check(&answer[..16])
        && answer[1] == request[1]
        && (request[1] != MEM_GET || answer[3..6] == request[3..6])
}

fn rot2(nibble: u8) -> u8 {
    ((nibble << 2) | (nibble >> 2)) & 0x0F
}

/// A stage's DPI from its low byte and the nibble of its `high` byte.
fn decode(modes: &[DpiMode], low: u8, nibble: u8) -> Option<u16> {
    let bits = rot2(nibble);
    let (mode, page) = (bits >> 2, (bits & 3) as u32);
    let m = modes.iter().find(|m| m.mode == mode)?;
    u16::try_from((low as u32 + m.base as u32 + 256 * page) * m.step as u32).ok()
}

/// The bytes Pulsar's app writes for `dpi`: the finest mode that reaches it, if `dpi` is on its step.
fn encode(modes: &[DpiMode], dpi: u16) -> Option<[u8; 3]> {
    let m = modes.iter().find(|m| dpi <= m.up_to)?;
    if dpi == 0 || !dpi.is_multiple_of(m.step) {
        return None;
    }
    let index = (dpi / m.step).checked_sub(m.base)?;
    if index > 1023 {
        return None;
    }
    let (page, low) = ((index / 256) as u8, (index % 256) as u8);
    let nibble = rot2((m.mode << 2) | page);
    Some([low, low, (nibble << 4) | nibble])
}

/// The value of a `(v, 0x55 − v)` pair, if it adds up.
fn pair(mem: &[u8], address: u8) -> Option<u8> {
    let (v, c) = (mem[address as usize], mem[address as usize + 1]);
    (check(&[v]) == c).then_some(v)
}

pub struct Nordic {
    p: Params,
    /// Memory as last read, and the DPI stages read from it: X and Y.
    mem: Vec<u8>,
    stages: Vec<(u16, u16)>,
    stored_hz: Option<u16>,
    read_once: bool,
}

impl Nordic {
    pub fn new(p: Params) -> Self {
        Self { p, mem: Vec::new(), stages: Vec::new(), stored_hz: None, read_once: false }
    }

    fn exchange(io: &mut Io, request: [u8; FRAME]) -> Result<[u8; FRAME]> {
        let mut answer = [0u8; FRAME];
        io.numbered_output_exchange(&request, &mut answer, TIMEOUT, |a| answers(&request, a))?;
        Ok(answer)
    }

    fn read_memory(io: &mut Io) -> Result<Vec<u8>> {
        let mut mem = Vec::with_capacity(READ_END);
        for address in (0..READ_END).step_by(10) {
            let answer = Self::exchange(io, frame(MEM_GET, address as u8, &[0; 10]))?;
            mem.extend_from_slice(&answer[6..16]);
        }
        Ok(mem)
    }

    fn read_all(&mut self, io: &mut Io) -> Result<Reading> {
        self.mem.clear();
        self.stages.clear();
        self.stored_hz = None;
        if !self.p.writable {
            io.forbid_changes();
        }
        let f = Self::exchange(io, frame(FIRMWARE, 0, &[]))?;
        let mut reading = Reading { firmware: Some(format!("{:x}.{:02x}", f[6], f[7])), ..Default::default() };
        let profile = Self::exchange(io, frame(PROFILE, 0, &[]))?[6] as u16 + 1;
        if profile > self.p.profiles as u16 {
            reading.problem = Some(format!("profile {profile} in use"));
            return Ok(reading);
        }
        reading.active_profile = Some(profile as u8);
        let mem = Self::read_memory(io)?;
        let mut problems = Vec::new();
        let mut value = |address: u8, name: &str| {
            let v = pair(&mem, address);
            if v.is_none() {
                problems.push(format!("{name} does not add up"));
            }
            v
        };
        let polling = value(POLLING, "the polling rate");
        let count = value(STAGE_COUNT, "the number of DPI stages");
        let active = value(ACTIVE_STAGE, "the DPI stage in use");
        let lift_off = value(LIFT_OFF, "the lift-off distance");
        let debounce = value(DEBOUNCE, "the debounce time");
        let switches: Vec<Option<u8>> = SWITCHES.iter().map(|(_, a, label)| value(*a, label)).collect();
        let settings = &mut reading.settings;
        match polling.and_then(|c| POLLING_CODES.iter().find(|(code, _)| *code == c)) {
            Some((_, hz)) => {
                settings.polling_hz = Some(*hz);
                self.stored_hz = Some(*hz);
            }
            None => problems.push(format!("polling rate code {:02x?}", polling)),
        }
        let count = count.map(|c| c as usize).filter(|c| (1..=self.p.max_stages).contains(c));
        match (count, active) {
            (Some(count), Some(active)) if (active as usize) < count => {
                for i in 0..count {
                    let at = DPI_BASE as usize + 4 * i;
                    let record = &mem[at..at + 4];
                    let (high_x, high_y) = (record[2] & 0x0F, record[2] >> 4);
                    let xy = (decode(self.p.dpi, record[0], high_x), decode(self.p.dpi, record[1], high_y));
                    match xy {
                        (Some(x), Some(y)) if record[3] == check(&record[..3]) => self.stages.push((x, y)),
                        _ => {
                            problems.push(format!("DPI stage {} reads {}", i + 1, super::hex(record)));
                            self.stages.clear();
                            break;
                        }
                    }
                }
                reading.active_dpi = Some(active + 1);
            }
            _ => problems.push(format!("{count:?} DPI stages, stage {active:?} in use")),
        }
        settings.dpi_levels = self.stages.iter().map(|(x, _)| *x).collect();
        match lift_off.filter(|c| self.p.lift_off.iter().any(|(code, _)| code == c)) {
            Some(code) => {
                settings.options.insert(LIFT_OFF_OPTION.into(), code as u16);
            }
            None => problems.push(format!("lift-off code {lift_off:02x?}")),
        }
        match debounce.filter(|d| *d <= self.p.debounce_max) {
            Some(ms) => {
                settings.options.insert(DEBOUNCE_OPTION.into(), ms as u16);
            }
            None => problems.push(format!("debounce {debounce:?}")),
        }
        for ((id, _, label), v) in SWITCHES.iter().zip(switches) {
            match v.filter(|v| *v <= 1) {
                Some(v) => {
                    settings.options.insert((*id).into(), v as u16);
                }
                None => problems.push(format!("{label} {v:?}")),
            }
        }
        let separate_xy = self.stages.iter().any(|(x, y)| x != y);
        let first = self.p.dpi[0];
        reading.limits.dpi = (self.p.writable && !separate_xy && !self.stages.is_empty()).then_some(DpiLimits {
            min: first.step * first.base,
            max: self.p.dpi.last().map(|m| m.up_to).unwrap_or(first.up_to),
            step: first.step,
        });
        // A profile holding a faster rate for the dongle keeps it: shown, not changed.
        if self.p.writable && self.stored_hz.is_some_and(|hz| hz <= CABLE_MAX_HZ) {
            reading.limits.polling_hz =
                POLLING_CODES.iter().map(|(_, hz)| *hz).filter(|hz| *hz <= CABLE_MAX_HZ).collect();
        }
        let on_off = || vec![(0, "Off".to_string()), (1, "On".to_string())];
        reading.limits.options.push(Choice {
            id: DEBOUNCE_OPTION,
            label: "Debounce time",
            values: (0..=self.p.debounce_max as u16).map(|ms| (ms, format!("{ms} ms"))).collect(),
        });
        reading.limits.options.push(Choice {
            id: LIFT_OFF_OPTION,
            label: "Lift-off distance",
            values: self.p.lift_off.iter().map(|(code, label)| (*code as u16, label.to_string())).collect(),
        });
        for (id, _, label) in SWITCHES {
            reading.limits.options.push(Choice { id, label, values: on_off() });
        }
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected settings: {}", problems.join(", ")));
        }
        self.mem = mem;
        Ok(reading)
    }
}

impl Driver for Nordic {
    fn commands(&self) -> Vec<Command> {
        let mut commands = READS.to_vec();
        if self.p.writable {
            commands.extend_from_slice(&WRITES);
        }
        commands
    }

    fn source(&self) -> &'static str {
        if self.p.writable { "pulsar-mouse-linux (MIT)" } else { "lamzu-cfg and pulsar-mouse-linux (MIT)" }
    }

    fn profiles(&self) -> u8 {
        self.p.profiles
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        self.p.writable
    }

    /// A mouse that does not answer its first reading gets a read-only page with the test report.
    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        match self.read_all(io) {
            Ok(reading) => {
                self.read_once = true;
                Ok(reading)
            }
            Err(e @ (Error::Hid(_) | Error::Timeout)) if !self.read_once => Ok(Reading {
                problem: Some(format!("the mouse did not answer as documented: {e}")),
                ..Default::default()
            }),
            Err(e) => Err(e),
        }
    }

    /// Everything is checked before the first write; only what changed is written.
    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        if !self.p.writable {
            return Err(Error::Invalid("this mouse is read only".into()));
        }
        let byte = |s: &Settings, id: &str| -> Result<u8> {
            s.options.get(id).and_then(|&v| u8::try_from(v).ok()).ok_or_else(|| Error::Invalid(id.into()))
        };
        let mut writes: Vec<(u8, Vec<u8>)> = Vec::new();
        let pair_of = |v: u8| vec![v, check(&[v])];
        if to.polling_hz != from.polling_hz {
            let hz = to.polling_hz.ok_or_else(|| Error::Invalid("polling rate".into()))?;
            if hz > CABLE_MAX_HZ || self.stored_hz.is_none_or(|stored| stored > CABLE_MAX_HZ) {
                return Err(Error::Invalid(format!("{hz} Hz on the cable")));
            }
            let (code, _) =
                POLLING_CODES.iter().find(|(_, h)| *h == hz).ok_or_else(|| Error::Invalid(format!("{hz} Hz")))?;
            writes.push((POLLING, pair_of(*code)));
        }
        let lift_off = byte(to, LIFT_OFF_OPTION)?;
        if lift_off != byte(from, LIFT_OFF_OPTION)? {
            if !self.p.lift_off.iter().any(|(c, _)| *c == lift_off) {
                return Err(Error::Invalid(format!("lift-off code {lift_off}")));
            }
            writes.push((LIFT_OFF, pair_of(lift_off)));
        }
        let ms = byte(to, DEBOUNCE_OPTION)?;
        if ms != byte(from, DEBOUNCE_OPTION)? {
            if ms > self.p.debounce_max {
                return Err(Error::Invalid(format!("debounce {ms} ms")));
            }
            writes.push((DEBOUNCE, pair_of(ms)));
        }
        for (id, address, _) in SWITCHES {
            let v = byte(to, id)?;
            if v != byte(from, id)? {
                if v > 1 {
                    return Err(Error::Invalid(id.into()));
                }
                writes.push((address, pair_of(v)));
            }
        }
        if to.dpi_levels != from.dpi_levels {
            if to.dpi_levels.len() != self.stages.len()
                || self.stages.len() > W_DPI.len()
                || self.stages.iter().any(|(x, y)| x != y)
            {
                return Err(Error::Invalid("DPI".into()));
            }
            for (i, (dpi, (old, _))) in to.dpi_levels.iter().zip(&self.stages).enumerate() {
                if dpi == old {
                    continue;
                }
                let [x, y, high] = encode(self.p.dpi, *dpi).ok_or_else(|| {
                    Error::Invalid(format!(
                        "{dpi} DPI: this mouse takes steps of 10 to 10240, of 50 to 25600, of 100 to 32000"
                    ))
                })?;
                writes.push((DPI_BASE + 4 * i as u8, vec![x, y, high, check(&[x, y, high])]));
            }
        }
        for (address, data) in writes {
            Self::exchange(io, frame(MEM_SET, address, &data))?;
        }
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("these mice store every change by themselves".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// The memory of an X2 CrazyLight as Pulsar's app read it (docs/protocol-x2-crazylight.md):
    /// 2000 Hz for the dongle, four stages 400/800/1600/3200 with the third in use, 1 mm, debounce
    /// 0, motion sync on.
    const DUMP: [&str; 19] = [
        "10 45 04 51 02 53 00 55 00 55",
        "01 54 27 27 00 07 4f 4f 00 b7",
        "9f 9f 00 17 3f 3f 44 93 7f 7f",
        "88 cf 37 37 22 c5 37 37 22 c5",
        "37 37 22 c5 ff 00 00 56 00 00",
        "ff 56 00 ff 00 56 ff ff 00 57",
        "ff a3 00 b3 f2 0a ea 6f f2 0a",
        "ea 6f f2 0a ea 6f 01 54 10 45",
        "03 52 00 55 ff 00 ff 57 00 55",
        "80 d5 03 52 00 55 01 01 00 53",
        "01 02 00 52 01 04 00 50 01 08",
        "00 4c 01 10 00 44 02 01 00 52",
        "02 02 00 51 02 03 00 50 07 00",
        "00 4e 08 04 00 49 04 0a 03 44",
        "00 00 00 55 00 00 00 55 00 00",
        "00 55 00 00 00 55 00 00 00 55",
        "01 ff 00 ff 07 09 46 00 55 00",
        "55 01 54 06 4f 00 55 00 55 00",
        "55 00 55 06 4f 00 55 0a 4b 00",
    ];

    fn dump() -> Vec<u8> {
        let mut mem: Vec<u8> =
            DUMP.iter().flat_map(|line| line.split(' ').map(|b| u8::from_str_radix(b, 16).unwrap())).collect();
        mem.resize(0x100, 0xff);
        mem
    }

    struct Mouse {
        mem: Vec<u8>,
        profile: u8,
        sent: Vec<[u8; FRAME]>,
        /// Sends an event of its own before each answer.
        chatty: bool,
        /// Sends a late answer to a read of another address before each memory read's answer.
        stale: bool,
    }

    type Shared = Arc<Mutex<Mouse>>;

    fn mouse(cable_rate: bool) -> Shared {
        let mut mem = dump();
        if cable_rate {
            mem[0] = 0x01;
            mem[1] = check(&[0x01]);
        }
        Arc::new(Mutex::new(Mouse { mem, profile: 1, sent: Vec::new(), chatty: false, stale: false }))
    }

    fn answer(m: &mut Mouse, r: &[u8]) -> Vec<Vec<u8>> {
        let r: [u8; FRAME] = r.try_into().expect("17-byte frames");
        assert_eq!((r[0], r[16]), (ID, check(&r[..16])), "report id and checksum");
        m.sent.push(r);
        let mut a = r;
        let (address, len) = (r[4] as usize, r[5] as usize);
        match r[1] {
            MEM_GET => a[6..6 + len].copy_from_slice(&m.mem[address..address + len]),
            MEM_SET => m.mem[address..address + len].copy_from_slice(&r[6..6 + len]),
            PROFILE => a[6] = m.profile,
            FIRMWARE => a[6..8].copy_from_slice(&[0x03, 0x05]),
            other => panic!("unexpected command {other:02x}"),
        }
        a[16] = check(&a[..16]);
        let mut reports = Vec::new();
        if m.chatty {
            reports.push(frame(0x0A, 0, &[0x01]).to_vec());
        }
        if m.stale && r[1] == MEM_GET {
            let other = (address + 10) % 0xF0;
            reports.push(frame(MEM_GET, other as u8, &m.mem[other..other + len]).to_vec());
        }
        reports.push(a.to_vec());
        reports
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(move |r| answer(&mut state.lock().unwrap(), r));
        hid.0.lock().unwrap().numbered_output = true;
        hid
    }

    fn x2() -> &'static Model {
        find(0x3710, 0x3414).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-nordic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn writes(state: &Shared) -> Vec<[u8; FRAME]> {
        state.lock().unwrap().sent.iter().filter(|r| r[1] == MEM_SET).copied().collect()
    }

    #[test]
    fn the_dpi_bytes_read_off_pulsars_app_decode_and_encode_alike() {
        let pairs: [([u8; 3], u16); 14] = [
            ([0x27, 0x27, 0x00], 400),
            ([0x4f, 0x4f, 0x00], 800),
            ([0x9f, 0x9f, 0x00], 1600),
            ([0xef, 0xef, 0x00], 2400),
            ([0x3f, 0x3f, 0x44], 3200),
            ([0x7f, 0x7f, 0x88], 6400),
            ([0x1f, 0x1f, 0xcc], 8000),
            ([0xe7, 0xe7, 0xcc], 10000),
            ([0x37, 0x37, 0x22], 12800),
            ([0x77, 0x77, 0x22], 16000),
            ([0xc7, 0xc7, 0x22], 20000),
            ([0x17, 0x17, 0x66], 24000),
            ([0x37, 0x37, 0x66], 25600),
            ([0x77, 0x77, 0x33], 32000),
        ];
        for (bytes, dpi) in pairs {
            assert_eq!(decode(CRAZYLIGHT_DPI, bytes[0], bytes[2] & 0x0F), Some(dpi), "{bytes:02x?}");
            assert_eq!(encode(CRAZYLIGHT_DPI, dpi), Some(bytes), "{dpi}");
        }
        for dpi in [0, 5, 10245, 10250 + 25, 32050, 32100] {
            assert_eq!(encode(CRAZYLIGHT_DPI, dpi), None, "{dpi}");
        }
        // The Lamzu: steps of 50 from 50, the low byte only up to 12800 (lamzu-cfg's own formula).
        assert_eq!(decode(LAMZU_DPI, 0x0f, 0), Some(800));
        assert_eq!(encode(LAMZU_DPI, 26000), Some([0x07, 0x07, 0x88]));
    }

    #[test]
    fn the_captured_memory_reads_as_documented_and_only_reads() {
        let state = mouse(false);
        let task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("read")).unwrap();
        assert!(task.info.problem.is_none() && task.info.editable && task.info.changes_stay);
        assert_eq!(task.info.firmware.as_deref(), Some("3.05"));
        assert_eq!((task.info.active_profile, task.info.profiles), (Some(2), 4));
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600, 3200]);
        assert_eq!(task.info.active_dpi, Some(3));
        assert_eq!(task.info.settings.polling_hz, Some(2000));
        assert!(task.info.limits.polling_hz.is_empty(), "the dongle's 2000 Hz is kept");
        let options = &task.info.settings.options;
        assert_eq!((options[DEBOUNCE_OPTION], options["motion_sync"], options[LIFT_OFF_OPTION]), (0, 1, 1));
        assert_eq!(task.info.limits.dpi, Some(DpiLimits { min: 10, max: 32000, step: 10 }));
        assert!(writes(&state).is_empty());
        let reads: Vec<u8> = state.lock().unwrap().sent.iter().filter(|r| r[1] == MEM_GET).map(|r| r[4]).collect();
        assert_eq!(reads, (0..0xB4).step_by(10).collect::<Vec<u8>>());
    }

    #[test]
    fn a_change_writes_only_what_changed_at_its_address_and_is_undone_unless_kept() {
        let state = mouse(true);
        let before = state.lock().unwrap().mem.clone();
        let mut task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("change")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[1] = 12800;
        wanted.polling_hz = Some(500);
        wanted.options.insert(DEBOUNCE_OPTION.into(), 4);
        task.change(&wanted, false, Some(2)).unwrap();
        let written: Vec<Vec<u8>> = writes(&state).iter().map(|r| r[3..6 + r[5] as usize].to_vec()).collect();
        assert_eq!(
            written,
            [
                vec![0, POLLING, 2, 0x02, 0x53],
                vec![0, DEBOUNCE, 2, 4, 0x51],
                vec![0, DPI_BASE + 4, 4, 0x37, 0x37, 0x22, 0xc5],
            ]
        );
        assert_eq!(task.info.settings, wanted);
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().mem, before);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_mouse_does_not_take_are_refused_before_any_write() {
        let state = mouse(true);
        let mut task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("refuse")).unwrap();
        for dpi in [10260, 25610, 32010] {
            let mut wanted = task.info.settings.clone();
            wanted.dpi_levels[0] = dpi;
            assert!(task.change(&wanted, false, None).is_err(), "{dpi}");
        }
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(2000);
        assert!(task.change(&wanted, false, None).is_err(), "2000 Hz on the cable");
        assert!(writes(&state).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_profile_holding_the_dongles_rate_keeps_it() {
        let state = mouse(false);
        let mut task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("dongle")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(1000);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn memory_that_does_not_add_up_leaves_the_mouse_read_only() {
        type Damage = fn(&mut Vec<u8>);
        let cases: [(&str, Damage); 4] = [
            ("polling pair", |m| m[1] ^= 1),
            ("DPI record", |m| m[0x0F] ^= 1),
            ("debounce over 20", |m| {
                m[0xA9] = 25;
                m[0xAA] = check(&[25]);
            }),
            ("x and y in different modes", |m| m[0x0E] = 0x24),
        ];
        for (name, damage) in cases {
            let state = mouse(true);
            damage(&mut state.lock().unwrap().mem);
            let task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("odd")).unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
            assert!(writes(&state).is_empty(), "{name}");
        }
    }

    #[test]
    fn the_mouses_own_events_and_late_answers_are_skipped() {
        let state = mouse(true);
        state.lock().unwrap().chatty = true;
        state.lock().unwrap().stale = true;
        let task = start_for_test(Box::new(fake(state.clone())), x2(), 0x3414, true, temp("events")).unwrap();
        assert!(task.info.problem.is_none());
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600, 3200]);
    }

    #[test]
    fn the_lamzu_is_read_only() {
        let state = mouse(true);
        let lamzu = find(0x3554, 0xF50F).unwrap();
        let mut task = start_for_test(Box::new(fake(state.clone())), lamzu, 0xF50F, true, temp("lamzu")).unwrap();
        assert!(!task.info.editable && !task.info.changes_stay && task.info.problem.is_none());
        // The same bytes in steps of 50: 0x27 + 1 = 40 steps.
        assert_eq!(task.info.settings.dpi_levels, [2000, 4000, 8000, 16000]);
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(DEBOUNCE_OPTION.into(), 4);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = mouse(true);
        let mut io = Io::new(Box::new(fake(state.clone())), Nordic::new(CRAZYLIGHT).commands(), true);
        for request in [
            frame(0x09, 0, &[]),                 // factory reset
            frame(0x0F, 0, &[1]),                // switch profile
            frame(0x01, 0, &[0; 4]),             // model query
            frame(MEM_SET, 0x60, &[1, 1, 0, 0]), // a button
            frame(MEM_SET, STAGE_COUNT, &[3, check(&[3])]),
            frame(MEM_SET, DPI_BASE + 24, &[0x27, 0x27, 0, 7]), // stage 7
        ] {
            assert!(Nordic::exchange(&mut io, request).is_err(), "{:02x?}", &request[..6]);
        }
        let mut page_1 = frame(MEM_GET, 0, &[0; 10]);
        page_1[3] = 1;
        page_1[16] = check(&page_1[..16]);
        assert!(Nordic::exchange(&mut io, page_1).is_err(), "memory outside page 0");
        assert!(state.lock().unwrap().sent.is_empty());
    }
}

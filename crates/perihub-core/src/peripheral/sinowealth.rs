//! Glorious mice built on a SinoWealth controller. The protocol is in
//! `docs/protocols/sinowealth-glorious.md`, from libratbag's SinoWealth driver (MIT, see
//! `THIRD-PARTY-NOTICES.md`). None has been tried with PeriHub.
//!
//! There is no command that changes a setting without storing it: every change rewrites the
//! configuration block in the mouse's memory. Like the Razer mice, every change is treated as one
//! that stays: No warranty box, backup, read-back and undo. Only the generic SinoWealth ids are left
//! out: other brands' mice, some with another protocol, share them.
//!
//! A block is written only on top of the one read when the mouse was connected (or the last one
//! PeriHub wrote and read back): a reading that differs from it anywhere but in the DPI, the polling
//! rate and the slot in use is never written back. OpenRGB notes that a reading taken right after a
//! write comes back with zeros from byte `0x2C`: such a reading must never reach the mouse's memory.

use std::time::Duration;

use super::task::Driver;
use super::{Command, DpiLimits, Effect, Io, Kind, Limits, Model, Protocol, Reading, Settings};
use crate::error::{Error, Result};

/// The vendor collection the commands go to (OpenRGB).
const USAGE_PAGE: u16 = 0xFF00;

/// Feature report of the short commands, and of the configuration block on short and long mice.
const CMD_REPORT: u8 = 5;
const CONFIG_SHORT: u8 = 4;
const CONFIG_LONG: u8 = 6;
/// A short command, report number included.
const CMD_SIZE: usize = 6;
/// The block libratbag reads and writes with, report number included. An answer of this length
/// (as on Windows, where OpenRGB reads it so) does not say how long the block really is.
const BLOCK: usize = 520;
/// What a configuration answer may measure, report number included (libratbag).
const CONFIG_MIN: usize = 123;
const CONFIG_MAX: usize = 167;

const FIRMWARE: u8 = 0x01;
const CONFIG: u8 = 0x11;
const READ_FIRMWARE: [u8; CMD_SIZE] = [CMD_REPORT, FIRMWARE, 0, 0, 0, 0];
const READ_CONFIG: [u8; CMD_SIZE] = [CMD_REPORT, CONFIG, 0, 0, 0, 0];

/// Everything PeriHub may send to these mice. Profile switching (`02` with an argument), firmware
/// update mode (`75`), buttons, macros, debounce, angle snapping and the other profiles are left out.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &READ_FIRMWARE, at: 0, effect: Effect::Read, name: "read firmware" },
    Command { prefix: &READ_CONFIG, at: 0, effect: Effect::Read, name: "ask for the settings" },
    Command { prefix: &[CONFIG_SHORT, CONFIG], at: 0, effect: Effect::Save, name: "store the settings" },
    Command { prefix: &[CONFIG_LONG, CONFIG], at: 0, effect: Effect::Save, name: "store the settings (long report)" },
];

/// Offsets in the configuration block, report number included.
const WRITE_LEN: usize = 3;
const SENSOR: usize = 9;
/// Polling rate in the low 4 bits, X and Y DPI separate in bit 7.
const RATE: usize = 10;
const SEPARATE_XY: u8 = 0x80;
/// DPI slots on in the low 4 bits, the slot in use (from 1, counting only those on) in the high ones.
const SLOTS: usize = 11;
/// One bit per DPI slot: set means off.
const SLOTS_OFF: usize = 12;
const DPIS: usize = 13;
const DPI_SLOTS: usize = 8;

const DPI_MIN: u16 = 100;
const DPI_STEP: u16 = 100;

/// Sensor byte, highest DPI, and whether the stored value is DPI / 100 − 1 (else DPI / 100).
const SENSORS: [(u8, u16, bool); 4] =
    [(0x06, 12000, true), (0x08, 7200, false), (0x0E, 10200, true), (0x0F, 16000, false)];

const POLLING: [(u8, u16); 4] = [(1, 125), (2, 250), (3, 500), (4, 1000)];

/// After a write the block is read back this many times, this far apart, until it matches: OpenRGB
/// waits 50 ms, since a reading right after a write is not yet complete.
const READ_BACK_TRIES: usize = 5;
const READ_BACK_WAIT: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// The firmware libratbag documents for this mouse: with any other, only the firmware is read.
    pub firmware: &'static str,
}

const fn glorious(vendor_id: u16, product_ids: &'static [u16], name: &'static str, firmware: &'static str) -> Model {
    Model {
        vendor_id,
        product_ids,
        brand: "Glorious",
        name,
        kind: Kind::Mouse,
        interface: None,
        usage_page: Some(USAGE_PAGE),
        protocol: Protocol::Sinowealth(Params { firmware }),
    }
}

/// From libratbag's `data/devices/glorious-*.device`: only the ids that belong to Glorious mice.
pub const MODELS: &[Model] = &[
    glorious(0x258A, &[0x0036], "Model O/O-", "V103"),
    glorious(0x258A, &[0x0033], "Model D", "V102"),
    glorious(0x3794, &[0xA000], "Model O Eternal", "1009"),
];

/// What a configuration block says, and what in it did not look as documented.
struct Parsed {
    settings: Settings,
    limits: Limits,
    active_dpi: Option<u8>,
    /// The slots that are on, in order.
    slots_on: Vec<usize>,
    problems: Vec<String>,
}

fn sensor(block: &[u8]) -> Option<(u8, u16, bool)> {
    SENSORS.iter().copied().find(|(id, _, _)| *id == block[SENSOR])
}

fn to_dpi(block: &[u8], raw: u8) -> u16 {
    // An unknown sensor reads as libratbag reads it: DPI / 100.
    let minus_one = sensor(block).is_some_and(|(_, _, m)| m);
    (raw as u16 + u16::from(minus_one)) * 100
}

fn slots_on(block: &[u8]) -> Vec<usize> {
    (0..DPI_SLOTS).filter(|i| block[SLOTS_OFF] & (1 << i) == 0).collect()
}

fn parse(block: &[u8]) -> Parsed {
    let mut problems = Vec::new();
    let mut settings = Settings::default();
    let mut limits = Limits::default();
    let separate_xy = block[RATE] & SEPARATE_XY != 0;
    match POLLING.iter().find(|(code, _)| *code == block[RATE] & 0x0F) {
        Some((_, hz)) => settings.polling_hz = Some(*hz),
        None => problems.push(format!("polling rate code {:02x}", block[RATE] & 0x0F)),
    }
    let count = (block[SLOTS] & 0x0F) as usize;
    let active = block[SLOTS] >> 4;
    let slots_on = slots_on(block);
    if slots_on.len() != count {
        problems.push(format!("{count} DPI slots on in byte 11, {} in byte 12", slots_on.len()));
    }
    if active == 0 || active as usize > slots_on.len() {
        problems.push(format!("DPI slot {active} in use"));
    }
    for &slot in &slots_on {
        let raw = if separate_xy { block[DPIS + 2 * slot] } else { block[DPIS + slot] };
        settings.dpi_levels.push(to_dpi(block, raw));
    }
    if let Some((_, max, _)) = sensor(block) {
        if let Some(bad) = settings.dpi_levels.iter().find(|&&d| !(DPI_MIN..=max).contains(&d)) {
            problems.push(format!("DPI {bad}, outside {DPI_MIN}–{max}"));
        }
        // Separate X and Y DPI are shown as X and not changed.
        if !separate_xy {
            limits.dpi = Some(DpiLimits { min: DPI_MIN, max, step: DPI_STEP });
        }
    }
    limits.polling_hz = POLLING.iter().map(|(_, hz)| *hz).collect();
    Parsed { settings, limits, active_dpi: (active > 0).then_some(active), slots_on, problems }
}

/// The bits of byte `i` a reading may change from the reference block: the DPI of the slots on, the
/// polling rate, and the slot in use, which the mouse's own DPI button changes.
fn changeable_bits(reference: &[u8], i: usize) -> u8 {
    let dpi_slot = i.checked_sub(DPIS).filter(|&s| s < DPI_SLOTS);
    match i {
        RATE => 0x0F,
        SLOTS => 0xF0,
        _ if reference[RATE] & SEPARATE_XY == 0 && dpi_slot.is_some_and(|s| slots_on(reference).contains(&s)) => 0xFF,
        _ => 0x00,
    }
}

/// The bytes where `block` differs from `reference` in something PeriHub never changes.
fn foreign_changes(reference: &[u8], block: &[u8]) -> Vec<usize> {
    if reference.len() != block.len() {
        return vec![block.len()];
    }
    (0..block.len()).filter(|&i| (reference[i] ^ block[i]) & !changeable_bits(reference, i) != 0).collect()
}

/// The block that turns `block`'s settings into `to`: only the DPI of the slots on and the polling
/// rate change, everything else stays as read.
fn rewrite(block: &[u8], parsed: &Parsed, to: &Settings) -> Result<Vec<u8>> {
    let mut new = block.to_vec();
    if to.dpi_levels != parsed.settings.dpi_levels {
        let (_, _, minus_one) = sensor(block).ok_or_else(|| Error::Invalid("DPI: unknown sensor".into()))?;
        if block[RATE] & SEPARATE_XY != 0 || to.dpi_levels.len() != parsed.slots_on.len() {
            return Err(Error::Invalid("DPI".into()));
        }
        for (slot, dpi) in parsed.slots_on.iter().zip(&to.dpi_levels) {
            let raw = dpi / 100 - u16::from(minus_one);
            new[DPIS + slot] = u8::try_from(raw).map_err(|_| Error::Invalid(format!("{dpi} DPI")))?;
        }
    }
    if to.polling_hz != parsed.settings.polling_hz {
        let code = POLLING
            .iter()
            .find(|(_, hz)| Some(*hz) == to.polling_hz)
            .map(|(c, _)| *c)
            .ok_or_else(|| Error::Invalid("polling rate".into()))?;
        new[RATE] = (block[RATE] & 0xF0) | code;
    }
    Ok(new)
}

pub struct Sinowealth {
    p: Params,
    /// The report the configuration answered on: 4 (short) or 6 (long).
    report: Option<u8>,
    /// The block read when the mouse was connected, or the last one PeriHub wrote and read back.
    reference: Option<Vec<u8>>,
}

impl Sinowealth {
    pub fn new(p: Params) -> Self {
        Self { p, report: None, reference: None }
    }

    fn firmware(&self, io: &mut Io) -> Result<String> {
        io.send_numbered(&READ_FIRMWARE)?;
        let mut answer = [0u8; CMD_SIZE];
        let n = io.read_numbered(CMD_REPORT, &mut answer)?;
        if n != CMD_SIZE || answer[1] != FIRMWARE {
            return Err(Error::Protocol(format!("firmware answer: {}", super::hex(&answer))));
        }
        let version = &answer[2..];
        if !version.iter().all(|b| b.is_ascii_graphic()) {
            return Err(Error::Protocol(format!("firmware {}", super::hex(version))));
        }
        Ok(String::from_utf8_lossy(version).into_owned())
    }

    /// The configuration block, report number included, as the mouse answers it: 123 to 167 bytes,
    /// or [`BLOCK`] bytes when the system does not say how many the mouse sent.
    fn block(&mut self, io: &mut Io) -> Result<Vec<u8>> {
        let reports = match self.report {
            Some(id) => vec![id],
            None => vec![CONFIG_SHORT, CONFIG_LONG],
        };
        let mut why = Vec::new();
        for id in reports {
            io.send_numbered(&READ_CONFIG)?;
            let mut buf = vec![0u8; BLOCK];
            match io.read_numbered(id, &mut buf) {
                Ok(n) if ((CONFIG_MIN..=CONFIG_MAX).contains(&n) || n == BLOCK) && buf[0] == id && buf[1] == CONFIG => {
                    self.report = Some(id);
                    buf.truncate(n);
                    return Ok(buf);
                }
                Ok(n) => why.push(format!(
                    "report {id} answered {n} bytes for command {:02x}, not {CONFIG_MIN}–{CONFIG_MAX} for {CONFIG:02x}",
                    buf[1]
                )),
                Err(e) => why.push(format!("report {id}: {e}")),
            }
        }
        Err(Error::Protocol(why.join("; ")))
    }
}

impl Driver for Sinowealth {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "libratbag (MIT)"
    }

    fn profiles(&self) -> u8 {
        1
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        true
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let firmware = match self.firmware(io) {
            Ok(firmware) => firmware,
            // A page with the test report rather than no page at all.
            Err(e @ (Error::Hid(_) | Error::Protocol(_))) => {
                return Ok(Reading {
                    problem: Some(format!("the firmware could not be read as documented: {e}")),
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };
        let mut reading = Reading { firmware: Some(firmware.clone()), ..Default::default() };
        if firmware != self.p.firmware {
            reading.problem = Some(format!(
                "firmware {firmware}: libratbag knows this mouse with firmware {}, so PeriHub reads only its firmware",
                self.p.firmware
            ));
            return Ok(reading);
        }
        let block = match self.block(io) {
            Ok(block) => block,
            Err(e) => {
                reading.problem = Some(format!("the settings did not come as documented: {e}"));
                return Ok(reading);
            }
        };
        let parsed = parse(&block);
        reading.settings = parsed.settings;
        reading.limits = parsed.limits;
        reading.active_dpi = parsed.active_dpi;
        let mut problems = parsed.problems;
        if block.len() == BLOCK {
            problems.push(format!(
                "the settings arrive as {BLOCK} bytes, so their real length, which a write must give, is unknown"
            ));
        } else if problems.is_empty() {
            match &self.reference {
                None => self.reference = Some(block),
                Some(reference) => {
                    let foreign = foreign_changes(reference, &block);
                    if !foreign.is_empty() {
                        problems.push(format!(
                            "bytes {foreign:?} of the settings differ from those read when the mouse was connected \
                             (another program changed them, or the reading is incomplete)"
                        ));
                    }
                }
            }
        }
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected settings: {}", problems.join(", ")));
        }
        Ok(reading)
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let reference =
            self.reference.clone().ok_or_else(|| Error::Invalid("the settings were not read yet".into()))?;
        let block = self.block(io)?;
        let foreign = foreign_changes(&reference, &block);
        if !foreign.is_empty() {
            return Err(Error::Protocol(format!(
                "bytes {foreign:?} of the settings differ from those read before: nothing was sent"
            )));
        }
        let parsed = parse(&block);
        if !parsed.problems.is_empty() {
            return Err(Error::Protocol(format!("unexpected settings: {}", parsed.problems.join(", "))));
        }
        if parsed.settings != *from {
            return Err(Error::Invalid("the mouse's settings changed since they were read: nothing was sent".into()));
        }
        let new = rewrite(&block, &parsed, to)?;
        let mut report = vec![0u8; BLOCK];
        report[..new.len()].copy_from_slice(&new);
        report[WRITE_LEN] = (new.len() - 8) as u8;
        io.send_numbered(&report)?;
        for _ in 0..READ_BACK_TRIES {
            std::thread::sleep(READ_BACK_WAIT);
            let Ok(back) = self.block(io) else { continue };
            if back.len() == new.len() && (0..new.len()).all(|i| i == WRITE_LEN || back[i] == new[i]) {
                self.reference = Some(new);
                return Ok(());
            }
        }
        Err(Error::Protocol("the mouse reads back a different block from the one it was sent".into()))
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("these mice store every change by themselves".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::task::start_for_test;
    use super::super::{find, models};
    use super::*;

    /// Where a reading right after a write stops being complete (OpenRGB).
    const INCOMPLETE_FROM: usize = 0x2C;

    /// A simulated Glorious mouse: its firmware, its configuration report and block, what it was
    /// last asked, every report it got, whether it stores what it is sent, and how many readings
    /// after a write come back with zeros from byte `0x2C`.
    struct Mouse {
        firmware: &'static [u8; 4],
        report: u8,
        block: Vec<u8>,
        asked: Option<u8>,
        sent: Vec<Vec<u8>>,
        stores: bool,
        incomplete_reads: usize,
        incomplete_left: usize,
        /// What a reading of the block measures, if not the block's own length (Windows).
        answer_len: Option<usize>,
    }

    type Shared = Arc<Mutex<Mouse>>;

    /// A Model O with a 131-byte block: PMW3360, 1000 Hz, slots 0–2 on (400, 800, 1600 DPI), slot 2
    /// in use. The bytes nobody knows hold their own offset, to see that they are kept.
    fn model_o_block(report: u8) -> Vec<u8> {
        let mut b: Vec<u8> = (0..131).map(|i| i as u8).collect();
        b[0] = report;
        b[1] = CONFIG;
        b[WRITE_LEN] = 0;
        b[SENSOR] = 0x06;
        b[RATE] = 0x04;
        b[SLOTS] = 0x23;
        b[SLOTS_OFF] = 0xF8;
        b[DPIS..DPIS + 8].copy_from_slice(&[3, 7, 15, 31, 0, 0, 0, 0]);
        b
    }

    fn mouse(report: u8) -> Shared {
        Arc::new(Mutex::new(Mouse {
            firmware: b"V103",
            report,
            block: model_o_block(report),
            asked: None,
            sent: Vec::new(),
            stores: true,
            incomplete_reads: 0,
            incomplete_left: 0,
            answer_len: None,
        }))
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        let (set, get) = (state.clone(), state);
        hid.0.lock().unwrap().numbered = Some((
            Box::new(move |data: &[u8]| {
                let mut m = set.lock().unwrap();
                m.sent.push(data.to_vec());
                if data[0] == CMD_REPORT {
                    m.asked = Some(data[1]);
                } else if data[0] == m.report && data[1] == CONFIG {
                    assert_eq!(data.len(), BLOCK, "the whole block is sent");
                    let size = m.block.len();
                    assert_eq!(data[WRITE_LEN] as usize, size - 8, "byte 3: the length minus 8");
                    assert!(data[size..].iter().all(|&b| b == 0), "zeros after the block");
                    if m.stores {
                        m.block = data[..size].to_vec();
                        m.block[WRITE_LEN] = 0;
                    }
                    m.incomplete_left = m.incomplete_reads;
                } else {
                    return false;
                }
                true
            }),
            Box::new(move |id: u8| {
                let mut m = get.lock().unwrap();
                match (id, m.asked) {
                    (CMD_REPORT, Some(FIRMWARE)) => {
                        Some([CMD_REPORT, FIRMWARE].iter().chain(m.firmware.iter()).copied().collect())
                    }
                    (id, Some(CONFIG)) if id == m.report => {
                        let mut block = m.block.clone();
                        if m.incomplete_left > 0 {
                            m.incomplete_left -= 1;
                            block[INCOMPLETE_FROM..].fill(0);
                        }
                        if let Some(n) = m.answer_len {
                            block.resize(n, 0);
                        }
                        Some(block)
                    }
                    _ => None,
                }
            }),
        ));
        hid
    }

    fn model_o() -> &'static Model {
        find(0x258A, 0x0036).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-sinowealth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn reads_only(state: &Shared) -> bool {
        state.lock().unwrap().sent.iter().all(|r| r[..] == READ_FIRMWARE || r[..] == READ_CONFIG)
    }

    fn writes(state: &Shared) -> Vec<Vec<u8>> {
        state.lock().unwrap().sent.iter().filter(|r| r[1] == CONFIG && r[0] != CMD_REPORT).cloned().collect()
    }

    #[test]
    fn only_the_glorious_ids_are_in_the_table() {
        let ids: Vec<(u16, u16)> = models()
            .filter(|m| matches!(m.protocol, Protocol::Sinowealth(_)))
            .flat_map(|m| m.product_ids.iter().map(move |id| (m.vendor_id, *id)))
            .collect();
        assert_eq!(ids, [(0x258A, 0x0036), (0x258A, 0x0033), (0x3794, 0xA000)]);
        // Generic SinoWealth ids shared with other brands, and the wireless Glorious mice.
        for id in [0x0027, 0x0029, 0x0051, 0x1007, 0x2011, 0x2012, 0x2022, 0x2023] {
            assert!(find(0x258A, id).is_none(), "{id:04x}");
        }
    }

    #[test]
    fn a_model_o_reads_as_documented_and_only_reads() {
        let state = mouse(CONFIG_SHORT);
        let task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, false, temp("read")).unwrap();
        assert_eq!(task.info.firmware.as_deref(), Some("V103"));
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600]);
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        assert_eq!(task.info.active_dpi, Some(2));
        assert_eq!(task.info.limits.dpi, Some(DpiLimits { min: 100, max: 12000, step: 100 }));
        assert!(task.info.changes_stay && !task.info.can_save && task.info.problem.is_none());
        assert!(reads_only(&state));
    }

    #[test]
    fn a_long_mouse_answers_on_report_6_and_is_written_there() {
        let state = mouse(CONFIG_LONG);
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("long")).unwrap();
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600]);
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(500);
        task.change(&wanted, false, None).unwrap();
        let written = writes(&state);
        assert_eq!(written.len(), 1);
        assert_eq!(written[0][0], CONFIG_LONG, "nothing is written on report 4");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_change_rewrites_only_dpi_and_polling_and_is_undone_unless_kept() {
        let state = mouse(CONFIG_SHORT);
        // As OpenRGB saw: the first reading after a write is incomplete.
        state.lock().unwrap().incomplete_reads = 1;
        let before = state.lock().unwrap().block.clone();
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("change")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels = vec![400, 1200, 3200];
        wanted.polling_hz = Some(250);
        task.change(&wanted, false, None).unwrap();
        assert_eq!(writes(&state).len(), 1, "one write, despite the incomplete reading after it");
        let after = state.lock().unwrap().block.clone();
        let changed: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
        assert_eq!(changed, [RATE, DPIS + 1, DPIS + 2]);
        assert_eq!((after[RATE], after[DPIS + 1], after[DPIS + 2]), (0x02, 11, 31));
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().block, before, "every byte as it was");
        assert_eq!(writes(&state).len(), 2);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn an_incomplete_reading_never_reaches_the_mouse() {
        let state = mouse(CONFIG_SHORT);
        state.lock().unwrap().incomplete_reads = usize::MAX;
        let before = state.lock().unwrap().block.clone();
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("zeros")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 500;
        assert!(task.change(&wanted, false, None).is_err());
        let written = writes(&state);
        assert!(!written.is_empty());
        for w in &written {
            assert_eq!(w[INCOMPLETE_FROM..before.len()], before[INCOMPLETE_FROM..], "no write carries the zeros");
        }
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn settings_changed_by_another_program_are_never_written_over() {
        let state = mouse(CONFIG_SHORT);
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("foreign")).unwrap();
        state.lock().unwrap().block[100] ^= 0xFF;
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 500;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
        assert!(task.info.problem.as_deref().unwrap().contains("[100]"));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn the_dpi_button_does_not_stop_a_change() {
        let state = mouse(CONFIG_SHORT);
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("button")).unwrap();
        state.lock().unwrap().block[SLOTS] = 0x33;
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 500;
        task.change(&wanted, false, None).unwrap();
        assert_eq!(state.lock().unwrap().block[SLOTS], 0x33, "the slot in use is kept");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_mouse_that_does_not_take_the_block_is_reported() {
        let state = mouse(CONFIG_SHORT);
        state.lock().unwrap().stores = false;
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("ignore")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 500;
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("reads back a different block"), "{error}");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn another_firmware_is_read_for_its_firmware_only() {
        let state = mouse(CONFIG_SHORT);
        state.lock().unwrap().firmware = b"V901";
        let task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("firmware")).unwrap();
        assert_eq!(task.info.firmware.as_deref(), Some("V901"));
        assert!(!task.info.editable && task.info.problem.as_deref().unwrap().contains("V103"));
        assert!(task.info.settings.dpi_levels.is_empty());
        assert!(state.lock().unwrap().sent.iter().all(|r| r[..] == READ_FIRMWARE));
    }

    #[test]
    fn a_collection_without_the_commands_still_gets_a_page() {
        let state = mouse(CONFIG_SHORT);
        let hid = fake(state.clone());
        // This collection takes no report 5: on Windows it may be the one with the block only.
        let (mut set, get) = hid.0.lock().unwrap().numbered.take().unwrap();
        hid.0.lock().unwrap().numbered = Some((Box::new(move |d: &[u8]| d[0] != CMD_REPORT && set(d)), get));
        let task = start_for_test(Box::new(hid), model_o(), 0x0036, true, temp("collection")).unwrap();
        assert!(!task.info.editable && task.info.problem.as_deref().unwrap().contains("firmware"));
    }

    #[test]
    fn a_block_of_unknown_length_is_shown_but_never_written() {
        let state = mouse(CONFIG_SHORT);
        state.lock().unwrap().answer_len = Some(BLOCK);
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("windows")).unwrap();
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600]);
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        assert!(!task.info.editable && task.info.problem.as_deref().unwrap().contains("520"));
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(500);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(reads_only(&state));
    }

    #[test]
    fn blocks_that_do_not_look_as_documented_leave_the_mouse_read_only() {
        type Damage = fn(&mut Vec<u8>);
        let cases: [(&str, Damage); 3] = [
            ("too short", |b| b.truncate(100)),
            ("slots on that do not match", |b| b[SLOTS_OFF] = 0xF0),
            ("an unknown polling code", |b| b[RATE] = 0x07),
        ];
        for (name, damage) in cases {
            let state = mouse(CONFIG_SHORT);
            damage(&mut state.lock().unwrap().block);
            let task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("odd")).unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
            assert!(reads_only(&state), "{name}");
        }
    }

    #[test]
    fn separate_x_and_y_dpi_are_shown_but_not_changed() {
        let state = mouse(CONFIG_SHORT);
        {
            let mut m = state.lock().unwrap();
            m.block[RATE] |= SEPARATE_XY;
            m.block[DPIS..DPIS + 6].copy_from_slice(&[3, 5, 7, 9, 15, 17]);
        }
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("xy")).unwrap();
        assert_eq!(task.info.settings.dpi_levels, [400, 800, 1600], "X of each slot");
        assert!(task.info.limits.dpi.is_none());
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels[0] = 500;
        assert!(task.change(&wanted, false, None).is_err());
        assert!(reads_only(&state));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_mouse_does_not_take_are_refused() {
        let state = mouse(CONFIG_SHORT);
        let mut task = start_for_test(Box::new(fake(state.clone())), model_o(), 0x0036, true, temp("refuse")).unwrap();
        for dpi in [0, 50, 12100, 450] {
            let mut wanted = task.info.settings.clone();
            wanted.dpi_levels[0] = dpi;
            assert!(task.change(&wanted, false, None).is_err(), "{dpi}");
        }
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(2000);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(reads_only(&state));
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = mouse(CONFIG_SHORT);
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), true);
        let refused: [&[u8]; 6] = [
            &[CMD_REPORT, 0x75, 0, 0, 0, 0],    // firmware update mode
            &[CMD_REPORT, 0x02, 0x01, 0, 0, 0], // profile switch
            &[CMD_REPORT, 0x02, 0, 0, 0, 0],    // active profile: not needed, not sent
            &[CMD_REPORT, 0x12, 0, 0, 0, 0],    // buttons
            &[CONFIG_SHORT, 0x12, 0, 80],       // writing buttons
            &[CMD_REPORT, 0x1A, 0, 0, 0, 0],    // debounce
        ];
        for report in refused {
            assert!(io.send_numbered(report).is_err(), "{report:02x?}");
        }
        assert!(io.send_numbered(&[0, 0x11]).is_err(), "report number 0");
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), false);
        assert!(io.send_numbered(&model_o_block(CONFIG_SHORT)).is_err(), "no store on a read-only connection");
        assert!(state.lock().unwrap().sent.is_empty());
    }
}

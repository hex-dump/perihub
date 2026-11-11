//! The Logitech G600, a wired MMO mouse with three onboard profiles in vendor feature reports. The
//! protocol is in `docs/protocols/logitech-g600.md`, from libratbag's G600 driver (MIT, see
//! `THIRD-PARTY-NOTICES.md`); OpenRGB names its interface and usage page. None has been tried with
//! PeriHub.
//!
//! Writing a profile stores it: every change is treated as one that stays, like the Razer mice. A
//! profile is written only on top of the one read when the mouse was connected (or the last one
//! PeriHub wrote and read back), and only the DPI of the slots that are on, the polling rate and the
//! lighting change: buttons, G-Shift and unknown bytes go back as read.

use std::time::Duration;

use super::task::Driver;
use super::{
    ButtonView, Choice, Command, DpiLimits, Effect, Io, Kind, Led, LedLimits, Limits, Model, Protocol, Reading,
    Settings,
};
use crate::color::Rgb;
use crate::error::{Error, Result};

/// The vendor collection, on USB interface 1 (OpenRGB).
const USAGE_PAGE: u16 = 0xFF80;
const INTERFACE: i32 = 1;

const ACTIVE: u8 = 0xF0;
const ACTIVE_LEN: usize = 4;
/// The profiles' reports, 1 to 3.
const PROFILES: [u8; 3] = [0xF3, 0xF4, 0xF5];
const PROFILE_LEN: usize = 154;
/// Feature reports are read into a buffer this long: on Windows a collection may pad them.
const READ_BUF: usize = 256;

/// Everything PeriHub may send: a profile (stored), and the DPI slot in use, put back after writing
/// the profile in use. Switching profile (`f0 8x`) and the lighting report `f1` are left out.
pub const COMMANDS: &[Command] = &[
    Command { prefix: &[0xF3], at: 0, effect: Effect::Save, name: "store profile 1" },
    Command { prefix: &[0xF4], at: 0, effect: Effect::Save, name: "store profile 2" },
    Command { prefix: &[0xF5], at: 0, effect: Effect::Save, name: "store profile 3" },
    Command { prefix: &[ACTIVE, 0x40], at: 0, effect: Effect::Change, name: "DPI slot 1 in use" },
    Command { prefix: &[ACTIVE, 0x42], at: 0, effect: Effect::Change, name: "DPI slot 2 in use" },
    Command { prefix: &[ACTIVE, 0x44], at: 0, effect: Effect::Change, name: "DPI slot 3 in use" },
    Command { prefix: &[ACTIVE, 0x46], at: 0, effect: Effect::Change, name: "DPI slot 4 in use" },
];

/// Offsets in a profile, report number included.
const RED: usize = 1;
const EFFECT: usize = 4;
const DURATION: usize = 5;
const FREQUENCY: usize = 11;
const DPIS: usize = 14;
const DPI_SLOTS: usize = 4;
const BUTTONS: usize = 31;
const BUTTON_COUNT: usize = 20;

const DPI_MIN: u16 = 200;
const DPI_MAX: u16 = 8200;
const DPI_STEP: u16 = 50;
const DURATION_MAX: u8 = 15;
const DURATION_OPTION: &str = "led_duration";

const EFFECTS: [(u8, &str); 3] = [(0, "Steady"), (1, "Breathing"), (2, "Colour cycle")];

const READ_BACK_TRIES: usize = 3;
const READ_BACK_WAIT: Duration = Duration::from_millis(50);

pub const MODELS: &[Model] = &[Model {
    vendor_id: 0x046D,
    product_ids: &[0xC24A],
    brand: "Logitech",
    name: "G600",
    kind: Kind::Mouse,
    interface: Some(INTERFACE),
    usage_page: Some(USAGE_PAGE),
    protocol: Protocol::LogitechG600,
}];

fn polling_hz(code: u8) -> Option<u16> {
    (code <= 7).then(|| 1000 / (code as u16 + 1))
}

fn polling_code(hz: u16) -> Option<u8> {
    (0..=7).find(|&code| polling_hz(code) == Some(hz))
}

fn key_name(key: u8) -> String {
    match key {
        0x04..=0x1D => format!("{}", (b'A' + key - 0x04) as char),
        0x1E..=0x26 => format!("{}", key - 0x1D),
        0x27 => "0".into(),
        0x28 => "Enter".into(),
        0x29 => "Esc".into(),
        0x2B => "Tab".into(),
        0x2C => "Space".into(),
        0x3A..=0x45 => format!("F{}", key - 0x39),
        _ => format!("key code {key:02x}"),
    }
}

fn action_name(code: u8, modifiers: u8, key: u8) -> String {
    match code {
        0x00 if modifiers == 0 && key == 0 => "Nothing".into(),
        0x00 => {
            let names = ["Ctrl", "Shift", "Alt", "Win", "Right Ctrl", "Right Shift", "Right Alt", "Right Win"];
            let mut parts: Vec<String> =
                (0..8).filter(|bit| modifiers & (1 << bit) != 0).map(|bit| names[bit].to_string()).collect();
            if key != 0 {
                parts.push(key_name(key));
            }
            format!("Key {}", parts.join("+"))
        }
        0x01 => "Left click".into(),
        0x02 => "Right click".into(),
        0x03 => "Middle click".into(),
        0x04 => "Back".into(),
        0x05 => "Forward".into(),
        0x11 => "DPI up".into(),
        0x12 => "DPI down".into(),
        0x13 => "DPI cycle".into(),
        0x14 => "Profile cycle".into(),
        0x15 => "DPI shift".into(),
        0x17 => "G-Shift".into(),
        _ => format!("Action {code:02x}"),
    }
}

/// The slots that are on: a DPI of 0 turns a slot off.
fn slots_on(profile: &[u8]) -> Vec<usize> {
    (0..DPI_SLOTS).filter(|&i| profile[DPIS + i] != 0).collect()
}

/// What a profile says, and what in it did not look as documented.
fn parse(profile: &[u8], problems: &mut Vec<String>) -> (Settings, Limits) {
    let mut settings = Settings::default();
    for i in slots_on(profile) {
        let dpi = profile[DPIS + i] as u16 * DPI_STEP;
        if !(DPI_MIN..=DPI_MAX).contains(&dpi) {
            problems.push(format!("DPI {dpi}"));
        }
        settings.dpi_levels.push(dpi);
    }
    if settings.dpi_levels.is_empty() {
        problems.push("no DPI slot on".into());
    }
    match polling_hz(profile[FREQUENCY]) {
        Some(hz) => settings.polling_hz = Some(hz),
        None => problems.push(format!("polling rate code {:02x}", profile[FREQUENCY])),
    }
    if !EFFECTS.iter().any(|(e, _)| *e == profile[EFFECT]) {
        problems.push(format!("lighting effect {:02x}", profile[EFFECT]));
    }
    let color = Rgb::from_bytes(&profile[RED..RED + 3]);
    settings.leds.push(Led { mode: profile[EFFECT], brightness: None, color });
    let duration = profile[DURATION];
    settings.options.insert(DURATION_OPTION.into(), duration as u16);
    let mut durations: Vec<u16> = (1..=DURATION_MAX as u16).collect();
    if !durations.contains(&(duration as u16)) {
        durations.insert(0, duration as u16);
    }
    let limits = Limits {
        dpi: Some(DpiLimits { min: DPI_MIN, max: DPI_MAX, step: DPI_STEP }),
        polling_hz: (0..=7).rev().filter_map(polling_hz).collect(),
        options: vec![Choice {
            id: DURATION_OPTION,
            label: "Effect duration",
            values: durations.into_iter().map(|s| (s, format!("{s} s"))).collect(),
        }],
        leds: vec![LedLimits { name: "Lighting".into(), modes: EFFECTS.to_vec(), brightness_step: None, color: true }],
    };
    (settings, limits)
}

/// The profile that turns `profile`'s settings into `to`: only the DPI of the slots that are on, the
/// polling rate and the lighting change.
fn rewrite(profile: &[u8], to: &Settings) -> Result<Vec<u8>> {
    let mut new = profile.to_vec();
    let slots = slots_on(profile);
    if to.dpi_levels.len() != slots.len() {
        return Err(Error::Invalid("DPI slots".into()));
    }
    for (slot, dpi) in slots.iter().zip(&to.dpi_levels) {
        if !(DPI_MIN..=DPI_MAX).contains(dpi) || dpi % DPI_STEP != 0 {
            return Err(Error::Invalid(format!("{dpi} DPI")));
        }
        new[DPIS + slot] = (dpi / DPI_STEP) as u8;
    }
    let hz = to.polling_hz.ok_or_else(|| Error::Invalid("polling rate".into()))?;
    new[FREQUENCY] = polling_code(hz).ok_or_else(|| Error::Invalid(format!("{hz} Hz")))?;
    let [led] = to.leds.as_slice() else { return Err(Error::Invalid("lighting".into())) };
    if !EFFECTS.iter().any(|(e, _)| *e == led.mode) {
        return Err(Error::Invalid(format!("lighting effect {}", led.mode)));
    }
    new[EFFECT] = led.mode;
    new[RED..RED + 3].copy_from_slice(&led.color.to_bytes());
    let duration = *to.options.get(DURATION_OPTION).ok_or_else(|| Error::Invalid("effect duration".into()))?;
    new[DURATION] = u8::try_from(duration)
        .ok()
        .filter(|d| *d <= DURATION_MAX || *d == profile[DURATION])
        .ok_or_else(|| Error::Invalid(format!("effect duration {duration}")))?;
    Ok(new)
}

pub struct G600 {
    /// The profile in use (0–2) and its DPI slot (0–3), as last read.
    active: Option<(usize, u8)>,
    /// The profile read when the mouse was connected, or the last one written and read back, by
    /// profile.
    reference: [Option<Vec<u8>>; 3],
}

impl G600 {
    pub fn new() -> Self {
        Self { active: None, reference: [None, None, None] }
    }

    fn read_active(io: &mut Io) -> Result<(usize, u8)> {
        let mut buf = [0u8; READ_BUF];
        let n = io.read_numbered(ACTIVE, &mut buf)?;
        if n < ACTIVE_LEN || buf[0] != ACTIVE {
            return Err(Error::Protocol(format!("active profile answer of {n} bytes")));
        }
        let profile = (buf[1] >> 4) as usize;
        if profile >= PROFILES.len() {
            return Err(Error::Protocol(format!("profile {profile} in use")));
        }
        Ok((profile, (buf[1] >> 1) & 0x03))
    }

    fn read_profile(io: &mut Io, profile: usize) -> Result<Vec<u8>> {
        let id = PROFILES[profile];
        let mut buf = vec![0u8; READ_BUF];
        let n = io.read_numbered(id, &mut buf)?;
        if n < PROFILE_LEN || buf[0] != id {
            return Err(Error::Protocol(format!("profile {} answer of {n} bytes", profile + 1)));
        }
        buf.truncate(PROFILE_LEN);
        Ok(buf)
    }
}

impl Default for G600 {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver for G600 {
    fn commands(&self) -> Vec<Command> {
        COMMANDS.to_vec()
    }

    fn source(&self) -> &'static str {
        "libratbag (MIT)"
    }

    fn profiles(&self) -> u8 {
        PROFILES.len() as u8
    }

    fn can_save(&self) -> bool {
        false
    }

    fn changes_stay(&self) -> bool {
        true
    }

    fn read(&mut self, io: &mut Io) -> Result<Reading> {
        let (profile, slot) = match Self::read_active(io) {
            Ok(active) => active,
            Err(e @ (Error::Hid(_) | Error::Protocol(_))) => {
                return Ok(Reading {
                    problem: Some(format!("the profile in use could not be read as documented: {e}")),
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };
        self.active = Some((profile, slot));
        let mut reading = Reading { active_profile: Some(profile as u8 + 1), ..Default::default() };
        let block = match Self::read_profile(io, profile) {
            Ok(block) => block,
            Err(e) => {
                reading.problem = Some(format!("the profile did not come as documented: {e}"));
                return Ok(reading);
            }
        };
        let mut problems = Vec::new();
        let (settings, limits) = parse(&block, &mut problems);
        let on = slots_on(&block);
        reading.active_dpi = on.iter().position(|&s| s == slot as usize).map(|p| p as u8 + 1);
        reading.settings = settings;
        reading.limits = limits;
        reading.buttons = (0..BUTTON_COUNT)
            .map(|i| {
                let b = &block[BUTTONS + 3 * i..BUTTONS + 3 * i + 3];
                ButtonView { name: format!("Button {}", i + 1), action: action_name(b[0], b[1], b[2]) }
            })
            .collect();
        if problems.is_empty() {
            match &self.reference[profile] {
                None => self.reference[profile] = Some(block),
                Some(reference) if *reference != block => problems.push(format!(
                    "profile {} differs from the one read when the mouse was connected: another program changed it",
                    profile + 1
                )),
                Some(_) => {}
            }
        }
        if !problems.is_empty() {
            reading.problem = Some(format!("unexpected settings: {}", problems.join(", ")));
        }
        Ok(reading)
    }

    fn apply(&mut self, io: &mut Io, from: &Settings, to: &Settings) -> Result<()> {
        let (profile, slot) = Self::read_active(io)?;
        let reference =
            self.reference[profile].clone().ok_or_else(|| Error::Invalid("this profile was not read yet".into()))?;
        let block = Self::read_profile(io, profile)?;
        if block != reference {
            return Err(Error::Protocol(format!(
                "profile {} differs from the one read before: nothing was sent",
                profile + 1
            )));
        }
        let mut problems = Vec::new();
        let (settings, _) = parse(&block, &mut problems);
        if !problems.is_empty() || settings != *from {
            return Err(Error::Invalid("the mouse's settings changed since they were read: nothing was sent".into()));
        }
        let new = rewrite(&block, to)?;
        io.send_numbered(&new)?;
        let mut stored = false;
        for _ in 0..READ_BACK_TRIES {
            std::thread::sleep(READ_BACK_WAIT);
            if Self::read_profile(io, profile).is_ok_and(|back| back == new) {
                stored = true;
                break;
            }
        }
        // Writing the profile in use may bring back its default DPI slot: the one in use goes back.
        if let Err(e) = io.send_numbered(&[ACTIVE, 0x40 | (slot << 1), 0, 0]) {
            log::info!("G600: the DPI slot in use was not put back: {e}");
        }
        if !stored {
            return Err(Error::Protocol("the mouse reads back a different profile from the one it was sent".into()));
        }
        self.reference[profile] = Some(new);
        Ok(())
    }

    fn save(&mut self, _io: &mut Io) -> Result<()> {
        Err(Error::Invalid("this mouse stores every change by itself".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::fake::FakeHid;
    use super::super::find;
    use super::super::task::start_for_test;
    use super::*;

    /// A simulated G600: its three profiles, the profile and DPI slot in use, and every report it got.
    struct Mouse {
        profiles: [Vec<u8>; 3],
        profile: u8,
        slot: u8,
        sent: Vec<Vec<u8>>,
        stores: bool,
    }

    type Shared = Arc<Mutex<Mouse>>;

    /// Profile `n`: DPI 800, 1600, off, 3200 (default slot 1), 1000 Hz, red breathing for 4 s, and
    /// unknown bytes that hold their own offset, to see that they are kept.
    fn profile(n: usize) -> Vec<u8> {
        let mut p: Vec<u8> = (0..PROFILE_LEN).map(|i| i as u8).collect();
        p[0] = PROFILES[n];
        p[RED..RED + 3].copy_from_slice(&[0xFF, 0, 0]);
        p[EFFECT] = 1;
        p[DURATION] = 4;
        p[FREQUENCY] = 0;
        p[13] = 1;
        p[DPIS..DPIS + 4].copy_from_slice(&[16, 32, 0, 64]);
        for i in 0..BUTTON_COUNT {
            p[BUTTONS + 3 * i..BUTTONS + 3 * i + 3].copy_from_slice(&[0, 0, 0]);
        }
        p[BUTTONS..BUTTONS + 3].copy_from_slice(&[0x01, 0, 0]);
        p[BUTTONS + 3..BUTTONS + 6].copy_from_slice(&[0x00, 0x01, 0x06]);
        p
    }

    fn mouse() -> Shared {
        Arc::new(Mutex::new(Mouse {
            profiles: [profile(0), profile(1), profile(2)],
            profile: 1,
            slot: 1,
            sent: Vec::new(),
            stores: true,
        }))
    }

    fn fake(state: Shared) -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        let (set, get) = (state.clone(), state);
        hid.0.lock().unwrap().numbered = Some((
            Box::new(move |data: &[u8]| {
                let mut m = set.lock().unwrap();
                m.sent.push(data.to_vec());
                match data[0] {
                    ACTIVE => {
                        assert_eq!(data.len(), ACTIVE_LEN);
                        assert_eq!(data[1] & 0xF9, 0x40, "only the DPI slot in use is switched");
                        m.slot = (data[1] >> 1) & 3;
                    }
                    id => {
                        let n = PROFILES.iter().position(|&p| p == id).expect("a profile report");
                        assert_eq!(data.len(), PROFILE_LEN);
                        if m.stores {
                            m.profiles[n] = data.to_vec();
                        }
                        // Writing the profile in use brings back its default DPI slot.
                        if n == m.profile as usize {
                            m.slot = m.profiles[n][13] - 1;
                        }
                    }
                }
                true
            }),
            Box::new(move |id: u8| {
                let m = get.lock().unwrap();
                match id {
                    ACTIVE => Some(vec![ACTIVE, (m.profile << 4) | (m.slot << 1), 0, 0]),
                    _ => PROFILES.iter().position(|&p| p == id).map(|n| m.profiles[n].clone()),
                }
            }),
        ));
        hid
    }

    fn g600() -> &'static Model {
        find(0x046D, 0xC24A).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-g600-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn writes(state: &Shared) -> Vec<Vec<u8>> {
        state.lock().unwrap().sent.iter().filter(|r| r[0] != ACTIVE).cloned().collect()
    }

    #[test]
    fn the_profile_in_use_reads_as_documented_and_only_reads() {
        let state = mouse();
        let task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, false, temp("read")).unwrap();
        assert_eq!(task.info.active_profile, Some(2));
        assert_eq!(task.info.profiles, 3);
        assert_eq!(task.info.settings.dpi_levels, [800, 1600, 3200], "the slot that is off is left out");
        assert_eq!(task.info.active_dpi, Some(2));
        assert_eq!(task.info.settings.polling_hz, Some(1000));
        assert_eq!(task.info.settings.leds, [Led { mode: 1, brightness: None, color: Rgb::new(255, 0, 0) }]);
        assert_eq!(task.info.settings.options[DURATION_OPTION], 4);
        assert_eq!(task.info.buttons[0].action, "Left click");
        assert_eq!(task.info.buttons[1].action, "Key Ctrl+C");
        assert_eq!(task.info.buttons[2].action, "Nothing");
        assert!(task.info.changes_stay && !task.info.can_save && task.info.problem.is_none());
        assert!(state.lock().unwrap().sent.is_empty(), "reading sends nothing");
    }

    #[test]
    fn a_change_rewrites_only_what_was_changed_and_is_undone_unless_kept() {
        let state = mouse();
        let before = state.lock().unwrap().profiles.clone();
        let mut task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, true, temp("change")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.dpi_levels = vec![800, 2000, 3200];
        wanted.polling_hz = Some(500);
        wanted.leds[0] = Led { mode: 0, brightness: None, color: Rgb::new(0, 0x80, 0xFF) };
        wanted.options.insert(DURATION_OPTION.into(), 6);
        task.change(&wanted, false, None).unwrap();
        let after = state.lock().unwrap().profiles.clone();
        assert_eq!(after[0], before[0]);
        assert_eq!(after[2], before[2], "only the profile in use");
        let changed: Vec<usize> = (0..PROFILE_LEN).filter(|&i| before[1][i] != after[1][i]).collect();
        assert_eq!(changed, [RED, RED + 1, RED + 2, EFFECT, DURATION, FREQUENCY, DPIS + 1]);
        assert_eq!(state.lock().unwrap().slot, 1, "the DPI slot in use is put back");
        assert_eq!(task.info.active_dpi, Some(2));
        task.expire_undo();
        crate::worker::DeviceTask::tick(&mut task).unwrap();
        assert_eq!(state.lock().unwrap().profiles, before, "every byte as it was");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_profile_changed_by_another_program_is_never_written_over() {
        let state = mouse();
        let mut task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, true, temp("foreign")).unwrap();
        state.lock().unwrap().profiles[1][100] ^= 0xFF;
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(500);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(writes(&state).is_empty());
        assert!(task.info.problem.is_some() && !task.info.editable);
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn a_mouse_that_does_not_take_the_profile_is_reported() {
        let state = mouse();
        state.lock().unwrap().stores = false;
        let mut task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, true, temp("ignore")).unwrap();
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(250);
        let error = task.change(&wanted, false, None).unwrap_err().to_string();
        assert!(error.contains("reads back a different profile"), "{error}");
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn values_the_mouse_does_not_take_are_refused() {
        let state = mouse();
        let mut task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, true, temp("refuse")).unwrap();
        for dpi in [0, 150, 8250, 825] {
            let mut wanted = task.info.settings.clone();
            wanted.dpi_levels[0] = dpi;
            assert!(task.change(&wanted, false, None).is_err(), "{dpi}");
        }
        let mut wanted = task.info.settings.clone();
        wanted.polling_hz = Some(300);
        assert!(task.change(&wanted, false, None).is_err());
        let mut wanted = task.info.settings.clone();
        wanted.leds[0].mode = 3;
        assert!(task.change(&wanted, false, None).is_err());
        let mut wanted = task.info.settings.clone();
        wanted.options.insert(DURATION_OPTION.into(), 16);
        assert!(task.change(&wanted, false, None).is_err());
        assert!(state.lock().unwrap().sent.is_empty());
        let _ = std::fs::remove_dir_all(task.backups());
    }

    #[test]
    fn profiles_that_do_not_look_as_documented_leave_the_mouse_read_only() {
        type Damage = fn(&mut Mouse);
        let cases: [(&str, Damage); 4] = [
            ("profile 4 in use", |m| m.profile = 3),
            ("unknown polling code", |m| m.profiles[1][FREQUENCY] = 9),
            ("unknown effect", |m| m.profiles[1][EFFECT] = 5),
            ("every slot off", |m| m.profiles[1][DPIS..DPIS + 4].fill(0)),
        ];
        for (name, damage) in cases {
            let state = mouse();
            damage(&mut state.lock().unwrap());
            let task = start_for_test(Box::new(fake(state.clone())), g600(), 0xC24A, true, temp("odd")).unwrap();
            assert!(task.info.problem.is_some() && !task.info.editable, "{name}");
            assert!(state.lock().unwrap().sent.is_empty(), "{name}");
        }
    }

    #[test]
    fn commands_outside_the_list_never_leave() {
        let state = mouse();
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), true);
        for report in [
            &[ACTIVE, 0x80, 0, 0][..], // profile 1
            &[ACTIVE, 0x90, 0, 0],     // profile 2
            &[0xF1, 0xFF, 0, 0, 0, 0, 0, 0],
            &[0xF2, 0, 0, 0],
            &[0xF6, 0, 0, 0],
        ] {
            assert!(io.send_numbered(report).is_err(), "{report:02x?}");
        }
        let mut io = Io::new(Box::new(fake(state.clone())), COMMANDS.to_vec(), false);
        assert!(io.send_numbered(&profile(0)).is_err(), "no store on a read-only connection");
        assert!(state.lock().unwrap().sent.is_empty());
    }
}

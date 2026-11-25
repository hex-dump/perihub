//! App profiles: while an app is in focus, the keyboard shows one of the saved lightings and the
//! mouse switches to one of its onboard profiles. When no rule matches, both go back to how they were.
//!
//! Only the active profile changes on the mouse: its memory is never written. Key mappings are not
//! part of app profiles, because changing them writes the keyboard's memory.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::config::{AppProfiles, LightingPreset};
use crate::error::Result;
use crate::focus;
use crate::manager::{DeviceStatus, Manager};

/// Apps seen recently, offered when adding a rule.
const RECENT_APPS: usize = 12;
/// How long focus must settle before sending profile changes: skips transient window flips (such
/// as Alt-Tab, taskbar clicks or elevated windows polling `None` on Windows) so a wireless mouse is
/// not flooded with rapid `0x8100` switches.
const FOCUS_DEBOUNCE: Duration = Duration::from_millis(800);

#[derive(Default)]
struct PendingUpdate {
    deadline: Option<Instant>,
    stopped: bool,
}

#[derive(Default)]
struct Debouncer {
    pending: Mutex<PendingUpdate>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    profiles: AppProfiles,
    presets: Vec<LightingPreset>,
    focused: Option<String>,
    recent: Vec<String>,
    /// Preset shown on the keyboard right now (`None`: the normal lighting).
    lighting: Option<String>,
    /// Profile a rule activated on the mouse, and the profile to go back to afterwards.
    mouse: Option<u8>,
    mouse_before: Option<u8>,
    keyboard_connected: bool,
    mouse_connected: bool,
}

pub struct AppProfileEngine {
    manager: Manager,
    state: Mutex<State>,
    /// Held while changing the devices, so two updates never interleave.
    applying: Mutex<()>,
    watching: AtomicBool,
    debouncer_started: AtomicBool,
    debouncer: Arc<Debouncer>,
    debounce_delay: Duration,
}

impl Drop for AppProfileEngine {
    fn drop(&mut self) {
        self.debouncer.pending.lock().unwrap().stopped = true;
        self.debouncer.wake.notify_all();
    }
}

impl AppProfileEngine {
    pub fn new(manager: Manager, profiles: AppProfiles, presets: Vec<LightingPreset>) -> Arc<Self> {
        Self::with_debounce(manager, profiles, presets, FOCUS_DEBOUNCE)
    }

    fn with_debounce(
        manager: Manager,
        profiles: AppProfiles,
        presets: Vec<LightingPreset>,
        debounce_delay: Duration,
    ) -> Arc<Self> {
        let state = State { profiles, presets, ..State::default() };
        Arc::new(Self {
            manager,
            state: Mutex::new(state),
            applying: Mutex::new(()),
            watching: AtomicBool::new(false),
            debouncer_started: AtomicBool::new(false),
            debouncer: Arc::new(Debouncer::default()),
            debounce_delay,
        })
    }

    /// Starts following the focused app (once). `notify` is called on every change, for the interface.
    pub fn watch_focus(self: &Arc<Self>, notify: impl Fn(Option<&str>) + Send + 'static) {
        if self.watching.swap(true, Ordering::SeqCst) {
            return;
        }
        self.ensure_debouncer();
        let engine = self.clone();
        let started = focus::watch(move |app| {
            if engine.focus_changed(app) {
                notify(engine.focused().as_deref());
            }
        });
        if !started {
            self.watching.store(false, Ordering::SeqCst);
        }
    }

    fn ensure_debouncer(self: &Arc<Self>) {
        if self.debouncer_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(self);
        let debouncer = Arc::clone(&self.debouncer);
        let spawned = std::thread::Builder::new().name("perihub-app-profiles".into()).spawn(move || {
            loop {
                let should_update = {
                    let mut pending = debouncer.pending.lock().unwrap();
                    loop {
                        if pending.stopped || weak.strong_count() == 0 {
                            return;
                        }
                        let Some(at) = pending.deadline else {
                            pending = debouncer.wake.wait(pending).unwrap();
                            continue;
                        };
                        let now = Instant::now();
                        if now >= at {
                            pending.deadline = None;
                            break true;
                        }
                        let (guard, _) =
                            debouncer.wake.wait_timeout(pending, at.saturating_duration_since(now)).unwrap();
                        pending = guard;
                    }
                };
                if should_update {
                    let Some(engine) = weak.upgrade() else { break };
                    engine.update();
                }
            }
        });
        if spawned.is_err() {
            self.debouncer_started.store(false, Ordering::SeqCst);
        }
    }

    fn schedule_update(self: &Arc<Self>) {
        if self.debounce_delay.is_zero() {
            self.debouncer.pending.lock().unwrap().deadline = None;
            self.update();
            return;
        }
        self.ensure_debouncer();
        self.debouncer.pending.lock().unwrap().deadline = Some(Instant::now() + self.debounce_delay);
        self.debouncer.wake.notify_one();
    }

    pub fn set_profiles(&self, profiles: AppProfiles, presets: Vec<LightingPreset>) {
        self.debouncer.pending.lock().unwrap().deadline = None;
        {
            let mut state = self.state.lock().unwrap();
            state.profiles = profiles;
            state.presets = presets;
        }
        self.update();
    }

    /// Returns `true` if the focused app changed.
    fn focus_changed(self: &Arc<Self>, app: Option<String>) -> bool {
        {
            let mut state = self.state.lock().unwrap();
            if state.focused == app {
                return false;
            }
            if let Some(app) = &app {
                state.recent.retain(|a| a != app);
                state.recent.insert(0, app.clone());
                state.recent.truncate(RECENT_APPS);
            }
            state.focused = app;
        }
        self.schedule_update();
        true
    }

    pub fn focused(&self) -> Option<String> {
        self.state.lock().unwrap().focused.clone()
    }

    pub fn recent_apps(&self) -> Vec<String> {
        self.state.lock().unwrap().recent.clone()
    }

    /// The normal lighting was just applied from the interface.
    pub fn lighting_reset(&self) {
        self.state.lock().unwrap().lighting = None;
    }

    /// A device that connects starts from its own settings, so the matching rule is applied again.
    pub fn devices_changed(&self, status: &DeviceStatus) {
        self.debouncer.pending.lock().unwrap().deadline = None;
        {
            let mut state = self.state.lock().unwrap();
            if state.keyboard_connected != status.keyboard.is_some() {
                state.keyboard_connected = status.keyboard.is_some();
                state.lighting = None;
            }
            if state.mouse_connected != status.mouse.is_some() {
                state.mouse_connected = status.mouse.is_some();
                state.mouse = None;
                state.mouse_before = None;
            }
        }
        self.update();
    }

    fn update(&self) {
        // Read-only mode: nothing is sent; the rule is applied at the next change after it is off.
        if crate::writes::read_only() {
            return;
        }
        let _applying = self.applying.lock().unwrap();
        let (lighting, mouse) = self.targets();
        if let Err(error) = self.apply_lighting(lighting) {
            log::warn!("app profiles: keyboard lighting not changed: {error}");
        }
        if let Err(error) = self.apply_mouse(mouse) {
            log::warn!("app profiles: mouse profile not changed: {error}");
        }
    }

    /// What the matching rule asks for: a preset name and a mouse profile.
    fn targets(&self) -> (Option<String>, Option<u8>) {
        let state = self.state.lock().unwrap();
        let rule = state
            .focused
            .as_deref()
            .filter(|_| state.profiles.enabled)
            .and_then(|app| state.profiles.rules.iter().find(|r| r.app.trim().eq_ignore_ascii_case(app)));
        let lighting =
            rule.and_then(|r| r.lighting.clone()).filter(|name| state.presets.iter().any(|p| &p.name == name));
        (lighting, rule.and_then(|r| r.mouse_profile))
    }

    fn apply_lighting(&self, target: Option<String>) -> Result<()> {
        let lighting = {
            let state = self.state.lock().unwrap();
            if state.lighting == target || !state.keyboard_connected {
                return Ok(());
            }
            match &target {
                Some(name) => state.presets.iter().find(|p| &p.name == name).map(|p| p.lighting.clone()),
                None => None,
            }
        };
        let lighting = lighting.unwrap_or_else(|| self.manager.keyboard_config().lighting);
        self.manager.keyboard()?.call(move |k| k.set_lighting(lighting))?;
        self.state.lock().unwrap().lighting = target;
        Ok(())
    }

    fn apply_mouse(&self, target: Option<u8>) -> Result<()> {
        let before = {
            let state = self.state.lock().unwrap();
            if state.mouse == target || !state.mouse_connected {
                return Ok(());
            }
            state.mouse_before
        };
        // Untested models and Bluetooth connections are read-only: their profile is left alone.
        if !self.manager.status().mouse.is_some_and(|m| m.verified && m.bluetooth.is_none()) {
            return Ok(());
        }
        let back = self.manager.mouse()?.call(move |m| {
            let active = m.active_profile()?;
            match target {
                Some(number) => {
                    if m.is_profile_enabled(number)? {
                        if number != active {
                            m.set_active_profile(number)?;
                        }
                        // Remember the profile in use before the first rule, to go back to it later.
                        Ok(before.or(Some(active)))
                    } else {
                        log::warn!("app profiles: profile {number} is disabled on the mouse; skipping");
                        if let Some(prev) = before
                            && prev != active
                            && m.is_profile_enabled(prev)?
                        {
                            m.set_active_profile(prev)?;
                        }
                        Ok(None)
                    }
                }
                None => {
                    if let Some(prev) = before
                        && prev != active
                        && m.is_profile_enabled(prev)?
                    {
                        m.set_active_profile(prev)?;
                    }
                    Ok(None)
                }
            }
        })?;
        let mut state = self.state.lock().unwrap();
        state.mouse = target;
        state.mouse_before = back;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppRule;
    use crate::hidpp::models::MODELS;
    use crate::hidpp::tests::{FakeDevice, reply};
    use crate::hidpp::{HidppDevice, OnboardInfo, feature, profile};
    use crate::mouse::MouseTask;
    use std::sync::Mutex as StdMutex;

    /// Minimal simulated G502 HERO over a receiver or cable for testing AppProfileEngine.
    #[derive(Clone)]
    struct TestMouse {
        features: &'static [u16],
        onboard: OnboardInfo,
        user: Vec<Vec<u8>>,
        active: [u8; 2],
        selected: Vec<[u8; 2]>,
    }

    fn sim_mouse(enabled: &[bool]) -> (MouseTask, Arc<StdMutex<TestMouse>>) {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/g502");
        let mut user: Vec<Vec<u8>> =
            (0..6).map(|s| std::fs::read(format!("{dir}/sector_{s:04x}.bin")).unwrap()).collect();
        user.resize(16, vec![0; 256]);
        user[0] = profile::build_directory(enabled, 256);
        let onboard = OnboardInfo {
            memory_model: 1,
            profile_format: 2,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 5,
            button_count: 11,
            sector_count: 16,
            sector_size: 256,
            has_gshift: true,
            has_dpi_shift: true,
        };
        let state = Arc::new(StdMutex::new(TestMouse {
            features: &[
                feature::ROOT,
                feature::FEATURE_SET,
                feature::DEVICE_INFO,
                feature::DEVICE_NAME,
                feature::ONBOARD_PROFILES,
                feature::ADJUSTABLE_DPI,
                feature::REPORT_RATE,
            ],
            onboard,
            user,
            active: [0, 1],
            selected: Vec::new(),
        }));
        let mouse = state.clone();
        let fake = FakeDevice::new(move |request| {
            let mut m = mouse.lock().unwrap();
            let (index, function) = (request[2] as usize, request[3] >> 4);
            let payload: Vec<u8> = if index == 0 && function == 1 {
                vec![4, 2, request[6]]
            } else if index == 0 && function == 0 {
                let id = u16::from_be_bytes([request[4], request[5]]);
                let found = m.features.iter().position(|&f| f == id).map_or(0, |i| i as u8);
                vec![found]
            } else {
                let p = &request[4..];
                match (m.features[index], function) {
                    (feature::DEVICE_INFO, 0) => vec![1],
                    (feature::DEVICE_INFO, 1) => vec![0, b'U', b'1', b' ', 0x27, 0x03, 0, 0],
                    (feature::FEATURE_SET, 0) => vec![m.features.len() as u8 - 1],
                    (feature::FEATURE_SET, 1) => {
                        let id = m.features[p[0] as usize];
                        vec![(id >> 8) as u8, id as u8, 0, 0]
                    }
                    (feature::DEVICE_NAME, 0) => vec![4],
                    (feature::DEVICE_NAME, 1) => b"G502\0\0\0\0\0\0\0\0\0\0\0\0".to_vec(),
                    (feature::ADJUSTABLE_DPI, 1) => vec![0, 0, 0x64, 0xE0, 0x32, 0x64, 0],
                    (feature::ADJUSTABLE_DPI, 2) => vec![0, 0x03, 0x20, 0x03, 0x20],
                    (feature::REPORT_RATE, 0) => vec![0x8B],
                    (feature::ONBOARD_PROFILES, 0) => {
                        let [hi, lo] = m.onboard.sector_size.to_be_bytes();
                        vec![
                            m.onboard.memory_model,
                            m.onboard.profile_format,
                            m.onboard.macro_format,
                            m.onboard.profile_count,
                            m.onboard.rom_profile_count,
                            m.onboard.button_count,
                            m.onboard.sector_count,
                            hi,
                            lo,
                            0x0A,
                        ]
                    }
                    (feature::ONBOARD_PROFILES, 2) => vec![1],
                    (feature::ONBOARD_PROFILES, 3) => {
                        let active = [p[0], p[1]];
                        m.active = active;
                        m.selected.push(active);
                        vec![]
                    }
                    (feature::ONBOARD_PROFILES, 4) => m.active.to_vec(),
                    (feature::ONBOARD_PROFILES, 5) => {
                        let sector = u16::from_be_bytes([p[0], p[1]]) as usize;
                        let offset = u16::from_be_bytes([p[2], p[3]]) as usize;
                        m.user[sector][offset..offset + 16].to_vec()
                    }
                    _ => return vec![],
                }
            };
            vec![reply(request, &payload)]
        });
        let model = MODELS.iter().find(|m| m.product_id == 0xC08B).unwrap();
        let dev = HidppDevice::new(fake, 0xFF, true).unwrap();
        (MouseTask::open(dev, model, false).unwrap(), state)
    }

    #[test]
    fn skips_disabled_mouse_profiles_instead_of_sending_0x8100_switches() {
        let (task, sim) = sim_mouse(&[true, true, false, false, false]);
        let manager = Manager::with_mouse_for_test(task);
        let status = manager.status();
        let profiles = AppProfiles {
            enabled: true,
            rules: vec![
                AppRule { app: "game.exe".into(), lighting: None, mouse_profile: Some(4) },
                AppRule { app: "editor.exe".into(), lighting: None, mouse_profile: Some(2) },
            ],
        };
        let engine = AppProfileEngine::with_debounce(manager, profiles, Vec::new(), Duration::ZERO);
        engine.devices_changed(&status);

        // Rule targeting disabled profile 4 sends nothing to the mouse.
        assert!(engine.focus_changed(Some("game.exe".into())));
        assert!(sim.lock().unwrap().selected.is_empty());

        // Switching to an enabled rule switches to profile 2, and disabled rule restores profile 1.
        assert!(engine.focus_changed(Some("editor.exe".into())));
        assert!(engine.focus_changed(Some("game.exe".into())));
        assert_eq!(sim.lock().unwrap().selected, [[0, 2], [0, 1]]);
    }

    #[test]
    fn coalesces_transient_focus_flips_before_switching_the_mouse() {
        let (task, sim) = sim_mouse(&[true, true, true, false, false]);
        let manager = Manager::with_mouse_for_test(task);
        let status = manager.status();
        let profiles = AppProfiles {
            enabled: true,
            rules: vec![AppRule { app: "game.exe".into(), lighting: None, mouse_profile: Some(2) }],
        };
        let engine = AppProfileEngine::with_debounce(manager, profiles, Vec::new(), Duration::from_millis(80));
        engine.devices_changed(&status);

        // Settle onto game.exe -> profile 2.
        assert!(engine.focus_changed(Some("game.exe".into())));
        for _ in 0..40 {
            if !sim.lock().unwrap().selected.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        assert_eq!(sim.lock().unwrap().selected, [[0, 2]]);

        // Transient focus flip away (`None`) and back within debounce writes nothing.
        assert!(engine.focus_changed(None));
        assert!(engine.focus_changed(Some("game.exe".into())));
        std::thread::sleep(Duration::from_millis(130));
        assert_eq!(sim.lock().unwrap().selected, [[0, 2]]);
    }

    #[test]
    fn dropping_engine_stops_debouncer_without_leaking_strong_ref() {
        let (task, _sim) = sim_mouse(&[true, true, false, false, false]);
        let manager = Manager::with_mouse_for_test(task);
        let engine =
            AppProfileEngine::with_debounce(manager, AppProfiles::default(), Vec::new(), Duration::from_millis(80));
        let debouncer = Arc::clone(&engine.debouncer);
        let weak = Arc::downgrade(&engine);
        assert!(engine.focus_changed(Some("game.exe".into())));
        drop(engine);
        assert_eq!(weak.strong_count(), 0);
        assert!(debouncer.pending.lock().unwrap().stopped);
    }
}

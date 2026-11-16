//! App profiles: while an app is in focus, the keyboard shows one of the saved lightings and the
//! mouse switches to one of its onboard profiles. When no rule matches, both go back to how they were.
//!
//! Only the active profile changes on the mouse: its memory is never written. Key mappings are not
//! part of app profiles, because changing them writes the keyboard's memory.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::{AppProfiles, LightingPreset};
use crate::error::Result;
use crate::focus;
use crate::manager::{DeviceStatus, Manager};

/// Apps seen recently, offered when adding a rule.
const RECENT_APPS: usize = 12;

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
}

impl AppProfileEngine {
    pub fn new(manager: Manager, profiles: AppProfiles, presets: Vec<LightingPreset>) -> Arc<Self> {
        let state = State { profiles, presets, ..State::default() };
        Arc::new(Self { manager, state: Mutex::new(state), applying: Mutex::new(()), watching: AtomicBool::new(false) })
    }

    /// Starts following the focused app (once). `notify` is called on every change, for the interface.
    pub fn watch_focus(self: &Arc<Self>, notify: impl Fn(Option<&str>) + Send + 'static) {
        if self.watching.swap(true, Ordering::SeqCst) {
            return;
        }
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

    pub fn set_profiles(&self, profiles: AppProfiles, presets: Vec<LightingPreset>) {
        {
            let mut state = self.state.lock().unwrap();
            state.profiles = profiles;
            state.presets = presets;
        }
        self.update();
    }

    /// Returns `true` if the focused app changed.
    fn focus_changed(&self, app: Option<String>) -> bool {
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
        self.update();
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
        // Untested models are read-only: their profile is left alone.
        if !self.manager.status().mouse.is_some_and(|m| m.verified) {
            return Ok(());
        }
        let back = self.manager.mouse()?.call(move |m| {
            let active = m.active_profile()?;
            if let Some(number) = target.or(before)
                && number != active
            {
                m.set_active_profile(number)?;
            }
            // Remember the profile in use before the first rule, to go back to it later.
            Ok(target.and(before.or(Some(active))))
        })?;
        let mut state = self.state.lock().unwrap();
        state.mouse = target;
        state.mouse_before = back;
        Ok(())
    }
}

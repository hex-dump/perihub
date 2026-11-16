//! Quick actions from the tray menu and from `perihub-cli app ...` (see perihub_core::quick): the
//! changes that only switch what is saved. Both ways end in [`run`].

use std::sync::Mutex;

use perihub_core::quick::QuickAction;
use tauri::{AppHandle, Manager as _};

use crate::AppState;

/// What the tray menu offers for the mouse, read from it (not on every menu build: that would
/// talk to the mouse from the device listener).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseChoices {
    /// Profiles in use, by number.
    pub profiles: Vec<u8>,
    /// DPI of each level of the active profile.
    pub dpi_levels: Vec<u16>,
}

static MOUSE: Mutex<Option<MouseChoices>> = Mutex::new(None);

pub fn mouse_choices() -> Option<MouseChoices> {
    MOUSE.lock().unwrap().clone()
}

/// The tray menu ids of the quick actions: "quick:profile:2", "quick:dpi-level:1", "quick:preset:Name".
const MENU_PREFIX: &str = "quick:";

pub fn menu_id(action: &QuickAction) -> String {
    match action {
        QuickAction::Profile { number } => format!("{MENU_PREFIX}profile:{number}"),
        QuickAction::DpiLevel { number } => format!("{MENU_PREFIX}dpi-level:{number}"),
        QuickAction::Preset { name } => format!("{MENU_PREFIX}preset:{name}"),
    }
}

pub fn from_menu_id(id: &str) -> Option<QuickAction> {
    let (kind, value) = id.strip_prefix(MENU_PREFIX)?.split_once(':')?;
    QuickAction::from_args(&[kind, value]).ok()
}

/// Whether the mouse can be switched: models whose memory format was tested only, like app profiles.
fn mouse_verified(app: &AppHandle) -> bool {
    app.state::<AppState>().manager.status().mouse.is_some_and(|m| m.verified)
}

/// Does the action now. Blocks while the device answers.
pub fn run(app: &AppHandle, action: QuickAction) -> Result<(), String> {
    let state = app.state::<AppState>();
    let result = match action {
        QuickAction::Profile { number } => {
            check_mouse(app)?;
            state.manager.mouse().and_then(|m| m.call(move |m| m.set_active_profile(number)))
        }
        QuickAction::DpiLevel { number } => {
            check_mouse(app)?;
            state.manager.mouse().and_then(|m| {
                m.call(move |m| {
                    let levels = m.state().map(|s| {
                        s.profiles
                            .iter()
                            .find(|p| p.number == s.active_profile)
                            .map_or(0, |p| p.settings.dpi_levels.len())
                    })?;
                    if number == 0 || number as usize > levels {
                        return Err(perihub_core::Error::Invalid(format!(
                            "the active profile has {levels} DPI level{}",
                            if levels == 1 { "" } else { "s" }
                        )));
                    }
                    m.set_active_dpi_index(number - 1)
                })
            })
        }
        QuickAction::Preset { name } => return preset(app, &name),
    };
    if result.is_ok() {
        refresh_mouse_choices(app);
    }
    result.map_err(|e| e.to_string())
}

fn check_mouse(app: &AppHandle) -> Result<(), String> {
    if mouse_verified(app) { Ok(()) } else { Err("no mouse that PeriHub fully supports is connected".into()) }
}

fn preset(app: &AppHandle, name: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.manager.status().keyboard.is_none() {
        return Err("no keyboard is connected".into());
    }
    let lighting = {
        let config = state.config.lock().unwrap();
        let found = config.lighting_presets.iter().find(|p| p.name.eq_ignore_ascii_case(name));
        found.map(|p| p.lighting.clone()).ok_or_else(|| format!("there is no lighting preset called \"{name}\""))?
    };
    // Like choosing it in the window: it becomes the lighting the keyboard starts with.
    crate::commands::update_keyboard_config(&state, |c| c.lighting = lighting.clone())?;
    state.manager.keyboard().and_then(|k| k.call(move |k| k.set_lighting(lighting))).map_err(|e| e.to_string())?;
    state.profiles.lighting_reset();
    Ok(())
}

/// Reads the mouse's profiles and DPI levels for the tray menu, in a thread of its own, and
/// builds the menu again if they changed.
pub fn refresh_mouse_choices(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let choices = if mouse_verified(&app) {
            app.state::<AppState>()
                .manager
                .mouse()
                .and_then(|m| m.call(|m| m.state()))
                .map(|s| {
                    let active = s.profiles.iter().find(|p| p.number == s.active_profile);
                    MouseChoices {
                        profiles: s.profiles.iter().filter(|p| p.enabled).map(|p| p.number).collect(),
                        dpi_levels: active.map(|p| p.settings.dpi_levels.clone()).unwrap_or_default(),
                    }
                })
                .ok()
        } else {
            None
        };
        let changed = std::mem::replace(&mut *MOUSE.lock().unwrap(), choices.clone()) != choices;
        if changed {
            crate::window::refresh_menu(&app);
        }
    });
}

/// A tray menu choice: runs in a thread, since the device takes a moment to answer.
pub fn run_from_menu(app: &AppHandle, action: QuickAction) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = run(&app, action.clone()) {
            log::warn!("quick action {action:?} from the tray: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_round_trip() {
        for action in [
            QuickAction::Profile { number: 3 },
            QuickAction::DpiLevel { number: 2 },
            QuickAction::Preset { name: "Night: dim".into() },
        ] {
            assert_eq!(from_menu_id(&menu_id(&action)), Some(action));
        }
        assert_eq!(from_menu_id("open"), None);
        assert_eq!(from_menu_id("quick:factory:1"), None);
        assert_eq!(from_menu_id("quick:profile:0"), None);
    }
}

//! Commands called by the interface. Device operations can take a few seconds,
//! so they run off the main thread to avoid freezing the window.

use perihub_core::apex::{KeyBinding, keys};
use perihub_core::config::{AppProfiles, KeyboardConfig, LightingPreset};
use perihub_core::effects::KeyboardLighting;
use perihub_core::focus::{self, FocusSupport};
use perihub_core::hidpp::profile::{LedEffect, ProfileSettings};
use perihub_core::keyboard::Keymap;
use perihub_core::mouse::{MouseLive, MouseState};
use perihub_core::{DeviceStatus, Error};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::AppState;

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> perihub_core::Result<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> DeviceStatus {
    state.manager.status()
}

// ---- Keyboard ----

#[tauri::command]
pub fn kb_get_config(state: State<'_, AppState>) -> KeyboardConfig {
    state.config.lock().unwrap().keyboard.clone()
}

/// Saves the keyboard settings (also while it is unplugged: they are applied when it is connected).
fn update_keyboard_config(state: &AppState, change: impl FnOnce(&mut KeyboardConfig)) -> CmdResult<()> {
    let mut config = state.config.lock().unwrap();
    change(&mut config.keyboard);
    state.manager.set_keyboard_config(config.keyboard.clone());
    config.save().map_err(|e| e.to_string())
}

fn ignore_disconnected(result: perihub_core::Result<()>) -> perihub_core::Result<()> {
    match result {
        Err(Error::NotConnected) => Ok(()),
        other => other,
    }
}

#[tauri::command]
pub async fn kb_set_lighting(state: State<'_, AppState>, lighting: KeyboardLighting) -> CmdResult<()> {
    update_keyboard_config(&state, |c| c.lighting = lighting.clone())?;
    let manager = state.manager.clone();
    blocking(move || ignore_disconnected(manager.keyboard().and_then(|k| k.call(move |k| k.set_lighting(lighting)))))
        .await?;
    state.profiles.lighting_reset();
    Ok(())
}

#[tauri::command]
pub async fn kb_set_brightness(state: State<'_, AppState>, level: u8) -> CmdResult<()> {
    update_keyboard_config(&state, |c| c.brightness = level)?;
    let manager = state.manager.clone();
    blocking(move || ignore_disconnected(manager.keyboard().and_then(|k| k.call(move |k| k.set_brightness(level)))))
        .await
}

/// Reads the brightness from the keyboard. If the user changed it with SteelSeries + F11/F12,
/// the saved value follows, so it is not undone when the keyboard is plugged in again.
#[tauri::command]
pub async fn kb_read_brightness(state: State<'_, AppState>) -> CmdResult<u8> {
    let manager = state.manager.clone();
    let level = blocking(move || manager.keyboard()?.call(|k| k.brightness())).await?;
    if state.config.lock().unwrap().keyboard.brightness != level {
        update_keyboard_config(&state, |c| c.brightness = level)?;
    }
    Ok(level)
}

#[tauri::command]
pub async fn kb_get_keymap(state: State<'_, AppState>) -> CmdResult<Keymap> {
    let manager = state.manager.clone();
    blocking(move || manager.keyboard()?.call(|k| k.keymap())).await
}

#[derive(Debug, Deserialize)]
pub struct KeyChange {
    layer: u8,
    source: u8,
    binding: KeyBinding,
}

#[tauri::command]
pub async fn kb_apply_keymap(state: State<'_, AppState>, changes: Vec<KeyChange>) -> CmdResult<Keymap> {
    let changes: Vec<_> = changes.into_iter().map(|c| (c.layer, c.source, c.binding)).collect();
    let manager = state.manager.clone();
    let keymap = blocking(move || manager.keyboard()?.call(move |k| k.apply_keymap(&changes))).await;
    // A key may now start a software macro, or no longer.
    state.soft.watch_keyboard(state.manager.keyboard_triggers());
    keymap
}

#[tauri::command]
pub async fn kb_reset_keymap(state: State<'_, AppState>) -> CmdResult<Keymap> {
    let manager = state.manager.clone();
    let keymap = blocking(move || manager.keyboard()?.call(|k| k.reset_keymap())).await;
    state.soft.watch_keyboard(state.manager.keyboard_triggers());
    keymap
}

#[derive(Serialize)]
pub struct KeyCatalog {
    targets: &'static [keys::KeyInfo],
    sources: Vec<u8>,
}

#[tauri::command]
pub fn kb_key_catalog() -> KeyCatalog {
    KeyCatalog { targets: keys::TARGET_KEYS, sources: (0..=255u8).filter(|&u| keys::is_remappable_source(u)).collect() }
}

#[tauri::command]
pub fn kb_get_presets(state: State<'_, AppState>) -> Vec<LightingPreset> {
    state.config.lock().unwrap().lighting_presets.clone()
}

/// Saves the presets and the rules, and passes them to the app profiles.
fn save_profiles(state: &AppState, change: impl FnOnce(&mut Vec<LightingPreset>, &mut AppProfiles)) -> CmdResult<()> {
    let mut config = state.config.lock().unwrap();
    let config = &mut *config;
    change(&mut config.lighting_presets, &mut config.app_profiles);
    config.save().map_err(|e| e.to_string())?;
    // Applying may talk to the devices: not on the thread that runs the window.
    let (engine, profiles, presets) =
        (state.profiles.clone(), config.app_profiles.clone(), config.lighting_presets.clone());
    std::thread::spawn(move || engine.set_profiles(profiles, presets));
    Ok(())
}

/// Saves a lighting under a name, replacing a preset with the same name.
#[tauri::command]
pub fn kb_save_preset(
    state: State<'_, AppState>,
    name: String,
    lighting: KeyboardLighting,
) -> CmdResult<Vec<LightingPreset>> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("the preset needs a name".into());
    }
    save_profiles(&state, |presets, _| match presets.iter_mut().find(|p| p.name == name) {
        Some(preset) => preset.lighting = lighting,
        None => presets.push(LightingPreset { name, lighting }),
    })?;
    Ok(kb_get_presets(state))
}

/// Deletes a preset; rules that used it keep the keyboard as it is.
#[tauri::command]
pub fn kb_delete_preset(state: State<'_, AppState>, name: String) -> CmdResult<Vec<LightingPreset>> {
    save_profiles(&state, |presets, profiles| {
        presets.retain(|p| p.name != name);
        for rule in profiles.rules.iter_mut().filter(|r| r.lighting.as_ref() == Some(&name)) {
            rule.lighting = None;
        }
    })?;
    Ok(kb_get_presets(state))
}

// ---- Mouse ----

#[tauri::command]
pub async fn mouse_get_state(state: State<'_, AppState>) -> CmdResult<MouseState> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(|m| m.state())).await
}

/// Active profile and DPI, read again while the mouse page is open: the mouse's buttons change them.
#[tauri::command]
pub async fn mouse_live(state: State<'_, AppState>) -> CmdResult<MouseLive> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(|m| m.live())).await
}

#[tauri::command]
pub async fn mouse_save_profile(
    state: State<'_, AppState>,
    number: u8,
    settings: ProfileSettings,
) -> CmdResult<MouseState> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.save_profile(number, &settings))).await
}

#[tauri::command]
pub async fn mouse_restore_factory(state: State<'_, AppState>, number: u8) -> CmdResult<MouseState> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.restore_factory_profile(number))).await
}

#[tauri::command]
pub async fn mouse_set_profile_enabled(state: State<'_, AppState>, number: u8, enabled: bool) -> CmdResult<MouseState> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.set_profile_enabled(number, enabled))).await
}

#[tauri::command]
pub async fn mouse_set_active_profile(state: State<'_, AppState>, number: u8) -> CmdResult<MouseState> {
    let manager = state.manager.clone();
    blocking(move || {
        manager.mouse()?.call(move |m| {
            m.set_active_profile(number)?;
            m.state()
        })
    })
    .await
}

#[tauri::command]
pub async fn mouse_preview_dpi(state: State<'_, AppState>, dpi: u16) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.preview_dpi(dpi))).await
}

#[tauri::command]
pub async fn mouse_preview_led(state: State<'_, AppState>, zone: u8, effect: LedEffect) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.preview_led(zone, &effect))).await
}

#[tauri::command]
pub async fn mouse_revert_preview(state: State<'_, AppState>) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || ignore_disconnected(manager.mouse().and_then(|m| m.call(|m| m.revert_preview())))).await
}

// ---- App profiles ----

#[derive(Serialize)]
pub struct ProfilesInfo {
    support: FocusSupport,
    focused: Option<String>,
    recent: Vec<String>,
    profiles: AppProfiles,
    presets: Vec<String>,
}

/// App profiles and the focused app. Opening the page starts following the focused app,
/// so it can be shown while setting up the rules.
#[tauri::command]
pub fn profiles_get(app: AppHandle, state: State<'_, AppState>) -> ProfilesInfo {
    crate::watch_focus(&app, &state.profiles);
    let config = state.config.lock().unwrap();
    ProfilesInfo {
        support: focus::support(),
        focused: state.profiles.focused(),
        recent: state.profiles.recent_apps(),
        profiles: config.app_profiles.clone(),
        presets: config.lighting_presets.iter().map(|p| p.name.clone()).collect(),
    }
}

#[tauri::command]
pub fn profiles_set(app: AppHandle, state: State<'_, AppState>, profiles: AppProfiles) -> CmdResult<()> {
    save_profiles(&state, |_, current| *current = profiles)?;
    crate::watch_focus(&app, &state.profiles);
    Ok(())
}

// ---- App ----

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    start_hidden: bool,
    autostart: bool,
    check_updates: bool,
    /// This build can update itself (Windows): read only.
    #[serde(default)]
    updates_supported: bool,
    /// The package manager that installed PeriHub and updates it, such as Scoop: read only.
    #[serde(default)]
    package_manager: Option<String>,
    /// Folder of the log file: read only.
    #[serde(default)]
    log_dir: String,
}

#[tauri::command]
pub fn app_get_settings(app: AppHandle, state: State<'_, AppState>) -> AppSettings {
    let config = state.config.lock().unwrap();
    AppSettings {
        start_hidden: config.start_hidden,
        autostart: crate::autostart::is_enabled(&app),
        check_updates: config.check_updates,
        updates_supported: crate::updates::supported(),
        package_manager: crate::updates::package_manager().map(String::from),
        log_dir: crate::logfile::dir().display().to_string(),
    }
}

#[tauri::command]
pub fn app_set_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> CmdResult<AppSettings> {
    crate::autostart::set_enabled(&app, settings.autostart).map_err(|e| format!("autostart: {e}"))?;
    let mut config = state.config.lock().unwrap();
    config.start_hidden = settings.start_hidden;
    config.check_updates = settings.check_updates;
    config.save().map_err(|e| e.to_string())?;
    drop(config);
    Ok(app_get_settings(app, state))
}

/// Opens the folder of the log file in the file manager, to attach it to a bug report.
#[tauri::command]
pub fn app_open_log_folder() -> CmdResult<()> {
    let dir = crate::logfile::dir();
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot open {}: {e}", dir.display()))
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    crate::window::quit(&app);
}

//! Commands called by the interface. Device operations can take a few seconds,
//! so they run off the main thread to avoid freezing the window.

use perihub_core::apex::{KeyBinding, keys};
use perihub_core::color::Rgb;
use perihub_core::config::{AppProfiles, BackupEntry, KeyboardConfig, LightingPreset};
use perihub_core::corsair::device::CorsairInfo;
use perihub_core::effects::KeyboardLighting;
use perihub_core::focus::{self, FocusSupport};
use perihub_core::g300::{G300Info, G300Settings};
use perihub_core::hidpp::host::Look;
use perihub_core::hidpp::profile::{LedEffect, ProfileSettings};
use perihub_core::keyboard::{Keymap, KeymapRestore};
use perihub_core::logitech::LogitechLive;
use perihub_core::mouse::{MouseLive, MouseRestore, MouseState};
use perihub_core::peripheral::{BlindValue, PeripheralInfo, Settings};
use perihub_core::writes::WriteEntry;
use perihub_core::{DeviceStatus, Error};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;

type CmdResult<T> = Result<T, String>;

/// The forms on GitHub (the test report and the device request); the interface adds the fields it
/// fills in.
const TEST_FORM_URLS: &[&str] = &[
    "https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml&",
    "https://github.com/hex-dump/perihub/issues/new?template=device_request.yml&",
];

/// The links of the Support entry in the sidebar.
const SUPPORT_URLS: &[&str] = &["https://buymeacoffee.com/hexdump1", "https://github.com/hex-dump/perihub"];

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> perihub_core::Result<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> DeviceStatus {
    state.manager.status()
}

/// The connected devices PeriHub does not know, for a device request.
#[tauri::command]
pub fn devices_for_report(state: State<'_, AppState>) -> Vec<perihub_core::report_devices::ReportDevice> {
    state.manager.report_devices()
}

// ---- Keyboard ----

/// The layout the system types with, for the keys drawn on the keyboards' pages. Read as the user,
/// without opening any device.
#[tauri::command]
pub async fn kb_system_layout() -> CmdResult<Option<perihub_core::kblayout::SystemLayout>> {
    tauri::async_runtime::spawn_blocking(perihub_core::kblayout::system).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn kb_get_config(state: State<'_, AppState>) -> KeyboardConfig {
    state.config.lock().unwrap().keyboard.clone()
}

/// Saves the keyboard settings (also while it is unplugged: they are applied when it is connected).
pub(crate) fn update_keyboard_config(state: &AppState, change: impl FnOnce(&mut KeyboardConfig)) -> CmdResult<()> {
    let mut config = state.config.lock().unwrap();
    change(&mut config.keyboard);
    state.manager.set_keyboard_config(config.keyboard.clone());
    config.save().map_err(|e| e.to_string())
}

/// Every command that changes a device starts here: in read-only mode it is refused before
/// anything is saved or sent. The core refuses the writes too, under every command.
fn writes_allowed() -> CmdResult<()> {
    perihub_core::writes::refuse_if_read_only().map_err(|e| e.to_string())
}

fn ignore_disconnected(result: perihub_core::Result<()>) -> perihub_core::Result<()> {
    match result {
        Err(Error::NotConnected) => Ok(()),
        other => other,
    }
}

#[tauri::command]
pub async fn kb_set_lighting(state: State<'_, AppState>, lighting: KeyboardLighting) -> CmdResult<()> {
    writes_allowed()?;
    update_keyboard_config(&state, |c| c.lighting = lighting.clone())?;
    let manager = state.manager.clone();
    blocking(move || ignore_disconnected(manager.keyboard().and_then(|k| k.call(move |k| k.set_lighting(lighting)))))
        .await?;
    state.profiles.lighting_reset();
    Ok(())
}

#[tauri::command]
pub async fn kb_set_brightness(state: State<'_, AppState>, level: u8) -> CmdResult<()> {
    writes_allowed()?;
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
    writes_allowed()?;
    let changes: Vec<_> = changes.into_iter().map(|c| (c.layer, c.source, c.binding)).collect();
    let manager = state.manager.clone();
    let keymap = blocking(move || manager.keyboard()?.call(move |k| k.apply_keymap(&changes))).await;
    // A key may now start a software macro, or no longer.
    state.soft.watch_keyboard(state.manager.keyboard_triggers());
    keymap
}

#[tauri::command]
pub async fn kb_reset_keymap(state: State<'_, AppState>) -> CmdResult<Keymap> {
    writes_allowed()?;
    let manager = state.manager.clone();
    let keymap = blocking(move || manager.keyboard()?.call(|k| k.reset_keymap())).await;
    state.soft.watch_keyboard(state.manager.keyboard_triggers());
    keymap
}

#[tauri::command]
pub async fn kb_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupEntry>> {
    let manager = state.manager.clone();
    blocking(move || manager.keyboard()?.call(|k| Ok(k.backups()))).await
}

#[tauri::command]
pub async fn kb_restore_backup(state: State<'_, AppState>, id: String) -> CmdResult<KeymapRestore> {
    writes_allowed()?;
    let manager = state.manager.clone();
    let restored = blocking(move || manager.keyboard()?.call(move |k| k.restore_backup(&id))).await;
    state.soft.watch_keyboard(state.manager.keyboard_triggers());
    restored
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

/// Battery and DPI of a device PeriHub knows by its features, in `slot` of the receiver `receiver_id`.
#[tauri::command]
pub async fn logitech_live(state: State<'_, AppState>, receiver_id: u16, slot: u8) -> CmdResult<LogitechLive> {
    let manager = state.manager.clone();
    blocking(move || manager.logitech(receiver_id, slot)?.call(|d| d.live())).await
}

/// Sets the DPI of a device PeriHub knows by its features (experimental), and keeps it to set it
/// again every time the device connects.
#[tauri::command]
pub async fn logitech_set_dpi(state: State<'_, AppState>, receiver_id: u16, slot: u8, dpi: u16) -> CmdResult<()> {
    writes_allowed()?;
    let manager = state.manager.clone();
    let key = blocking(move || manager.logitech(receiver_id, slot)?.call(move |d| d.set_dpi(dpi))).await?;
    save_logitech_dpi(&state, key, Some(dpi))
}

/// Puts the device's own default DPI back, and stops setting one.
#[tauri::command]
pub async fn logitech_reset_dpi(state: State<'_, AppState>, receiver_id: u16, slot: u8) -> CmdResult<()> {
    writes_allowed()?;
    let manager = state.manager.clone();
    let key = blocking(move || manager.logitech(receiver_id, slot)?.call(|d| d.reset_dpi())).await?;
    save_logitech_dpi(&state, key, None)
}

/// Takes the lights of a keyboard PeriHub knows by its features over (host mode, experimental) and
/// shows `look`, or shows it if PeriHub drives them already. The look and the wish are saved in the
/// settings: PeriHub takes the lights with it every time the keyboard connects, at every start too,
/// until the user gives them back.
#[tauri::command]
pub async fn logitech_drive_lights(
    app: AppHandle,
    state: State<'_, AppState>,
    receiver_id: u16,
    slot: u8,
    look: Look,
) -> CmdResult<()> {
    writes_allowed()?;
    let manager = state.manager.clone();
    let kept = look.clone();
    let key = blocking(move || manager.logitech(receiver_id, slot)?.call(move |d| d.drive_lights(look))).await?;
    state.manager.set_logitech_look(key.clone(), Some(kept.clone()));
    crate::window::refresh_menu(&app);
    let mut config = state.config.lock().unwrap();
    if !config.logitech_drive.contains(&key) {
        config.logitech_drive.push(key.clone());
    }
    config.logitech_looks.insert(key, kept);
    config.save().map_err(|e| e.to_string())
}

/// Gives the keyboard its own lights back, and stops taking them when it connects.
#[tauri::command]
pub async fn logitech_give_back_lights(
    app: AppHandle,
    state: State<'_, AppState>,
    receiver_id: u16,
    slot: u8,
) -> CmdResult<()> {
    let manager = state.manager.clone();
    let key = blocking(move || manager.logitech(receiver_id, slot)?.call(|d| d.give_back_lights())).await?;
    state.manager.set_logitech_look(key.clone(), None);
    crate::window::refresh_menu(&app);
    let mut config = state.config.lock().unwrap();
    config.logitech_drive.retain(|k| *k != key);
    config.save().map_err(|e| e.to_string())
}

/// Sets the brightness of a keyboard's lights (experimental); the keyboard keeps it, as with its key.
#[tauri::command]
pub async fn logitech_set_brightness(
    state: State<'_, AppState>,
    receiver_id: u16,
    slot: u8,
    value: u16,
) -> CmdResult<()> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.logitech(receiver_id, slot)?.call(move |d| d.set_brightness(value))).await
}

/// Turns a keyboard with a raw HID collection on or off as a VIA keyboard ("vvvv:pppp"). PeriHub
/// opens it, or closes it, at its next look at the devices.
#[tauri::command]
pub fn qmk_set_enabled(state: State<'_, AppState>, key: String, on: bool) -> CmdResult<()> {
    let mut config = state.config.lock().unwrap();
    config.qmk_keyboards.retain(|k| *k != key);
    if on {
        config.qmk_keyboards.push(key.clone());
    }
    state.manager.set_qmk_enabled(&key, on);
    config.save().map_err(|e| e.to_string())
}

fn save_logitech_dpi(state: &AppState, key: String, dpi: Option<u16>) -> CmdResult<()> {
    let mut config = state.config.lock().unwrap();
    match dpi {
        Some(dpi) => config.logitech_dpi.insert(key.clone(), dpi),
        None => config.logitech_dpi.remove(&key),
    };
    state.manager.set_logitech_dpi(key, dpi);
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mouse_save_profile(
    state: State<'_, AppState>,
    number: u8,
    settings: ProfileSettings,
) -> CmdResult<MouseState> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.save_profile(number, &settings))).await
}

#[tauri::command]
pub async fn mouse_restore_factory(state: State<'_, AppState>, number: u8) -> CmdResult<MouseState> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.restore_factory_profile(number))).await
}

#[tauri::command]
pub async fn mouse_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupEntry>> {
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(|m| Ok(m.backups()))).await
}

#[tauri::command]
pub async fn mouse_restore_backup(state: State<'_, AppState>, id: String) -> CmdResult<MouseRestore> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.restore_backup(&id))).await
}

#[tauri::command]
pub async fn mouse_set_profile_enabled(state: State<'_, AppState>, number: u8, enabled: bool) -> CmdResult<MouseState> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.set_profile_enabled(number, enabled))).await
}

#[tauri::command]
pub async fn mouse_set_active_profile(state: State<'_, AppState>, number: u8) -> CmdResult<MouseState> {
    writes_allowed()?;
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
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.preview_dpi(dpi))).await
}

#[tauri::command]
pub async fn mouse_preview_led(state: State<'_, AppState>, zone: u8, effect: LedEffect) -> CmdResult<()> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(move |m| m.preview_led(zone, &effect))).await
}

/// "Turn off experimental lighting" in the tray menu: every Corsair device back to its own settings,
/// and every Logitech keyboard whose lights PeriHub drives back to its own lights, no longer taken
/// when it connects.
pub fn turn_off_experimental(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        app.state::<AppState>().manager.release_corsair();
        app.state::<AppState>().manager.release_logitech(true);
        {
            let state = app.state::<AppState>();
            let mut config = state.config.lock().unwrap();
            config.logitech_drive.clear();
            if let Err(e) = config.save() {
                log::warn!("saving the settings failed: {e}");
            }
        }
        crate::window::refresh_menu(&app);
        corsair_changed(&app);
    });
}

// ---- Logitech G300 ----

/// The G300's profiles, with the active profile and DPI level read again now.
#[tauri::command]
pub async fn g300_info(state: State<'_, AppState>) -> CmdResult<G300Info> {
    let manager = state.manager.clone();
    blocking(move || manager.g300()?.call(|g| g.info())).await
}

/// Saves a G300 profile. Undone unless kept.
#[tauri::command]
pub async fn g300_save(state: State<'_, AppState>, number: u8, settings: G300Settings) -> CmdResult<G300Info> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.g300()?.call(move |g| g.save(number, &settings))).await
}

/// The user keeps a save to the G300: the core stops counting down to undo it.
#[tauri::command]
pub async fn g300_keep_changes(state: State<'_, AppState>) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || {
        manager.g300()?.call(|g| {
            g.keep_changes();
            Ok(())
        })
    })
    .await
}

/// Puts back the G300 profiles saved before the last save.
#[tauri::command]
pub async fn g300_undo_last_write(state: State<'_, AppState>) -> CmdResult<G300Info> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.g300()?.call(|g| g.undo_last_write())).await
}

// ---- Mice and keyboards of other brands (experimental) ----

/// Keeps the status the interface shows in step with a device of another brand.
fn peripheral_changed(app: &AppHandle, info: &PeripheralInfo) {
    let manager = &app.state::<AppState>().manager;
    manager.peripheral_changed(info);
    let _ = app.emit("devices-changed", manager.status());
}

/// A device of another brand, read again now.
#[tauri::command]
pub async fn peripheral_info(app: AppHandle, key: String) -> CmdResult<PeripheralInfo> {
    let manager = app.state::<AppState>().manager.clone();
    let info = blocking(move || manager.peripheral(&key)?.call(|p| p.info())).await?;
    peripheral_changed(&app, &info);
    Ok(info)
}

/// Changes a device of another brand (experimental), and saves to its memory if `save`. Undone
/// unless kept.
#[tauri::command]
pub async fn peripheral_change(
    app: AppHandle,
    key: String,
    settings: Settings,
    save: bool,
    profile: Option<u8>,
) -> CmdResult<PeripheralInfo> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    let worker = manager.peripheral(&key).map_err(|e| e.to_string())?;
    let result = blocking(move || worker.call(move |p| p.change(&settings, save, profile))).await;
    // Also after an error: the change may have been undone, and what the device shows changed.
    let manager = app.state::<AppState>().manager.clone();
    if let Ok(info) = blocking(move || manager.peripheral(&key)?.call(|p| Ok(p.info.clone()))).await {
        peripheral_changed(&app, &info);
    }
    result
}

/// Sends one setting to a device of another brand that cannot report its settings (experimental).
/// Unless the device keeps the setting and PeriHub remembers it (the lighting effect of a Razer keyboard: see
/// [`peripheral_put_back_blind`]), nothing is saved to its memory, and nothing can be undone but by unplugging it.
#[tauri::command]
pub async fn peripheral_set_blind(
    app: AppHandle,
    key: String,
    id: String,
    value: BlindValue,
) -> CmdResult<PeripheralInfo> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    let result = blocking({
        let key = key.clone();
        move || manager.peripheral(&key)?.call(move |p| p.set_blind(&id, &value))
    })
    .await;
    refresh_after_send(&app, key).await;
    result
}

/// Sends a setting of a device of another brand that PeriHub remembers (the lighting effect of a Razer keyboard) again
/// as it was before the last change PeriHub made to it. Only what PeriHub itself sent can be put back.
#[tauri::command]
pub async fn peripheral_put_back_blind(app: AppHandle, key: String, id: String) -> CmdResult<PeripheralInfo> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    let result = blocking({
        let key = key.clone();
        move || manager.peripheral(&key)?.call(move |p| p.put_back_blind(&id))
    })
    .await;
    refresh_after_send(&app, key).await;
    result
}

/// Tells the interface what the device shows after a setting was sent to it, also when the device refused it: what it
/// answered is kept for the test report.
async fn refresh_after_send(app: &AppHandle, key: String) {
    let manager = app.state::<AppState>().manager.clone();
    if let Ok(info) = blocking(move || manager.peripheral(&key)?.call(|p| Ok(p.info.clone()))).await {
        peripheral_changed(app, &info);
    }
}

/// The user keeps a change to a device of another brand: the core stops counting down to undo it.
#[tauri::command]
pub async fn peripheral_keep_changes(state: State<'_, AppState>, key: String) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || {
        manager.peripheral(&key)?.call(|p| {
            p.keep_changes();
            Ok(())
        })
    })
    .await
}

/// Puts back the settings from before the last change to a device of another brand.
#[tauri::command]
pub async fn peripheral_undo(app: AppHandle, key: String) -> CmdResult<PeripheralInfo> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    let info = blocking(move || manager.peripheral(&key)?.call(|p| p.undo_last_change())).await?;
    peripheral_changed(&app, &info);
    Ok(info)
}

// ---- Corsair mice and keyboards (experimental) ----

/// Tells the interface and the tray menu that a Corsair device's lighting changed.
fn corsair_changed(app: &AppHandle) {
    crate::window::refresh_menu(app);
    let _ = app.emit("devices-changed", app.state::<AppState>().manager.status());
}

#[tauri::command]
pub async fn corsair_set_lighting(app: AppHandle, product_id: u16, color: Rgb) -> CmdResult<CorsairInfo> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    let result = blocking({
        let manager = manager.clone();
        move || manager.corsair(product_id)?.call(move |m| m.set_lighting(color))
    })
    .await;
    // On error the device was given back: the status shows it either way.
    if let Ok(worker) = manager.corsair(product_id)
        && let Ok(info) = worker.call(|m| Ok(m.info()))
    {
        manager.corsair_changed(&info);
    }
    corsair_changed(&app);
    result
}

/// The user checked that the mouse still works: the core stops counting down to give it back.
#[tauri::command]
pub async fn corsair_keep_lighting(state: State<'_, AppState>, product_id: u16) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || {
        manager.corsair(product_id)?.call(|m| {
            m.keep_lighting();
            Ok(())
        })
    })
    .await
}

#[tauri::command]
pub async fn corsair_lighting_off(app: AppHandle, product_id: u16) -> CmdResult<()> {
    writes_allowed()?;
    let manager = app.state::<AppState>().manager.clone();
    blocking(move || {
        let info = manager.corsair(product_id)?.call(|m| m.turn_off_lighting())?;
        manager.corsair_changed(&info);
        Ok(())
    })
    .await?;
    corsair_changed(&app);
    Ok(())
}

/// The user keeps an experimental save: the core stops counting down to undo it.
#[tauri::command]
pub async fn mouse_keep_changes(state: State<'_, AppState>) -> CmdResult<()> {
    let manager = state.manager.clone();
    blocking(move || {
        manager.mouse()?.call(|m| {
            m.keep_changes();
            Ok(())
        })
    })
    .await
}

/// Puts back the mouse memory saved before the last write (the "Keep these changes?" countdown).
#[tauri::command]
pub async fn mouse_undo_last_write(state: State<'_, AppState>) -> CmdResult<MouseRestore> {
    writes_allowed()?;
    let manager = state.manager.clone();
    blocking(move || manager.mouse()?.call(|m| m.undo_last_write())).await
}

#[tauri::command]
pub async fn mouse_revert_preview(state: State<'_, AppState>) -> CmdResult<()> {
    writes_allowed()?;
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

/// The games installed through Steam, the Epic Games Launcher, Lutris and Heroic, for suggesting rules.
/// Reads files only (and, on Windows, where Steam is installed from the registry).
#[tauri::command]
pub async fn games_installed() -> CmdResult<Vec<perihub_core::games::InstalledGame>> {
    blocking(|| Ok(perihub_core::games::installed_games())).await
}

/// The one-time step that makes detecting the focused app possible (on GNOME: installing and
/// enabling PeriHub's Shell extension). Returns what to tell the user.
#[tauri::command]
pub async fn profiles_setup_focus(app: AppHandle) -> CmdResult<String> {
    let message = tauri::async_runtime::spawn_blocking(focus::setup).await.map_err(|e| e.to_string())??;
    if let Some(state) = app.try_state::<AppState>() {
        crate::watch_focus(&app, &state.profiles);
    }
    Ok(message)
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
    /// Notifications about the battery of wireless devices.
    #[serde(default)]
    battery: perihub_core::battery::BatteryAlerts,
    /// This build can update itself (Windows): read only.
    #[serde(default)]
    updates_supported: bool,
    /// The package manager that installed PeriHub and updates it, such as Scoop: read only.
    #[serde(default)]
    package_manager: Option<String>,
    /// Folder of the log file: read only.
    #[serde(default)]
    log_dir: String,
    /// Folder of the settings and backups: read only.
    #[serde(default)]
    config_dir: String,
    /// Operating system and architecture, for test reports: read only.
    #[serde(default)]
    system: String,
    /// The branch and commit of a test build ("test/x@abc1234"), `None` in a release: read only.
    #[serde(default)]
    build: Option<String>,
    /// Why "close to tray" is unavailable (the desktop has no tray), `None` when it works: read only.
    #[serde(default)]
    no_tray_reason: Option<String>,
}

#[tauri::command]
pub fn app_get_settings(app: AppHandle, state: State<'_, AppState>) -> AppSettings {
    let config = state.config.lock().unwrap();
    AppSettings {
        start_hidden: config.start_hidden,
        autostart: crate::autostart::is_enabled(&app),
        check_updates: config.check_updates,
        battery: config.battery,
        updates_supported: crate::updates::supported(),
        package_manager: crate::updates::package_manager().map(String::from),
        log_dir: crate::logfile::dir().display().to_string(),
        config_dir: perihub_core::config::config_dir().display().to_string(),
        system: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        build: option_env!("PERIHUB_BUILD").map(String::from),
        no_tray_reason: crate::tray_support::unavailable_reason(),
    }
}

#[tauri::command]
pub fn app_set_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> CmdResult<AppSettings> {
    crate::autostart::set_enabled(&app, settings.autostart).map_err(|e| format!("autostart: {e}"))?;
    let mut config = state.config.lock().unwrap();
    config.start_hidden = settings.start_hidden;
    config.check_updates = settings.check_updates;
    config.battery = perihub_core::battery::BatteryAlerts {
        low_percent: settings.battery.low_percent.clamp(1, 100),
        ..settings.battery
    };
    config.save().map_err(|e| e.to_string())?;
    drop(config);
    Ok(app_get_settings(app, state))
}

/// Why the settings file was not used at start-up, if it was not (see `AppConfig::load_checked`).
#[tauri::command]
pub fn app_config_problem(state: State<'_, AppState>) -> Option<String> {
    state.config_problem.clone()
}

#[tauri::command]
pub fn app_read_only() -> bool {
    perihub_core::writes::read_only()
}

/// Turns read-only mode on or off, from the settings or the tray, at once for every device, and
/// keeps it for the next start. Before it goes on, Corsair devices with experimental lighting are
/// given back to their own settings, and Logitech keyboards their own lights: afterwards nothing
/// could be sent to do it.
pub fn set_read_only(app: &AppHandle, on: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if on && !perihub_core::writes::read_only() {
        state.manager.release_corsair();
        state.manager.release_logitech_for_read_only();
    }
    perihub_core::writes::set_read_only(on);
    let saved = {
        let mut config = state.config.lock().unwrap();
        config.read_only = on;
        config.save().map_err(|e| e.to_string())
    };
    crate::window::refresh_menu(app);
    let _ = app.emit("read-only-changed", on);
    let _ = app.emit("devices-changed", state.manager.status());
    saved
}

#[tauri::command]
pub async fn app_set_read_only(app: AppHandle, on: bool) -> CmdResult<bool> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || set_read_only(&handle, on)).await.map_err(|e| e.to_string())??;
    Ok(perihub_core::writes::read_only())
}

/// The writes sent to the devices in this session, and those refused, oldest first.
#[tauri::command]
pub fn app_write_log() -> Vec<WriteEntry> {
    perihub_core::writes::entries()
}

#[tauri::command]
pub fn app_clear_write_log() {
    perihub_core::writes::clear();
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

/// Opens one of PeriHub's test forms in the browser. Only those addresses: the page cannot ask
/// for any other website or a program, and the fields it fills in are in the query.
#[tauri::command]
pub fn app_open_test_form(url: String) -> CmdResult<()> {
    if !TEST_FORM_URLS.iter().any(|form| url.starts_with(form)) {
        return Err("not PeriHub's test form".to_string());
    }
    perihub_core::softmacro::inject::open_target(&url).map_err(|e| e.to_string())
}

/// Opens one of the two links of the Support entry in the browser. Those exact addresses only.
#[tauri::command]
pub fn app_open_support_link(url: String) -> CmdResult<()> {
    if !SUPPORT_URLS.contains(&url.as_str()) {
        return Err("not a PeriHub support link".to_string());
    }
    perihub_core::softmacro::inject::open_target(&url).map_err(|e| e.to_string())
}

/// Opens the release notes of a version on GitHub, the link of the "What's new" dialog. A version number only.
#[tauri::command]
pub fn app_open_release_notes(version: String) -> CmdResult<()> {
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err("not a version number".to_string());
    }
    perihub_core::softmacro::inject::open_target(&format!(
        "https://github.com/hex-dump/perihub/releases/tag/v{version}"
    ))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    crate::window::quit(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The commands that send changes to a device.
    const WRITES: &[&str] = &[
        "kb_set_lighting",
        "kb_set_brightness",
        "kb_apply_keymap",
        "kb_reset_keymap",
        "kb_restore_backup",
        "logitech_set_dpi",
        "logitech_reset_dpi",
        "logitech_drive_lights",
        "logitech_set_brightness",
        "mouse_save_profile",
        "mouse_restore_factory",
        "mouse_restore_backup",
        "mouse_set_profile_enabled",
        "mouse_set_active_profile",
        "mouse_preview_dpi",
        "mouse_preview_led",
        "g300_save",
        "g300_undo_last_write",
        "peripheral_change",
        "peripheral_set_blind",
        "peripheral_put_back_blind",
        "peripheral_undo",
        "corsair_set_lighting",
        "corsair_lighting_off",
        "mouse_undo_last_write",
        "mouse_revert_preview",
    ];
    /// The commands that only read the devices, or change PeriHub's own settings (an app profile
    /// does not apply in read-only mode, see `AppProfileEngine`).
    const OTHERS: &[&str] = &[
        "get_status",
        "devices_for_report",
        "kb_get_config",
        "kb_read_brightness",
        "kb_get_keymap",
        "kb_backups",
        "kb_key_catalog",
        "kb_get_presets",
        "kb_save_preset",
        "kb_delete_preset",
        "kb_system_layout",
        "mouse_get_state",
        "mouse_live",
        "logitech_live",
        // Without host mode it only stops PeriHub taking the lights; giving them back is refused by
        // the core in read-only mode, and the app gives them back before read-only mode goes on.
        "logitech_give_back_lights",
        "qmk_set_enabled",
        "mouse_backups",
        "g300_info",
        "g300_keep_changes",
        "peripheral_info",
        "peripheral_keep_changes",
        "corsair_keep_lighting",
        "mouse_keep_changes",
        "profiles_get",
        "profiles_setup_focus",
        "profiles_set",
        "app_get_settings",
        "app_set_settings",
        "app_read_only",
        "app_config_problem",
        "app_set_read_only",
        "app_write_log",
        "app_clear_write_log",
        "app_open_log_folder",
        "app_open_test_form",
        "app_open_support_link",
        "app_open_release_notes",
        "app_quit",
        "games_installed",
    ];

    /// The commands of this file, with their bodies.
    fn commands() -> Vec<(String, String)> {
        // Windows checkouts may have CRLF line endings.
        let source = include_str!("commands.rs").replace("\r\n", "\n");
        let source = &source[..source.find(concat!("#[cfg(test)]\nmod ", "tests")).unwrap()];
        source
            .split(concat!("#[tauri", "::command]"))
            .skip(1)
            .map(|chunk| {
                let name = chunk[chunk.find("fn ").unwrap() + 3..].split('(').next().unwrap().to_string();
                (name, chunk[chunk.find('{').unwrap() + 1..].to_string())
            })
            .collect()
    }

    #[test]
    fn every_command_that_changes_a_device_is_refused_first_in_read_only_mode() {
        let commands = commands();
        assert!(commands.len() >= WRITES.len() + OTHERS.len());
        for (name, body) in &commands {
            if WRITES.contains(&name.as_str()) {
                assert!(
                    body.trim_start().starts_with("writes_allowed()?;"),
                    "{name} must start with writes_allowed()?"
                );
            } else {
                assert!(
                    OTHERS.contains(&name.as_str()),
                    "{name} is new: add it to WRITES (and start it with writes_allowed()?) if it changes a device, or to OTHERS"
                );
            }
        }
    }

    #[test]
    fn writes_are_refused_with_a_clear_message_in_read_only_mode() {
        perihub_core::writes::set_read_only(true);
        assert_eq!(writes_allowed(), Err("Read-only mode is on".to_string()));
        perihub_core::writes::set_read_only(false);
        assert_eq!(writes_allowed(), Ok(()));
    }
}

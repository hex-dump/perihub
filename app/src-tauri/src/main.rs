// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
#[cfg(target_os = "linux")]
mod flatpak;
mod key_access;
mod logfile;
mod macro_files;
mod qmk_access;
mod soft_macros;
mod unifying_access;
mod updates;
mod window;

use std::sync::{Arc, Mutex, OnceLock};

use perihub_core::Manager;
use perihub_core::app_profiles::AppProfileEngine;
use perihub_core::config::{self, AppConfig, DeviceLock};
use perihub_core::softmacro::SoftMacroEngine;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, WindowEvent};

pub struct AppState {
    pub manager: Manager,
    pub config: Mutex<AppConfig>,
    pub profiles: Arc<AppProfileEngine>,
    pub soft: Arc<SoftMacroEngine>,
    /// Keeps perihub-cli away from the devices while the app runs.
    _devices: Option<DeviceLock>,
}

/// Starts following the focused app; the interface gets an event at every change.
pub fn watch_focus(app: &AppHandle, profiles: &Arc<AppProfileEngine>) {
    let handle = app.clone();
    profiles.watch_focus(move |focused| {
        let _ = handle.emit("focus-changed", focused);
    });
}

/// Argument passed by autostart: start hidden in the system tray.
pub const HIDDEN_ARG: &str = "--hidden";
/// Argument the Windows installer passes when it starts PeriHub after an update (see updates.rs).
pub const AFTER_UPDATE_ARG: &str = "--after-update";

/// Whether PeriHub starts in the tray without its window. After an update the window comes back
/// even with "Always start hidden" on: the update was started from the window or the tray, and
/// a PeriHub that stayed hidden would look as if the update had not finished.
fn starts_hidden(args: &[String], start_hidden_setting: bool) -> bool {
    if args.iter().any(|a| a == AFTER_UPDATE_ARG) {
        return false;
    }
    start_hidden_setting || args.iter().any(|a| a == HIDDEN_ARG)
}

fn main() {
    // PeriHub's own messages at info level, other libraries only when something goes wrong.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,perihub=info,perihub_core=info"))
        .target(env_logger::Target::Pipe(Box::new(logfile::LogWriter::open())))
        .init();
    let args: Vec<String> = std::env::args().collect();

    tauri::Builder::default()
        // Launching a second time brings the already open window to the front, except autostart's
        // launch, which comes after Windows has reopened a PeriHub left open at sign-out. The second
        // copy closes before `setup`, so only the copy that stays logs its start.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == HIDDEN_ARG) {
                log::info!("PeriHub started again by autostart: it stays in the tray");
            } else {
                log::info!("PeriHub started again: showing its window");
                window::show(app, None);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .setup(move |app| {
            log::info!("PeriHub {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
            if args.iter().any(|a| a == AFTER_UPDATE_ARG) {
                log::info!("started by the update, with its window");
            }
            // Autostart: the HKLM entry goes to HKCU, old entries get quoted (see autostart.rs).
            autostart::startup(app.handle());
            autostart::restart_hidden();
            let config = AppConfig::load();
            let devices = match config::try_lock_devices() {
                Ok(Some(lock)) => Some(lock),
                // perihub-cli is running: its commands take a few seconds.
                Ok(None) => {
                    log::info!("waiting for perihub-cli to finish before opening the devices");
                    config::lock_devices().map_err(|e| log::warn!("device lock: {e}")).ok()
                }
                Err(e) => {
                    log::warn!("device lock: {e}");
                    None
                }
            };
            let handle = app.handle().clone();
            // The device listener starts before the app profiles exist: they are set right after.
            let profiles_slot = Arc::new(OnceLock::<Arc<AppProfileEngine>>::new());
            let soft_slot = Arc::new(OnceLock::<(Arc<SoftMacroEngine>, Manager)>::new());
            let (slot, soft_listener) = (profiles_slot.clone(), soft_slot.clone());
            let manager = Manager::start(
                config.keyboard.clone(),
                // The interface asks whether to keep every save to a mouse whose memory format is untested.
                true,
                config.logitech_dpi.clone(),
                config.qmk_keyboards.clone(),
                Box::new(move |status| {
                    if let Some(profiles) = slot.get() {
                        profiles.devices_changed(status);
                    }
                    if let Some((soft, manager)) = soft_listener.get() {
                        soft.watch_mouse(manager.mouse_id());
                        soft.watch_keyboard(manager.keyboard_triggers());
                    }
                    let _ = handle.emit("devices-changed", status);
                }),
            );
            let soft = SoftMacroEngine::new(config.software_macros.clone());
            let _ = soft_slot.set((soft.clone(), manager.clone()));
            soft.watch_mouse(manager.mouse_id());
            soft.watch_keyboard(manager.keyboard_triggers());
            let profiles =
                AppProfileEngine::new(manager.clone(), config.app_profiles.clone(), config.lighting_presets.clone());
            let _ = profiles_slot.set(profiles.clone());
            profiles.devices_changed(&manager.status());
            if config.app_profiles.enabled {
                watch_focus(app.handle(), &profiles);
            }
            let start_hidden = starts_hidden(&args, config.start_hidden);
            app.manage(AppState { manager, config: Mutex::new(config), profiles, soft, _devices: devices });

            app.manage(updates::Available::default());
            window::build_tray(app.handle())?;
            updates::watch(app.handle().clone());

            // Started hidden (autostart), PeriHub creates no window until it is opened.
            if !start_hidden {
                window::show(app.handle(), None);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window destroys it (see window.rs). A DPI or LED preview the interface
            // could not cancel goes back to the saved settings.
            if let WindowEvent::Destroyed = event {
                let manager = window.state::<AppState>().manager.clone();
                std::thread::spawn(move || {
                    if let Ok(mouse) = manager.mouse() {
                        let _ = mouse.call(|m| m.revert_preview_if_any());
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::devices_for_report,
            commands::kb_get_config,
            commands::kb_set_lighting,
            commands::kb_set_brightness,
            commands::kb_read_brightness,
            commands::kb_get_keymap,
            commands::kb_apply_keymap,
            commands::kb_reset_keymap,
            commands::kb_backups,
            commands::kb_restore_backup,
            commands::kb_key_catalog,
            commands::kb_get_presets,
            commands::kb_save_preset,
            commands::kb_delete_preset,
            key_access::kb_key_access,
            key_access::kb_grant_key_access,
            key_access::kb_revoke_key_access,
            qmk_access::qmk_access,
            qmk_access::qmk_grant_access,
            qmk_access::qmk_revoke_access,
            commands::qmk_set_enabled,
            unifying_access::unifying_access,
            unifying_access::unifying_grant_access,
            unifying_access::unifying_revoke_access,
            commands::mouse_get_state,
            commands::mouse_live,
            commands::logitech_live,
            commands::logitech_set_dpi,
            commands::logitech_reset_dpi,
            commands::mouse_save_profile,
            commands::mouse_restore_factory,
            commands::mouse_backups,
            commands::mouse_restore_backup,
            commands::mouse_set_profile_enabled,
            commands::mouse_set_active_profile,
            commands::mouse_preview_dpi,
            commands::mouse_preview_led,
            commands::mouse_revert_preview,
            commands::mouse_undo_last_write,
            commands::mouse_keep_changes,
            commands::g300_info,
            commands::g300_save,
            commands::g300_keep_changes,
            commands::g300_undo_last_write,
            commands::peripheral_info,
            commands::peripheral_change,
            commands::peripheral_keep_changes,
            commands::peripheral_undo,
            commands::peripheral_set_blind,
            commands::corsair_set_lighting,
            commands::corsair_lighting_off,
            commands::corsair_keep_lighting,
            commands::profiles_get,
            commands::profiles_set,
            commands::app_get_settings,
            commands::app_set_settings,
            commands::app_quit,
            commands::app_open_log_folder,
            commands::app_open_test_form,
            soft_macros::soft_get,
            soft_macros::soft_save,
            soft_macros::soft_delete,
            soft_macros::soft_stop_all,
            soft_macros::soft_grant_access,
            soft_macros::soft_revoke_access,
            macro_files::macro_export,
            macro_files::macro_import,
            updates::update_check,
            updates::update_available,
            updates::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start PeriHub")
        .run(|_app, event| {
            // Without windows PeriHub keeps running in the tray; Quit sets an exit code.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("perihub.exe").chain(list.iter().copied()).map(String::from).collect()
    }

    #[test]
    fn the_window_comes_back_after_an_update_even_with_start_hidden_on() {
        assert!(!starts_hidden(&args(&[]), false));
        assert!(starts_hidden(&args(&[]), true));
        assert!(starts_hidden(&args(&[HIDDEN_ARG]), false));
        assert!(!starts_hidden(&args(&[AFTER_UPDATE_ARG]), true));
        assert!(!starts_hidden(&args(&[AFTER_UPDATE_ARG, HIDDEN_ARG]), true));
    }
}

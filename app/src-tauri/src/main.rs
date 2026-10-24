// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod key_access;
mod logfile;
mod macro_files;
mod soft_macros;
mod updates;
mod window;

use std::sync::{Arc, Mutex, OnceLock};

use perihub_core::Manager;
use perihub_core::app_profiles::AppProfileEngine;
use perihub_core::config::AppConfig;
use perihub_core::softmacro::SoftMacroEngine;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, WindowEvent};

pub struct AppState {
    pub manager: Manager,
    pub config: Mutex<AppConfig>,
    pub profiles: Arc<AppProfileEngine>,
    pub soft: Arc<SoftMacroEngine>,
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

fn main() {
    // PeriHub's own messages at info level, other libraries only when something goes wrong.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,perihub=info,perihub_core=info"))
        .target(env_logger::Target::Pipe(Box::new(logfile::LogWriter::open())))
        .init();
    log::info!("PeriHub {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    let hidden_requested = std::env::args().any(|a| a == HIDDEN_ARG);

    tauri::Builder::default()
        // Launching a second time brings the already open window to the front, except autostart's
        // launch, which comes after Windows has reopened a PeriHub left open at sign-out.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == HIDDEN_ARG) {
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
            // Autostart: the HKLM entry goes to HKCU, old entries get quoted (see autostart.rs).
            autostart::startup(app.handle());
            autostart::restart_hidden();
            let config = AppConfig::load();
            let handle = app.handle().clone();
            // The device listener starts before the app profiles exist: they are set right after.
            let profiles_slot = Arc::new(OnceLock::<Arc<AppProfileEngine>>::new());
            let soft_slot = Arc::new(OnceLock::<(Arc<SoftMacroEngine>, Manager)>::new());
            let (slot, soft_listener) = (profiles_slot.clone(), soft_slot.clone());
            let manager = Manager::start(
                config.keyboard.clone(),
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
            let start_hidden = hidden_requested || config.start_hidden;
            app.manage(AppState { manager, config: Mutex::new(config), profiles, soft });

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
            commands::kb_get_config,
            commands::kb_set_lighting,
            commands::kb_set_brightness,
            commands::kb_read_brightness,
            commands::kb_get_keymap,
            commands::kb_apply_keymap,
            commands::kb_reset_keymap,
            commands::kb_key_catalog,
            commands::kb_get_presets,
            commands::kb_save_preset,
            commands::kb_delete_preset,
            key_access::kb_key_access,
            key_access::kb_grant_key_access,
            key_access::kb_revoke_key_access,
            commands::mouse_get_state,
            commands::mouse_live,
            commands::mouse_save_profile,
            commands::mouse_restore_factory,
            commands::mouse_set_profile_enabled,
            commands::mouse_set_active_profile,
            commands::mouse_preview_dpi,
            commands::mouse_preview_led,
            commands::mouse_revert_preview,
            commands::profiles_get,
            commands::profiles_set,
            commands::app_get_settings,
            commands::app_set_settings,
            commands::app_quit,
            commands::app_open_log_folder,
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

//! Updates on Windows. PeriHub asks GitHub for the latest release shortly after starting and then
//! once a day, and tells about a newer version in the tray and in the interface. When the user
//! agrees, it downloads the installer, checks its signature and runs it: the installer closes
//! PeriHub and starts the new version. On Linux updates come from the packages, and on Windows
//! from Scoop when Scoop installed PeriHub: nothing is checked.

use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::AppState;

/// The package manager that installed PeriHub and updates it, if any. Scoop unpacks the installer
/// instead of running it and writes `manifest.json` and `install.json` next to the app: the
/// installer that PeriHub would run installs a second copy, outside Scoop.
pub fn package_manager() -> Option<&'static str> {
    static MANAGER: OnceLock<Option<&'static str>> = OnceLock::new();
    *MANAGER.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        (cfg!(windows) && unpacked_by_scoop(exe.parent()?)).then_some("Scoop")
    })
}

fn unpacked_by_scoop(dir: &std::path::Path) -> bool {
    dir.join("manifest.json").is_file() && dir.join("install.json").is_file()
}

/// This build can update itself.
pub fn supported() -> bool {
    cfg!(windows) && package_manager().is_none()
}

const FIRST_CHECK: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, PartialEq, Serialize)]
pub struct UpdateInfo {
    version: String,
}

/// Newer version found by the last check, if any.
#[derive(Default)]
pub struct Available(Mutex<Option<UpdateInfo>>);

async fn find(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    if let Some(manager) = package_manager() {
        return Err(format!("PeriHub was installed with {manager}: update it from there"));
    }
    if !supported() {
        return Err("on Linux, updates come from your packages".into());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| format!("cannot check for updates: {e}"))
}

/// Remembers the result of a check; a version not seen before is announced in the tray and to
/// the interface, if it is open.
fn found(app: &AppHandle, update: Option<UpdateInfo>) {
    let state = app.state::<Available>();
    let new = {
        let mut available = state.0.lock().unwrap();
        let new = update.is_some() && *available != update;
        *available = update.clone();
        new
    };
    if let (true, Some(update)) = (new, update) {
        log::info!("PeriHub {} is available", update.version);
        crate::window::show_update(app, &update.version);
        let _ = app.emit("update-available", &update);
    }
}

/// Checks in the background, if the user did not turn it off in Settings.
pub fn watch(app: AppHandle) {
    if let Some(manager) = package_manager() {
        log::info!("installed with {manager}: updates come from it");
    }
    if !supported() {
        return;
    }
    let spawned = thread::Builder::new().name("perihub-updates".into()).spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            if app.state::<AppState>().config.lock().unwrap().check_updates {
                match tauri::async_runtime::block_on(find(&app)) {
                    Ok(update) => found(&app, update.map(|u| UpdateInfo { version: u.version })),
                    Err(error) => log::warn!("{error}"),
                }
            }
            thread::sleep(CHECK_EVERY);
        }
    });
    if let Err(error) = spawned {
        log::warn!("update checks not started: {error}");
    }
}

/// Checks now ("Check now" in Settings).
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = find(&app).await?.map(|update| UpdateInfo { version: update.version });
    found(&app, update.clone());
    Ok(update)
}

/// Result of the last check, without asking GitHub again.
#[tauri::command]
pub fn update_available(available: State<'_, Available>) -> Option<UpdateInfo> {
    available.0.lock().unwrap().clone()
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = find(&app).await?.ok_or("PeriHub is already up to date")?;
    log::info!("installing PeriHub {}", update.version);
    // The installer closes PeriHub without asking: give a Corsair mouse back first.
    let manager = app.state::<crate::AppState>().manager.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || manager.release_corsair()).await;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("update failed: {e}"))?;
    // On Windows the installer has already closed PeriHub; elsewhere start the new version.
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoop_install_is_recognized_by_its_files() {
        let dir = std::env::temp_dir().join(format!("perihub-scoop-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("perihub.exe"), b"").unwrap();
        std::fs::write(dir.join("manifest.json"), b"{}").unwrap();
        assert!(!unpacked_by_scoop(&dir));
        std::fs::write(dir.join("install.json"), b"{}").unwrap();
        assert!(unpacked_by_scoop(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

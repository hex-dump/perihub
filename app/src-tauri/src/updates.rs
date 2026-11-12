//! Updates on Windows. PeriHub asks GitHub for the latest release shortly after starting and then
//! once a day, and tells about a newer version in the tray and in the interface. As soon as it
//! finds one it downloads the installer into its cache folder and checks its signature, so that
//! "Update and restart" starts it at once; nothing is installed before that click. The installer
//! closes PeriHub and starts the new version, with its window. On Linux updates come from the
//! packages, and on Windows from Scoop when Scoop installed PeriHub: nothing is checked.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

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
/// The installer's NSIS option that starts PeriHub after installing it (see `checker`).
const START_AFTER_INSTALL: &str = "/R";

#[derive(Clone, PartialEq, Serialize)]
pub struct UpdateInfo {
    version: String,
}

/// How far the download of the installer has gone, for the interface.
#[derive(Clone, Serialize)]
struct Progress {
    received: u64,
    total: Option<u64>,
}

/// Newer version found by the last check, if any, with what is needed to install it.
#[derive(Default)]
pub struct Available {
    info: Mutex<Option<UpdateInfo>>,
    /// The update itself, so that installing it does not ask GitHub again.
    update: Mutex<Option<Update>>,
    /// Held while the installer is being downloaded: an install waits for a download on its way.
    downloading: tauri::async_runtime::Mutex<()>,
}

/// Asks GitHub for the latest release. The installer runs with the plugin's own restart turned
/// off and `/R` instead: the plugin would start PeriHub again with the arguments it had, and a
/// copy that autostart started with `--hidden` would come back without its window, as if the
/// update had not finished. With `/R` and no `/ARGS`, Tauri's NSIS template starts it with none.
async fn find(app: &AppHandle) -> Result<Option<Update>, String> {
    if let Some(manager) = package_manager() {
        return Err(format!("PeriHub was installed with {manager}: update it from there"));
    }
    if !supported() {
        return Err("on Linux, updates come from your packages".into());
    }
    let updater = app
        .updater_builder()
        .restart_after_install(false)
        .installer_arg(START_AFTER_INSTALL)
        .build()
        .map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| format!("cannot check for updates: {e}"))
}

/// Where the installer of `version` waits once downloaded.
fn installer_path(app: &AppHandle, version: &str) -> Option<PathBuf> {
    let dir = app.path().app_cache_dir().ok()?.join("updates");
    Some(dir.join(format!("PeriHub_{version}_update.bin")))
}

/// Downloads the installer of `update` and checks its signature, unless it is already in the cache
/// folder; installers of other versions are removed. Returns where it is.
async fn download(app: &AppHandle, update: &Update) -> Result<PathBuf, String> {
    let available = app.state::<Available>();
    let _downloading = available.downloading.lock().await;
    let path = installer_path(app, &update.version).ok_or("no cache folder")?;
    if let Ok(bytes) = std::fs::read(&path) {
        if update.verify(&bytes).is_ok() {
            return Ok(path);
        }
        log::warn!("the downloaded installer of PeriHub {} does not match its signature", update.version);
    }
    let dir = path.parent().ok_or("no cache folder")?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let (mut received, mut shown) = (0u64, u64::MAX);
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                // About a hundred events for the whole download.
                let step = total.map_or(256 * 1024, |t| (t / 100).max(1));
                if received / step != shown {
                    shown = received / step;
                    let _ = app.emit("update-progress", Progress { received, total });
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    let partial = path.with_extension("part");
    std::fs::write(&partial, &bytes)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("cannot save the installer: {e}"))?;
    log::info!("PeriHub {} downloaded, ready to install", update.version);
    Ok(path)
}

/// Remembers the result of a check; a version not seen before is announced in the tray and to
/// the interface, if it is open, and its installer is downloaded.
fn found(app: &AppHandle, update: Option<Update>) {
    let state = app.state::<Available>();
    let info = update.as_ref().map(|u| UpdateInfo { version: u.version.clone() });
    let new = {
        let mut available = state.info.lock().unwrap();
        let new = info.is_some() && *available != info;
        *available = info.clone();
        new
    };
    *state.update.lock().unwrap() = update.clone();
    if let (true, Some(info), Some(update)) = (new, info, update) {
        log::info!("PeriHub {} is available", info.version);
        crate::window::show_update(app, &info.version);
        let _ = app.emit("update-available", &info);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = download(&app, &update).await {
                log::warn!("PeriHub {}: {error}", update.version);
            }
        });
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
    // The installer of the version now running, downloaded before it was installed.
    if let Some(path) = installer_path(&app, &app.package_info().version.to_string()) {
        let _ = std::fs::remove_file(path);
    }
    let spawned = thread::Builder::new().name("perihub-updates".into()).spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            if app.state::<AppState>().config.lock().unwrap().check_updates {
                match tauri::async_runtime::block_on(find(&app)) {
                    Ok(update) => found(&app, update),
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
    let update = find(&app).await?;
    let info = update.as_ref().map(|u| UpdateInfo { version: u.version.clone() });
    found(&app, update);
    Ok(info)
}

/// Result of the last check, without asking GitHub again.
#[tauri::command]
pub fn update_available(available: State<'_, Available>) -> Option<UpdateInfo> {
    available.info.lock().unwrap().clone()
}

/// Installs the update found by the last check, with the installer already downloaded if it is
/// there: only without a check yet is GitHub asked again.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let cached = app.state::<Available>().update.lock().unwrap().clone();
    let update = match cached {
        Some(update) => update,
        None => find(&app).await?.ok_or("PeriHub is already up to date")?,
    };
    let path = download(&app, &update).await?;
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read the installer: {e}"))?;
    log::info!("installing PeriHub {}", update.version);
    // The installer closes PeriHub without asking: give a Corsair mouse back first.
    let manager = app.state::<crate::AppState>().manager.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || manager.release_corsair()).await;
    // The signature is checked again before the installer runs.
    update.install(bytes).map_err(|e| format!("update failed: {e}"))?;
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

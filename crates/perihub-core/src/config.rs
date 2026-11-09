//! Settings saved to disk, and backups.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::effects::KeyboardLighting;
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardConfig {
    pub lighting: KeyboardLighting,
    /// Brightness 0–16 (the keyboard's scale).
    pub brightness: u8,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self { lighting: KeyboardLighting::default(), brightness: crate::apex::MAX_BRIGHTNESS }
    }
}

/// A keyboard lighting saved by name, so app profiles can use it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingPreset {
    pub name: String,
    pub lighting: KeyboardLighting,
}

/// What to change while an app is in focus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRule {
    /// Window class on Linux, executable name on Windows (compared ignoring case).
    pub app: String,
    /// Name of a lighting preset, or `None` to keep the keyboard as it is.
    pub lighting: Option<String>,
    /// Onboard profile to activate on the mouse, or `None` to keep it as it is.
    pub mouse_profile: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppProfiles {
    pub enabled: bool,
    pub rules: Vec<AppRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub keyboard: KeyboardConfig,
    /// Start hidden in the system tray.
    pub start_hidden: bool,
    pub lighting_presets: Vec<LightingPreset>,
    pub app_profiles: AppProfiles,
    /// Look for a new version on GitHub at startup (Windows).
    pub check_updates: bool,
    /// Macros run by PeriHub, started by buttons that send their trigger key.
    pub software_macros: Vec<crate::softmacro::SoftwareMacro>,
    /// USB product ids of untested mice that the user chose to edit anyway (experimental editing).
    pub experimental_mice: Vec<u16>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            keyboard: KeyboardConfig::default(),
            start_hidden: false,
            lighting_presets: Vec::new(),
            app_profiles: AppProfiles::default(),
            check_updates: true,
            software_macros: Vec::new(),
            experimental_mice: Vec::new(),
        }
    }
}

#[cfg(not(test))]
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("perihub")
}

/// Tests never touch the user's real settings and backups: each test gets a folder of its own.
#[cfg(test)]
pub fn config_dir() -> PathBuf {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("main").replace("::", "-");
    std::env::temp_dir().join(format!("perihub-test-config-{}-{name}", std::process::id()))
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

impl AppConfig {
    pub fn load() -> Self {
        match fs::read_to_string(config_path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                log::warn!("invalid config.json, using defaults: {error}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        fs::create_dir_all(path.parent().unwrap())?;
        // Atomic write: a crash halfway through does not leave a truncated file.
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_string_pretty(self)?)?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

/// Saves a backup before every write to a device's memory.
pub fn write_backup(device: &str, label: &str, files: &[(&str, &[u8])]) -> Result<PathBuf> {
    write_backup_in(&config_dir().join("backups").join(device), label, files)
}

/// [`write_backup`] into the backup folder `root` of a device.
pub(crate) fn write_backup_in(root: &Path, label: &str, files: &[(&str, &[u8])]) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let dir = root.join(format!("{stamp}-{label}"));
    fs::create_dir_all(&dir)?;
    for (name, data) in files {
        fs::write(dir.join(name), data)?;
    }
    Ok(dir)
}

/// Held while a PeriHub process, the app or perihub-cli, talks to the devices. Two of them at once
/// would mix their requests to the same device, and a write could be read back wrong. The operating
/// system releases the lock when the process ends, also if it crashes.
pub struct DeviceLock {
    _file: fs::File,
}

fn lock_file(root: &Path) -> Result<fs::File> {
    fs::create_dir_all(root)?;
    Ok(fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join("devices.lock"))?)
}

/// Takes the device lock, or returns `None` if another PeriHub process holds it.
pub fn try_lock_devices() -> Result<Option<DeviceLock>> {
    try_lock_in(&config_dir())
}

fn try_lock_in(root: &Path) -> Result<Option<DeviceLock>> {
    let file = lock_file(root)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(DeviceLock { _file: file })),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Takes the device lock, waiting for another PeriHub process to let it go.
pub fn lock_devices() -> Result<DeviceLock> {
    let file = lock_file(&config_dir())?;
    file.lock()?;
    Ok(DeviceLock { _file: file })
}

/// A backup saved by [`write_backup`], identified by its folder name: `<unix seconds>-<label>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupEntry {
    pub id: String,
    /// Unix time in seconds.
    pub created: u64,
    /// What was about to be written, such as `profile3`, `factory2`, `directory`, `keymap` or `restore`.
    pub label: String,
}

impl BackupEntry {
    fn parse(id: &str) -> Option<Self> {
        let (stamp, label) = id.split_once('-')?;
        let created = stamp.parse().ok()?;
        let valid = !label.is_empty() && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        valid.then(|| Self { id: id.to_string(), created, label: label.to_string() })
    }
}

/// The backups of a device, newest first.
pub fn list_backups(device: &str) -> Vec<BackupEntry> {
    list_backups_in(&config_dir().join("backups").join(device))
}

fn list_backups_in(root: &Path) -> Vec<BackupEntry> {
    let Ok(dirs) = fs::read_dir(root) else { return Vec::new() };
    let mut entries: Vec<BackupEntry> = dirs
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| BackupEntry::parse(&e.file_name().to_string_lossy()))
        .collect();
    entries.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
    entries
}

/// Reads one file of a backup. The id must be one that [`list_backups`] returns: it cannot point
/// outside the device's backup folder.
pub fn read_backup_file(device: &str, id: &str, name: &str) -> Result<Vec<u8>> {
    read_backup_file_in(&config_dir().join("backups").join(device), id, name)
}

pub(crate) fn read_backup_file_in(root: &Path, id: &str, name: &str) -> Result<Vec<u8>> {
    if BackupEntry::parse(id).is_none() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(Error::Invalid(format!("invalid backup {id:?}")));
    }
    fs::read(root.join(id).join(name)).map_err(|e| Error::Invalid(format!("backup {id}: cannot read {name}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("perihub-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn backups_are_listed_newest_first_and_odd_folders_are_skipped() {
        let root = temp_root("list");
        for dir in ["1792438882-profile5", "1793902939-reload-probe", "1792247177-keymap", "notes", "12-", "x-profile1"]
        {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("1799999999-file"), b"not a folder").unwrap();
        let ids: Vec<_> = list_backups_in(&root).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["1793902939-reload-probe", "1792438882-profile5", "1792247177-keymap"]);
        let entry = BackupEntry::parse("1792438882-profile5").unwrap();
        assert_eq!((entry.created, entry.label.as_str()), (1792438882, "profile5"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_one_process_at_a_time_holds_the_device_lock() {
        let root = temp_root("lock");
        let first = try_lock_in(&root).unwrap();
        assert!(first.is_some());
        // A second open file description stands for a second process.
        assert!(try_lock_in(&root).unwrap().is_none());
        drop(first);
        assert!(try_lock_in(&root).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_folder_has_no_backups() {
        assert!(list_backups_in(Path::new("/nonexistent/perihub/backups")).is_empty());
    }

    #[test]
    fn backup_files_cannot_be_read_outside_the_backup() {
        let root = temp_root("read");
        fs::create_dir_all(root.join("1792438882-profile5")).unwrap();
        fs::write(root.join("1792438882-profile5/sector_0000.bin"), [1, 2, 3]).unwrap();
        assert_eq!(read_backup_file_in(&root, "1792438882-profile5", "sector_0000.bin").unwrap(), [1, 2, 3]);
        for (id, name) in
            [("../g502", "sector_0000.bin"), ("1792438882-profile5", "../x"), ("1-a", "..\\x"), ("a", "b")]
        {
            assert!(read_backup_file_in(&root, id, name).is_err(), "{id} {name}");
        }
        assert!(read_backup_file_in(&root, "1792438882-profile5", "missing.bin").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

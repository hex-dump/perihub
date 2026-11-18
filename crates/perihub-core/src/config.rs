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
    /// Schema version of the file (see [`migrate`]). Files without it are version 1.
    pub version: u32,
    pub keyboard: KeyboardConfig,
    /// Start hidden in the system tray.
    pub start_hidden: bool,
    pub lighting_presets: Vec<LightingPreset>,
    pub app_profiles: AppProfiles,
    /// Look for a new version on GitHub at startup (Windows).
    pub check_updates: bool,
    /// Macros run by PeriHub, started by buttons that send their trigger key.
    pub software_macros: Vec<crate::softmacro::SoftwareMacro>,
    /// The DPI PeriHub sets every time a Logitech device it knows by its features connects, by the
    /// device's settings key (see `UnsupportedDevice::settings_key`).
    pub logitech_dpi: std::collections::HashMap<String, u16>,
    /// The last look PeriHub showed on a Logitech keyboard whose lights it drove, by the keyboard's
    /// settings key: shown in the editor again.
    pub logitech_looks: std::collections::HashMap<String, crate::hidpp::host::Look>,
    /// The keyboards whose lights the user wants PeriHub to drive, by settings key: taken with
    /// their look in `logitech_looks` every time they connect, at every start too, until the user
    /// gives them back.
    pub logitech_drive: Vec<String>,
    /// Keyboards with a raw HID collection the user turned on as VIA keyboards, by "vvvv:pppp".
    pub qmk_keyboards: Vec<String>,
    /// Read-only mode: nothing that changes a device is sent to it (see [`crate::writes`]). The
    /// app and the command line both follow it.
    pub read_only: bool,
    /// Notifications about the battery of wireless devices.
    pub battery: crate::battery::BatteryAlerts,
    /// PeriHub already said, the first time the window was closed, that it keeps running in the tray.
    pub close_hint_shown: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            keyboard: KeyboardConfig::default(),
            start_hidden: false,
            lighting_presets: Vec::new(),
            app_profiles: AppProfiles::default(),
            check_updates: true,
            software_macros: Vec::new(),
            logitech_dpi: std::collections::HashMap::new(),
            logitech_looks: std::collections::HashMap::new(),
            logitech_drive: Vec::new(),
            qmk_keyboards: Vec::new(),
            read_only: false,
            battery: Default::default(),
            close_hint_shown: false,
        }
    }
}

/// The schema version PeriHub writes. Raise it, and add a step to [`migrate`], when the format
/// changes in a way that old files cannot be read as they are.
pub const CONFIG_VERSION: u32 = 1;

/// Brings a parsed settings file up to [`CONFIG_VERSION`], so a format change can convert old
/// files instead of resetting them. No step exists yet: version 1 is the only format.
fn migrate(value: serde_json::Value) -> serde_json::Value {
    // Files from before the field existed are version 1.
    let _version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    // Future: if _version < 2 { ...convert...; set "version": 2 }
    value
}

/// Parses and migrates the text of a settings file.
fn parse_config(bytes: &[u8]) -> std::result::Result<AppConfig, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut config: AppConfig = serde_json::from_value(migrate(value)).map_err(|e| e.to_string())?;
    config.version = CONFIG_VERSION;
    Ok(config)
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

/// Why the settings file could not be used, for the interface to tell the user.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigProblem {
    /// What went wrong, as the parser or the system said it.
    pub error: String,
    /// Where a copy of the unreadable file was kept, when one could be made.
    pub backup: Option<PathBuf>,
    /// The last good save (`config.json.bak`) was valid and is in use instead.
    pub restored: bool,
}

impl ConfigProblem {
    /// A sentence for the user.
    pub fn message(&self) -> String {
        let kept = match &self.backup {
            Some(path) => format!(" The old file was kept as {}.", path.display()),
            None => String::new(),
        };
        if self.restored {
            return format!(
                "PeriHub could not read its settings file, so it restored the last good save (config.json.bak).{kept} Check that your settings are as you expect ({}).",
                self.error
            );
        }
        format!(
            "PeriHub could not read its settings file, so it started with the default settings and read-only mode on, to be safe.{kept} Turn read-only mode off when you are ready to change your devices ({}).",
            self.error
        )
    }
}

/// The settings, and what went wrong when they could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub problem: Option<ConfigProblem>,
}

impl AppConfig {
    /// The settings used when the file exists but cannot be read: the defaults, with read-only
    /// mode ON. The file may have said it was on, and nothing sent to a device can be undone by a
    /// settings file that is lost, so a doubt never turns it off.
    pub fn safe_defaults() -> Self {
        Self { read_only: true, ..Self::default() }
    }

    pub fn load() -> Self {
        Self::load_checked().config
    }

    /// Like [`AppConfig::load`], and also says why the file was not used. A missing file is a first
    /// start, not a problem. A file that cannot be read or parsed is copied to
    /// `config.json.bad-<unix seconds>` (the original stays until the next save replaces it, and a
    /// copy with the same content is not made twice), and the settings are [`AppConfig::safe_defaults`].
    pub fn load_checked() -> LoadedConfig {
        load_from(&config_path())
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        fs::create_dir_all(path.parent().unwrap())?;
        let text = serde_json::to_string_pretty(self)?;
        write_atomic(&path, text.as_bytes())?;
        // The last good config, for when config.json is damaged later. Failing here does not fail
        // the save: the settings themselves are already on disk.
        if let Err(e) = write_atomic(&bak_path(&path), text.as_bytes()) {
            log::warn!("could not update config.json.bak: {e}");
        }
        Ok(())
    }
}

/// Atomic write: a temporary file, then a rename, so a crash halfway through does not leave a
/// truncated file.
fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    fs::write(&temp, content)?;
    fs::rename(&temp, path)
}

fn bak_path(path: &Path) -> PathBuf {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    PathBuf::from(bak)
}

/// The settings could not be used: the last good save if it is valid, else the safe defaults.
fn recover(path: &Path, error: String, backup: Option<PathBuf>) -> LoadedConfig {
    if let Ok(bytes) = fs::read(bak_path(path))
        && let Ok(config) = parse_config(&bytes)
    {
        log::warn!("using config.json.bak, the last good save");
        return LoadedConfig { config, problem: Some(ConfigProblem { error, backup, restored: true }) };
    }
    LoadedConfig { config: AppConfig::safe_defaults(), problem: Some(ConfigProblem { error, backup, restored: false }) }
}

fn load_from(path: &Path) -> LoadedConfig {
    let text = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return LoadedConfig { config: AppConfig::default(), problem: None };
        }
        Err(e) => {
            log::warn!("cannot read config.json, using safe defaults: {e}");
            return recover(path, e.to_string(), None);
        }
    };
    match parse_config(&text) {
        Ok(config) => LoadedConfig { config, problem: None },
        Err(error) => {
            log::warn!("invalid config.json: {error}");
            let backup = keep_bad_copy(path, &text);
            recover(path, error, backup)
        }
    }
}

/// Copies an unreadable settings file to `<name>.bad-<unix seconds>` next to it, or returns the
/// copy that already holds the same content (PeriHub starts again and again with the same file).
fn keep_bad_copy(path: &Path, content: &[u8]) -> Option<PathBuf> {
    let dir = path.parent()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let prefix = format!("{name}.bad-");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix)
                && fs::read(entry.path()).is_ok_and(|old| old == content)
            {
                return Some(entry.path());
            }
        }
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Two different bad files in the same second must not overwrite each other.
    let mut copy = dir.join(format!("{prefix}{stamp}"));
    let mut n = 1;
    while copy.exists() {
        copy = dir.join(format!("{prefix}{stamp}-{n}"));
        n += 1;
    }
    match fs::write(&copy, content) {
        Ok(()) => Some(copy),
        Err(e) => {
            log::warn!("could not keep a copy of the unreadable config.json: {e}");
            None
        }
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
    fn read_only_mode_is_off_unless_the_settings_turn_it_on() {
        assert!(!AppConfig::default().read_only);
        // Settings saved by an older PeriHub have no such field.
        assert!(!serde_json::from_str::<AppConfig>("{}").unwrap().read_only);
        assert!(serde_json::from_str::<AppConfig>(r#"{"read_only": true}"#).unwrap().read_only);
    }

    #[test]
    fn a_missing_config_is_a_first_start_not_a_problem() {
        let root = temp_root("config-missing");
        let loaded = load_from(&root.join("config.json"));
        assert_eq!(loaded, LoadedConfig { config: AppConfig::default(), problem: None });
    }

    #[test]
    fn a_good_config_is_loaded_as_it_is() {
        let root = temp_root("config-good");
        fs::write(root.join("config.json"), r#"{"read_only": false, "start_hidden": true}"#).unwrap();
        let loaded = load_from(&root.join("config.json"));
        assert!(loaded.problem.is_none());
        assert!(loaded.config.start_hidden && !loaded.config.read_only);
    }

    #[test]
    fn a_corrupt_config_is_kept_and_read_only_mode_stays_on() {
        let root = temp_root("config-corrupt");
        let path = root.join("config.json");
        // A file cut short, as a crash or a full disk leaves it, that had read-only mode off.
        let broken = r#"{"read_only": false, "start_hidden": tr"#;
        fs::write(&path, broken).unwrap();
        let loaded = load_from(&path);
        assert!(loaded.config.read_only, "a doubt must never turn read-only mode off");
        let problem = loaded.problem.expect("the interface must be told");
        let backup = problem.backup.clone().expect("a copy is kept");
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with("config.json.bad-"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), broken);
        // The original is untouched until the next save, and the message names the copy.
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        assert!(problem.message().contains(&backup.display().to_string()));
        // Starting again with the same file does not pile up copies.
        assert_eq!(load_from(&path).problem.unwrap().backup, Some(backup));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
    }

    #[test]
    fn a_corrupt_config_is_replaced_by_the_last_good_save() {
        let root = temp_root("config-bak-good");
        let path = root.join("config.json");
        fs::write(&path, r#"{"read_only": false, "start_hidden": tr"#).unwrap();
        fs::write(root.join("config.json.bak"), r#"{"read_only": false, "start_hidden": true}"#).unwrap();
        let loaded = load_from(&path);
        assert!(loaded.config.start_hidden && !loaded.config.read_only);
        let problem = loaded.problem.expect("the interface must be told");
        assert!(problem.restored && problem.backup.is_some());
        assert!(problem.message().contains("last good save"));
    }

    #[test]
    fn a_corrupt_config_and_a_corrupt_bak_mean_safe_defaults() {
        let root = temp_root("config-bak-bad");
        let path = root.join("config.json");
        fs::write(&path, "{").unwrap();
        fs::write(root.join("config.json.bak"), "[").unwrap();
        let loaded = load_from(&path);
        assert!(loaded.config.read_only && !loaded.config.start_hidden);
        assert!(!loaded.problem.unwrap().restored);
    }

    #[test]
    fn a_corrupt_config_without_a_bak_means_safe_defaults() {
        let root = temp_root("config-bak-missing");
        let path = root.join("config.json");
        fs::write(&path, "{").unwrap();
        let loaded = load_from(&path);
        assert!(loaded.config.read_only);
        assert!(!loaded.problem.unwrap().restored);
    }

    #[test]
    fn saving_keeps_the_last_good_copy_and_the_version() {
        let config = AppConfig { start_hidden: true, ..AppConfig::default() };
        config.save().unwrap();
        let dir = config_dir();
        let saved = fs::read(dir.join("config.json")).unwrap();
        assert_eq!(saved, fs::read(dir.join("config.json.bak")).unwrap());
        assert!(!dir.join("config.json.tmp").exists());
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(value["version"], CONFIG_VERSION);
        assert_eq!(load_from(&dir.join("config.json")).config, config);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_without_a_version_is_version_one() {
        let config = parse_config(br#"{"start_hidden": true}"#).unwrap();
        assert_eq!(config.version, 1);
        assert!(config.start_hidden);
    }

    #[test]
    fn different_corrupt_files_keep_separate_copies() {
        let root = temp_root("config-corrupt-twice");
        let path = root.join("config.json");
        fs::write(&path, "{").unwrap();
        let first = load_from(&path).problem.unwrap().backup.unwrap();
        fs::write(&path, "[").unwrap();
        let second = load_from(&path).problem.unwrap().backup.unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "{");
        assert_eq!(fs::read_to_string(second).unwrap(), "[");
    }

    #[test]
    fn an_unreadable_config_also_means_read_only_mode_on() {
        let root = temp_root("config-unreadable");
        // A folder where the file should be: reading it fails with something other than "not found".
        fs::create_dir_all(root.join("config.json")).unwrap();
        let loaded = load_from(&root.join("config.json"));
        assert!(loaded.config.read_only);
        assert!(loaded.problem.is_some_and(|p| p.backup.is_none()));
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

    #[test]
    fn the_old_list_of_experimental_mice_is_ignored() {
        // Until 0.6.6 untested mice were edited after "Edit anyway…": the memory format decides now.
        let config: AppConfig =
            serde_json::from_str(r#"{"start_hidden": true, "experimental_mice": [49283]}"#).unwrap();
        assert!(config.start_hidden);
    }
}

//! Keyboard state and operations, run on the device's dedicated thread.

use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::apex::input::{KeyWatcher, KeyZones};
use crate::apex::{Apex3Tkl, KeyBinding, Layer, MAX_BRIGHTNESS};
use crate::config::{self, BackupEntry, KeyboardConfig, write_backup};
use crate::effects::{Frame, KeyboardLighting, NO_PRESSES};
use crate::error::{Error, Result};
use crate::softmacro::TRIGGER_KEYS;
use crate::worker::DeviceTask;

const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// Static colors are resent now and then: after a suspend the keyboard may fall back to the rainbow.
const STATIC_REFRESH: Duration = Duration::from_secs(5);
/// After a reconnect the key interfaces may not be accessible yet: reading them is retried this often.
const KEYS_RETRY: Duration = Duration::from_secs(3);
/// Folder of the key map backups.
const BACKUPS: &str = "apex3tkl";

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardInfo {
    pub name: &'static str,
    pub firmware: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keymap {
    pub normal: Layer,
    pub steelseries: Layer,
}

/// Paths of the interfaces that report key presses (interface 0, and interface 2 for keys beyond six).
#[derive(Debug, Clone, Default)]
pub struct KeyInterfaces {
    pub boot: Option<CString>,
    pub bitmap: Option<CString>,
}

pub struct KeyboardTask {
    dev: Apex3Tkl,
    pub info: KeyboardInfo,
    lighting: KeyboardLighting,
    started: Instant,
    last_frame: Option<Instant>,
    key_interfaces: KeyInterfaces,
    /// Reads key presses, only while the lighting reacts to them.
    keys: Option<KeyWatcher>,
    keys_failed: Option<Instant>,
    /// Keys that start software macros which the key map sends (see [`macro_keys`]).
    macro_keys: Arc<Mutex<Vec<u8>>>,
    /// Zone each key lights, from the key map.
    key_zones: KeyZones,
}

/// Keys that start software macros ([`TRIGGER_KEYS`]) which the key map sends, on either layer.
fn macro_keys(keymap: &Keymap) -> Vec<u8> {
    let mut keys: Vec<u8> = [&keymap.normal, &keymap.steelseries]
        .into_iter()
        .flat_map(|layer| layer.values())
        .filter_map(|binding| match binding {
            KeyBinding::Key { usage } if TRIGGER_KEYS.contains(usage) => Some(*usage),
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

impl KeyboardTask {
    pub fn open(dev: Apex3Tkl, config: &KeyboardConfig, key_interfaces: KeyInterfaces) -> Result<Self> {
        let firmware = dev.firmware_version()?;
        let mut task = Self {
            dev,
            info: KeyboardInfo { name: "SteelSeries Apex 3 TKL", firmware },
            lighting: config.lighting.clone(),
            started: Instant::now(),
            last_frame: None,
            key_interfaces,
            keys: None,
            keys_failed: None,
            macro_keys: Arc::default(),
            key_zones: KeyZones::default(),
        };
        task.dev.set_brightness(config.brightness.min(MAX_BRIGHTNESS))?;
        // Which keys start software macros, and which zones remapped keys light. The keyboard works
        // without it.
        match task.keymap() {
            Ok(keymap) => task.keymap_changed(&keymap),
            Err(error) => log::warn!("keyboard: key map not read: {error}"),
        }
        if let Err(error) = task.watch_keys() {
            log::warn!("reactive lighting: {error}");
        }
        task.render()?;
        Ok(task)
    }

    /// Starts or stops reading key presses, depending on the lighting.
    fn watch_keys(&mut self) -> Result<()> {
        self.keys_failed = None;
        if self.lighting.fade().is_none() {
            self.keys = None;
        } else if self.keys.is_none() {
            let (boot, bitmap) = (self.key_interfaces.boot.as_ref(), self.key_interfaces.bitmap.as_ref());
            match KeyWatcher::start(boot, bitmap, self.key_zones.clone()) {
                Ok(watcher) => self.keys = Some(watcher),
                Err(error) => {
                    self.keys_failed = Some(Instant::now());
                    return Err(Error::Invalid(format!("key presses cannot be read: {error}")));
                }
            }
        }
        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        let since_press = self.keys.as_ref().map_or(NO_PRESSES, KeyWatcher::since_press);
        match self.lighting.frame(self.started.elapsed(), &since_press) {
            Frame::Colors(colors) => self.dev.set_colors(&colors)?,
            Frame::HardwareRainbow => self.dev.set_hardware_rainbow()?,
        }
        self.last_frame = Some(Instant::now());
        Ok(())
    }

    /// Applies the lighting. If it reacts to keys and they cannot be read, the base color is shown
    /// and the error is returned.
    pub fn set_lighting(&mut self, lighting: KeyboardLighting) -> Result<()> {
        self.lighting = lighting;
        self.started = Instant::now();
        let watching = self.watch_keys();
        self.render()?;
        watching
    }

    /// Whether a pressed zone is still fading, so frames must keep coming.
    fn reacting(&self) -> bool {
        let (Some(keys), Some(fade)) = (&self.keys, self.lighting.fade()) else { return false };
        keys.since_press().iter().flatten().any(|&t| t < fade + 2 * FRAME_INTERVAL)
    }

    pub fn set_brightness(&mut self, level: u8) -> Result<()> {
        if level > MAX_BRIGHTNESS {
            return Err(Error::Invalid(format!("brightness {level}")));
        }
        self.dev.set_brightness(level)
    }

    /// Current brightness, which the user may have changed with SteelSeries + F11/F12.
    pub fn brightness(&self) -> Result<u8> {
        self.dev.brightness()
    }

    pub fn keymap(&self) -> Result<Keymap> {
        Ok(Keymap { normal: self.dev.read_layer(0)?, steelseries: self.dev.read_layer(1)? })
    }

    /// Changes the key map after saving a backup of it.
    pub fn apply_keymap(&mut self, changes: &[(u8, u8, KeyBinding)]) -> Result<Keymap> {
        self.write_keymap(changes, "keymap")
    }

    fn write_keymap(&mut self, changes: &[(u8, u8, KeyBinding)], label: &str) -> Result<Keymap> {
        let before = self.keymap()?;
        write_backup(BACKUPS, label, &[("keymap.json", serde_json::to_string_pretty(&before)?.as_bytes())])?;
        let [normal, steelseries] = self.dev.apply_changes(changes)?;
        let keymap = Keymap { normal, steelseries };
        self.keymap_changed(&keymap);
        Ok(keymap)
    }

    fn keymap_changed(&mut self, keymap: &Keymap) {
        *self.macro_keys.lock().unwrap() = macro_keys(keymap);
        self.key_zones = KeyZones::from_keymap(&keymap.normal, &keymap.steelseries);
        if let Some(keys) = &self.keys {
            keys.set_zones(self.key_zones.clone());
        }
    }

    /// Keys that start software macros which the key map sends, kept up to date.
    pub fn macro_keys_handle(&self) -> Arc<Mutex<Vec<u8>>> {
        self.macro_keys.clone()
    }

    /// Resets all keys to their factory values.
    pub fn reset_keymap(&mut self) -> Result<Keymap> {
        let current = self.keymap()?;
        let mut changes = Vec::new();
        for (layer, map) in [(0u8, &current.normal), (1u8, &current.steelseries)] {
            for (&source, &binding) in map {
                let factory = KeyBinding::factory(layer, source);
                if binding != factory && crate::apex::keys::is_remappable_source(source) {
                    changes.push((layer, source, factory));
                }
            }
        }
        self.apply_keymap(&changes)
    }

    /// The key map backups, newest first.
    pub fn backups(&self) -> Vec<BackupEntry> {
        config::list_backups(BACKUPS)
    }

    /// Puts back the key map saved in a backup, after backing up the current one.
    pub fn restore_backup(&mut self, id: &str) -> Result<KeymapRestore> {
        let data = config::read_backup_file(BACKUPS, id, "keymap.json")?;
        let backup: Keymap = serde_json::from_slice(&data)
            .map_err(|e| Error::Invalid(format!("backup {id}: unreadable key map: {e}")))?;
        let current = self.keymap()?;
        let changes = restore_changes(&current, &backup);
        if changes.is_empty() {
            return Ok(KeymapRestore { keymap: current, keys: 0 });
        }
        // Every binding is checked again before it is written (see Apex3Tkl::apply_changes).
        let keymap = self.write_keymap(&changes, "restore")?;
        log::info!("key map backup restored: {id}, {} keys", changes.len());
        Ok(KeymapRestore { keymap, keys: changes.len() })
    }
}

/// What restoring a key map backup changed.
#[derive(Debug, Clone, Serialize)]
pub struct KeymapRestore {
    pub keymap: Keymap,
    /// How many keys changed, on both layers.
    pub keys: usize,
}

/// Keys whose binding differs from the backup. Keys that cannot be remapped are left alone.
fn restore_changes(current: &Keymap, backup: &Keymap) -> Vec<(u8, u8, KeyBinding)> {
    let mut changes = Vec::new();
    for (layer, now, saved) in
        [(0u8, &current.normal, &backup.normal), (1u8, &current.steelseries, &backup.steelseries)]
    {
        for (&source, &binding) in saved {
            if crate::apex::keys::is_remappable_source(source) && now.get(&source) != Some(&binding) {
                changes.push((layer, source, binding));
            }
        }
    }
    changes
}

impl DeviceTask for KeyboardTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.keys_failed.is_some_and(|t| t.elapsed() >= KEYS_RETRY) {
            let _ = self.watch_keys();
        }
        // Reactive lighting checks for key presses every frame, but only sends colors when they change.
        if self.keys.is_some() {
            let refresh_due = self.last_frame.is_none_or(|t| t.elapsed() >= STATIC_REFRESH);
            if self.reacting() || refresh_due {
                self.render()?;
            }
            return Ok(FRAME_INTERVAL);
        }
        let interval = if self.lighting.is_animated() { FRAME_INTERVAL } else { STATIC_REFRESH };
        let due = self.last_frame.map_or(Duration::ZERO, |t| interval.saturating_sub(t.elapsed()));
        if due.is_zero() {
            self.render()?;
            return Ok(interval);
        }
        Ok(due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apex::keys::STEELSERIES_KEY;

    /// A key map backup saved by PeriHub from a real Apex 3 TKL.
    fn real_backup() -> Keymap {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/apex3tkl-keymap.json");
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn a_real_key_map_backup_can_be_read() {
        let backup = real_backup();
        assert_eq!(backup.normal.len(), 114);
        assert_eq!(backup.steelseries.len(), 114);
        assert!(restore_changes(&backup, &backup).is_empty());
    }

    #[test]
    fn restore_changes_only_the_keys_that_differ() {
        let backup = real_backup();
        let mut current = backup.clone();
        current.normal.insert(0x04, KeyBinding::Key { usage: 0x05 }); // A now types B
        current.steelseries.insert(0x1E, KeyBinding::Key { usage: 0x73 }); // SteelSeries + 1 now sends F24
        let changes = restore_changes(&current, &backup);
        assert_eq!(changes, [(0, 0x04, backup.normal[&0x04]), (1, 0x1E, backup.steelseries[&0x1E])]);
    }

    #[test]
    fn restore_leaves_keys_that_cannot_be_remapped() {
        let backup = real_backup();
        let mut current = backup.clone();
        current.normal.insert(STEELSERIES_KEY, KeyBinding::Key { usage: 0x05 });
        assert!(restore_changes(&current, &backup).is_empty());
    }
}

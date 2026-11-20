//! What PeriHub last sent to a setting of a device that cannot report it, and what it sent before.
//!
//! Some devices keep a setting in their memory but have no command that reads it back (the lighting effect of a
//! Razer keyboard). PeriHub cannot know what such a setting holds, but it knows what it sent itself, so it keeps
//! that in a file of its own settings folder, by the device's key ("vvvv:pppp", the key the interface uses for the
//! device: nothing tells two units of one model apart) and the setting's id, and can send the value from before again
//! ("put back"). What the device held before PeriHub's first change, set by the maker's own software, is not known
//! and is never guessed.
//!
//! The file only records what PeriHub sent. If the setting was changed elsewhere since, the record is out of date,
//! and the pages say so.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::BlindValue;
use crate::config;
use crate::error::Result;

const FILE: &str = "remembered-settings.json";
/// How many earlier values of a setting are kept to put back, one after the other.
const KEPT: usize = 8;

/// Two device threads (a keyboard on its cable and through its receiver) may write the file at once.
static LOCK: Mutex<()> = Mutex::new(());

/// What was sent to one setting: the last value, and the ones before it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    pub last: Option<BlindValue>,
    /// The values before `last`, the oldest first.
    pub before: Vec<BlindValue>,
}

impl History {
    /// The value "put back" sends: what was sent before the last one.
    pub fn previous(&self) -> Option<&BlindValue> {
        self.before.last()
    }

    /// `value` was sent. Sending what was sent last again changes nothing.
    pub fn sent(&mut self, value: BlindValue) {
        if self.last.as_ref() == Some(&value) {
            return;
        }
        if let Some(last) = self.last.replace(value) {
            self.before.push(last);
            if self.before.len() > KEPT {
                self.before.remove(0);
            }
        }
    }

    /// The previous value was sent again: it is the last one now, and the one before it is the next to put back.
    /// `None` when there is none.
    pub fn put_back(&mut self) -> Option<BlindValue> {
        let previous = self.before.pop()?;
        self.last = Some(previous.clone());
        Some(previous)
    }
}

type Store = BTreeMap<String, BTreeMap<String, History>>;

fn read() -> Store {
    let path = config::config_dir().join(FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            // Only a record of what PeriHub sent: starting again is better than refusing to change the device.
            log::warn!("{FILE} cannot be read, so what PeriHub sent before is forgotten: {e}");
            Store::new()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::new(),
        Err(e) => {
            log::warn!("{FILE} cannot be read: {e}");
            Store::new()
        }
    }
}

/// What PeriHub remembers of one setting of one device ("vvvv:pppp" and the setting's id). Nothing if nothing was
/// sent, or the file is lost.
pub fn load(device: &str, setting: &str) -> History {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read().get(device).and_then(|settings| settings.get(setting)).cloned().unwrap_or_default()
}

/// Saves what PeriHub remembers of one setting of one device.
pub fn store(device: &str, setting: &str, history: &History) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = read();
    all.entry(device.to_owned()).or_default().insert(setting.to_owned(), history.clone());
    let dir = config::config_dir();
    std::fs::create_dir_all(&dir)?;
    config::write_atomic(&dir.join(FILE), &serde_json::to_vec_pretty(&all)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use crate::peripheral::EffectValue;

    /// The tests of a run may share one folder (with one test thread): each starts from no file.
    fn clean() {
        let _ = std::fs::remove_file(config::config_dir().join(FILE));
    }

    fn effect(name: &str, color: Rgb) -> BlindValue {
        BlindValue::Effect(EffectValue { name: name.into(), colors: vec![color], speed: None })
    }

    #[test]
    fn nothing_is_known_before_the_first_change() {
        let h = History::default();
        assert_eq!((h.last.as_ref(), h.previous()), (None, None));
        assert_eq!(History::default().put_back(), None);
    }

    #[test]
    fn the_first_change_has_nothing_to_put_back_and_each_later_one_has() {
        let (a, b, c) = (effect("Static", Rgb::new(1, 2, 3)), effect("Breathing", Rgb::WHITE), BlindValue::Number(3));
        let mut h = History::default();
        h.sent(a.clone());
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&a), None), "what the keyboard had before is unknown");
        h.sent(b.clone());
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&b), Some(&a)));
        h.sent(c.clone());
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&c), Some(&b)));
        // Putting back goes one step at a time, and ends at the first thing PeriHub sent.
        assert_eq!(h.put_back(), Some(b.clone()));
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&b), Some(&a)));
        assert_eq!(h.put_back(), Some(a.clone()));
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&a), None));
        assert_eq!(h.put_back(), None);
        assert_eq!(h.last.as_ref(), Some(&a), "a put back with nothing before leaves the last one alone");
    }

    #[test]
    fn sending_the_same_value_again_does_not_fill_the_history() {
        let a = effect("Static", Rgb::new(1, 2, 3));
        let mut h = History::default();
        h.sent(a.clone());
        h.sent(a.clone());
        assert_eq!((h.last.as_ref(), h.previous()), (Some(&a), None));
        // A different colour of the same effect is a different value.
        h.sent(effect("Static", Rgb::new(9, 9, 9)));
        assert_eq!(h.previous(), Some(&a));
    }

    #[test]
    fn only_the_latest_values_are_kept() {
        let mut h = History::default();
        for n in 0..(KEPT as i32 + 5) {
            h.sent(BlindValue::Number(n));
        }
        assert_eq!(h.before.len(), KEPT);
        assert_eq!(h.last, Some(BlindValue::Number(KEPT as i32 + 4)));
        assert_eq!(h.previous(), Some(&BlindValue::Number(KEPT as i32 + 3)));
        assert_eq!(h.before[0], BlindValue::Number(4), "the oldest were dropped");
    }

    #[test]
    fn what_was_sent_survives_a_restart_by_device_and_setting() {
        clean();
        let mut h = History::default();
        h.sent(effect("Static", Rgb::new(1, 2, 3)));
        h.sent(effect("Breathing", Rgb::WHITE));
        store("test:0001", "effect", &h).unwrap();
        assert_eq!(load("test:0001", "effect"), h);
        // Another device, another setting of the same device, and the same model through its receiver are separate.
        assert_eq!(load("test:0002", "effect"), History::default());
        assert_eq!(load("test:0001", "macro"), History::default());
        let mut other = History::default();
        other.sent(BlindValue::Number(1));
        store("test:0002", "effect", &other).unwrap();
        assert_eq!(load("test:0001", "effect"), h, "the first device is untouched");
        assert_eq!(load("test:0002", "effect"), other);
        // The file is JSON a person can read: the device, the setting, the last value and the ones before.
        let text = std::fs::read_to_string(config::config_dir().join(FILE)).unwrap();
        assert!(text.contains("\"test:0001\"") && text.contains("\"last\"") && text.contains("\"before\""), "{text}");
    }

    #[test]
    fn a_lost_or_damaged_file_means_nothing_is_remembered_and_the_next_save_replaces_it() {
        let dir = config::config_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), b"{ not json").unwrap();
        assert_eq!(load("test:0001", "effect"), History::default());
        let mut h = History::default();
        h.sent(BlindValue::Number(7));
        store("test:0001", "effect", &h).unwrap();
        assert_eq!(load("test:0001", "effect"), h);
    }
}

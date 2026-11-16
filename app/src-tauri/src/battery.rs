//! Notifications and tray entries for the battery of wireless devices. The values come from what
//! the drivers read anyway (see perihub_core::battery): this adds no reads to any device.

use std::sync::mpsc;

use perihub_core::battery::{self, Alert, Entry, Tracker};
use tauri::AppHandle;
use tauri::Manager as _;
use tauri_plugin_notification::NotificationExt;

/// Starts listening. Call it after the app state exists.
pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<Vec<Entry>>();
    // The listener runs on a device's thread and only hands the values over: showing a
    // notification can take a moment.
    battery::watch(move |entries| {
        let _ = tx.send(entries.to_vec());
    });
    let spawned = std::thread::Builder::new().name("perihub-battery".into()).spawn(move || {
        let mut tracker = Tracker::default();
        let mut shown = Vec::<String>::new();
        while let Ok(mut entries) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                entries = newer;
            }
            let lines: Vec<String> = entries.iter().map(Entry::line).collect();
            if lines != shown {
                crate::window::set_batteries(&app, lines.clone());
                shown = lines;
            }
            let alerts = app.state::<crate::AppState>().config.lock().unwrap().battery;
            for entry in &entries {
                if let Some(alert) = tracker.update(entry, &alerts) {
                    let (title, body) = message(entry, alert);
                    if let Err(error) = app.notification().builder().title(title).body(body).show() {
                        log::warn!("cannot show a notification: {error}");
                    }
                }
            }
        }
    });
    if let Err(error) = spawned {
        log::warn!("cannot start the battery thread: {error}");
    }
}

fn message(entry: &Entry, alert: Alert) -> (String, String) {
    match alert {
        Alert::Low => (format!("{} battery is low", entry.name), format!("{}% left. Charge it soon.", entry.percent)),
        Alert::Full => (format!("{} is fully charged", entry.name), "You can unplug it.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(percent: u8) -> Entry {
        Entry { id: "mouse:1".into(), name: "G502".into(), percent, charging: false, full: false }
    }

    #[test]
    fn the_messages_are_short_and_plain() {
        assert_eq!(
            message(&entry(14), Alert::Low),
            ("G502 battery is low".to_string(), "14% left. Charge it soon.".to_string())
        );
        assert_eq!(message(&entry(100), Alert::Full).0, "G502 is fully charged");
    }
}

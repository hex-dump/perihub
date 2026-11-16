//! The battery of the wireless devices, as the drivers read it, and when to warn about it.
//!
//! Nothing here talks to a device. The drivers report the value they have just read (the
//! peripheral threads every few seconds, the Logitech pages when they are open), so the warnings
//! add no traffic to the radio. The app listens to [`watch`] and shows the notifications and the
//! tray entries.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// What the user chose in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BatteryAlerts {
    /// Warn when a battery gets low.
    pub enabled: bool,
    /// Percent: the warning comes when the battery goes below it.
    pub low_percent: u8,
    /// Also tell when a battery is fully charged.
    pub full: bool,
}

impl Default for BatteryAlerts {
    fn default() -> Self {
        Self { enabled: true, low_percent: 15, full: false }
    }
}

/// The battery must come this far above the limit before the next warning can come.
const REARM_MARGIN: u8 = 5;
/// Below this a charged battery counts as used again, and "fully charged" can come again.
const FULL_REARM: u8 = 95;

/// One device's battery at the last reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Tells the device apart, for example "mouse:c08b".
    pub id: String,
    pub name: String,
    pub percent: u8,
    pub charging: bool,
    /// The device says it is full, or it is at 100% on the charger.
    pub full: bool,
}

impl Entry {
    /// "Mouse name: 80%", or "Mouse name: 80% (charging)".
    pub fn line(&self) -> String {
        let state = if self.full {
            " (full)"
        } else if self.charging {
            " (charging)"
        } else {
            ""
        };
        format!("{}: {}%{state}", self.name, self.percent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    Low,
    Full,
}

#[derive(Debug, Clone, Copy, Default)]
struct Armed {
    low_sent: bool,
    full_sent: bool,
}

/// Decides when a reading is worth a notification: once when a battery goes below the limit,
/// again only after it was charged (or came back above the limit plus a margin). Unplugging and
/// plugging a device in again, or a device that sleeps and wakes, does not repeat a warning.
#[derive(Debug, Default)]
pub struct Tracker {
    seen: HashMap<String, Armed>,
}

impl Tracker {
    pub fn update(&mut self, entry: &Entry, alerts: &BatteryAlerts) -> Option<Alert> {
        let first = !self.seen.contains_key(&entry.id);
        let state = self.seen.entry(entry.id.clone()).or_default();
        let limit = alerts.low_percent.min(100);
        let mut alert = None;

        if entry.charging || entry.percent >= limit.saturating_add(REARM_MARGIN).min(100) {
            state.low_sent = false;
        } else if entry.percent < limit && !state.low_sent {
            state.low_sent = true;
            alert = Some(Alert::Low);
        }

        if !entry.full {
            if !entry.charging || entry.percent < FULL_REARM {
                state.full_sent = false;
            }
        } else if !state.full_sent {
            state.full_sent = true;
            // A device that is already full when PeriHub starts is not news.
            if !first {
                alert = alert.or(Some(Alert::Full));
            }
        }

        match alert {
            Some(Alert::Low) if !alerts.enabled => None,
            Some(Alert::Full) if !(alerts.enabled && alerts.full) => None,
            other => other,
        }
    }
}

/// How long after a device is ready its battery is read the first time.
const FIRST_READ: Duration = Duration::from_secs(20);
/// How often a battery is read when nothing else read it in the meantime.
const READ_EVERY: Duration = Duration::from_secs(5 * 60);

/// When a device's own thread reads its battery without the interface asking: once shortly after
/// it is ready, then every five minutes. A read the interface asked for counts too, so a window
/// that polls leaves nothing to do here. A read is a single read-only call, and one that gets no
/// answer (the device sleeps) is not repeated before the next time.
#[derive(Debug, Clone, Copy)]
pub struct Schedule {
    next: Instant,
}

impl Schedule {
    pub fn new(now: Instant) -> Self {
        Self { next: now + FIRST_READ }
    }

    /// Time to read? Starts the wait for the next one.
    pub fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + READ_EVERY;
        true
    }

    /// The battery was read (by the interface, or after an answer): the next read is a full
    /// interval away.
    pub fn read(&mut self, now: Instant) {
        self.next = now + READ_EVERY;
    }

    /// How long until the next read.
    pub fn wait(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }
}

type Listener = Box<dyn Fn(&[Entry]) + Send>;

struct Hub {
    entries: Vec<Entry>,
    listener: Option<Listener>,
}

static HUB: Mutex<Hub> = Mutex::new(Hub { entries: Vec::new(), listener: None });

/// `listener` gets every battery PeriHub knows, each time one of them changes. It runs on a
/// device's thread: it must not wait.
pub fn watch(listener: impl Fn(&[Entry]) + Send + 'static) {
    HUB.lock().unwrap().listener = Some(Box::new(listener));
}

/// The batteries as of now, in the order the devices connected.
pub fn entries() -> Vec<Entry> {
    HUB.lock().unwrap().entries.clone()
}

fn changed(hub: &Hub) {
    if let Some(listener) = &hub.listener {
        listener(&hub.entries);
    }
}

/// A driver read the battery of a device (`None`: it has none, or gave no value).
pub fn report(id: &str, name: &str, reading: Option<(u8, bool, bool)>) {
    let mut hub = HUB.lock().unwrap();
    let Some((percent, charging, full)) = reading else {
        drop(hub);
        forget(id);
        return;
    };
    let entry = Entry { id: id.into(), name: name.into(), percent: percent.min(100), charging, full };
    match hub.entries.iter_mut().find(|e| e.id == id) {
        Some(known) if *known == entry => return,
        Some(known) => *known = entry,
        None => hub.entries.push(entry),
    }
    changed(&hub);
}

/// The device went away: its battery is no longer shown.
pub fn forget(id: &str) {
    let mut hub = HUB.lock().unwrap();
    let before = hub.entries.len();
    hub.entries.retain(|e| e.id != id);
    if hub.entries.len() != before {
        changed(&hub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(percent: u8, charging: bool) -> Entry {
        Entry { id: "mouse:1".into(), name: "Mouse".into(), percent, charging, full: charging && percent >= 100 }
    }

    #[test]
    fn warns_once_when_the_battery_goes_below_the_limit() {
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts::default());
        assert_eq!(tracker.update(&entry(40, false), &alerts), None);
        assert_eq!(tracker.update(&entry(15, false), &alerts), None, "15% is not below 15%");
        assert_eq!(tracker.update(&entry(14, false), &alerts), Some(Alert::Low));
        assert_eq!(tracker.update(&entry(13, false), &alerts), None);
        assert_eq!(tracker.update(&entry(5, false), &alerts), None);
    }

    #[test]
    fn warns_again_only_after_the_battery_came_back_up() {
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts::default());
        assert_eq!(tracker.update(&entry(10, false), &alerts), Some(Alert::Low));
        // Wobbling around the limit does not warn again.
        assert_eq!(tracker.update(&entry(16, false), &alerts), None);
        assert_eq!(tracker.update(&entry(19, false), &alerts), None);
        assert_eq!(tracker.update(&entry(14, false), &alerts), None);
        // Above the limit plus the margin, it is ready again.
        assert_eq!(tracker.update(&entry(20, false), &alerts), None);
        assert_eq!(tracker.update(&entry(14, false), &alerts), Some(Alert::Low));
    }

    #[test]
    fn charging_resets_the_warning_and_is_never_warned_about() {
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts::default());
        assert_eq!(tracker.update(&entry(5, true), &alerts), None);
        assert_eq!(tracker.update(&entry(5, false), &alerts), Some(Alert::Low));
        assert_eq!(tracker.update(&entry(6, true), &alerts), None);
        assert_eq!(tracker.update(&entry(6, false), &alerts), Some(Alert::Low));
    }

    #[test]
    fn a_device_that_connects_again_does_not_repeat_the_warning() {
        // The tracker keeps its state for the device while it is away: only the readings come and go.
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts::default());
        assert_eq!(tracker.update(&entry(10, false), &alerts), Some(Alert::Low));
        assert_eq!(tracker.update(&entry(10, false), &alerts), None);
    }

    #[test]
    fn the_limit_and_the_switch_come_from_the_settings() {
        let mut tracker = Tracker::default();
        let off = BatteryAlerts { enabled: false, ..Default::default() };
        assert_eq!(tracker.update(&entry(5, false), &off), None);
        // Turned on while still low: no late warning for what was already seen.
        assert_eq!(tracker.update(&entry(5, false), &BatteryAlerts::default()), None);
        let high = BatteryAlerts { low_percent: 30, ..Default::default() };
        assert_eq!(tracker.update(&entry(29, false), &high), None, "already warned at 5%");
        assert_eq!(tracker.update(&entry(60, false), &high), None);
        assert_eq!(tracker.update(&entry(29, false), &high), Some(Alert::Low));
    }

    #[test]
    fn a_limit_of_100_still_has_a_margin_that_can_be_reached() {
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts { low_percent: 100, ..Default::default() });
        assert_eq!(tracker.update(&entry(99, false), &alerts), Some(Alert::Low));
        assert_eq!(tracker.update(&entry(100, false), &alerts), None);
        assert_eq!(tracker.update(&entry(99, false), &alerts), Some(Alert::Low));
    }

    #[test]
    fn fully_charged_is_optional_and_comes_once() {
        let (mut tracker, mut alerts) = (Tracker::default(), BatteryAlerts::default());
        alerts.full = true;
        assert_eq!(tracker.update(&entry(90, true), &alerts), None);
        assert_eq!(tracker.update(&entry(100, true), &alerts), Some(Alert::Full));
        assert_eq!(tracker.update(&entry(100, true), &alerts), None);
        // Used, then charged again.
        assert_eq!(tracker.update(&entry(80, false), &alerts), None);
        assert_eq!(tracker.update(&entry(100, true), &alerts), Some(Alert::Full));
        // Off by default.
        let mut other = Tracker::default();
        assert_eq!(other.update(&entry(90, true), &BatteryAlerts::default()), None);
        assert_eq!(other.update(&entry(100, true), &BatteryAlerts::default()), None);
    }

    #[test]
    fn a_device_already_full_at_the_start_is_not_announced() {
        let (mut tracker, alerts) = (Tracker::default(), BatteryAlerts { full: true, ..Default::default() });
        assert_eq!(tracker.update(&entry(100, true), &alerts), None);
    }

    #[test]
    fn the_first_read_comes_shortly_after_the_device_is_ready_then_every_five_minutes() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start);
        assert!(!schedule.due(start));
        assert!(!schedule.due(start + Duration::from_secs(19)));
        assert!(schedule.due(start + Duration::from_secs(20)));
        assert!(!schedule.due(start + Duration::from_secs(20 + 299)));
        assert!(schedule.due(start + Duration::from_secs(20 + 300)));
    }

    #[test]
    fn a_read_the_interface_asked_for_postpones_the_next_one() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start);
        let at = |s: u64| start + Duration::from_secs(s);
        // The window polls every few seconds: nothing is ever due.
        for s in (5..=900).step_by(5) {
            schedule.read(at(s));
            assert!(!schedule.due(at(s)), "{s}");
        }
        assert!(!schedule.due(at(900 + 299)));
        assert!(schedule.due(at(900 + 300)));
    }

    #[test]
    fn a_read_without_an_answer_waits_for_the_next_interval() {
        let start = Instant::now();
        let mut schedule = Schedule::new(start);
        let at = |s: u64| start + Duration::from_secs(s);
        assert!(schedule.due(at(20)));
        // The caller got an error and does nothing more: no retry before the next interval.
        assert!(!schedule.due(at(21)));
        assert!(!schedule.due(at(100)));
        assert_eq!(schedule.wait(at(100)), Duration::from_secs(220));
        assert!(schedule.due(at(320)));
    }

    #[test]
    fn the_lines_for_the_tray() {
        assert_eq!(entry(80, false).line(), "Mouse: 80%");
        assert_eq!(entry(80, true).line(), "Mouse: 80% (charging)");
        assert_eq!(entry(100, true).line(), "Mouse: 100% (full)");
    }

    #[test]
    fn the_hub_keeps_one_entry_per_device_and_tells_about_changes() {
        use std::sync::Arc;
        let seen = Arc::new(Mutex::new(Vec::<usize>::new()));
        let log = seen.clone();
        watch(move |entries| log.lock().unwrap().push(entries.len()));
        report("hub-test:1", "A", Some((50, false, false)));
        report("hub-test:1", "A", Some((50, false, false)));
        report("hub-test:1", "A", Some((49, false, false)));
        assert!(entries().iter().any(|e| e.id == "hub-test:1" && e.percent == 49));
        forget("hub-test:1");
        forget("hub-test:1");
        let count = seen.lock().unwrap().len();
        assert!(count >= 3, "add, change and forget each told once: {count}");
        assert!(!entries().iter().any(|e| e.id == "hub-test:1"));
    }
}

//! Whether the desktop can show a tray icon. PeriHub keeps running in the tray after the window is
//! closed; on a Linux desktop with no tray (GNOME without the AppIndicator extension, for one) the icon
//! would not show and a hidden PeriHub could not be brought back. Then closing the window quits it, and
//! it never starts hidden.
//!
//! On Linux the tray is the StatusNotifierItem protocol: a `org.kde.StatusNotifierWatcher` on the session
//! bus, with a host registered (a panel or an extension that draws the icons). The answer is found once,
//! at start-up, waiting at most [`WAIT`]; when the bus does not answer in time the tray is assumed to work,
//! which is what PeriHub did before. Windows and macOS always have one.

#[cfg(target_os = "linux")]
use std::sync::OnceLock;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// How long the start waits for the session bus: a slow one must not delay PeriHub.
#[cfg(target_os = "linux")]
const WAIT: Duration = Duration::from_millis(500);

#[cfg(target_os = "linux")]
static AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Looks for the tray. Call it once, before the tray is built; the answer is kept.
pub fn detect() {
    #[cfg(target_os = "linux")]
    {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(probe());
        });
        let found = match receive.recv_timeout(WAIT) {
            Ok(Ok((watcher, host))) => decide(watcher, host),
            Ok(Err(error)) => {
                log::info!("tray: cannot ask the session bus ({error}), assuming there is one");
                true
            }
            Err(_) => {
                log::info!("tray: the session bus did not answer in time, assuming there is one");
                true
            }
        };
        if !found {
            log::info!(
                "tray: no StatusNotifierWatcher with a host on this desktop; closing the window will quit PeriHub"
            );
        }
        let _ = AVAILABLE.set(found);
    }
}

/// Whether the desktop has a tray for PeriHub to live in.
pub fn available() -> bool {
    #[cfg(target_os = "linux")]
    {
        *AVAILABLE.get().unwrap_or(&true)
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Why there is no tray, for Settings; `None` when there is one.
pub fn unavailable_reason() -> Option<String> {
    (!available()).then(|| {
        "This desktop shows no system tray (no StatusNotifier host: on GNOME, install the AppIndicator extension), \
         so PeriHub cannot keep running hidden: closing the window quits it, and it never starts hidden."
            .to_string()
    })
}

/// The answer from what the bus said: `watcher` is whether the StatusNotifierWatcher exists, `host` whether
/// it says a host is registered (`None` when it would not say: then the watcher alone counts).
#[cfg(any(target_os = "linux", test))]
fn decide(watcher: bool, host: Option<bool>) -> bool {
    watcher && host.unwrap_or(true)
}

/// Asks the session bus: (is there a watcher, is a host registered).
#[cfg(target_os = "linux")]
fn probe() -> zbus::Result<(bool, Option<bool>)> {
    use zbus::blocking::{Connection, Proxy};
    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    let connection = Connection::session()?;
    let bus = Proxy::new(&connection, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus")?;
    let present: bool = bus.call("NameHasOwner", &(WATCHER,))?;
    if !present {
        return Ok((false, None));
    }
    let watcher = Proxy::new(&connection, WATCHER, "/StatusNotifierWatcher", WATCHER)?;
    let host = watcher.get_property::<bool>("IsStatusNotifierHostRegistered").ok();
    Ok((true, host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tray_needs_a_watcher_and_a_host() {
        assert!(decide(true, Some(true)));
        assert!(!decide(false, None));
        assert!(!decide(true, Some(false)));
    }

    #[test]
    fn a_watcher_that_does_not_say_whether_it_has_a_host_counts() {
        assert!(decide(true, None));
    }

    #[test]
    fn until_it_is_looked_for_there_is_a_tray() {
        assert!(available());
        assert!(unavailable_reason().is_none());
    }
}

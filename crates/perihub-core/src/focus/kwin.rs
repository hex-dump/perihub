//! The app in focus on KDE Plasma, on Wayland and on X11, from KWin.
//!
//! KWin does not tell other apps which window is active, but it runs scripts. PeriHub writes a small
//! one and loads it through KWin's scripting interface on D-Bus (`org.kde.KWin`, `/Scripting`). Each
//! time a window is activated the script calls PeriHub back over D-Bus with the window's class and
//! desktop file name, never its title. Nothing is polled. The script is unloaded when PeriHub quits.
//!
//! Source: KDE's KWin scripting API, <https://develop.kde.org/docs/plasma/kwin/api/>:
//! `workspace.windowActivated` and `workspace.activeWindow` on Plasma 6, `workspace.clientActivated`
//! and `workspace.activeClient` on Plasma 5, and `callDBus`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;
use zbus::message::Header;
use zbus::names::OwnedUniqueName;

use super::FocusSupport;

const KWIN: &str = "org.kde.KWin";
const SCRIPTING_PATH: &str = "/Scripting";
const SCRIPTING: &str = "org.kde.kwin.Scripting";
/// The name PeriHub owns for the script's calls. A Flatpak may own names under its app id.
const SERVICE: &str = "io.github.hex_dump.perihub.Focus";
const PATH: &str = "/io/github/hex_dump/perihub/Focus";
const INTERFACE: &str = "io.github.hex_dump.perihub.Focus";
/// The script's name in KWin.
const PLUGIN: &str = "perihub-focus";

/// D-Bus calls to KWin: all local, so a slow answer means something is wrong.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// The script reports the active window as soon as it runs.
const FIRST_REPORT: Duration = Duration::from_secs(3);
/// How long the App profiles page waits for the script's first report.
const START_WAIT: Duration = Duration::from_secs(5);

const NO_BUS: &str =
    "PeriHub could not connect to the session bus (D-Bus), which it needs to follow the app in focus on KDE Plasma.";
const NO_FILE: &str = "PeriHub could not save the small KWin script that tells it which app is in focus.";
const NO_KWIN: &str = "KWin does not answer PeriHub on D-Bus, so the app in focus cannot be followed. If PeriHub is a Flatpak, \
    allow it to talk to org.kde.KWin (in Flatseal, or with: flatpak override --user --talk-name=org.kde.KWin \
    io.github.hex_dump.perihub), then restart PeriHub.";
const NO_SCRIPT: &str = "KWin did not run PeriHub's script that tells it which app is in focus. Restart PeriHub; if it \
    keeps happening, please report it with the log from Settings.";
const NO_REPORT: &str = "KWin runs PeriHub's script, but its messages do not reach PeriHub, so the app in focus cannot be \
    followed. Restart PeriHub; if it keeps happening, please report it with the log from Settings.";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Starting,
    Running,
    Failed(&'static str),
}

static STATUS: (Mutex<Option<Status>>, Condvar) = (Mutex::new(None), Condvar::new());
/// The connection and the script file, to unload the script when PeriHub quits.
static LOADED: OnceLock<(Connection, PathBuf)> = OnceLock::new();

fn set_status(status: Status) {
    let (lock, changed) = &STATUS;
    *lock.lock().unwrap() = Some(status);
    changed.notify_all();
}

/// Whether the desktop is KDE Plasma (`XDG_CURRENT_DESKTOP` is a colon-separated list).
fn is_plasma(current_desktop: Option<&str>) -> bool {
    current_desktop.is_some_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("kde")))
}

pub fn on_plasma() -> bool {
    is_plasma(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref())
}

/// `None` when the desktop is not KDE Plasma. While the script is starting, waits for its first report.
pub fn support() -> Option<FocusSupport> {
    if !on_plasma() {
        return None;
    }
    let (lock, changed) = &STATUS;
    let status = lock.lock().unwrap();
    let (status, _) = changed.wait_timeout_while(status, START_WAIT, |s| *s == Some(Status::Starting)).unwrap();
    Some(match *status {
        Some(Status::Failed(reason)) => FocusSupport::Unsupported { reason },
        _ => FocusSupport::Supported { desktop: "KDE Plasma" },
    })
}

/// Starts following the active window on KDE Plasma. Returns `false` on other desktops.
pub fn watch(on_change: impl FnMut(Option<String>) + Send + 'static) -> bool {
    if !on_plasma() {
        return false;
    }
    set_status(Status::Starting);
    let started = thread::Builder::new().name("perihub-focus".into()).spawn(move || {
        if let Err(reason) = run(on_change) {
            set_status(Status::Failed(reason));
        }
    });
    if started.is_err() {
        set_status(Status::Failed(NO_SCRIPT));
    }
    started.is_ok()
}

/// Unloads the script, when PeriHub quits.
pub fn stop() {
    if let Some((connection, script)) = LOADED.get() {
        let _ = call(connection, "unloadScript", &(PLUGIN,));
        let _ = fs::remove_file(script);
    }
}

/// The app's identifier: the window class (the app id of a Wayland window), else its desktop file name.
fn app_id(resource_class: &str, desktop_file_name: &str) -> Option<String> {
    [resource_class, desktop_file_name].into_iter().map(str::trim).find(|s| !s.is_empty()).map(str::to_lowercase)
}

/// Only KWin may say which window is active: a call from another program is ignored.
fn from_kwin(sender: Option<&str>, kwin: Option<&str>) -> bool {
    sender.is_some_and(|sender| kwin == Some(sender))
}

fn script() -> String {
    format!(
        r#"// Written by PeriHub, loaded while it runs: tells it the class of the active window, never its title.
function report(window) {{
    callDBus("{SERVICE}", "{PATH}", "{INTERFACE}", "WindowActivated",
        window ? String(window.resourceClass || "") : "",
        window ? String(window.desktopFileName || "") : "");
}}
if (workspace.windowActivated) {{
    workspace.windowActivated.connect(report);
    report(workspace.activeWindow);
}} else {{
    workspace.clientActivated.connect(report);
    report(workspace.activeClient);
}}
"#
    )
}

/// The script goes where KWin can read it, also from a Flatpak: `$XDG_RUNTIME_DIR/app/<app id>` is
/// the same folder inside and outside the sandbox.
fn write_script() -> Result<PathBuf, &'static str> {
    let runtime = dirs::runtime_dir().ok_or(NO_FILE)?;
    let dir = match std::env::var("FLATPAK_ID") {
        Ok(id) if !id.is_empty() => runtime.join("app").join(id),
        _ => runtime.join("perihub"),
    };
    let path = dir.join("kwin-focus.js");
    fs::create_dir_all(&dir).and_then(|()| fs::write(&path, script())).map_err(|error| {
        log::warn!("app profiles: KWin script not written to {}: {error}", dir.display());
        NO_FILE
    })?;
    Ok(path)
}

enum Event {
    Activated(Option<String>),
    /// KWin started again (its new name on the bus), or went away.
    KWin(Option<OwnedUniqueName>),
}

/// Receives the script's calls.
struct Callback {
    events: mpsc::Sender<Event>,
    kwin: Arc<Mutex<Option<OwnedUniqueName>>>,
}

#[zbus::interface(name = "io.github.hex_dump.perihub.Focus")]
impl Callback {
    fn window_activated(&self, resource_class: String, desktop_file_name: String, #[zbus(header)] header: Header<'_>) {
        let kwin = self.kwin.lock().unwrap();
        if from_kwin(header.sender().map(|s| s.as_str()), kwin.as_ref().map(|k| k.as_str())) {
            let _ = self.events.send(Event::Activated(app_id(&resource_class, &desktop_file_name)));
        }
    }
}

fn call<B>(connection: &Connection, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    connection.call_method(Some(KWIN), SCRIPTING_PATH, Some(SCRIPTING), method, body)
}

fn load(connection: &Connection, script: &Path) -> Result<(), &'static str> {
    let path = script.to_str().ok_or(NO_FILE)?;
    // A script left by a PeriHub that did not quit normally has the same name: it is replaced.
    let _ = call(connection, "unloadScript", &(PLUGIN,));
    let id: i32 = call(connection, "loadScript", &(path, PLUGIN))
        .and_then(|reply| reply.body().deserialize())
        .map_err(|error| {
            log::warn!("app profiles: KWin did not load the script: {error}");
            NO_SCRIPT
        })?;
    if id < 0 {
        log::warn!("app profiles: KWin did not load the script ({id})");
        return Err(NO_SCRIPT);
    }
    call(connection, "start", &()).map_err(|error| {
        log::warn!("app profiles: KWin did not start the script: {error}");
        NO_SCRIPT
    })?;
    Ok(())
}

fn run(mut on_change: impl FnMut(Option<String>)) -> Result<(), &'static str> {
    let (events, received) = mpsc::channel();
    let kwin = Arc::new(Mutex::new(None));
    let callback = Callback { events: events.clone(), kwin: kwin.clone() };
    let bus_error = |error: zbus::Error| {
        log::warn!("app profiles: D-Bus: {error}");
        NO_BUS
    };
    let connection = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(SERVICE))
        .and_then(|b| b.serve_at(PATH, callback))
        .and_then(|b| b.method_timeout(CALL_TIMEOUT).build())
        .map_err(bus_error)?;
    let bus = DBusProxy::new(&connection).map_err(bus_error)?;
    // KWin replaced or restarted after a crash has lost the script: it is loaded again.
    let owners = bus.receive_name_owner_changed_with_args(&[(0, KWIN)]).map_err(bus_error)?;
    let forward = events.clone();
    thread::Builder::new()
        .name("perihub-kwin".into())
        .spawn(move || {
            for signal in owners {
                let Ok(args) = signal.args() else { continue };
                let owner = args.new_owner().as_ref().map(|name| OwnedUniqueName::from(name.to_owned()));
                if forward.send(Event::KWin(owner)).is_err() {
                    break;
                }
            }
        })
        .map_err(|_| NO_BUS)?;
    drop(events);

    let script = write_script()?;
    let owner = bus.get_name_owner(KWIN.try_into().map_err(|_| NO_KWIN)?).map_err(|error| {
        log::warn!("app profiles: KWin not found on D-Bus: {error}");
        NO_KWIN
    })?;
    *kwin.lock().unwrap() = Some(owner);
    load(&connection, &script)?;
    let _ = LOADED.set((connection.clone(), script.clone()));

    let mut reported = false;
    loop {
        let event = if reported {
            received.recv().map_err(|_| NO_REPORT)?
        } else {
            match received.recv_timeout(FIRST_REPORT) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    log::warn!("app profiles: no report from the KWin script");
                    stop();
                    return Err(NO_REPORT);
                }
            }
        };
        match event {
            Event::Activated(app) => {
                if !reported {
                    reported = true;
                    set_status(Status::Running);
                }
                on_change(app);
            }
            Event::KWin(owner) => {
                let back = owner.is_some();
                *kwin.lock().unwrap() = owner;
                on_change(None);
                if back {
                    reload(&connection, &script);
                }
            }
        }
    }
}

/// KWin is back: its scripting interface may come a moment after its name.
fn reload(connection: &Connection, script: &Path) {
    for attempt in 0..3 {
        thread::sleep(Duration::from_secs(1 + attempt));
        match load(connection, script) {
            Ok(()) => return set_status(Status::Running),
            Err(reason) => set_status(Status::Failed(reason)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plasma_from_the_current_desktop() {
        assert!(is_plasma(Some("KDE")));
        assert!(is_plasma(Some("ubuntu:KDE")));
        assert!(!is_plasma(Some("GNOME")));
        assert!(!is_plasma(Some("Hyprland")));
        assert!(!is_plasma(Some("")));
        assert!(!is_plasma(None));
    }

    #[test]
    fn identifies_the_app_by_class_then_desktop_file() {
        assert_eq!(app_id("steam_app_730", ""), Some("steam_app_730".into()));
        assert_eq!(app_id("org.kde.Dolphin", "org.kde.dolphin"), Some("org.kde.dolphin".into()));
        assert_eq!(app_id(" ", "Firefox"), Some("firefox".into()));
        assert_eq!(app_id("", ""), None);
    }

    #[test]
    fn listens_only_to_kwin() {
        assert!(from_kwin(Some(":1.42"), Some(":1.42")));
        assert!(!from_kwin(Some(":1.99"), Some(":1.42")));
        assert!(!from_kwin(Some(":1.42"), None));
        assert!(!from_kwin(None, Some(":1.42")));
    }

    #[test]
    fn the_script_calls_the_interface_perihub_serves() {
        use zbus::object_server::Interface;
        assert_eq!(Callback::name().as_str(), INTERFACE);
        let script = script();
        for part in [SERVICE, PATH, INTERFACE, "\"WindowActivated\""] {
            assert!(script.contains(part), "{part}");
        }
        assert!(!script.contains("caption"), "the script must never read window titles");
    }
}

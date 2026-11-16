//! GNOME (Wayland and X11). GNOME Shell does not tell other apps which window is focused (its
//! `Eval` method is disabled), so PeriHub ships a tiny Shell extension of its own,
//! `packaging/gnome-extension`, bundled here. It owns [`BUS_NAME`] on the session bus and emits
//! `FocusChanged(app_id, wm_class, pid)` when the focus moves: no polling.
//!
//! The App profiles page installs it on request: the files go to
//! `~/.local/share/gnome-shell/extensions`, then `org.gnome.Shell.Extensions.EnableExtension`.
//! GNOME loads a new extension only at sign-in, so the first time PeriHub leaves a mark and enables
//! it after the next sign-in, when the user opens PeriHub's App profiles or PeriHub starts with
//! them on.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy, fdo::DBusProxy, proxy::Builder};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use super::FocusSupport;

pub const UUID: &str = "perihub-focus@hex-dump.github.io";
const BUS_NAME: &str = "io.github.hex_dump.perihub.GnomeFocus";
const PATH: &str = "/io/github/hex_dump/perihub/GnomeFocus";
const INTERFACE: &str = "io.github.hex_dump.perihub.GnomeFocus";

/// The extension's files, bundled so that packages need nothing extra.
const FILES: [(&str, &str); 2] = [
    (
        "metadata.json",
        include_str!("../../../../packaging/gnome-extension/perihub-focus@hex-dump.github.io/metadata.json"),
    ),
    (
        "extension.js",
        include_str!("../../../../packaging/gnome-extension/perihub-focus@hex-dump.github.io/extension.js"),
    ),
];

/// Waiting again for the session bus after it went away.
const RETRY: Duration = Duration::from_secs(10);
const DESKTOP: &str = "GNOME";
const ACTION: &str = "Enable GNOME integration";

/// Running on GNOME (`XDG_CURRENT_DESKTOP` is e.g. `GNOME` or `ubuntu:GNOME`).
pub fn detected() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| is_gnome(&d))
}

fn is_gnome(desktops: &str) -> bool {
    desktops.split(':').any(|d| d.trim().eq_ignore_ascii_case("gnome"))
}

/// The identifier app profiles match: the window class, as on Hyprland (Wayland apps: their app
/// id, e.g. `org.gnome.nautilus`; X11 apps and games: their class, e.g. `steam_app_730`), else
/// the app's id.
fn app_id(app_id: &str, wm_class: &str) -> Option<String> {
    [wm_class, app_id].into_iter().map(str::trim).find(|s| !s.is_empty()).map(str::to_lowercase)
}

// ---- State of the extension ----

/// The installed copy of the extension, compared with the bundled one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Files {
    Missing,
    Current,
    Different,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    /// The extension is running: focus changes arrive.
    Running,
    NotInstalled,
    /// An older or changed copy is installed.
    Outdated,
    /// Installed, known to GNOME Shell, turned off.
    Off,
    /// Installed, but GNOME Shell has not seen it yet: it reads extensions at sign-in.
    NeedsSignIn,
    /// The user turned off all extensions in GNOME.
    ExtensionsOff,
    /// GNOME Shell could not start it (an error, or a GNOME version it does not list).
    Failed,
    /// No session bus, or no GNOME Shell on it.
    NoShell,
}

/// `state` in GNOME Shell's extension info (`ExtensionState`).
const ACTIVE: f64 = 1.0;
const ERROR: f64 = 3.0;
const OUT_OF_DATE: f64 = 4.0;
const ACTIVATING: f64 = 8.0;
const UNINSTALLED: f64 = 99.0;

/// `owned`: the extension's bus name has an owner. `state`: GNOME Shell's state for it, `None`
/// if the Shell does not know it. `user_extensions`: GNOME's switch for all extensions.
fn classify(owned: bool, files: Files, state: Option<f64>, user_extensions: bool) -> Status {
    let state = state.filter(|s| *s != UNINSTALLED);
    match (owned, files, state) {
        (true, _, _) => Status::Running,
        (_, Files::Missing, _) => Status::NotInstalled,
        (_, Files::Different, _) => Status::Outdated,
        _ if !user_extensions => Status::ExtensionsOff,
        (_, _, None) => Status::NeedsSignIn,
        (_, _, Some(s)) if s == ACTIVE || s == ACTIVATING => Status::Running,
        (_, _, Some(s)) if s == ERROR || s == OUT_OF_DATE => Status::Failed,
        _ => Status::Off,
    }
}

fn to_support(status: Status) -> FocusSupport {
    let setup = |reason| FocusSupport::Setup { desktop: DESKTOP, reason, action: ACTION };
    let unsupported = |reason| FocusSupport::Unsupported { reason };
    match status {
        Status::Running => FocusSupport::Supported { desktop: DESKTOP },
        Status::NotInstalled => setup(
            "On GNOME, PeriHub needs its small GNOME Shell extension to see which app is in focus. \
             It tells PeriHub only the app's window class, never window titles.",
        ),
        Status::Outdated => setup("PeriHub's GNOME Shell extension needs an update for this version of PeriHub."),
        Status::Off => setup("PeriHub's GNOME Shell extension is installed but turned off."),
        Status::NeedsSignIn => unsupported(
            "Log out and back in to finish turning on the GNOME integration: GNOME loads new \
             extensions only when you sign in.",
        ),
        Status::ExtensionsOff => {
            unsupported("Extensions are turned off in GNOME. Turn them on in the Extensions app, then come back here.")
        }
        Status::Failed => unsupported(
            "GNOME Shell could not start PeriHub's extension, perhaps because this GNOME version is \
             newer than the extension knows. An update of PeriHub should fix it.",
        ),
        Status::NoShell => unsupported("GNOME Shell could not be reached, so the app in focus cannot be detected."),
    }
}

// ---- Files ----

/// Where GNOME Shell looks for the user's extensions. In a Flatpak `XDG_DATA_HOME` points into the
/// sandbox, so the host's default is used.
fn extension_dir() -> Option<PathBuf> {
    let data = if Path::new("/.flatpak-info").exists() {
        dirs::home_dir().map(|h| h.join(".local/share"))
    } else {
        dirs::data_dir()
    };
    Some(data?.join("gnome-shell/extensions").join(UUID))
}

fn files_in(dir: &Path) -> Files {
    let read: Vec<_> =
        FILES.iter().map(|(name, content)| fs::read_to_string(dir.join(name)).map(|c| c == *content)).collect();
    if read.iter().all(|r| r.is_err()) {
        Files::Missing
    } else if read.iter().all(|r| matches!(r, Ok(true))) {
        Files::Current
    } else {
        Files::Different
    }
}

/// Writes the bundled files; returns `true` if something changed.
fn install(dir: &Path) -> std::io::Result<bool> {
    fs::create_dir_all(dir)?;
    let mut changed = false;
    for (name, content) in FILES {
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(content) {
            fs::write(path, content)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Left when the extension was installed but GNOME Shell could not enable it yet (before the next
/// sign-in). Only a click on the button leaves it: an extension the user turned off later stays off.
fn pending_mark() -> PathBuf {
    crate::config::config_dir().join("gnome-extension-pending")
}

// ---- D-Bus ----

fn shell_extensions(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Builder::new(conn)
        .destination("org.gnome.Shell.Extensions")?
        .path("/org/gnome/Shell/Extensions")?
        .interface("org.gnome.Shell.Extensions")?
        .cache_properties(CacheProperties::No)
        .build()
}

fn focus_proxy(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Builder::new(conn)
        .destination(BUS_NAME)?
        .path(PATH)?
        .interface(INTERFACE)?
        .cache_properties(CacheProperties::No)
        .build()
}

/// GNOME Shell's state for the extension, `None` if it does not know it.
fn shell_state(shell: &Proxy) -> zbus::Result<Option<f64>> {
    let info: HashMap<String, OwnedValue> = shell.call("GetExtensionInfo", &(UUID,))?;
    Ok(info.get("state").and_then(|v| f64::try_from(v).ok()))
}

fn owned(conn: &Connection) -> bool {
    let has_owner = |bus: DBusProxy| -> zbus::Result<bool> { Ok(bus.name_has_owner(BUS_NAME.try_into()?)?) };
    DBusProxy::new(conn).and_then(has_owner).unwrap_or(false)
}

/// Waits a moment for the extension to appear after enabling it.
fn wait_owned(conn: &Connection) -> bool {
    (0..20).any(|_| {
        thread::sleep(Duration::from_millis(100));
        owned(conn)
    })
}

/// Enables the extension if a click asked for it before GNOME Shell could load it.
fn finish_pending(conn: &Connection) {
    let mark = pending_mark();
    if !mark.exists() {
        return;
    }
    let Ok(shell) = shell_extensions(conn) else { return };
    let Ok(Some(state)) = shell_state(&shell) else { return };
    if state != UNINSTALLED {
        let enabled = shell.call::<_, _, bool>("EnableExtension", &(UUID,));
        log::info!("GNOME extension enabled after sign-in: {enabled:?}");
        let _ = fs::remove_file(mark);
    }
}

fn status(conn: &Connection) -> Status {
    finish_pending(conn);
    let Ok(shell) = shell_extensions(conn) else { return Status::NoShell };
    let Ok(state) = shell_state(&shell) else { return Status::NoShell };
    let user_extensions = shell.get_property::<bool>("UserExtensionsEnabled").unwrap_or(true);
    let files = extension_dir().map_or(Files::Missing, |d| files_in(&d));
    classify(owned(conn), files, state, user_extensions)
}

pub fn support() -> FocusSupport {
    to_support(Connection::session().map_or(Status::NoShell, |conn| status(&conn)))
}

fn unreachable(error: zbus::Error) -> String {
    log::warn!("GNOME Shell extensions: {error}");
    "GNOME Shell could not be reached.".into()
}

/// Installs and enables the extension; returns what to tell the user.
pub fn setup() -> Result<String, String> {
    let conn = Connection::session().map_err(unreachable)?;
    let shell = shell_extensions(&conn).map_err(unreachable)?;
    if !shell.get_property::<bool>("UserExtensionsEnabled").unwrap_or(true) {
        return Err("Extensions are turned off in GNOME. Turn them on in the Extensions app, then try again.".into());
    }
    let state = shell_state(&shell).map_err(unreachable)?;
    let dir = extension_dir().ok_or("Your home folder could not be found.")?;
    let changed = install(&dir).map_err(|e| format!("The extension could not be copied to {}: {e}", dir.display()))?;
    let known = state.is_some_and(|s| s != UNINSTALLED);
    if !known {
        let mark = pending_mark();
        let _ = fs::create_dir_all(mark.parent().unwrap_or(Path::new(".")));
        fs::write(&mark, UUID).map_err(|e| e.to_string())?;
        return Ok("Almost done: log out and back in. GNOME loads new extensions only when you sign in, \
                   and PeriHub turns this one on when you open it afterwards."
            .into());
    }
    let enabled: bool =
        shell.call("EnableExtension", &(UUID,)).map_err(|e| format!("GNOME did not enable the extension: {e}"))?;
    if changed {
        return Ok("The extension was updated: log out and back in so that GNOME loads the new version.".into());
    }
    if enabled && wait_owned(&conn) {
        Ok("GNOME integration is on.".into())
    } else {
        Err("GNOME did not start the extension. Check it in the Extensions app, or log out and back in.".into())
    }
}

// ---- Watching ----

pub fn run(mut on_change: impl FnMut(Option<String>)) {
    loop {
        match Connection::session() {
            Ok(conn) => {
                if let Err(error) = listen(&conn, &mut on_change) {
                    log::warn!("GNOME focus: {error}");
                }
            }
            Err(error) => log::warn!("GNOME focus: no session bus: {error}"),
        }
        thread::sleep(RETRY);
    }
}

/// Follows `FocusChanged` until the bus connection ends. The proxy follows the name's owner, so
/// the extension may start (or restart) later, and signals from anyone else are ignored.
fn listen(conn: &Connection, on_change: &mut impl FnMut(Option<String>)) -> zbus::Result<()> {
    finish_pending(conn);
    let focus = focus_proxy(conn)?;
    let changes = focus.receive_signal("FocusChanged")?;
    // Asked after subscribing, so no change is missed; fails quietly while the extension is off.
    if let Ok((app, class, _pid)) = focus.call::<_, _, (String, String, u32)>("GetFocus", &()) {
        on_change(app_id(&app, &class));
    }
    for message in changes {
        if let Ok((app, class, _pid)) = message.body().deserialize::<(String, String, u32)>() {
            on_change(app_id(&app, &class));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gnome_sessions() {
        assert!(is_gnome("GNOME"));
        assert!(is_gnome("ubuntu:GNOME"));
        assert!(is_gnome("pop:GNOME"));
        assert!(!is_gnome("KDE"));
        assert!(!is_gnome("GNOME-Flashback-not"));
        assert!(!is_gnome(""));
    }

    #[test]
    fn prefers_the_window_class() {
        assert_eq!(app_id("org.gnome.Nautilus", "org.gnome.Nautilus"), Some("org.gnome.nautilus".into()));
        assert_eq!(app_id("steam", "steam_app_730"), Some("steam_app_730".into()));
        assert_eq!(app_id("firefox", " "), Some("firefox".into()));
        assert_eq!(app_id("", ""), None);
    }

    #[test]
    fn classifies_the_extension() {
        use Files::*;
        assert_eq!(classify(true, Different, None, false), Status::Running);
        assert_eq!(classify(false, Missing, None, true), Status::NotInstalled);
        assert_eq!(classify(false, Different, Some(1.0), true), Status::Outdated);
        assert_eq!(classify(false, Current, Some(2.0), false), Status::ExtensionsOff);
        assert_eq!(classify(false, Current, None, true), Status::NeedsSignIn);
        assert_eq!(classify(false, Current, Some(99.0), true), Status::NeedsSignIn);
        assert_eq!(classify(false, Current, Some(2.0), true), Status::Off);
        assert_eq!(classify(false, Current, Some(6.0), true), Status::Off);
        assert_eq!(classify(false, Current, Some(4.0), true), Status::Failed);
        assert_eq!(classify(false, Current, Some(3.0), true), Status::Failed);
        assert_eq!(classify(false, Current, Some(1.0), true), Status::Running);
    }

    #[test]
    fn only_a_missing_or_changed_extension_offers_the_button() {
        for status in [Status::NotInstalled, Status::Outdated, Status::Off] {
            assert!(matches!(to_support(status), FocusSupport::Setup { .. }), "{status:?}");
        }
        for status in [Status::NeedsSignIn, Status::ExtensionsOff, Status::Failed, Status::NoShell] {
            assert!(matches!(to_support(status), FocusSupport::Unsupported { .. }), "{status:?}");
        }
    }

    #[test]
    fn installs_and_compares_the_bundled_files() {
        let dir = crate::config::config_dir().join("gnome-extension-test").join(UUID);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(files_in(&dir), Files::Missing);
        assert!(install(&dir).unwrap());
        assert_eq!(files_in(&dir), Files::Current);
        assert!(!install(&dir).unwrap());
        fs::write(dir.join("extension.js"), "// old").unwrap();
        assert_eq!(files_in(&dir), Files::Different);
        assert!(install(&dir).unwrap());
        assert_eq!(files_in(&dir), Files::Current);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn bundled_metadata_matches_the_code() {
        let metadata: serde_json::Value = serde_json::from_str(FILES[0].1).unwrap();
        assert_eq!(metadata["uuid"], UUID);
        let versions = metadata["shell-version"].as_array().unwrap();
        assert!(versions.iter().any(|v| v == "45"));
        let js = FILES[1].1;
        for name in [BUS_NAME, PATH, INTERFACE, "FocusChanged", "GetFocus"] {
            assert!(js.contains(name), "{name} missing from extension.js");
        }
    }
}

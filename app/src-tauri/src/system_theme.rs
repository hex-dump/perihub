//! The system's light or dark preference on Linux. WebKitGTK answers `prefers-color-scheme` from the GTK
//! theme *name* ("Breeze" is light even on a dark KDE Plasma), and Tauri's `setTheme(null)` turns the GTK
//! dark preference off, so the interface cannot ask the web view. The preference is read from the XDG
//! desktop portal instead (`org.freedesktop.appearance color-scheme`, which KDE, GNOME and the other
//! desktops serve, also to a Flatpak) and given to the interface in two ways: in `window.__PERIHUB_SYSTEM_THEME__`,
//! set before the page runs so the first paint is right, and in the "system-theme" event when it changes.
//! Without the portal, or when it has no preference, the GTK settings file is the fallback; without
//! either the interface decides with `matchMedia`.

use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Theme};
use zbus::blocking::proxy::Builder;
use zbus::blocking::{Connection, Proxy};
use zbus::proxy::CacheProperties;
use zbus::zvariant::Value;

const EVENT: &str = "system-theme";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";
/// How long the start waits for the portal: a missing or slow one must not delay the window.
const WAIT: Duration = Duration::from_millis(200);

/// What the portal said last: `None` until it has answered.
static LATEST: Mutex<Option<Option<Theme>>> = Mutex::new(None);

/// The theme the system prefers now, `None` when it has no preference we can read.
pub fn current() -> Option<Theme> {
    let known = *LATEST.lock().unwrap();
    known.unwrap_or_else(gtk_theme)
}

/// The script that gives the web view `current()` before the page runs, `None` when there is nothing to say.
pub fn init_script() -> Option<String> {
    let name = current().map(name)?;
    Some(format!("window.__PERIHUB_SYSTEM_THEME__ = \"{name}\";"))
}

/// Reads the portal and follows its changes on a thread of its own, waiting at most [`WAIT`] for the first answer.
pub fn watch(app: AppHandle) {
    let (ready, first) = mpsc::channel();
    std::thread::spawn(move || {
        if let Err(error) = follow(&app, &ready) {
            log::info!("system theme: no desktop portal ({error}), using the GTK settings");
        }
        // Wakes the start when the portal could not be reached at all.
        let _ = ready.send(());
    });
    let _ = first.recv_timeout(WAIT);
}

fn follow(app: &AppHandle, ready: &Sender<()>) -> zbus::Result<()> {
    let connection = Connection::session()?;
    let portal: Proxy = Builder::new(&connection)
        .destination("org.freedesktop.portal.Desktop")?
        .path("/org/freedesktop/portal/desktop")?
        .interface("org.freedesktop.portal.Settings")?
        .cache_properties(CacheProperties::No)
        .build()?;
    // Asked after subscribing, so no change is missed.
    let changes = portal.receive_signal("SettingChanged")?;
    // Old portals have only `Read`, which wraps the value in a second variant.
    let args = (NAMESPACE, KEY);
    let first: Option<Value> = portal
        .call::<_, _, zbus::zvariant::OwnedValue>("ReadOne", &args)
        .or_else(|_| portal.call("Read", &args))
        .ok()
        .map(Into::into);
    publish(app, resolve(first.as_ref().and_then(scheme)));
    let _ = ready.send(());
    for message in changes {
        if let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, Value)>()
            && namespace == NAMESPACE
            && key == KEY
        {
            publish(app, resolve(scheme(&value)));
        }
    }
    Ok(())
}

fn publish(app: &AppHandle, theme: Option<Theme>) {
    let mut latest = LATEST.lock().unwrap();
    if *latest != Some(theme) {
        *latest = Some(theme);
        let _ = app.emit(EVENT, theme.map(name));
    }
}

fn name(theme: Theme) -> &'static str {
    if theme == Theme::Dark { "dark" } else { "light" }
}

/// The portal's number, through the variants it may be wrapped in: `u` from `ReadOne` and the signal,
/// `v(v(u))` from the old `Read`.
fn scheme(value: &Value) -> Option<u32> {
    match value {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => scheme(inner),
        _ => None,
    }
}

/// 1 is dark and 2 is light; 0 (no preference) or a portal that does not say falls back to the GTK settings.
fn resolve(scheme: Option<u32>) -> Option<Theme> {
    match scheme {
        Some(1) => Some(Theme::Dark),
        Some(2) => Some(Theme::Light),
        _ => gtk_theme(),
    }
}

fn gtk_theme() -> Option<Theme> {
    let file = dirs::config_dir()?.join("gtk-3.0/settings.ini");
    gtk_settings(&std::fs::read_to_string(file).ok()?)
}

/// Dark when settings.ini asks for the dark variant or names a theme with "dark" in it; the light
/// themes say nothing worth trusting (a missing key is the default), so the interface decides.
fn gtk_settings(ini: &str) -> Option<Theme> {
    let dark = ini.lines().filter_map(|line| line.split_once('=')).any(|(key, value)| {
        let value = value.trim();
        match key.trim() {
            "gtk-application-prefer-dark-theme" => value == "true" || value == "1",
            "gtk-theme-name" => value.to_lowercase().contains("dark"),
            _ => false,
        }
    });
    dark.then_some(Theme::Dark)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested(n: u32) -> Value<'static> {
        Value::Value(Box::new(Value::Value(Box::new(Value::U32(n)))))
    }

    #[test]
    fn reads_the_portal_number_in_every_shape() {
        assert_eq!(scheme(&Value::U32(1)), Some(1));
        assert_eq!(scheme(&Value::Value(Box::new(Value::U32(2)))), Some(2));
        assert_eq!(scheme(&nested(1)), Some(1));
        assert_eq!(scheme(&Value::from("dark")), None);
        assert_eq!(scheme(&Value::Value(Box::new(Value::Bool(true)))), None);
    }

    #[test]
    fn one_is_dark_and_two_is_light() {
        assert_eq!(resolve(scheme(&Value::U32(1))), Some(Theme::Dark));
        assert_eq!(resolve(scheme(&nested(2))), Some(Theme::Light));
    }

    #[test]
    fn the_gtk_settings_say_dark_by_the_flag_or_by_the_name() {
        let ini = |body: &str| format!("[Settings]\ngtk-font-name=Noto Sans 10\n{body}\n");
        assert_eq!(gtk_settings(&ini("gtk-application-prefer-dark-theme=true")), Some(Theme::Dark));
        assert_eq!(gtk_settings(&ini("gtk-application-prefer-dark-theme = 1")), Some(Theme::Dark));
        assert_eq!(gtk_settings(&ini("gtk-theme-name=Breeze-Dark")), Some(Theme::Dark));
        assert_eq!(gtk_settings(&ini("gtk-theme-name=adw-gtk3-dark")), Some(Theme::Dark));
        assert_eq!(
            gtk_settings(&ini("gtk-theme-name=Breeze\ngtk-application-prefer-dark-theme=true")),
            Some(Theme::Dark)
        );
        assert_eq!(gtk_settings(&ini("gtk-theme-name=Breeze\ngtk-application-prefer-dark-theme=false")), None);
        assert_eq!(gtk_settings(""), None);
    }
}

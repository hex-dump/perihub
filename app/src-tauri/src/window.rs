//! The main window and the tray icon. The window exists only while it is open: closing it
//! destroys the web view, which is most of PeriHub's memory and CPU (measured on Windows with
//! 0.3.0: perihub.exe alone 4 MB and 0.03% of a core, its WebView2 processes about 45 MB and 2%).
//! PeriHub keeps running in the tray to animate the lighting and follow the devices.

use std::sync::Mutex;

use perihub_core::quick::QuickAction;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, Wry};

const MAIN: &str = "main";
const TRAY: &str = "main";

/// The new version shown in the tray menu, kept when the menu is built again.
static UPDATE: Mutex<Option<String>> = Mutex::new(None);
/// One line per wireless device with a battery, shown in the tray menu and tooltip.
static BATTERIES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Shows the window, creating it if needed, optionally on a page ("settings", "mouse:buttons"…).
pub fn show(app: &AppHandle, page: Option<&str>) {
    let window = match app.get_webview_window(MAIN) {
        Some(window) => window,
        None => match create(app) {
            Ok(window) => window,
            Err(error) => {
                log::warn!("cannot open the window: {error}");
                return;
            }
        },
    };
    if let Some(page) = page {
        open_page(&window, page);
    }
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app.config().app.windows.iter().find(|w| w.label == MAIN).expect("main window in tauri.conf.json");
    #[allow(unused_mut)]
    let mut builder = WebviewWindowBuilder::from_config(app, config)?;
    // Linux: the web view cannot tell a dark desktop from a light one (see system_theme.rs), so the
    // window opens with the theme the portal gave, the title bar included.
    #[cfg(target_os = "linux")]
    if let Some(script) = crate::system_theme::init_script() {
        builder = builder.theme(crate::system_theme::current()).initialization_script(script);
    }
    let window = builder.build()?;
    crate::autostart::restart_hidden();
    // Development only: PERIHUB_PAGE=mouse:buttons opens a page directly.
    #[cfg(debug_assertions)]
    if let Ok(page) = std::env::var("PERIHUB_PAGE") {
        open_page(&window, &page);
    }
    Ok(window)
}

fn open_page(window: &WebviewWindow, page: &str) {
    if let Ok(mut url) = window.url() {
        url.set_fragment(Some(page));
        let _ = window.navigate(url);
    }
}

/// One entry of the tray menu, before it is built (so the tests can see what it holds).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Item { id: String, text: String, enabled: bool },
    Submenu { text: String, items: Vec<(QuickAction, String)> },
    Check { id: String, text: String, checked: bool },
}

/// What the tray menu shows.
#[derive(Debug, Default)]
struct MenuFacts {
    update: Option<String>,
    batteries: Vec<String>,
    /// A Corsair device has experimental lighting on, or PeriHub drives a Logitech keyboard's lights.
    experimental: bool,
    read_only: bool,
    /// The mouse's name and what it can be switched to.
    mouse: Option<(String, crate::quick::MouseChoices)>,
    /// The keyboard's name and the saved lighting presets.
    keyboard: Option<(String, Vec<String>)>,
}

fn item(id: &str, text: impl Into<String>, enabled: bool) -> Entry {
    Entry::Item { id: id.to_string(), text: text.into(), enabled }
}

fn entries(facts: &MenuFacts) -> Vec<Entry> {
    let mut entries = vec![item("open", "Open PeriHub", true)];
    if let Some(version) = &facts.update {
        entries.push(item("update", format!("Update to {version}…"), true));
    }
    // Information only: nothing to click.
    for (i, line) in facts.batteries.iter().enumerate() {
        entries.push(item(&format!("battery-{i}"), line.clone(), false));
    }
    // Quick actions change a device: in read-only mode they are not offered at all (and refused
    // if asked for some other way, see quick.rs).
    if !facts.read_only {
        entries.extend(quick_submenus(facts));
    }
    entries.push(item("stop", "Stop software macros", true));
    // A way out that does not need the window, in case a Corsair mouse stops clicking, or to give a
    // Logitech keyboard its own lights back.
    if facts.experimental {
        entries.push(item("experimental-off", "Turn off experimental lighting", true));
    }
    entries.push(Entry::Check { id: "read-only".into(), text: "Read-only mode".into(), checked: facts.read_only });
    entries.push(item("quit", "Quit", true));
    entries
}

/// One submenu for each connected device with quick actions: they only switch what is saved.
fn quick_submenus(facts: &MenuFacts) -> Vec<Entry> {
    let mut submenus = Vec::new();
    if let Some((model, choices)) = &facts.mouse {
        let mut items = Vec::new();
        if choices.profiles.len() > 1 {
            for &number in &choices.profiles {
                items.push((QuickAction::Profile { number }, format!("Profile {number}")));
            }
        }
        if choices.dpi_levels.len() > 1 {
            for (i, dpi) in choices.dpi_levels.iter().enumerate() {
                let number = i as u8 + 1;
                items.push((QuickAction::DpiLevel { number }, format!("DPI level {number}: {dpi}")));
            }
        }
        if !items.is_empty() {
            submenus.push(Entry::Submenu { text: model.clone(), items });
        }
    }
    if let Some((name, presets)) = &facts.keyboard {
        let items: Vec<_> =
            presets.iter().map(|name| (QuickAction::Preset { name: name.clone() }, name.clone())).collect();
        if !items.is_empty() {
            submenus.push(Entry::Submenu { text: name.clone(), items });
        }
    }
    submenus
}

fn menu_facts(app: &AppHandle) -> MenuFacts {
    let mut facts = MenuFacts {
        update: UPDATE.lock().unwrap().clone(),
        batteries: BATTERIES.lock().unwrap().clone(),
        read_only: perihub_core::writes::read_only(),
        ..Default::default()
    };
    if let Some(state) = app.try_state::<crate::AppState>() {
        let status = state.manager.status();
        facts.experimental =
            status.corsair.iter().any(|d| d.lighting.is_some()) || state.manager.drives_logitech_lights();
        if let (Some(mouse), Some(choices)) = (&status.mouse, crate::quick::mouse_choices()) {
            facts.mouse = Some((mouse.model.clone(), choices));
        }
        if let Some(keyboard) = &status.keyboard {
            let presets = state.config.lock().unwrap().lighting_presets.iter().map(|p| p.name.clone()).collect();
            facts.keyboard = Some((keyboard.name.to_string(), presets));
        }
    }
    facts
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut built: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for entry in entries(&menu_facts(app)) {
        built.push(match entry {
            Entry::Item { id, text, enabled } => Box::new(MenuItem::with_id(app, id, text, enabled, None::<&str>)?),
            Entry::Check { id, text, checked } => {
                Box::new(CheckMenuItem::with_id(app, id, text, true, checked, None::<&str>)?)
            }
            Entry::Submenu { text, items } => {
                let items = items
                    .iter()
                    .map(|(action, text)| {
                        MenuItem::with_id(app, crate::quick::menu_id(action), text, true, None::<&str>)
                    })
                    .collect::<tauri::Result<Vec<_>>>()?;
                let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
                Box::new(Submenu::with_items(app, text, true, &refs)?)
            }
        });
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = built.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Builds the tray menu again after something it shows changed.
pub fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip(tooltip())
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app, None),
            "update" => show(app, Some("settings")),
            "stop" => app.state::<crate::AppState>().soft.stop_all(),
            id if crate::quick::from_menu_id(id).is_some() => {
                crate::quick::run_from_menu(app, crate::quick::from_menu_id(id).unwrap());
            }
            "experimental-off" => crate::commands::turn_off_experimental(app),
            "read-only" => {
                let app = app.clone();
                let on = !perihub_core::writes::read_only();
                // Turning it on may first give a Corsair device back: not on the thread of the menu.
                std::thread::spawn(move || {
                    if let Err(error) = crate::commands::set_read_only(&app, on) {
                        log::warn!("read-only mode: {error}");
                    }
                });
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show(tray.app_handle(), None);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Quits PeriHub after releasing the keys a running software macro holds, giving a Corsair mouse
/// with experimental lighting back to its own settings, and a Logitech keyboard its own lights.
pub fn quit(app: &AppHandle) {
    app.state::<crate::AppState>().soft.stop_all_and_wait();
    app.state::<crate::AppState>().manager.release_corsair();
    app.state::<crate::AppState>().manager.release_logitech(false);
    app.exit(0);
}

/// Tells about a new version from the tray, where PeriHub spends most of its time: in the
/// tooltip and with a menu entry that opens the settings.
pub fn show_update(app: &AppHandle, version: &str) {
    *UPDATE.lock().unwrap() = Some(version.to_string());
    refresh_tooltip(app);
    refresh_menu(app);
}

/// What the tray icon says when the pointer rests on it: the new version, if there is one, and
/// the battery of each wireless device.
fn tooltip() -> String {
    let mut text = match UPDATE.lock().unwrap().as_deref() {
        Some(version) => format!("PeriHub: version {version} is available"),
        None => "PeriHub".to_string(),
    };
    for line in BATTERIES.lock().unwrap().iter() {
        text.push('\n');
        text.push_str(line);
    }
    text
}

fn refresh_tooltip(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.set_tooltip(Some(tooltip()));
    }
}

/// Shows the battery of the wireless devices in the tray (see battery.rs).
pub fn set_batteries(app: &AppHandle, lines: Vec<String>) {
    *BATTERIES.lock().unwrap() = lines;
    refresh_tooltip(app);
    refresh_menu(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick::MouseChoices;

    fn facts(read_only: bool) -> MenuFacts {
        MenuFacts {
            batteries: vec!["G502 X: 80%".into()],
            read_only,
            mouse: Some(("G502 X".into(), MouseChoices { profiles: vec![1, 2], dpi_levels: vec![800, 1600] })),
            keyboard: Some(("Apex 3 TKL".into(), vec!["Night".into()])),
            ..Default::default()
        }
    }

    fn ids(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Item { id, .. } | Entry::Check { id, .. } => id.clone(),
                Entry::Submenu { text, .. } => format!("submenu {text}"),
            })
            .collect()
    }

    #[test]
    fn the_quick_actions_are_in_the_menu_when_read_only_mode_is_off() {
        let entries = entries(&facts(false));
        assert_eq!(
            ids(&entries),
            ["open", "battery-0", "submenu G502 X", "submenu Apex 3 TKL", "stop", "read-only", "quit"]
        );
        let Entry::Submenu { items, .. } = &entries[2] else { panic!() };
        let actions: Vec<String> = items.iter().map(|(a, _)| crate::quick::menu_id(a)).collect();
        assert_eq!(actions, ["quick:profile:1", "quick:profile:2", "quick:dpi-level:1", "quick:dpi-level:2"]);
        assert!(entries.contains(&Entry::Check {
            id: "read-only".into(),
            text: "Read-only mode".into(),
            checked: false
        }));
    }

    #[test]
    fn read_only_mode_takes_the_quick_actions_out_of_the_menu() {
        let entries = entries(&facts(true));
        assert_eq!(ids(&entries), ["open", "battery-0", "stop", "read-only", "quit"]);
        assert!(!entries.iter().any(|e| matches!(e, Entry::Submenu { .. })));
        assert!(entries.contains(&Entry::Check {
            id: "read-only".into(),
            text: "Read-only mode".into(),
            checked: true
        }));
        // The battery stays, as information only.
        assert!(entries.contains(&item("battery-0", "G502 X: 80%", false)));
    }

    #[test]
    fn a_mouse_with_one_profile_and_one_dpi_level_has_no_submenu() {
        let mut facts = facts(false);
        facts.mouse = Some(("G305".into(), MouseChoices { profiles: vec![1], dpi_levels: vec![800] }));
        facts.keyboard = Some(("Apex 3 TKL".into(), Vec::new()));
        assert!(!entries(&facts).iter().any(|e| matches!(e, Entry::Submenu { .. })));
    }
}

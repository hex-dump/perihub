//! The main window and the tray icon. The window exists only while it is open: closing it
//! destroys the web view, which is most of PeriHub's memory and CPU (measured on Windows with
//! 0.3.0: perihub.exe alone 4 MB and 0.03% of a core, its WebView2 processes about 45 MB and 2%).
//! PeriHub keeps running in the tray to animate the lighting and follow the devices.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem};
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

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut items = vec![MenuItem::with_id(app, "open", "Open PeriHub", true, None::<&str>)?];
    if let Some(version) = UPDATE.lock().unwrap().as_deref() {
        items.push(MenuItem::with_id(app, "update", format!("Update to {version}…"), true, None::<&str>)?);
    }
    // Information only: nothing to click.
    for (i, line) in BATTERIES.lock().unwrap().iter().enumerate() {
        items.push(MenuItem::with_id(app, format!("battery-{i}"), line, false, None::<&str>)?);
    }
    items.push(MenuItem::with_id(app, "stop", "Stop software macros", true, None::<&str>)?);
    // A way out that does not need the window, in case a Corsair mouse stops clicking.
    let experimental = app
        .try_state::<crate::AppState>()
        .is_some_and(|state| state.manager.status().corsair.iter().any(|d| d.lighting.is_some()));
    if experimental {
        items.push(MenuItem::with_id(app, "experimental-off", "Turn off experimental lighting", true, None::<&str>)?);
    }
    let read_only = CheckMenuItem::with_id(
        app,
        "read-only",
        "Read-only mode",
        true,
        perihub_core::writes::read_only(),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let refs: Vec<&dyn IsMenuItem<Wry>> =
        items.iter().map(|i| i as &dyn IsMenuItem<Wry>).chain([&read_only as &dyn IsMenuItem<Wry>, &quit]).collect();
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

/// Quits PeriHub after releasing the keys a running software macro holds, and giving a Corsair
/// mouse with experimental lighting back to its own settings.
pub fn quit(app: &AppHandle) {
    app.state::<crate::AppState>().soft.stop_all_and_wait();
    app.state::<crate::AppState>().manager.release_corsair();
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

//! The main window and the tray icon. The window exists only while it is open: closing it
//! destroys the web view, which is most of PeriHub's memory and CPU (measured on Windows with
//! 0.3.0: perihub.exe alone 4 MB and 0.03% of a core, its WebView2 processes about 45 MB and 2%).
//! PeriHub keeps running in the tray to animate the lighting and follow the devices.

use std::sync::Mutex;

use perihub_core::quick::QuickAction;
use tauri::menu::{IsMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, Wry};

const MAIN: &str = "main";
const TRAY: &str = "main";

/// The new version shown in the tray menu, kept when the menu is built again.
static UPDATE: Mutex<Option<String>> = Mutex::new(None);

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
    let mut head = vec![MenuItem::with_id(app, "open", "Open PeriHub", true, None::<&str>)?];
    if let Some(version) = UPDATE.lock().unwrap().as_deref() {
        head.push(MenuItem::with_id(app, "update", format!("Update to {version}…"), true, None::<&str>)?);
    }
    let submenus = device_submenus(app)?;
    let mut items = vec![MenuItem::with_id(app, "stop", "Stop software macros", true, None::<&str>)?];
    // A way out that does not need the window, in case a Corsair mouse stops clicking.
    let experimental = app
        .try_state::<crate::AppState>()
        .is_some_and(|state| state.manager.status().corsair.iter().any(|d| d.lighting.is_some()));
    if experimental {
        items.push(MenuItem::with_id(app, "experimental-off", "Turn off experimental lighting", true, None::<&str>)?);
    }
    items.push(MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?);
    let refs: Vec<&dyn IsMenuItem<Wry>> = head
        .iter()
        .map(|i| i as &dyn IsMenuItem<Wry>)
        .chain(submenus.iter().map(|i| i as &dyn IsMenuItem<Wry>))
        .chain(items.iter().map(|i| i as &dyn IsMenuItem<Wry>))
        .collect();
    Menu::with_items(app, &refs)
}

/// One submenu for each connected device with quick actions: they only switch what is saved.
fn device_submenus(app: &AppHandle) -> tauri::Result<Vec<Submenu<Wry>>> {
    let Some(state) = app.try_state::<crate::AppState>() else { return Ok(Vec::new()) };
    let status = state.manager.status();
    let item = |action: QuickAction, text: String| {
        MenuItem::with_id(app, crate::quick::menu_id(&action), text, true, None::<&str>)
    };
    let mut submenus = Vec::new();
    if let (Some(mouse), Some(choices)) = (&status.mouse, crate::quick::mouse_choices()) {
        let mut items = Vec::new();
        for number in choices.profiles.iter().copied().filter(|_| choices.profiles.len() > 1) {
            items.push(item(QuickAction::Profile { number }, format!("Profile {number}"))?);
        }
        let dpi = choices.dpi_levels.iter().enumerate().filter(|_| choices.dpi_levels.len() > 1);
        for (i, dpi) in dpi {
            let number = i as u8 + 1;
            items.push(item(QuickAction::DpiLevel { number }, format!("DPI level {number}: {dpi}"))?);
        }
        let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        if !refs.is_empty() {
            submenus.push(Submenu::with_items(app, &mouse.model, true, &refs)?);
        }
    }
    if let Some(keyboard) = &status.keyboard {
        let presets = state.config.lock().unwrap().lighting_presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        let mut items = Vec::new();
        for name in presets {
            items.push(item(QuickAction::Preset { name: name.clone() }, name)?);
        }
        let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        if !refs.is_empty() {
            submenus.push(Submenu::with_items(app, keyboard.name, true, &refs)?);
        }
    }
    Ok(submenus)
}

/// Builds the tray menu again after something it shows changed.
pub fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip("PeriHub")
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
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.set_tooltip(Some(format!("PeriHub: version {version} is available")));
    }
    *UPDATE.lock().unwrap() = Some(version.to_string());
    refresh_menu(app);
}

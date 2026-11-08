//! Command-line tool to try PeriHub without the graphical interface.

use std::time::{Duration, Instant};

use perihub_core::apex::KeyBinding;
use perihub_core::config::AppConfig;
use perihub_core::effects::KeyboardLighting;
use perihub_core::hidpp::profile::{LedEffect, ProfileSettings};
use perihub_core::{Manager, Result};

const USAGE: &str = "usage:
  perihub-cli status
  perihub-cli focus [seconds]
  perihub-cli keyboard keymap
  perihub-cli keyboard brightness [0-16]
  perihub-cli keyboard light '<json KeyboardLighting>' [seconds]
  perihub-cli mouse state
  perihub-cli mouse preview-dpi <dpi>
  perihub-cli mouse preview-led <zone> '<json LedEffect>'
  perihub-cli mouse profile <number>
  perihub-cli mouse dpi-level <index>
  perihub-cli mouse get-profile <number>
  perihub-cli mouse save-profile <number> '<json ProfileSettings>'
  perihub-cli mouse factory <number>
  perihub-cli mouse backups
  perihub-cli mouse restore <backup id>
  perihub-cli mouse sector <hex number>
  perihub-cli keyboard remap <layer> <hex usage> '<json KeyBinding>'
  perihub-cli keyboard reset-keymap
  perihub-cli keyboard backups
  perihub-cli keyboard restore <backup id>";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Err(error) = run(&args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn wait_for_devices(manager: &Manager) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        let status = manager.status();
        let done = |present: bool, error: &Option<String>| present || error.is_some();
        if done(status.keyboard.is_some(), &status.keyboard_error) && done(status.mouse.is_some(), &status.mouse_error)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Prints the app in focus every time it changes (does not use the devices).
fn watch_focus(seconds: u64) -> Result<()> {
    println!("{:?}", perihub_core::focus::support());
    if !perihub_core::focus::watch(|app| println!("{}", app.as_deref().unwrap_or("(none)"))) {
        return Err(invalid("focus detection is not available here"));
    }
    std::thread::sleep(Duration::from_secs(seconds));
    Ok(())
}

fn run(args: &[&str]) -> Result<()> {
    match args {
        ["focus"] => return watch_focus(30),
        ["focus", seconds] => return watch_focus(seconds.parse().map_err(|_| invalid("seconds"))?),
        _ => {}
    }
    let Some(_devices) = perihub_core::config::try_lock_devices()? else {
        return Err(invalid("PeriHub is open: quit it from the tray first, since both would talk to the same devices"));
    };
    let config = AppConfig::load();
    let manager = Manager::start(config.keyboard, &[], Box::new(|_| {}));
    wait_for_devices(&manager);
    match args {
        ["status"] => print_json(&manager.status()),
        ["keyboard", "keymap"] => print_json(&manager.keyboard()?.call(|k| k.keymap())?),
        ["keyboard", "brightness"] => print_json(&manager.keyboard()?.call(|k| k.brightness())?),
        ["keyboard", "brightness", level] => {
            let level: u8 = level.parse().map_err(|_| invalid("brightness"))?;
            manager.keyboard()?.call(move |k| k.set_brightness(level))
        }
        ["keyboard", "light", json, rest @ ..] => {
            let lighting: KeyboardLighting = serde_json::from_str(json)?;
            let seconds: u64 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(5);
            manager.keyboard()?.call(move |k| k.set_lighting(lighting))?;
            std::thread::sleep(Duration::from_secs(seconds));
            Ok(())
        }
        ["mouse", "state"] => {
            print_json(&manager.status().mouse)?;
            print_json(&manager.mouse()?.call(|m| m.state())?)
        }
        ["mouse", "preview-dpi", dpi] => {
            let dpi: u16 = dpi.parse().map_err(|_| invalid("DPI"))?;
            manager.mouse()?.call(move |m| m.preview_dpi(dpi))
        }
        ["mouse", "preview-led", zone, json] => {
            let zone: u8 = zone.parse().map_err(|_| invalid("zone"))?;
            let effect: LedEffect = serde_json::from_str(json)?;
            manager.mouse()?.call(move |m| m.preview_led(zone, &effect))
        }
        ["mouse", "profile", number] => {
            let number: u8 = number.parse().map_err(|_| invalid("profile"))?;
            manager.mouse()?.call(move |m| m.set_active_profile(number))
        }
        ["mouse", "dpi-level", index] => {
            let index: u8 = index.parse().map_err(|_| invalid("level"))?;
            manager.mouse()?.call(move |m| m.set_active_dpi_index(index))
        }
        ["mouse", "get-profile", number] => {
            let number: u8 = number.parse().map_err(|_| invalid("profile"))?;
            let state = manager.mouse()?.call(|m| m.state())?;
            let profile = state.profiles.into_iter().find(|p| p.number == number).ok_or_else(|| invalid("profile"))?;
            println!("{}", serde_json::to_string(&profile.settings)?);
            Ok(())
        }
        ["mouse", "save-profile", number, json] => {
            let number: u8 = number.parse().map_err(|_| invalid("profile"))?;
            let settings: ProfileSettings = serde_json::from_str(json)?;
            manager.mouse()?.call(move |m| m.save_profile(number, &settings)).map(|_| ())
        }
        ["mouse", "factory", number] => {
            let number: u8 = number.parse().map_err(|_| invalid("profile"))?;
            manager.mouse()?.call(move |m| m.restore_factory_profile(number)).map(|_| ())
        }
        ["mouse", "backups"] => print_json(&manager.mouse()?.call(|m| Ok(m.backups()))?),
        ["mouse", "restore", id] => {
            let id = id.to_string();
            let restore = manager.mouse()?.call(move |m| m.restore_backup(&id))?;
            print_json(
                &serde_json::json!({"profiles": restore.profiles, "macros": restore.macros, "directory": restore.directory}),
            )
        }
        ["mouse", "sector", sector] => {
            let sector = u16::from_str_radix(sector, 16).map_err(|_| invalid("sector"))?;
            let data = manager.mouse()?.call(move |m| m.read_raw_sector(sector))?;
            println!("{}", data.iter().map(|b| format!("{b:02x}")).collect::<String>());
            Ok(())
        }
        ["keyboard", "remap", layer, source, json] => {
            let layer: u8 = layer.parse().map_err(|_| invalid("level"))?;
            let source = u8::from_str_radix(source.trim_start_matches("0x"), 16).map_err(|_| invalid("key"))?;
            let binding: KeyBinding = serde_json::from_str(json)?;
            let keymap = manager.keyboard()?.call(move |k| k.apply_keymap(&[(layer, source, binding)]))?;
            let layer_map = if layer == 0 { &keymap.normal } else { &keymap.steelseries };
            print_json(&layer_map[&source])
        }
        ["keyboard", "reset-keymap"] => manager.keyboard()?.call(|k| k.reset_keymap()).map(|_| ()),
        ["keyboard", "backups"] => print_json(&manager.keyboard()?.call(|k| Ok(k.backups()))?),
        ["keyboard", "restore", id] => {
            let id = id.to_string();
            let restore = manager.keyboard()?.call(move |k| k.restore_backup(&id))?;
            print_json(&serde_json::json!({"keys": restore.keys}))
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn invalid(what: &str) -> perihub_core::Error {
    perihub_core::Error::Invalid(what.into())
}

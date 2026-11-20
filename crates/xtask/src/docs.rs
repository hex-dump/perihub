//! The tables of docs/devices.md, between the markers the generator writes around them. The text
//! around them is written by hand.

use perihub_core::peripheral::{Protocol, asus, razer, razer_keyboard};

use crate::catalog::{Access, Device, Kind, Level, Source};
use crate::select::{self, block, fill, select};

pub const PATH: &str = "docs/devices.md";

pub fn level(level: Level) -> &'static str {
    match level {
        Level::Supported => "✅ Supported",
        Level::Experimental => "🧪 Experimental",
        Level::ReadOnly => "👁 Read-only",
        Level::ByMemoryFormat => "depends on its memory format",
        Level::Recognized => "recognized",
    }
}

fn opened(device: &Device) -> String {
    match device.access {
        Access::Interface(n) => format!("interface {n}"),
        Access::Whole => format!("whole {}", if device.kind == Kind::Keyboard { "keyboard" } else { "mouse" }),
        Access::ReceiverHidpp => "the receiver's HID++ interface".into(),
        Access::Closed if device.bluetooth_id.is_some() => {
            "through a receiver, or its Bluetooth node (whole mouse, read only)".into()
        }
        Access::Closed if device.product_ids.is_empty() => "through a receiver".into(),
        Access::Closed => "—".into(),
    }
}

fn usb_ids(device: &Device) -> String {
    if device.product_ids.is_empty() {
        return match device.bluetooth_id {
            Some(id) => format!("wireless only, Bluetooth `{:04x}:{id:04x}`", device.vendor_id),
            None => "wireless only".into(),
        };
    }
    device.product_ids.iter().map(|id| format!("`{:04x}:{id:04x}`", device.vendor_id)).collect::<Vec<_>>().join(", ")
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The column a family adds to its table, with what its driver's parameters say about each model.
/// A column's title, and what it says about one device.
type Column = (&'static str, fn(&Device) -> String);

fn extra(family: &str) -> Option<Column> {
    Some(match family {
        "logitech-onboard" => {
            ("Through a receiver", |d| d.wireless_id.map(|id| format!("`{id:04x}`")).unwrap_or_default())
        }
        "asus" => ("Notes", |d| {
            let Source::Peripheral(m) = d.model else { unreachable!() };
            let Protocol::Asus(p) = m.protocol else { unreachable!() };
            let mut notes = Vec::new();
            if p.leds == 0 {
                notes.push("no lighting".to_owned());
            }
            if p.quirks & asus::SEPARATE_XY_DPI != 0 {
                notes.push("DPI shown, not changed (separate X and Y)".into());
            }
            if p.quirks & asus::RAW_BRIGHTNESS != 0 {
                notes.push("LED brightness kept as it is".into());
            }
            if notes.is_empty() { "—".into() } else { notes.join("; ") }
        }),
        "steelseries-mice" => ("Settings PeriHub sends", |d| {
            let Source::Peripheral(m) = d.model else { unreachable!() };
            let Protocol::SteelSeries(p) = m.protocol else { unreachable!() };
            let battery = if p.battery.is_some() { ", battery" } else { "" };
            format!("{}{battery}", p.settings.len())
        }),
        "razer" => ("What PeriHub changes", |d| {
            let Source::Peripheral(m) = d.model else { unreachable!() };
            let Protocol::Razer(p) = m.protocol else { unreachable!() };
            let mut first = Vec::new();
            if p.dpi_max > 0 && p.tid.dpi_set.is_some() && p.tid.dpi_set == p.tid.dpi_get {
                first.push(format!("DPI up to {}", p.dpi_max));
            }
            if p.tid.poll_set.is_some() || p.poll2.is_some() {
                first.push("polling rate".into());
            } else if p.tid.poll_get.is_some() {
                first.push("polling rate shown".into());
            }
            let mut parts = vec![first.join(", ")];
            if let Some(b) = p.brightness {
                let names: Vec<String> = b
                    .leds
                    .iter()
                    .map(|id| {
                        razer::LIGHTS
                            .iter()
                            .find(|(l, _)| l == id)
                            .map_or(format!("LED {id:02x}"), |(_, n)| n.to_lowercase())
                    })
                    .collect();
                parts.push(format!("brightness of {}", and_list(&names)));
            }
            if p.frame.is_some() {
                parts.push("one colour, not saved".into());
            }
            if p.dongle_led.is_some() {
                parts.push("what the dongle light shows".into());
            }
            if p.battery.is_some() {
                parts.push("battery shown".into());
            }
            parts.retain(|p| !p.is_empty());
            if parts.is_empty() { "—".into() } else { parts.join("; ") }
        }),
        "razer-keyboard" => ("What PeriHub changes", |d| {
            let Source::Peripheral(m) = d.model else { unreachable!() };
            let Protocol::RazerKeyboard(p) = m.protocol else { unreachable!() };
            razer_keyboard::changes(&p).map_or("—".into(), |changes| and_list(&changes))
        }),
        "glorious" => ("Firmware", |d| {
            let Source::Peripheral(m) = d.model else { unreachable!() };
            let Protocol::Sinowealth(p) = m.protocol else { unreachable!() };
            format!("`{}`", p.firmware)
        }),
        _ => return None,
    })
}

/// What is particular to one model, for its page on the website: the same facts as the extra column of
/// its family's table, for the families whose column is about the model and not about the driver.
pub fn note(device: &Device) -> Option<String> {
    if !matches!(device.family, "razer" | "asus" | "razer-keyboard") {
        return None;
    }
    let (_, cell) = extra(device.family)?;
    Some(cell(device)).filter(|text| text != "—")
}

/// The table of the devices a selector takes. Models with the same name share a row.
pub fn table(devices: &[Device], selector: &str) -> String {
    let mut rows: Vec<Device> = Vec::new();
    let found = select(devices, selector);
    let family = found[0].family;
    for d in found {
        match rows
            .iter_mut()
            .find(|r| r.name == d.name && r.brand == d.brand && r.level == d.level && r.access == d.access)
        {
            Some(r) => r.product_ids.extend(&d.product_ids),
            None => rows.push(d.clone()),
        }
    }
    let brands = rows.iter().any(|r| r.brand != rows[0].brand);
    let levels = rows.iter().any(|r| r.level != rows[0].level);
    let what = if rows.iter().all(|r| r.kind == Kind::Mouse) {
        "Mouse"
    } else if rows.iter().all(|r| r.kind == Kind::Keyboard) {
        "Keyboard"
    } else {
        "Device"
    };
    let extra = extra(family);
    let mut head = vec![what, "USB ID", "Opened on Linux"];
    if levels {
        head.push("Level");
    }
    if let Some((title, _)) = extra {
        head.push(title);
    }
    let mut out = format!("| {} |\n|{}\n", head.join(" | "), "---|".repeat(head.len()));
    for r in &rows {
        let name = if brands { format!("{} {}", r.brand, r.name) } else { r.name.clone() };
        let mut cells = vec![name, usb_ids(r), opened(r)];
        if levels {
            cells.push(level(r.level).into());
        }
        if let Some((_, cell)) = extra {
            cells.push(cell(r));
        }
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

/// The names a selector takes, comma separated and wrapped, for devices listed in a sentence.
pub fn names(devices: &[Device], selector: &str) -> String {
    let names = select::names(devices, selector);
    let mut out = String::new();
    let mut line = 0;
    for (i, name) in names.iter().enumerate() {
        let word = if i + 1 == names.len() { format!("{name}.") } else { format!("{name},") };
        if line > 0 && line + 1 + word.len() > 100 {
            out.push('\n');
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(&word);
        line += word.len();
    }
    out + "\n"
}

/// docs/devices.md, and the counts in README.md.
pub fn render(path: &str, old: &str, devices: &[Device]) -> String {
    fill(old, path, |kind, selector, indent| match kind {
        "table" => block(table(devices, selector).lines().map(str::to_owned), indent),
        "names" => block(names(devices, selector).lines().map(str::to_owned), indent),
        "count" => select::names(devices, selector).len().to_string(),
        _ => panic!("{path}: unknown generated block {kind:?}"),
    })
}

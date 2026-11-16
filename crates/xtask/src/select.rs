//! Which devices a generated block lists: `razer`, `Razer:experimental`, `pulsar+nordic`…
//!
//! A selector is one or more groups joined by `+`. A group is words joined by `:`, and takes the
//! devices that match every word: a family (`razer`, `logitech-onboard`), a brand (`Cooler Master`),
//! a level (`supported`, `experimental`, `read-only`, `by-memory-format`, `recognized`), a kind
//! (`mouse`, `keyboard`, `receiver`), `cable` (it has a USB id of its own), `wireless` (it is
//! reached through a LIGHTSPEED receiver) or `colour` (a Razer mouse that takes one colour).

use perihub_core::peripheral::Protocol;

use crate::catalog::{Device, Kind, Level, Source};

fn matches(device: &Device, word: &str) -> bool {
    match word {
        "supported" => device.level == Level::Supported,
        "experimental" => device.level == Level::Experimental,
        "read-only" => device.level == Level::ReadOnly,
        "by-memory-format" => device.level == Level::ByMemoryFormat,
        "recognized" => device.level == Level::Recognized,
        "mouse" => device.kind == Kind::Mouse,
        "keyboard" => device.kind == Kind::Keyboard,
        "receiver" => device.kind == Kind::Receiver,
        "cable" => !device.product_ids.is_empty(),
        "wireless" => device.wireless_id.is_some(),
        "colour" => {
            matches!(device.model, Source::Peripheral(m) if matches!(m.protocol, Protocol::Razer(p) if p.frame.is_some()))
        }
        _ => device.family == word || device.brand == word,
    }
}

/// The devices a selector takes, in the order of the tables. It must take at least one: a selector
/// that takes none is a typo, or a family that was renamed.
pub fn select<'a>(devices: &'a [Device], selector: &str) -> Vec<&'a Device> {
    let groups: Vec<Vec<&str>> = selector.split('+').map(|g| g.split(':').map(str::trim).collect()).collect();
    for word in groups.iter().flatten() {
        assert!(devices.iter().any(|d| matches(d, word)), "selector {selector:?}: nothing is {word:?}");
    }
    let found: Vec<&Device> =
        devices.iter().filter(|d| groups.iter().any(|g| g.iter().all(|w| matches(d, w)))).collect();
    assert!(!found.is_empty(), "selector {selector:?} takes no device");
    found
}

/// The names of the devices a selector takes, each once.
pub fn names<'a>(devices: &'a [Device], selector: &str) -> Vec<&'a str> {
    let mut names: Vec<&str> = Vec::new();
    for d in select(devices, selector) {
        if !names.contains(&d.name.as_str()) {
            names.push(&d.name);
        }
    }
    names
}

/// Fills every generated block of a file. A block starts with `<!-- generated: KIND SELECTOR -->`
/// and ends with `<!-- end generated -->`. `render` gets the kind, the selector and the indentation
/// of the line the block starts on, and returns what goes between the markers.
pub fn fill(text: &str, path: &str, render: impl Fn(&str, &str, &str) -> String) -> String {
    const START: &str = "<!-- generated: ";
    const END: &str = "<!-- end generated -->";
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(START) {
        let close = i + rest[i..].find("-->").unwrap_or_else(|| panic!("{path}: unclosed marker")) + 3;
        let spec = rest[i + START.len()..close - 3].trim();
        let (kind, selector) =
            spec.split_once(' ').unwrap_or_else(|| panic!("{path}: marker {spec:?} needs a kind and a selector"));
        let end = close + rest[close..].find(END).unwrap_or_else(|| panic!("{path}: {spec:?} has no end marker"));
        let line_start = rest[..i].rfind('\n').map_or(0, |n| n + 1);
        let indent: String = rest[line_start..i].chars().take_while(|c| c.is_whitespace()).collect();
        out.push_str(&rest[..close]);
        out.push_str(&render(kind, selector, &indent));
        rest = &rest[end..];
    }
    out + rest
}

/// Lines between the markers of a block that spans lines.
pub fn block(lines: impl IntoIterator<Item = String>, indent: &str) -> String {
    let mut out = String::from("\n");
    for line in lines {
        out.push_str(indent);
        out.push_str(&line);
        out.push('\n');
    }
    out + indent
}

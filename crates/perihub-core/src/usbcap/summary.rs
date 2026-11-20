//! A recording as text for a device request: the device's transfers, step by step, with what
//! could tell what was typed, or which unit it is, left out.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::descriptors::{self, DeviceDescriptor, Inputs, Interface};
use super::packet::{self, Kind, Transfer};
use super::pcap::{Frame, Link};

/// The device a recording is for, with what the system says of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Its bus and address, when the system says (Linux); otherwise found from its device
    /// descriptor in the recording.
    pub addresses: Vec<(u16, u16)>,
    /// Its USB descriptors as the system gives them (Linux's sysfs `descriptors`).
    pub usb_descriptors: Option<Vec<u8>>,
    /// The HID report descriptor of each interface, as the system gives it.
    pub report_descriptors: Vec<(u8, Vec<u8>)>,
}

/// One step of the user's list: its name, and when it started (milliseconds since 1970).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub at_ms: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub text: String,
    /// The device was found in the recording.
    pub found: bool,
    /// Transfers written (before repeats are folded).
    pub transfers: usize,
}

/// The longest text: a GitHub issue takes 65536 characters, and the request has more than this.
pub const MAX_TEXT: usize = 50_000;

const GET_DESCRIPTOR: u8 = 6;
const DESCRIPTOR_DEVICE: u8 = 1;
const DESCRIPTOR_CONFIGURATION: u8 = 2;
const DESCRIPTOR_STRING: u8 = 3;
const DESCRIPTOR_HID_REPORT: u8 = 0x22;

fn descriptor_request(t: &Transfer) -> Option<(u8, u8, u16)> {
    let s = t.setup?;
    (s[0] & 0x80 != 0 && s[0] & 0x60 == 0 && s[1] == GET_DESCRIPTOR)
        .then(|| (s[3], s[2], u16::from_le_bytes([s[4], s[5]])))
}

/// Which bus and address hold the device: what the system said, and every address whose device
/// descriptor has its ids (a device that was plugged in again gets a new address).
pub fn find_addresses(transfers: &[Transfer], target: &Target) -> Vec<(u16, u16)> {
    let mut found: BTreeSet<(u16, u16)> = target.addresses.iter().copied().collect();
    for t in transfers {
        if descriptor_request(t).is_some_and(|(kind, ..)| kind == DESCRIPTOR_DEVICE)
            && let Some(d) = descriptors::device_descriptor(&t.data)
            && (d.vendor_id, d.product_id) == (target.vendor_id, target.product_id)
        {
            found.insert((t.bus, t.address));
        }
    }
    found.into_iter().collect()
}

/// What is known of the device: from its descriptors in the recording, else from the system.
struct Device {
    descriptor: Option<DeviceDescriptor>,
    interfaces: Vec<Interface>,
    inputs: BTreeMap<u8, Inputs>,
}

impl Device {
    fn new(transfers: &[&Transfer], target: &Target) -> Self {
        let mut descriptor = None;
        let mut configuration = None;
        let mut reports: BTreeMap<u8, Vec<u8>> = target.report_descriptors.iter().cloned().collect();
        for t in transfers {
            let Some((kind, _, index)) = descriptor_request(t) else { continue };
            match kind {
                DESCRIPTOR_DEVICE => descriptor = descriptors::device_descriptor(&t.data).or(descriptor),
                DESCRIPTOR_CONFIGURATION if t.data.len() >= 9 => configuration = Some(t.data.clone()),
                DESCRIPTOR_HID_REPORT if t.setup.is_some_and(|s| s[0] & 0x1F == 1) && !t.data.is_empty() => {
                    reports.insert(index as u8, t.data.clone());
                }
                _ => {}
            }
        }
        let system = target.usb_descriptors.as_deref();
        let descriptor = descriptor.or_else(|| system.and_then(descriptors::device_descriptor));
        let mut interfaces = configuration.as_deref().map(descriptors::interfaces).unwrap_or_default();
        if interfaces.is_empty() {
            interfaces = system.map(descriptors::interfaces).unwrap_or_default();
        }
        let inputs = interfaces
            .iter()
            .map(|i| (i.number, descriptors::inputs(i, reports.get(&i.number).map(Vec::as_slice))))
            .collect();
        Self { descriptor, interfaces, inputs }
    }

    fn interface_of(&self, endpoint: u8) -> Option<&Interface> {
        self.interfaces.iter().find(|i| i.endpoints.contains(&endpoint))
    }

    /// The strings that are safe to show: the language list, the maker's name and the product's.
    fn string_shown(&self, index: u8) -> bool {
        index == 0
            || self.descriptor.is_some_and(|d| index != d.serial && (index == d.manufacturer || index == d.product))
    }
}

/// What a recording left out, counted for the text.
#[derive(Default)]
struct LeftOut {
    strings: usize,
    inputs: BTreeMap<String, usize>,
    isochronous: usize,
}

/// "11ff001a04025a(00×13)": the bytes in hex, with a run of 4 or more zeros at the end shortened.
pub fn hex_short(data: &[u8]) -> String {
    let zeros = data.iter().rev().take_while(|&&b| b == 0).count();
    let (kept, zeros) = if zeros >= 4 { (&data[..data.len() - zeros], zeros) } else { (data, 0) };
    let mut s: String = kept.iter().map(|b| format!("{b:02x}")).collect();
    if zeros > 0 {
        let _ = write!(s, "(00×{zeros})");
    }
    s
}

const REPORT_TYPES: [&str; 4] = ["type 0", "input", "output", "feature"];

/// What a control request is, in the words of the USB and HID specifications.
fn request_name(setup: [u8; 8]) -> String {
    let (kind, request) = (setup[0], setup[1]);
    let value = u16::from_le_bytes([setup[2], setup[3]]);
    let index = u16::from_le_bytes([setup[4], setup[5]]);
    let to = match kind & 0x1F {
        0 => "device".to_string(),
        1 => format!("if{}", index & 0xFF),
        2 => format!("ep{:02x}", index & 0xFF),
        other => format!("recipient {other}"),
    };
    match (kind >> 5) & 3 {
        0 => match request {
            GET_DESCRIPTOR => {
                let which = match setup[3] {
                    DESCRIPTOR_DEVICE => "device".to_string(),
                    DESCRIPTOR_CONFIGURATION => "configuration".to_string(),
                    DESCRIPTOR_STRING => format!("string {}", setup[2]),
                    0x0F => "BOS".to_string(),
                    0x21 => format!("HID if{}", index),
                    DESCRIPTOR_HID_REPORT => format!("HID report if{}", index),
                    other => format!("type {other:02x}"),
                };
                format!("GET_DESCRIPTOR {which}")
            }
            0 => format!("GET_STATUS {to}"),
            1 => format!("CLEAR_FEATURE {value} {to}"),
            3 => format!("SET_FEATURE {value} {to}"),
            5 => format!("SET_ADDRESS {value}"),
            8 => "GET_CONFIGURATION".to_string(),
            9 => format!("SET_CONFIGURATION {value}"),
            10 => format!("GET_INTERFACE {to}"),
            11 => format!("SET_INTERFACE {to} alt {value}"),
            other => format!("standard request {other:02x} value {value:04x} index {index:04x}"),
        },
        1 if kind & 0x1F == 1 && matches!(request, 1 | 9) => format!(
            "{} {} id {:02x} {to}",
            if request == 1 { "GET_REPORT" } else { "SET_REPORT" },
            REPORT_TYPES[usize::from(setup[3] & 3)],
            setup[2]
        ),
        1 if kind & 0x1F == 1 && matches!(request, 2 | 3 | 0x0A | 0x0B) => {
            let name = match request {
                2 => "GET_IDLE",
                3 => "GET_PROTOCOL",
                0x0A => "SET_IDLE",
                _ => "SET_PROTOCOL",
            };
            format!("{name} value {value:04x} {to}")
        }
        1 => format!("class request {request:02x} value {value:04x} index {index:04x} to {to}"),
        2 => format!("vendor request {request:02x} value {value:04x} index {index:04x} to {to}"),
        _ => format!("reserved request {request:02x} value {value:04x} index {index:04x}"),
    }
}

/// One transfer as a line, without its time.
fn line(t: &Transfer, device: &Device) -> String {
    let arrow = if t.is_in() { "←" } else { "→" };
    let mut s = match t.setup {
        Some(setup) => format!("{arrow} {}", request_name(setup)),
        None => {
            let kind = if t.kind == Kind::Bulk { "bulk " } else { "" };
            let direction = if t.is_in() { "in" } else { "out" };
            let iface = device.interface_of(t.endpoint).map(|i| format!(" if{}", i.number)).unwrap_or_default();
            format!("{arrow} {kind}{direction} ep{:02x}{iface}", t.endpoint)
        }
    };
    if !t.answered {
        s.push_str(" (no answer)");
    } else if !t.ok {
        s.push_str(" (failed)");
    }
    if !t.data.is_empty() {
        let _ = write!(s, ": {}", hex_short(&t.data));
    }
    s
}

/// Whether a transfer may go into the text, and why not.
fn kept(t: &Transfer, device: &Device, left: &mut LeftOut) -> bool {
    if t.kind == Kind::Isochronous {
        left.isochronous += 1;
        return false;
    }
    if let Some((DESCRIPTOR_STRING, index, _)) = descriptor_request(t) {
        if !device.string_shown(index) {
            left.strings += 1;
            return false;
        }
        return true;
    }
    if t.setup.is_some() || !t.is_in() {
        return true;
    }
    let iface = device.interface_of(t.endpoint);
    let inputs = iface.and_then(|i| device.inputs.get(&i.number)).unwrap_or(&Inputs::Nothing);
    let keep = match inputs {
        Inputs::All => true,
        Inputs::Ids(ids) => t.data.first().is_some_and(|id| ids.contains(id)),
        Inputs::Nothing => false,
    };
    if !keep {
        let name = iface.map_or_else(|| format!("ep{:02x}", t.endpoint), |i| format!("if{}", i.number));
        *left.inputs.entry(name).or_default() += 1;
    }
    keep
}

/// Lines folded where they repeat: a run of the same 1 to 4 lines becomes one copy with a count.
fn fold(lines: &[(i64, String)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (mut best_period, mut best_count) = (1, 1);
        for period in 1..=4 {
            let mut count = 1;
            while i + (count + 1) * period <= lines.len()
                && (0..period).all(|k| lines[i + k].1 == lines[i + count * period + k].1)
            {
                count += 1;
            }
            if count >= 2 && count * period > best_period * best_count {
                (best_period, best_count) = (period, count);
            }
        }
        let time = |at: usize| format!("{:>9}", format!("+{:.3}", lines[at].0 as f64 / 1_000_000.0));
        if best_count >= 2 && best_period == 1 {
            out.push(format!("{} {} ×{best_count}", time(i), lines[i].1));
        } else if best_count >= 2 {
            out.push(format!("{} [the next {best_period} lines ×{best_count}]", time(i)));
            for k in 0..best_period {
                out.push(format!("{} {}", time(i + k), lines[i + k].1));
            }
        } else {
            out.push(format!("{} {}", time(i), lines[i].1));
        }
        i += best_period * best_count;
    }
    out
}

fn inputs_text(inputs: &Inputs) -> String {
    match inputs {
        Inputs::All => "its reports kept".into(),
        Inputs::Ids(ids) => format!(
            "reports {} kept (vendor-defined), the others left out",
            ids.iter().map(|id| format!("{id:02x}")).collect::<Vec<_>>().join(" ")
        ),
        Inputs::Nothing => "its reports left out (input, or not known to be vendor-defined)".into(),
    }
}

/// The recording as text, step by step. `source` names the tool ("USBPcap 1.5.4.0", "usbmon").
pub fn summarize(link: Link, frames: &[Frame], target: &Target, steps: &[Step], source: &str) -> Summary {
    let all = packet::transfers(frames.iter().filter_map(|f| packet::parse(link, f)));
    let addresses = find_addresses(&all, target);
    let id = format!("{:04x}:{:04x}", target.vendor_id, target.product_id);
    let mut text = String::new();
    if addresses.is_empty() {
        let seen: BTreeSet<String> = all
            .iter()
            .filter(|t| descriptor_request(t).is_some_and(|(kind, ..)| kind == DESCRIPTOR_DEVICE))
            .filter_map(|t| descriptors::device_descriptor(&t.data))
            .map(|d| format!("{:04x}:{:04x}", d.vendor_id, d.product_id))
            .collect();
        let _ = writeln!(text, "USB recording ({source}): the device {id} was not found in it.");
        let _ = write!(
            text,
            "Devices seen: {}. Nothing of them is included.",
            if seen.is_empty() {
                "none named itself".to_string()
            } else {
                seen.into_iter().collect::<Vec<_>>().join(", ")
            }
        );
        return Summary { text, found: false, transfers: 0 };
    }
    let mine: Vec<&Transfer> = all.iter().filter(|t| addresses.contains(&(t.bus, t.address))).collect();
    let device = Device::new(&mine, target);
    let mut left = LeftOut::default();
    let kept: Vec<&Transfer> = mine.into_iter().filter(|t| kept(t, &device, &mut left)).collect();

    let at =
        addresses.iter().map(|(bus, address)| format!("bus {bus} address {address}")).collect::<Vec<_>>().join(", ");
    let _ = writeln!(text, "USB recording of {id} ({source}, {at})");
    if device.interfaces.is_empty() {
        let _ = writeln!(
            text,
            "Interfaces: not known (no configuration descriptor), so every report from the device is left out"
        );
    } else {
        for i in &device.interfaces {
            let endpoints = i.endpoints.iter().map(|e| format!("ep{e:02x}")).collect::<Vec<_>>().join(" ");
            let inputs = device.inputs.get(&i.number).map(inputs_text).unwrap_or_default();
            let _ = writeln!(
                text,
                "Interface {} ({}): {}; {inputs}",
                i.number,
                i.class_name(),
                if endpoints.is_empty() { "no endpoints" } else { &endpoints }
            );
        }
    }
    let mut notes = Vec::new();
    if left.strings > 0 {
        notes.push(format!("{} string descriptors (they can hold the serial number)", left.strings));
    }
    for (iface, count) in &left.inputs {
        notes.push(format!("{count} reports from {iface}"));
    }
    if left.isochronous > 0 {
        notes.push(format!("{} isochronous transfers (sound or video)", left.isochronous));
    }
    if !notes.is_empty() {
        let _ = writeln!(text, "Left out for privacy: {}", notes.join(", "));
    }
    let _ = writeln!(
        text,
        "→ goes to the device, ← comes from it; (00×N): N zero bytes; ×N: N times in a row; +s: seconds into the step"
    );

    // The steps, in the order they started; what came before the first one is a step of its own.
    let mut bounds: Vec<(i64, String)> = steps.iter().map(|s| (s.at_ms * 1000, s.name.clone())).collect();
    bounds.sort_by_key(|b| b.0);
    let start = bounds.first().map_or(i64::MIN, |b| b.0);
    let mut sections: Vec<(String, Vec<(i64, String)>)> = Vec::new();
    let before: Vec<&&Transfer> = kept.iter().filter(|t| bounds.is_empty() || t.time_us < start).collect();
    if !before.is_empty() || bounds.is_empty() {
        let from = before.first().map_or(0, |t| t.time_us);
        let name = if bounds.is_empty() { "Recording".to_string() } else { "Before the first step".to_string() };
        sections.push((name, before.iter().map(|t| (t.time_us - from, line(t, &device))).collect()));
    }
    for (n, (from, name)) in bounds.iter().enumerate() {
        let to = bounds.get(n + 1).map_or(i64::MAX, |b| b.0);
        let lines = kept
            .iter()
            .filter(|t| t.time_us >= *from && t.time_us < to)
            .map(|t| (t.time_us - from, line(t, &device)))
            .collect();
        sections.push((format!("Step {}: {name} (at {:.1} s)", n + 1, (from - start) as f64 / 1_000_000.0), lines));
    }
    let total: usize = sections.iter().map(|s| s.1.len()).sum();
    let mut written = 0;
    'sections: for (title, lines) in &sections {
        let _ = write!(text, "\n{title}\n");
        if lines.is_empty() {
            text.push_str("  (nothing sent or received)\n");
        }
        let folded = fold(lines);
        let per_line = lines.len().max(1) as f64 / folded.len().max(1) as f64;
        for (k, l) in folded.iter().enumerate() {
            if text.len() + l.len() + 100 > MAX_TEXT {
                let rest = total - written - ((k as f64) * per_line) as usize;
                let _ =
                    writeln!(text, "(cut here: about {rest} more transfers; the recording is too long for a report)");
                break 'sections;
            }
            text.push_str(l);
            text.push('\n');
        }
        written += lines.len();
    }
    Summary { text: text.trim_end().to_string(), found: true, transfers: kept.len() }
}

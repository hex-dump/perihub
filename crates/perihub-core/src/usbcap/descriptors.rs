//! The USB descriptors that say which device a capture holds and what each of its interfaces is.

use std::collections::BTreeSet;

use crate::hid_descriptor;

/// The device descriptor's ids, and the indexes of its strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: u8,
    pub product: u8,
    pub serial: u8,
}

/// The device descriptor at the start of `data` (18 bytes, type 1).
pub fn device_descriptor(data: &[u8]) -> Option<DeviceDescriptor> {
    if data.len() < 18 || data[0] != 18 || data[1] != 1 {
        return None;
    }
    Some(DeviceDescriptor {
        vendor_id: u16::from_le_bytes([data[8], data[9]]),
        product_id: u16::from_le_bytes([data[10], data[11]]),
        manufacturer: data[14],
        product: data[15],
        serial: data[16],
    })
}

/// One interface (all its alternate settings), with its endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Interface {
    pub number: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoints: BTreeSet<u8>,
}

pub const CLASS_HID: u8 = 3;
pub const CLASS_VENDOR: u8 = 0xFF;

impl Interface {
    /// A keyboard in boot protocol: its reports carry no id, and are what is typed.
    pub fn boot_keyboard(&self) -> bool {
        self.class == CLASS_HID && self.subclass == 1 && self.protocol == 1
    }

    pub fn class_name(&self) -> String {
        match (self.class, self.subclass, self.protocol) {
            (CLASS_HID, 1, 1) => "HID boot keyboard".into(),
            (CLASS_HID, 1, 2) => "HID boot mouse".into(),
            (CLASS_HID, ..) => "HID".into(),
            (CLASS_VENDOR, ..) => "vendor-specific".into(),
            (1, ..) => "audio".into(),
            (2, ..) | (0x0A, ..) => "communications".into(),
            (8, ..) => "mass storage".into(),
            (0x0E, ..) => "video".into(),
            (class, ..) => format!("class {class:02x}"),
        }
    }
}

/// The interfaces of the configurations in `data`: one or more configuration descriptors with the
/// descriptors that follow them, as a device sends them, or what Linux shows in sysfs (the device
/// descriptor, then each configuration).
pub fn interfaces(data: &[u8]) -> Vec<Interface> {
    let mut found: Vec<Interface> = Vec::new();
    let mut current: Option<usize> = None;
    let mut i = 0;
    while i + 2 <= data.len() {
        let (len, kind) = (usize::from(data[i]), data[i + 1]);
        if len < 2 || i + len > data.len() {
            break;
        }
        let d = &data[i..i + len];
        match kind {
            4 if len >= 9 => {
                let at = match found.iter().position(|f| f.number == d[2]) {
                    Some(at) => at,
                    None => {
                        found.push(Interface {
                            number: d[2],
                            class: d[5],
                            subclass: d[6],
                            protocol: d[7],
                            ..Default::default()
                        });
                        found.len() - 1
                    }
                };
                current = Some(at);
            }
            5 if len >= 7 => {
                if let Some(at) = current {
                    found[at].endpoints.insert(d[2]);
                }
            }
            _ => {}
        }
        i += len;
    }
    found
}

/// Which reports from the device (interrupt or bulk IN) of one interface go into a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inputs {
    /// A vendor-defined interface: all of them.
    All,
    /// Those whose first byte is one of these report ids, the ids of vendor-defined collections.
    Ids(BTreeSet<u8>),
    /// None: they can be what is typed or how the mouse moves, or what they are is not known.
    Nothing,
}

/// A usage page of the vendor-defined range (0xFF00 to 0xFFFF).
fn vendor_page(page: u16) -> bool {
    page >= 0xFF00
}

/// What may be kept of the reports `iface` sends, from its class and its HID report descriptor
/// (`report`). Kept: a vendor-specific interface; on a HID interface the reports of
/// vendor-defined collections, told apart from the others by their report id. Never kept: a boot
/// keyboard, the reports of any other collection (keyboard, mouse, consumer keys, joystick…), a
/// HID interface whose report descriptor is not known or has no report ids while one of its
/// collections is not vendor-defined, and every other class (audio, video, storage…).
pub fn inputs(iface: &Interface, report: Option<&[u8]>) -> Inputs {
    if iface.class == CLASS_VENDOR {
        return Inputs::All;
    }
    if iface.class != CLASS_HID || iface.boot_keyboard() {
        return Inputs::Nothing;
    }
    let Some(collections) = report.and_then(hid_descriptor::summarize) else {
        return Inputs::Nothing;
    };
    if collections.is_empty() {
        return Inputs::Nothing;
    }
    if collections.iter().all(|c| vendor_page(c.usage_page)) {
        return Inputs::All;
    }
    let ids = |vendor: bool| -> BTreeSet<u8> {
        collections
            .iter()
            .filter(|c| vendor_page(c.usage_page) == vendor)
            .flat_map(|c| c.reports.iter().filter(|r| r.input > 0 || !vendor).map(|r| r.id))
            .collect()
    };
    let others = ids(false);
    if others.contains(&0) {
        // Reports without an id: nothing tells a key press from a vendor report.
        return Inputs::Nothing;
    }
    let kept: BTreeSet<u8> = ids(true).difference(&others).copied().filter(|&id| id != 0).collect();
    if kept.is_empty() { Inputs::Nothing } else { Inputs::Ids(kept) }
}

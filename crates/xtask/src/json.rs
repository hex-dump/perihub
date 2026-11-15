//! site/devices.json: every device PeriHub recognizes, for the website and for the workflow that
//! answers device requests.

use serde::Serialize;

use crate::catalog::{Access, Device, Kind, Level};

pub const PATH: &str = "site/devices.json";

#[derive(Serialize)]
struct Catalog<'a> {
    /// What the file is, for whoever finds it.
    about: &'static str,
    devices: Vec<Entry<'a>>,
}

#[derive(Serialize)]
struct Entry<'a> {
    family: &'a str,
    brand: &'a str,
    name: &'a str,
    kind: Kind,
    level: Level,
    /// "vvvv:pppp", as lsusb shows them. Empty for a mouse reached only through a receiver.
    usb_ids: Vec<String>,
    /// The id a Logitech mouse reports through a LIGHTSPEED receiver.
    #[serde(skip_serializing_if = "Option::is_none")]
    wireless_id: Option<String>,
    /// What the Linux udev rule opens.
    linux: Access,
}

pub fn render(devices: &[Device]) -> String {
    let catalog = Catalog {
        about: "Devices PeriHub recognizes, generated from its driver tables (cargo xtask gen). Levels: https://github.com/hex-dump/perihub/blob/main/docs/devices.md",
        devices: devices
            .iter()
            .map(|d| Entry {
                family: d.family,
                brand: d.brand,
                name: &d.name,
                kind: d.kind,
                level: d.level,
                usb_ids: d.product_ids.iter().map(|id| format!("{:04x}:{id:04x}", d.vendor_id)).collect(),
                wireless_id: d.wireless_id.map(|id| format!("{id:04x}")),
                linux: d.access,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&catalog).unwrap() + "\n"
}

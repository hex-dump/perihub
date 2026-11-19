//! site/devices.json: every device PeriHub recognizes, for the website and for the workflow that
//! answers device requests.

use std::collections::HashMap;

use serde::Serialize;

use crate::catalog::{self, Access, Device, Kind, Level};
use crate::docs;

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
    /// The model's page on the website, relative to its root. Models with the same name share one.
    page: String,
    /// "vvvv:pppp", as lsusb shows them. Empty for a mouse reached only through a receiver.
    usb_ids: Vec<String>,
    /// The id a Logitech mouse reports through a LIGHTSPEED receiver.
    #[serde(skip_serializing_if = "Option::is_none")]
    wireless_id: Option<String>,
    /// What the Linux udev rule opens.
    linux: Access,
    /// What is particular to this model, when the driver's table says (Razer and ASUS mice).
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

pub fn render(devices: &[Device]) -> String {
    // Two models must never share a page: the address would show only one of them.
    let mut pages: HashMap<String, (&str, &str)> = HashMap::new();
    for d in devices {
        let page = catalog::page(d.brand, &d.name);
        let owner = pages.entry(page.clone()).or_insert((d.brand, &d.name));
        assert_eq!(*owner, (d.brand, d.name.as_str()), "{page} is the page of two different models");
    }
    let catalog = Catalog {
        about: "Devices PeriHub recognizes, generated from its driver tables (cargo xtask gen). Levels: https://github.com/hex-dump/perihub/blob/main/docs/devices.md",
        devices: devices
            .iter()
            .map(|d| Entry {
                family: d.family,
                brand: d.brand,
                name: &d.name,
                page: catalog::page(d.brand, &d.name),
                kind: d.kind,
                level: d.level,
                usb_ids: d.product_ids.iter().map(|id| format!("{:04x}:{id:04x}", d.vendor_id)).collect(),
                wireless_id: d.wireless_id.map(|id| format!("{id:04x}")),
                linux: d.access,
                note: docs::note(d),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&catalog).unwrap() + "\n"
}

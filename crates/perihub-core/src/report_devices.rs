//! The USB and Bluetooth HID devices connected that PeriHub does not know, for a device request:
//! what a user can send so that support for their device can be looked into. The list goes into a
//! public issue, so it holds no serial numbers and no paths, only what `lsusb` would show and the
//! collections of each interface.

use hidapi::{BusType, DeviceInfo};
use serde::Serialize;

use crate::hidpp::{models, receiver};
use crate::{corsair, g300, peripheral};

const STEELSERIES: u16 = 0x1038;
/// The Apex 3 TKL, and the Apex 3, which PeriHub recognizes and names without opening it.
const STEELSERIES_KEYBOARDS: [u16; 2] = [0x1622, 0x161A];
const LOGITECH: u16 = 0x046D;

/// One device, with every collection of its interfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// "USB" or "Bluetooth".
    pub bus: &'static str,
    pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Collection {
    /// -1 when the system does not say (Bluetooth).
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
}

/// What one HID collection says about its device, taken from hidapi.
#[derive(Debug, Clone)]
pub struct Entry {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub bus: &'static str,
    pub collection: Collection,
}

impl Entry {
    /// USB and Bluetooth devices only: the rest (I2C, SPI) is built into laptops.
    pub fn from_hid(d: &DeviceInfo) -> Option<Self> {
        let bus = match d.bus_type() {
            BusType::Usb => "USB",
            BusType::Bluetooth => "Bluetooth",
            _ => return None,
        };
        let text = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        Some(Self {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            manufacturer: text(d.manufacturer_string()),
            product: text(d.product_string()),
            bus,
            collection: Collection { interface: d.interface_number(), usage_page: d.usage_page(), usage: d.usage() },
        })
    }
}

/// A device PeriHub opens, or names, by itself: it has its own entry in the interface already.
pub fn known(vendor_id: u16, product_id: u16) -> bool {
    match vendor_id {
        STEELSERIES if STEELSERIES_KEYBOARDS.contains(&product_id) => true,
        LOGITECH if models::find(product_id).is_some() || receiver::is_receiver(product_id) => true,
        LOGITECH if product_id == g300::PRODUCT_ID => true,
        corsair::CORSAIR if corsair::models::find(product_id).is_some() || corsair::models::is_receiver(product_id) => {
            true
        }
        _ => peripheral::find(vendor_id, product_id).is_some(),
    }
}

/// The devices PeriHub does not know, one per vendor and product id (Windows lists every
/// collection on its own), each with its collections in order and without repeats.
pub fn unknown_devices(entries: impl IntoIterator<Item = Entry>) -> Vec<ReportDevice> {
    let mut devices: Vec<ReportDevice> = Vec::new();
    for e in entries.into_iter().filter(|e| !known(e.vendor_id, e.product_id)) {
        let device = match devices.iter_mut().find(|d| (d.vendor_id, d.product_id) == (e.vendor_id, e.product_id)) {
            Some(device) => device,
            None => {
                devices.push(ReportDevice {
                    vendor_id: e.vendor_id,
                    product_id: e.product_id,
                    manufacturer: None,
                    product: None,
                    bus: e.bus,
                    collections: Vec::new(),
                });
                devices.last_mut().unwrap()
            }
        };
        device.manufacturer = device.manufacturer.take().or(e.manufacturer);
        device.product = device.product.take().or(e.product);
        if !device.collections.contains(&e.collection) {
            device.collections.push(e.collection);
        }
    }
    for d in &mut devices {
        d.collections.sort_by_key(|c| (c.interface, c.usage_page, c.usage));
    }
    devices.sort_by(|a, b| (&a.product, a.vendor_id, a.product_id).cmp(&(&b.product, b.vendor_id, b.product_id)));
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(vendor_id: u16, product_id: u16, product: Option<&str>, interface: i32, page: u16, usage: u16) -> Entry {
        Entry {
            vendor_id,
            product_id,
            manufacturer: Some("Maker".into()),
            product: product.map(str::to_owned),
            bus: "USB",
            collection: Collection { interface, usage_page: page, usage },
        }
    }

    #[test]
    fn one_device_per_id_with_its_collections_once_each() {
        let devices = unknown_devices([
            entry(0x1234, 0x0001, Some("Mouse X"), 1, 0xFF00, 1),
            entry(0x1234, 0x0001, None, 0, 1, 2),
            entry(0x1234, 0x0001, Some("Mouse X"), 1, 0xFF00, 1),
            entry(0x1234, 0x0002, Some("A keyboard"), 0, 1, 6),
        ]);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].product.as_deref(), Some("A keyboard"));
        let mouse = &devices[1];
        assert_eq!(mouse.product.as_deref(), Some("Mouse X"));
        assert_eq!(
            mouse.collections,
            [
                Collection { interface: 0, usage_page: 1, usage: 2 },
                Collection { interface: 1, usage_page: 0xFF00, usage: 1 }
            ]
        );
    }

    #[test]
    fn devices_perihub_shows_by_itself_are_left_out() {
        let devices = unknown_devices([
            entry(0x1038, 0x1622, Some("Apex 3 TKL"), 1, 0xFFC0, 1),
            entry(0x046D, 0xC08B, Some("G502 HERO"), 1, 0xFF00, 2),
            entry(0x046D, 0xC548, Some("Bolt receiver"), 2, 0xFF00, 2),
            entry(0x046D, 0xC246, Some("G300"), 1, 0xFF80, 1),
            entry(0x1B1C, 0x1B93, Some("KATAR PRO"), 1, 0xFF42, 1),
            entry(0x3710, 0x3414, Some("X2 CrazyLight"), 1, 1, 2),
            entry(0x046D, 0xC52B, Some("Unifying receiver"), 2, 0xFF00, 2),
            entry(0x046D, 0x0A87, Some("A Logitech headset"), 3, 0x0C, 1),
        ]);
        let names: Vec<_> = devices.iter().map(|d| d.product.as_deref().unwrap()).collect();
        assert_eq!(names, ["A Logitech headset"]);
    }
}

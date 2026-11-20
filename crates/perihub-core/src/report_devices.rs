//! The USB and Bluetooth HID devices connected that PeriHub does not know, for a device request:
//! what a user can send so that support for their device can be looked into. The list goes into a
//! public issue, so it holds no serial numbers and no paths, only what `lsusb` would show, the
//! collections of each interface and the shape of their report descriptors. Nothing is written to a
//! device and no input report is read. The one thing asked of a device is the current contents of the
//! feature reports its vendor-defined collections declare, read only and only while the Send a
//! report page is open: see [`feature_reports`] for why that is safe.

use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;
#[cfg(target_os = "linux")]
use std::path::Path;

use hidapi::{BusType, DeviceInfo, HidApi};
use serde::Serialize;

use crate::feature_reports::{self, FeatureRead, Outcome};
use crate::hid_descriptor::{self, TopCollection};
use crate::hidpp::{direct, models, receiver};
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
    /// The device's release number (bcdDevice), when the system gives it.
    pub release: Option<u16>,
    /// How many interfaces the USB device has, HID or not, when the system says (Linux).
    pub usb_interfaces: Option<u8>,
    /// The report descriptor of each interface, one entry for an interface whose descriptor could
    /// not be read.
    pub descriptors: Vec<InterfaceDescriptor>,
    /// The feature reports of each interface that declares vendor-defined ones, read only.
    /// `None`: not read (yet). `Some` and empty: there was nothing to read.
    pub feature_reports: Option<Vec<InterfaceFeatures>>,
}

/// The feature reports of one interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceFeatures {
    pub interface: i32,
    #[serde(flatten)]
    pub read: FeatureRead,
}

/// What the report descriptor of one interface says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceDescriptor {
    pub interface: i32,
    /// The descriptor in hex; `None`: not read.
    pub hex: Option<String>,
    /// Its report ids and lengths; `None`: not read, or not a descriptor that makes sense.
    pub collections: Option<Vec<TopCollection>>,
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
    pub release: Option<u16>,
    pub details: NodeDetails,
}

/// What reading one HID node gave besides what hidapi lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeDetails {
    pub descriptor: Option<Vec<u8>>,
    pub usb_interfaces: Option<u8>,
    /// The feature reports, read once the Send a report page is open. `None`: not read yet.
    pub features: Option<FeatureRead>,
}

/// What was read from each node already, by the node's path: a node is read once, however often the
/// list of devices is taken again, and forgotten when it goes away.
pub type DetailsCache = HashMap<String, NodeDetails>;

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
            release: Some(d.release_number()).filter(|&r| r != 0),
            details: NodeDetails::default(),
        })
    }
}

/// The entries of every HID device hidapi lists, with the descriptor of each device PeriHub does not
/// know, read once (see [`DetailsCache`]). `features`: also its feature reports, once each, which
/// needs the node opened (see [`feature_reports`]): only for the Send a report page.
pub fn entries(api: &HidApi, cache: &mut DetailsCache, features: bool) -> Vec<Entry> {
    let mut seen = Vec::new();
    let entries = api
        .device_list()
        .filter_map(|d| {
            let mut entry = Entry::from_hid(d)?;
            if !known(entry.vendor_id, entry.product_id) {
                let path = d.path().to_string_lossy().into_owned();
                let details = cache.entry(path.clone()).or_insert_with(|| read_node(api, d, &path));
                if features && details.features.is_none() {
                    details.features = Some(read_features(api, d, details.descriptor.as_deref()));
                }
                entry.details = details.clone();
                seen.push(path);
            }
            Some(entry)
        })
        .collect();
    cache.retain(|path, _| seen.contains(path));
    entries
}

/// The feature reports a node's vendor-defined collections declare, read only (see
/// [`feature_reports`]). Nothing is opened unless the descriptor declares some and the node is not
/// a mouse or a keyboard; the node is opened like any other (no rights asked for), and closed when
/// the reading is over.
fn read_features(api: &HidApi, d: &DeviceInfo, descriptor: Option<&[u8]>) -> FeatureRead {
    let mouse_or_keyboard = d.usage_page() == 0x01 && matches!(d.usage(), 0x01 | 0x02 | 0x06 | 0x07);
    let requests = match descriptor {
        Some(descriptor) if !mouse_or_keyboard => feature_reports::plan(descriptor),
        _ => Vec::new(),
    };
    if requests.is_empty() {
        return FeatureRead::new(Outcome::NothingToRead);
    }
    match d.open_device(api) {
        Ok(device) => feature_reports::read(device, &requests, feature_reports::REPORT_TIMEOUT),
        Err(_) => FeatureRead::new(Outcome::NotOpened),
    }
}

/// The descriptor of a node and the number of interfaces of its USB device, or what of them could
/// be read. On Linux the descriptor comes from sysfs, which every user may read, without opening the
/// device (a hidraw node needs a rule to be opened).
#[cfg(target_os = "linux")]
fn read_node(_api: &HidApi, _d: &DeviceInfo, path: &str) -> NodeDetails {
    read_sysfs(Path::new("/sys/class/hidraw"), path)
}

/// On other systems the descriptor comes from hidapi, with the device opened as hidapi does (on
/// Windows without read or write access when the system keeps the device to itself, as keyboards and
/// mice are). Nothing is read from it or written to it.
#[cfg(not(target_os = "linux"))]
fn read_node(api: &HidApi, d: &DeviceInfo, _path: &str) -> NodeDetails {
    let descriptor = d.open_device(api).ok().and_then(|device| {
        let mut buffer = [0u8; hid_descriptor::MAX_DESCRIPTOR];
        let length = device.get_report_descriptor(&mut buffer).ok()?;
        Some(buffer[..length].to_vec())
    });
    NodeDetails { descriptor, usb_interfaces: None, features: None }
}

/// A hidraw node's descriptor and the interface count of its USB device, from sysfs under
/// `class_dir` (`/sys/class/hidraw`). The node's `device` is the HID device, whose parent is the
/// USB interface, whose parent is the USB device.
#[cfg(target_os = "linux")]
fn read_sysfs(class_dir: &Path, node_path: &str) -> NodeDetails {
    let Some(name) = Path::new(node_path).file_name().and_then(OsStr::to_str).filter(|n| n.starts_with("hidraw"))
    else {
        return NodeDetails::default();
    };
    let Ok(device) = std::fs::canonicalize(class_dir.join(name).join("device")) else {
        return NodeDetails::default();
    };
    let descriptor = std::fs::read(device.join("report_descriptor"))
        .ok()
        .filter(|d| !d.is_empty() && d.len() <= hid_descriptor::MAX_DESCRIPTOR);
    let usb_interfaces = device
        .parent()
        .and_then(Path::parent)
        .and_then(|usb| std::fs::read_to_string(usb.join("bNumInterfaces")).ok())
        .and_then(|n| n.trim().parse().ok());
    NodeDetails { descriptor, usb_interfaces, features: None }
}

/// A device PeriHub opens, or names, by itself: it has its own entry in the interface already. On
/// Linux a model PeriHub does not open there (see [`peripheral::opened_on_linux`]) is listed, so that a
/// device request can show what its interfaces carry.
pub fn known(vendor_id: u16, product_id: u16) -> bool {
    match vendor_id {
        STEELSERIES if STEELSERIES_KEYBOARDS.contains(&product_id) => true,
        LOGITECH
            if models::find(product_id).is_some()
                || models::find_bluetooth(product_id).is_some()
                || receiver::is_receiver(product_id) =>
        {
            true
        }
        LOGITECH if direct::find(product_id).is_some() => true,
        LOGITECH if product_id == g300::PRODUCT_ID => true,
        corsair::CORSAIR if corsair::models::find(product_id).is_some() || corsair::models::is_receiver(product_id) => {
            true
        }
        _ => peripheral::find(vendor_id, product_id)
            .is_some_and(|m| !cfg!(target_os = "linux") || peripheral::opened_on_linux(m)),
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
                    release: None,
                    usb_interfaces: None,
                    descriptors: Vec::new(),
                    feature_reports: Some(Vec::new()),
                });
                devices.last_mut().unwrap()
            }
        };
        device.manufacturer = device.manufacturer.take().or(e.manufacturer);
        device.product = device.product.take().or(e.product);
        device.release = device.release.or(e.release);
        device.usb_interfaces = device.usb_interfaces.or(e.details.usb_interfaces);
        if !device.collections.contains(&e.collection) {
            device.collections.push(e.collection);
        }
        let hex = e.details.descriptor.as_deref().map(hid_descriptor::hex);
        if !device.descriptors.iter().any(|d| (d.interface, &d.hex) == (e.collection.interface, &hex)) {
            let collections = e.details.descriptor.as_deref().and_then(hid_descriptor::summarize);
            device.descriptors.push(InterfaceDescriptor { interface: e.collection.interface, hex, collections });
        }
        // One node not read yet leaves the whole device "not read".
        match (&mut device.feature_reports, e.details.features) {
            (Some(list), Some(read)) => {
                let interface = e.collection.interface;
                if read.outcome != Outcome::NothingToRead
                    && !list.iter().any(|f| (f.interface, &f.read) == (interface, &read))
                {
                    list.push(InterfaceFeatures { interface, read });
                }
            }
            (slot, None) => *slot = None,
            _ => {}
        }
    }
    for d in &mut devices {
        d.collections.sort_by_key(|c| (c.interface, c.usage_page, c.usage));
        d.descriptors.sort_by(|a, b| (a.interface, &a.hex).cmp(&(b.interface, &b.hex)));
        if let Some(list) = &mut d.feature_reports {
            list.sort_by_key(|f| f.interface);
        }
        // An interface whose descriptor was read once does not need a line saying it was not read.
        let read: Vec<i32> = d.descriptors.iter().filter(|x| x.hex.is_some()).map(|x| x.interface).collect();
        d.descriptors.retain(|x| x.hex.is_some() || !read.contains(&x.interface));
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
            release: Some(0x0100),
            details: NodeDetails::default(),
        }
    }

    #[test]
    fn a_mouse_known_only_by_its_bluetooth_id_is_not_listed_as_unknown() {
        // The G604 over Bluetooth (issue #43), but not an id nobody knows.
        assert!(known(0x046D, 0xB024));
        assert!(!known(0x046D, 0xB999));
        let devices = unknown_devices([
            entry(0x046D, 0xB024, Some("G604"), -1, 0xFF43, 0x0202),
            entry(0x046D, 0xB999, Some("Another Bluetooth mouse"), -1, 0xFF43, 0x0202),
        ]);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].product_id, 0xB999);
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
            entry(0x046D, 0xC339, Some("PRO K/DA"), 1, 0xFF43, 0x0604),
            entry(0x046D, 0x0A87, Some("A Logitech headset"), 3, 0x0C, 1),
        ]);
        let names: Vec<_> = devices.iter().map(|d| d.product.as_deref().unwrap()).collect();
        assert_eq!(names, ["A Logitech headset"]);
    }

    #[test]
    fn a_keyboard_perihub_does_not_open_on_linux_is_listed_there() {
        let devices = unknown_devices([entry(0x1532, 0x0203, Some("BlackWidow Chroma"), 1, 1, 6)]);
        assert_eq!(devices.len(), usize::from(cfg!(target_os = "linux")));
        assert!(unknown_devices([entry(0x1532, 0x02B9, Some("BlackWidow V4 Mini"), 3, 0x0C, 1)]).is_empty());
    }

    const MOUSE: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00,
        0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0xC0, 0xC0,
    ];

    fn with_descriptor(mut e: Entry, descriptor: Option<&[u8]>, usb_interfaces: Option<u8>) -> Entry {
        e.details = NodeDetails { descriptor: descriptor.map(<[u8]>::to_vec), usb_interfaces, features: None };
        e
    }

    #[test]
    fn each_interface_has_its_descriptor_once_and_the_ones_not_read_say_so() {
        let devices = unknown_devices([
            with_descriptor(entry(0x1234, 1, Some("Mouse X"), 0, 1, 2), Some(MOUSE), Some(3)),
            // Windows: the same interface again, with the same descriptor.
            with_descriptor(entry(0x1234, 1, Some("Mouse X"), 0, 1, 2), Some(MOUSE), None),
            with_descriptor(entry(0x1234, 1, Some("Mouse X"), 1, 0xFF00, 1), None, None),
            with_descriptor(entry(0x1234, 1, Some("Mouse X"), 1, 0xFF00, 2), None, None),
        ]);
        let d = &devices[0];
        assert_eq!((d.release, d.usb_interfaces), (Some(0x0100), Some(3)));
        assert_eq!(d.descriptors.len(), 2);
        assert_eq!(d.descriptors[0].interface, 0);
        assert_eq!(d.descriptors[0].hex.as_deref(), Some(hid_descriptor::hex(MOUSE).as_str()));
        assert_eq!(d.descriptors[0].collections.as_ref().unwrap()[0].usage, 2);
        assert_eq!(d.descriptors[1], InterfaceDescriptor { interface: 1, hex: None, collections: None });
    }

    #[test]
    fn what_is_serialized_holds_no_serial_number_and_no_path() {
        let devices =
            unknown_devices([with_descriptor(entry(0x1234, 1, Some("Mouse X"), 0, 1, 2), Some(MOUSE), Some(1))]);
        let json = serde_json::to_value(&devices).unwrap();
        let mut keys: Vec<&str> = json[0].as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "bus",
                "collections",
                "descriptors",
                "feature_reports",
                "manufacturer",
                "product",
                "product_id",
                "release",
                "usb_interfaces",
                "vendor_id"
            ]
        );
    }

    fn with_features(mut e: Entry, features: Option<FeatureRead>) -> Entry {
        e.details.features = features;
        e
    }

    fn read_of(outcome: Outcome, ids: &[u8]) -> FeatureRead {
        let reports = ids
            .iter()
            .map(|&id| feature_reports::Report { id, length: 3, hex: Some(format!("{id:02x}0102")) })
            .collect();
        FeatureRead { outcome, reports }
    }

    #[test]
    fn a_device_whose_nodes_are_not_read_yet_says_not_read_and_one_with_nothing_declared_says_so() {
        let devices = unknown_devices([
            with_features(entry(0x1234, 1, Some("A"), 0, 1, 2), Some(FeatureRead::new(Outcome::NothingToRead))),
            with_features(entry(0x1234, 1, Some("A"), 1, 0xFF00, 1), Some(FeatureRead::new(Outcome::NothingToRead))),
            with_features(entry(0x1234, 2, Some("B"), 0, 1, 2), Some(FeatureRead::new(Outcome::NothingToRead))),
            with_features(entry(0x1234, 2, Some("B"), 1, 0xFF00, 1), None),
        ]);
        assert_eq!(devices[0].feature_reports, Some(Vec::new()));
        assert_eq!(devices[1].feature_reports, None);
    }

    #[test]
    fn feature_reports_are_listed_by_interface_once_each() {
        let ok = read_of(Outcome::Read, &[0x20, 0x21]);
        let devices = unknown_devices([
            with_features(entry(0x1234, 1, Some("A"), 2, 0xFF01, 1), Some(FeatureRead::new(Outcome::NotOpened))),
            with_features(entry(0x1234, 1, Some("A"), 1, 0xFF00, 1), Some(ok.clone())),
            // The same node listed twice (Windows lists every collection).
            with_features(entry(0x1234, 1, Some("A"), 1, 0xFF00, 1), Some(ok.clone())),
            with_features(entry(0x1234, 1, Some("A"), 0, 1, 2), Some(FeatureRead::new(Outcome::NothingToRead))),
        ]);
        assert_eq!(
            devices[0].feature_reports,
            Some(vec![
                InterfaceFeatures { interface: 1, read: ok },
                InterfaceFeatures { interface: 2, read: FeatureRead::new(Outcome::NotOpened) },
            ])
        );
        let json = serde_json::to_value(&devices[0]).unwrap();
        assert_eq!(json["feature_reports"][1]["outcome"], "not_opened");
        assert_eq!(json["feature_reports"][0]["reports"][0]["hex"], "200102");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_gives_the_descriptor_and_the_interface_count_without_opening_the_node() {
        let root = std::env::temp_dir().join(format!("perihub-test-report-sysfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let usb = root.join("devices/usb1/1-2");
        let hid = usb.join("1-2:1.1/0003:1234:0001.0003");
        std::fs::create_dir_all(&hid).unwrap();
        std::fs::write(usb.join("bNumInterfaces"), " 3\n").unwrap();
        std::fs::write(hid.join("report_descriptor"), MOUSE).unwrap();
        let node = root.join("class/hidraw4");
        std::fs::create_dir_all(&node).unwrap();
        std::os::unix::fs::symlink(&hid, node.join("device")).unwrap();

        let details = read_sysfs(&root.join("class"), "/dev/hidraw4");
        assert_eq!(details, NodeDetails { descriptor: Some(MOUSE.to_vec()), usb_interfaces: Some(3), features: None });
        // A node that is not there, or not a hidraw node, says nothing.
        assert_eq!(read_sysfs(&root.join("class"), "/dev/hidraw9"), NodeDetails::default());
        assert_eq!(read_sysfs(&root.join("class"), "/dev/input/event4"), NodeDetails::default());
        let _ = std::fs::remove_dir_all(&root);
    }
}

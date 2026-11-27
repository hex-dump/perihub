//! Logitech HID++ 2.0 devices on a USB connection of their own, without a receiver, such as a
//! keyboard on its cable. PeriHub only reads them until someone tests one: nothing but the reads of
//! [`super::READ_ONLY_CALLS`] is ever sent (docs/protocols/logitech-usb-hidpp.md).
//!
//! Newer Logitech devices put HID++ in collections of the vendor usage page 0xFF43 instead of
//! 0xFF00. Which collection carries the long reports PeriHub sends is not documented, so each HID++
//! collection is tried in turn: on Windows, where each collection has a path of its own, a write
//! to a collection without the long report fails at once.

use std::cmp::Reverse;

use super::models::WIRED_INDEX;
use super::receiver::UnsupportedDevice;
use super::{HidppDevice, Transport};
use crate::error::{Error, Result};

/// The vendor usage page of the HID++ collections of the newer Logitech devices.
pub const USAGE_PAGE_HIDPP: u16 = 0xFF43;
/// The usage page of the HID++ collections of the older ones, and of the receivers.
pub const USAGE_PAGE_HIDPP_OLD: u16 = 0xFF00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectKind {
    Keyboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectModel {
    pub product_id: u16,
    pub name: &'static str,
    pub kind: DirectKind,
    /// The USB interface with the HID++ collections, as the device reports them.
    pub interface: i32,
    /// How it connects, as its page and its test report say it.
    pub connection: &'static str,
    /// Its HID++ interface also carries what is typed on the keyboard. On Linux one hidraw node holds
    /// every collection of an interface: PeriHub never opens it there, and the udev rule leaves it
    /// closed. On Windows the HID++ collection has a path of its own.
    pub typing: bool,
}

/// The devices PeriHub reads on their own USB connection. Interfaces and collections come from the
/// owners' device requests.
pub const DIRECT: &[DirectModel] = &[
    // Issue #40: interface 0 keyboard; interface 1 keyboard, consumer keys and HID++ (FF43, usages
    // 0602 and 0604).
    DirectModel {
        product_id: 0xC339,
        name: "PRO K/DA",
        kind: DirectKind::Keyboard,
        interface: 1,
        connection: "USB cable",
        typing: true,
    },
    // Issue #146: interface 0 keyboard; interface 1 mouse, system and consumer collections; interface 2
    // only HID++ (FF00, usages 0001 and 0002), so no typing shares it.
    DirectModel {
        product_id: 0xC35B,
        name: "PRO X RAPID",
        kind: DirectKind::Keyboard,
        interface: 2,
        connection: "USB cable",
        typing: false,
    },
];

/// Logitech's USB vendor id.
const LOGITECH: u16 = 0x046D;

/// The direct model with this product id, unless a driver of its own has the device. The HID++
/// collections of such a device may be on the very usage page this module looks for (the Logitech
/// G213's are on 0xFF43, docs/protocols/logitech-g213.md): a model there is opened by its driver
/// only, never read a second time as a direct device.
pub fn find(product_id: u16) -> Option<&'static DirectModel> {
    find_in(DIRECT, product_id)
}

fn find_in(table: &'static [DirectModel], product_id: u16) -> Option<&'static DirectModel> {
    if crate::peripheral::find(LOGITECH, product_id).is_some() {
        return None;
    }
    table.iter().find(|m| m.product_id == product_id)
}

impl DirectModel {
    /// The kind HID++ 0x0005 function 2 would report, for its page before it has answered.
    pub fn hidpp_kind(&self) -> Option<u8> {
        match self.kind {
            DirectKind::Keyboard => Some(0),
        }
    }
}

/// What its page and test report show of a device that has not answered yet: its name from the
/// table, and `why` among what could not be read.
pub fn not_read(model: &DirectModel, why: String) -> UnsupportedDevice {
    UnsupportedDevice {
        slot: INDEXES[0],
        name: Some(model.name.into()),
        kind: model.hidpp_kind(),
        protocol: "not read".into(),
        wireless_id: None,
        unit_id: None,
        device_info: None,
        firmware: Vec::new(),
        features: Vec::new(),
        dpi: None,
        dpi_extended: None,
        report_rates_ms: Vec::new(),
        report_rates_hz: Vec::new(),
        onboard: None,
        lighting: None,
        layout: None,
        led_zones: Vec::new(),
        battery: None,
        battery_reply: None,
        read_only: true,
        read_errors: vec![why],
        transcript: String::new(),
    }
}

/// The device indexes tried, in order. 0xFF is the index of a device without a receiver: Linux's
/// hid-logitech-hidpp sends every request with it, and Solaar uses it for devices on their own
/// USB connection.
pub const INDEXES: &[u8] = &[WIRED_INDEX];

/// Whether a collection may carry HID++: the vendor usage pages above, or 0 when hidapi did not read
/// the usage (older hidapi on Linux).
pub fn hidpp_collection(usage_page: u16) -> bool {
    matches!(usage_page, USAGE_PAGE_HIDPP | USAGE_PAGE_HIDPP_OLD | 0)
}

/// The order the collections are tried in: 0xFF43 first, then the higher usages, which on the
/// devices seen so far are the long reports' collections.
pub fn try_order(usage_page: u16, usage: u16) -> (bool, Reverse<u16>) {
    (usage_page != USAGE_PAGE_HIDPP, Reverse(usage))
}

/// Asks the device for its HID++ version (a read: root function 1) at each of [`INDEXES`], on a
/// collection `open` opens again for every try, and returns the device at the first index where it
/// answers as HID++ 2.0 or newer, read-only. Every answer goes into `answers` for the test report,
/// as "`label`, index ff: HID++ 4.2". A collection that refuses the write is not asked again.
pub fn connect(
    open: &mut dyn FnMut() -> Result<Box<dyn Transport>>,
    label: &str,
    answers: &mut Vec<String>,
) -> Option<(HidppDevice, u8)> {
    for &index in INDEXES {
        let result = open().and_then(|dev| HidppDevice::new(dev, index, true));
        match result {
            Ok(mut dev) => {
                let version = dev.protocol_version().map(|(major, minor)| format!("HID++ {major}.{minor}"));
                answers.push(format!("{label}, index {index:02x}: {}", version.unwrap_or_else(|e| e.to_string())));
                return Some((dev, index));
            }
            Err(error) => {
                answers.push(format!("{label}, index {index:02x}: {error}"));
                if matches!(error, Error::Hid(_)) {
                    break;
                }
            }
        }
    }
    None
}

impl Transport for Box<dyn Transport> {
    fn write(&self, data: &[u8]) -> Result<()> {
        (**self).write(data)
    }

    fn read_timeout(&self, buf: &mut [u8], millis: i32) -> Result<usize> {
        (**self).read_timeout(buf, millis)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{FakeDevice, reply};
    use super::super::{REPORT_LONG, feature, is_read_call};
    use super::*;

    /// The features of the keyboard below, by index (made up, in the shape of a wired G keyboard).
    const FEATURES: &[u16] = &[
        feature::ROOT,
        feature::FEATURE_SET,
        feature::DEVICE_INFO,
        feature::DEVICE_NAME,
        0x1B04,
        feature::COLOR_LED_EFFECTS,
    ];

    /// A keyboard on its cable that answers HID++ 2.0 at index 0xFF only, with long reports.
    fn keyboard() -> FakeDevice {
        FakeDevice::new(|request| {
            if request[0] != REPORT_LONG || request[1] != WIRED_INDEX {
                return Vec::new();
            }
            let (index, function, param) = (request[2] as usize, request[3] >> 4, request[4]);
            let payload: Vec<u8> = match (FEATURES.get(index).copied(), function) {
                // 11 ff 00 1b 00 00 5a → 11 ff 00 1b 04 02 5a: HID++ 4.2.
                (Some(feature::ROOT), 1) => vec![4, 2, request[6]],
                (Some(feature::ROOT), 0) => {
                    let id = u16::from_be_bytes([request[4], request[5]]);
                    vec![FEATURES.iter().position(|&f| f == id).unwrap_or(0) as u8, 0, 0]
                }
                (Some(feature::FEATURE_SET), 0) => vec![FEATURES.len() as u8 - 1],
                (Some(feature::FEATURE_SET), 1) => {
                    let [high, low] = FEATURES[param as usize].to_be_bytes();
                    vec![high, low, 0, 0]
                }
                // Two entities, unit id 11 22 33 44, USB only (transport bit 3), product id c339.
                (Some(feature::DEVICE_INFO), 0) => vec![2, 0x11, 0x22, 0x33, 0x44, 0x00, 0x08, 0xC3, 0x39],
                (Some(feature::DEVICE_INFO), 1) if param == 0 => vec![0, b'K', b'B', b'D', 0x01, 0x02, 0x00, 0x15],
                (Some(feature::DEVICE_INFO), 1) => vec![1, b'B', b'O', b'T', 0x01, 0x00, 0x00, 0x05],
                (Some(feature::DEVICE_NAME), 0) => vec![8],
                (Some(feature::DEVICE_NAME), 1) => b"PRO K/DA".to_vec(),
                (Some(feature::DEVICE_NAME), 2) => vec![0],
                // Two zones: the keys (location 1) with off, fixed colour and colour cycle; the logo
                // (location 2) with off and fixed colour.
                (Some(feature::COLOR_LED_EFFECTS), 0) => vec![2],
                (Some(feature::COLOR_LED_EFFECTS), 1) => vec![param, 0x00, param + 1, if param == 0 { 3 } else { 2 }],
                (Some(feature::COLOR_LED_EFFECTS), 2) => {
                    vec![param, request[5], 0x00, [0x00, 0x01, 0x03][request[5] as usize]]
                }
                // Anything else: "invalid function".
                _ => {
                    return vec![
                        [&[REPORT_LONG, WIRED_INDEX, 0xFF, request[2], request[3], 0x07][..], &[0; 14]].concat(),
                    ];
                }
            };
            vec![reply(request, &payload)]
        })
    }

    /// A collection that refuses every write, as Windows does for one without the long report.
    struct Refusing;

    impl Transport for Refusing {
        fn write(&self, _: &[u8]) -> Result<()> {
            Err(Error::Hid(hidapi::HidError::HidApiError { message: "The parameter is incorrect.".into() }))
        }

        fn read_timeout(&self, _: &mut [u8], _: i32) -> Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn the_newer_hidpp_collections_are_tried_first() {
        let mut found = [(0xFF00, 0x0002), (0xFF43, 0x0602), (0xFF43, 0x0604), (0x000C, 0x0001)];
        found.sort_by_key(|&(page, usage)| try_order(page, usage));
        assert_eq!(&found[..3], [(0xFF43, 0x0604), (0xFF43, 0x0602), (0xFF00, 0x0002)]);
        assert!(hidpp_collection(0xFF43) && hidpp_collection(0xFF00) && hidpp_collection(0));
        assert!(!hidpp_collection(0x0001) && !hidpp_collection(0x000C));
        assert_eq!(find(0xC339).unwrap().interface, 1);
        // Issue #146: its HID++ collections (ff00) are alone on interface 2, with no typing.
        let rapid = find(0xC35B).unwrap();
        assert_eq!((rapid.interface, rapid.typing), (2, false));
        assert!(find(0xC339).unwrap().typing, "its HID++ interface also carries the keyboard");
    }

    #[test]
    fn a_device_with_a_driver_of_its_own_is_never_a_direct_device() {
        use crate::peripheral::{self, Protocol};
        // The G213 keeps its HID++ in the same usage page (ff43) as the PRO K/DA: the page alone does not tell
        // them apart, so the product id decides, and the G213 is opened by its own driver only.
        let g213 = peripheral::find(LOGITECH, 0xC336).expect("the G213 has a driver");
        assert!(matches!(g213.protocol, Protocol::LogitechG213));
        assert_eq!(g213.usage_page, Some(USAGE_PAGE_HIDPP));
        assert!(hidpp_collection(USAGE_PAGE_HIDPP));
        assert!(find(0xC336).is_none());
        // Even a row for it in the table is ignored.
        static MISTAKE: [DirectModel; 1] = [DirectModel {
            product_id: 0xC336,
            name: "G213",
            kind: DirectKind::Keyboard,
            interface: 1,
            connection: "USB cable",
            typing: true,
        }];
        assert!(find_in(&MISTAKE, 0xC336).is_none());
        // No id is in both tables: every direct model passes, no model with a driver does.
        for model in DIRECT {
            assert!(peripheral::find(LOGITECH, model.product_id).is_none(), "{}", model.name);
            assert_eq!(find(model.product_id), Some(model), "{}", model.name);
        }
        for model in peripheral::models().filter(|m| m.vendor_id == LOGITECH) {
            for &id in model.product_ids {
                assert!(find(id).is_none(), "{} {id:04x}", model.name);
            }
        }
    }

    #[test]
    fn a_collection_that_refuses_the_write_is_not_asked_again() {
        let mut answers = Vec::new();
        let mut open = || Ok(Box::new(Refusing) as Box<dyn Transport>);
        assert!(connect(&mut open, "usage page ff43, usage 0602", &mut answers).is_none());
        assert_eq!(answers.len(), 1);
        assert!(answers[0].starts_with("usage page ff43, usage 0602, index ff: HID error"), "{answers:?}");
    }

    #[test]
    fn a_keyboard_on_its_cable_is_read_with_reads_only() {
        let fake = keyboard();
        let written = fake.written.clone();
        let mut fake = Some(fake);
        let mut open = || Ok(Box::new(fake.take().unwrap()) as Box<dyn Transport>);
        let mut answers = Vec::new();
        let (mut dev, index) = connect(&mut open, "usage page ff43, usage 0604", &mut answers).unwrap();
        assert_eq!(index, 0xFF);
        assert_eq!(answers, ["usage page ff43, usage 0604, index ff: HID++ 4.2"]);

        let mut device = UnsupportedDevice::read(&mut dev, index).unwrap();
        device.read_only = true;
        assert_eq!(device.name.as_deref(), Some("PRO K/DA"));
        assert_eq!((device.kind, device.protocol.as_str()), (Some(0), "4.2"));
        assert_eq!(device.firmware, ["KBD 01.02.0015", "BOT 01.00.0005"]);
        assert_eq!(device.device_info.as_deref(), Some("02 xx xx xx xx 00 08 c3 39 00 00 00 00 00 00 00"));
        assert_eq!(device.wireless_id, None, "no receiver transport");
        let ids: Vec<u16> = device.features.iter().map(|f| f.id).collect();
        assert_eq!(ids, [0x0001, 0x0003, 0x0005, 0x1B04, 0x8070]);
        let zones: Vec<(u16, Vec<u16>)> = device.led_zones.iter().map(|z| (z.location, z.effects.clone())).collect();
        assert_eq!(zones, [(1, vec![0x00, 0x01, 0x03]), (2, vec![0x00, 0x01])]);
        assert_eq!((device.battery, device.battery_reply.as_deref()), (None, None), "no battery on a cable");
        assert!(device.lighting.is_none() && device.dpi.is_none() && device.onboard.is_none());
        assert!(device.read_errors.is_empty(), "{:?}", device.read_errors);
        assert!(!crate::logitech::lights_drivable(&device) && !crate::logitech::dpi_changeable(&device));

        // Long reports to index 0xFF, and only reads: the root, then the features by their index.
        for request in written.lock().unwrap().iter() {
            assert_eq!(&request[..2], [REPORT_LONG, WIRED_INDEX]);
            let (index, function) = (request[2] as usize, request[3] >> 4);
            if index > 0 {
                assert!(is_read_call(FEATURES[index], function, &request[4..]), "{:04x} {function}", FEATURES[index]);
            }
        }
        // Nothing else gets through on this connection.
        assert!(dev.preview_led_effect(&device.led_zones[0], &super::super::profile::LedEffect::Off).is_err());
    }

    #[test]
    fn a_device_that_has_not_answered_shows_its_name_and_why() {
        let device = not_read(find(0xC339).unwrap(), "no HID++ answer yet".into());
        assert_eq!((device.name.as_deref(), device.kind, device.slot), (Some("PRO K/DA"), Some(0), 0xFF));
        assert!(device.read_only);
        assert_eq!(device.read_errors, ["no HID++ answer yet"]);
    }
}

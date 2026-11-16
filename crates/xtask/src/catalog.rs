//! Every device PeriHub recognizes by its USB id, gathered from the drivers' own tables. Each
//! generated file (udev rule, device list, website, AppStream metadata) is written from this list.

use perihub_core::corsair::models as corsair;
use perihub_core::hidpp::{direct, models as hidpp, receiver};
use perihub_core::manager;
use perihub_core::peripheral::{self, Effect, Kind as PeripheralKind, Protocol, task};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Receiver,
}

/// How far PeriHub supports a device: the levels of docs/devices.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Tested on real hardware.
    Supported,
    /// Changes at the user's own risk, under a warning.
    Experimental,
    /// Read, never changed.
    ReadOnly,
    /// A Logitech mouse with onboard profiles: the level follows the memory format it reports.
    ByMemoryFormat,
    /// Named, nothing sent to it.
    Recognized,
}

/// What the Linux udev rule opens to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "interface")]
pub enum Access {
    /// One USB interface.
    Interface(i32),
    /// Every interface of the device: no source names the one with the configuration.
    Whole,
    /// A Logitech receiver's own HID++ interface (02), not the devices Linux creates for what is paired.
    ReceiverHidpp,
    /// Not opened by 70-perihub.rules.
    Closed,
}

#[derive(Debug, Clone)]
pub struct Device {
    /// The section of the device list, and of the udev rule, the device belongs to.
    pub family: &'static str,
    pub brand: &'static str,
    pub name: String,
    pub kind: Kind,
    pub vendor_id: u16,
    /// USB product ids: a wireless mouse may have one on its cable and one through its own receiver.
    pub product_ids: Vec<u16>,
    /// The id a Logitech mouse reports through a LIGHTSPEED receiver.
    pub wireless_id: Option<u16>,
    pub level: Level,
    pub access: Access,
    /// The driver's own description of the model, for the device list.
    pub model: Source,
}

/// Where a device comes from, for the columns of the device list that depend on its driver.
#[derive(Debug, Clone, Copy)]
pub enum Source {
    Apex,
    Logitech,
    Receiver,
    G300,
    Corsair,
    Peripheral(&'static peripheral::Model),
}

const LOGITECH: u16 = 0x046D;
const STEELSERIES: u16 = 0x1038;
const CORSAIR: u16 = 0x1B1C;
/// The Apex 3 TKL's configuration interface (see perihub-core/src/apex).
const APEX_INTERFACE: i32 = 1;

/// The family of a driver's table, as the device list names its section.
pub fn family(protocol: &Protocol) -> Option<&'static str> {
    Some(match protocol {
        Protocol::Asus(_) => "asus",
        Protocol::SteelSeries(_) => "steelseries-mice",
        Protocol::Razer(_) => "razer",
        Protocol::Sinowealth(_) => "glorious",
        Protocol::LogitechG600 => "g600",
        Protocol::LogitechHidpp10(_) => "logitech-hidpp10",
        Protocol::Roccat(_) => "roccat",
        Protocol::Pulsar(_) => "pulsar",
        Protocol::CoolerMaster(_) => "coolermaster",
        Protocol::Nordic(_) => "nordic",
        Protocol::Evga(_) => "evga",
        // Known by their collection and serial number, not by a USB id.
        Protocol::Qmk(_) => return None,
    })
}

/// The level of a device in a driver's table: experimental if the driver may send anything but reads.
pub fn peripheral_level(model: &'static peripheral::Model) -> Level {
    if task::driver(model).commands().iter().all(|c| c.effect == Effect::Read) {
        Level::ReadOnly
    } else {
        Level::Experimental
    }
}

#[allow(clippy::too_many_arguments)]
fn device(
    family: &'static str,
    brand: &'static str,
    name: &str,
    kind: Kind,
    vendor_id: u16,
    product_ids: Vec<u16>,
    level: Level,
    access: Access,
    model: Source,
) -> Device {
    Device {
        family,
        brand,
        name: name.to_owned(),
        kind,
        vendor_id,
        product_ids,
        wireless_id: None,
        level,
        access,
        model,
    }
}

pub fn devices() -> Vec<Device> {
    let mut list = Vec::new();

    list.push(device(
        "apex",
        "SteelSeries",
        "Apex 3 TKL",
        Kind::Keyboard,
        STEELSERIES,
        vec![manager::APEX_3_TKL],
        Level::Supported,
        Access::Interface(APEX_INTERFACE),
        Source::Apex,
    ));
    list.push(device(
        "apex",
        "SteelSeries",
        "Apex 3",
        Kind::Keyboard,
        STEELSERIES,
        vec![manager::APEX_3],
        Level::Recognized,
        Access::Closed,
        Source::Apex,
    ));

    for m in hidpp::MODELS {
        let level = if m.verified { Level::Supported } else { Level::ByMemoryFormat };
        let (ids, access) = if m.has_cable() { (vec![m.product_id], Access::Whole) } else { (vec![], Access::Closed) };
        list.push(device(
            "logitech-onboard",
            "Logitech",
            m.name,
            Kind::Mouse,
            LOGITECH,
            ids,
            level,
            access,
            Source::Logitech,
        ));
        list.last_mut().unwrap().wireless_id = m.wireless_id;
    }
    for m in direct::DIRECT {
        let kind = match m.kind {
            direct::DirectKind::Keyboard => Kind::Keyboard,
            direct::DirectKind::Headset => Kind::Headset,
        };
        // A HID++ interface that also carries typing stays closed on Linux (see `DirectModel::typing`).
        let access = if m.typing { Access::Closed } else { Access::Interface(m.interface) };
        list.push(device(
            "logitech-direct",
            "Logitech",
            m.name,
            kind,
            LOGITECH,
            vec![m.product_id],
            Level::ReadOnly,
            access,
            Source::Logitech,
        ));
    }
    for &id in receiver::RECEIVERS {
        let (family, name, access) = if id == receiver::BOLT {
            ("bolt", "Logi Bolt receiver", Access::ReceiverHidpp)
        } else if receiver::UNIFYING.contains(&id) {
            ("unifying", "Unifying receiver", Access::Closed)
        } else {
            ("lightspeed", "LIGHTSPEED receiver", Access::ReceiverHidpp)
        };
        list.push(device(
            family,
            "Logitech",
            name,
            Kind::Receiver,
            LOGITECH,
            vec![id],
            Level::Experimental,
            access,
            Source::Receiver,
        ));
    }
    list.push(device(
        "g300",
        "Logitech",
        "G300",
        Kind::Mouse,
        LOGITECH,
        vec![perihub_core::g300::PRODUCT_ID],
        Level::Supported,
        Access::Interface(perihub_core::g300::INTERFACE),
        Source::G300,
    ));

    for kind in [corsair::Kind::Mouse, corsair::Kind::Keyboard] {
        for m in corsair::MODELS.iter().filter(|m| m.kind == kind) {
            let (family, kind) = match kind {
                corsair::Kind::Mouse => ("corsair-mouse", Kind::Mouse),
                corsair::Kind::Keyboard => ("corsair-keyboard", Kind::Keyboard),
            };
            list.push(device(
                family,
                "Corsair",
                m.name,
                kind,
                CORSAIR,
                vec![m.product_id],
                Level::Experimental,
                Access::Interface(perihub_core::corsair::CONTROL_INTERFACE),
                Source::Corsair,
            ));
        }
    }
    for &(id, name) in corsair::RECEIVERS {
        list.push(device(
            "corsair-receiver",
            "Corsair",
            name,
            Kind::Receiver,
            CORSAIR,
            vec![id],
            Level::Experimental,
            Access::Interface(perihub_core::corsair::CONTROL_INTERFACE),
            Source::Corsair,
        ));
    }

    for m in peripheral::models() {
        let Some(family) = family(&m.protocol) else { continue };
        let kind = match m.kind {
            PeripheralKind::Mouse => Kind::Mouse,
            PeripheralKind::Keyboard => Kind::Keyboard,
        };
        let access = m.interface.map_or(Access::Whole, Access::Interface);
        list.push(device(
            family,
            m.brand,
            m.name,
            kind,
            m.vendor_id,
            m.product_ids.to_vec(),
            peripheral_level(m),
            access,
            Source::Peripheral(m),
        ));
    }
    list
}

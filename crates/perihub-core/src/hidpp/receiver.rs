//! LIGHTSPEED and Logi Bolt receivers. A wireless mouse is reached through the receiver, with the
//! receiver's slot (1–6) as the HID++ device index.
//!
//! PeriHub only talks to the mice behind a receiver, with the same rules as for a mouse on a
//! cable. It never sends the receiver's own commands (HID++ 1.0 registers), which also pair and
//! unpair devices. Receiver ids: Linux's hid-logitech-dj and Solaar (ids only, no code).

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{
    FeatureEntry, HIDPP10_ERROR, HidppDevice, REPORT_LONG, REPORT_SHORT, SHORT_LEN, SOFTWARE_ID, Transport,
    unreachable_code,
};
use crate::error::{Error, Result};

pub const RECEIVERS: &[u16] = &[0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC543, 0xC545, 0xC547, 0xC54D, BOLT];

/// The Logi Bolt receiver, for the MX and other office mice and keyboards. PeriHub does not support
/// any device behind it yet: it only describes them in a test report (issue #17).
pub const BOLT: u16 = 0xC548;

/// Slots a receiver can have. LIGHTSPEED receivers use the first one or two.
pub const SLOTS: RangeInclusive<u8> = 1..=6;

/// The receiver answers at once, also for a mouse that is asleep.
const PING_TIMEOUT: Duration = Duration::from_millis(300);

pub fn is_receiver(product_id: u16) -> bool {
    RECEIVERS.contains(&product_id)
}

/// What the receiver is called in messages and test reports.
pub fn name(product_id: u16) -> &'static str {
    if product_id == BOLT { "Logi Bolt receiver" } else { "LIGHTSPEED receiver" }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// A HID++ 2.0 device answers.
    Online,
    /// A device is paired in the slot but turned off or asleep.
    Asleep,
    /// Nothing usable in the slot: no device, or a HID++ 1.0 one.
    Empty,
}

/// Asks the device in `slot` for its HID++ version, which only reads. The request is a short
/// report, which the receiver answers for every slot: on Windows `channel` must be the
/// receiver's short HID++ collection.
pub fn ping(channel: &dyn Transport, slot: u8) -> Result<Presence> {
    Ok(ping_answer(channel, slot)?.0)
}

/// [`ping`], with what the slot answered, for test reports: the device's HID++ version, or the
/// receiver's HID++ 1.0 error code.
pub fn ping_answer(channel: &dyn Transport, slot: u8) -> Result<(Presence, String)> {
    const MARK: u8 = 0x5A;
    let address = (1 << 4) | SOFTWARE_ID;
    let mut request = [0u8; SHORT_LEN];
    request[..4].copy_from_slice(&[REPORT_SHORT, slot, 0, address]);
    request[6] = MARK;
    channel.write(&request)?;

    let deadline = Instant::now() + PING_TIMEOUT;
    let mut buf = [0u8; 64];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::Timeout);
        }
        let n = channel.read_timeout(&mut buf, remaining.as_millis().max(1) as i32)?;
        if n < SHORT_LEN || !(buf[0] == REPORT_SHORT || buf[0] == REPORT_LONG) || buf[1] != slot {
            continue;
        }
        if buf[2] == 0 && buf[3] == address && buf[6] == MARK {
            let presence = if buf[4] >= 2 { Presence::Online } else { Presence::Empty };
            return Ok((presence, format!("HID++ {}.{}", buf[4], buf[5])));
        }
        if buf[0] == REPORT_SHORT && buf[2] == HIDPP10_ERROR && buf[3] == 0 && buf[4] == address {
            // 0x08 is an empty slot, 0x01 a HID++ 1.0 device.
            let asleep = unreachable_code(buf[5]) && buf[5] != 0x08;
            let presence = if asleep { Presence::Asleep } else { Presence::Empty };
            return Ok((presence, format!("HID++ 1.0 error {:#04x}", buf[5])));
        }
    }
}

/// A device in a receiver's slot that PeriHub does not support, as far as it can be read without
/// changing anything: what a test report needs to add it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnsupportedDevice {
    pub slot: u8,
    pub name: Option<String>,
    /// HID++ version, such as "4.5".
    pub protocol: String,
    /// The id it reports through a LIGHTSPEED receiver, if it lists one.
    pub wireless_id: Option<u16>,
    /// 0x0003 function 0 as read, in hex: entity count, unit id, transport bits and product ids.
    /// The unit id identifies the single device, so it is left out ("xx"): the report is public.
    pub device_info: Option<String>,
    pub firmware: Vec<String>,
    pub features: Vec<FeatureEntry>,
    /// What could not be read.
    pub read_errors: Vec<String>,
}

impl UnsupportedDevice {
    /// Reads the device in `slot` with read commands only. A device that falls asleep meanwhile
    /// gives [`Error::Unreachable`], to be read again when it wakes up.
    pub fn read(dev: &mut HidppDevice, slot: u8) -> Result<Self> {
        let (major, minor) = dev.protocol_version()?;
        let mut device = Self {
            slot,
            name: None,
            protocol: format!("{major}.{minor}"),
            wireless_id: None,
            device_info: None,
            firmware: Vec::new(),
            features: Vec::new(),
            read_errors: Vec::new(),
        };
        let mut failed = |what: &str, error: Error| match error {
            Error::Unreachable => Err(Error::Unreachable),
            error => {
                device.read_errors.push(format!("{what}: {error}"));
                Ok(())
            }
        };
        match dev.device_name() {
            Ok(name) => device.name = Some(name),
            Err(error) => failed("name", error)?,
        }
        match dev.device_info() {
            Ok(info) => {
                let bytes = info.iter().enumerate();
                let hex = bytes.map(|(i, b)| if (1..5).contains(&i) { "xx".into() } else { format!("{b:02x}") });
                device.device_info = Some(hex.collect::<Vec<_>>().join(" "));
                device.wireless_id = super::wireless_id_from_device_info(&info);
            }
            Err(error) => failed("device info", error)?,
        }
        match dev.firmware() {
            Ok(firmware) => device.firmware = firmware.iter().map(|f| format!("{} {}", f.name, f.version)).collect(),
            Err(error) => failed("firmware", error)?,
        }
        match dev.feature_table() {
            Ok(features) => device.features = features,
            Err(error) => failed("features", error)?,
        }
        Ok(device)
    }

    /// The device in a sentence, such as "MX Master 3S (b034)".
    pub fn label(&self) -> String {
        let name = self.name.clone().unwrap_or_else(|| "a device".into());
        match self.wireless_id {
            Some(id) => format!("{name} ({id:04x})"),
            None => name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{FakeDevice, receiver_error, reply};
    use super::*;

    /// A receiver with an online mouse in slot 1 and one asleep in slot 2.
    fn receiver() -> FakeDevice {
        FakeDevice::new(|request| match request[1] {
            1 => vec![reply(request, &[4, 2, request[6]])],
            2 => vec![receiver_error(request, 0x09)],
            _ => vec![receiver_error(request, 0x08)],
        })
    }

    #[test]
    fn ping_tells_online_asleep_and_empty_slots_apart() {
        let receiver = receiver();
        assert_eq!(ping(&receiver, 1).unwrap(), Presence::Online);
        assert_eq!(ping(&receiver, 2).unwrap(), Presence::Asleep);
        assert_eq!(ping(&receiver, 3).unwrap(), Presence::Empty);
        assert_eq!(ping_answer(&receiver, 1).unwrap().1, "HID++ 4.2");
        assert_eq!(ping_answer(&receiver, 2).unwrap().1, "HID++ 1.0 error 0x09");
    }

    #[test]
    fn ping_only_sends_a_short_hidpp_20_version_request() {
        let receiver = receiver();
        for slot in SLOTS {
            ping(&receiver, slot).unwrap();
        }
        for (request, slot) in receiver.written.lock().unwrap().iter().zip(SLOTS) {
            // Short report, the slot, root feature (0), function 1 (version): never a register.
            assert_eq!(request[..4], [REPORT_SHORT, slot, 0, 0x1B]);
            assert_eq!(request.len(), SHORT_LEN);
        }
    }

    /// A device on a Logi Bolt receiver, in slot 2, that lists its product id under Bluetooth Low
    /// Energy only (the id here is made up).
    fn bolt_device() -> FakeDevice {
        use super::super::feature;
        const FEATURES: &[u16] =
            &[feature::ROOT, feature::FEATURE_SET, feature::DEVICE_INFO, feature::DEVICE_NAME, 0x2201];
        const NAME: &[u8] = b"Office mouse with a long name";
        FakeDevice::new(|request| {
            let (index, function, param) = (request[2] as usize, request[3] >> 4, request[4] as usize);
            if request[1] != 2 {
                return vec![receiver_error(request, 0x08)];
            }
            let payload: Vec<u8> = match (FEATURES.get(index).copied(), function) {
                (Some(feature::ROOT), 1) => vec![4, 5, request[6]],
                (Some(feature::ROOT), 0) => {
                    let id = u16::from_be_bytes([request[4], request[5]]);
                    vec![FEATURES.iter().position(|&f| f == id).unwrap_or(0) as u8]
                }
                (Some(feature::FEATURE_SET), 0) => vec![FEATURES.len() as u8 - 1],
                (Some(feature::FEATURE_SET), 1) => {
                    let [high, low] = FEATURES[param].to_be_bytes();
                    vec![high, low, if FEATURES[param] == 0x2201 { 0x40 } else { 0 }, 1]
                }
                (Some(feature::DEVICE_INFO), 0) => vec![1, 0xA1, 0xB2, 0xC3, 0xD4, 0, 0x02, 0x12, 0x34],
                (Some(feature::DEVICE_INFO), 1) => vec![0, b'R', b'B', b'M', 0x12, 0x01, 0x00, 0x08],
                (Some(feature::DEVICE_NAME), 0) => vec![NAME.len() as u8],
                (Some(feature::DEVICE_NAME), 1) => NAME[param..(param + 16).min(NAME.len())].to_vec(),
                _ => {
                    return vec![vec![
                        REPORT_LONG,
                        2,
                        0xFF,
                        request[2],
                        request[3],
                        0x02,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ]];
                }
            };
            vec![reply(request, &payload)]
        })
    }

    #[test]
    fn a_bolt_receiver_is_named_apart_from_the_lightspeed_ones() {
        assert!(is_receiver(BOLT));
        assert_eq!(name(BOLT), "Logi Bolt receiver");
        assert_eq!(name(0xC539), "LIGHTSPEED receiver");
    }

    #[test]
    fn an_unsupported_device_is_described_with_read_commands_only() {
        let fake = bolt_device();
        let written = fake.written.clone();
        let mut dev = HidppDevice::behind_receiver(fake, None, 2, true).unwrap();
        let device = UnsupportedDevice::read(&mut dev, 2).unwrap();
        assert_eq!(device.name.as_deref(), Some("Office mouse with a long name"));
        assert_eq!(device.protocol, "4.5");
        // No LIGHTSPEED id: only the Bluetooth Low Energy bit is set. The raw bytes keep it.
        assert_eq!(device.wireless_id, None);
        // The unit id (bytes 1 to 4) stays out of the public report.
        assert!(device.device_info.as_deref().unwrap().starts_with("01 xx xx xx xx 00 02 12 34"));
        assert_eq!(device.firmware, ["RBM 12.01.0008"]);
        let ids: Vec<u16> = device.features.iter().map(|f| f.id).collect();
        assert_eq!(ids, [0x0001, 0x0003, 0x0005, 0x2201]);
        assert_eq!(device.features[3].kind, 0x40);
        assert!(device.read_errors.is_empty(), "{:?}", device.read_errors);
        assert_eq!(device.label(), "Office mouse with a long name");

        // Only the root, the feature table, device info and name were asked; 0x2201 (DPI) was
        // listed but never called, and no request went to the receiver itself.
        for request in written.lock().unwrap().iter() {
            assert_eq!(request[1], 2);
            assert!(request[2] <= 3, "feature index {} was called", request[2]);
        }
    }

    #[test]
    fn behind_a_receiver_only_hidpp_20_reads_reach_a_read_only_mouse() {
        use super::super::{HidppDevice, feature};
        const FEATURES: &[u16] = &[feature::ROOT, feature::ONBOARD_PROFILES];
        let fake = super::super::tests::fake_mouse(FEATURES, |_, _| vec![0; 16]);
        let written = fake.written.clone();
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        assert!(dev.set_onboard_mode(1).is_err());
        assert!(dev.set_active_profile(false, 1).is_err());
        dev.onboard_mode().unwrap();
        // Every request went to slot 1 as HID++ 2.0 (a feature index), none as a receiver register
        // (sub ids 0x80–0x83 to the receiver itself, index 0xFF).
        for request in written.lock().unwrap().iter() {
            assert_eq!(request[1], 1);
            assert!(!(0x80..=0x83).contains(&request[2]));
        }
    }
}

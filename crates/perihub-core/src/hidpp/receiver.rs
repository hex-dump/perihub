//! LIGHTSPEED receivers. A wireless mouse is reached through the receiver, with the receiver's
//! slot (1–6) as the HID++ device index.
//!
//! PeriHub only talks to the mice behind a receiver, with the same rules as for a mouse on a
//! cable. It never sends the receiver's own commands (HID++ 1.0 registers), which also pair and
//! unpair devices. Receiver ids: Linux's hid-logitech-dj and Solaar (ids only, no code).

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use super::{HIDPP10_ERROR, REPORT_LONG, REPORT_SHORT, SHORT_LEN, SOFTWARE_ID, Transport, unreachable_code};
use crate::error::{Error, Result};

pub const RECEIVERS: &[u16] = &[0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC543, 0xC545, 0xC547, 0xC54D];

/// Slots a receiver can have. LIGHTSPEED receivers use the first one or two.
pub const SLOTS: RangeInclusive<u8> = 1..=6;

/// The receiver answers at once, also for a mouse that is asleep.
const PING_TIMEOUT: Duration = Duration::from_millis(300);

pub fn is_receiver(product_id: u16) -> bool {
    RECEIVERS.contains(&product_id)
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
            return Ok(if buf[4] >= 2 { Presence::Online } else { Presence::Empty });
        }
        if buf[0] == REPORT_SHORT && buf[2] == HIDPP10_ERROR && buf[3] == 0 && buf[4] == address {
            // 0x08 is an empty slot, 0x01 a HID++ 1.0 device.
            let asleep = unreachable_code(buf[5]) && buf[5] != 0x08;
            return Ok(if asleep { Presence::Asleep } else { Presence::Empty });
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

    #[test]
    fn behind_a_receiver_only_hidpp_20_reads_reach_a_read_only_mouse() {
        use super::super::{HidppDevice, feature};
        const FEATURES: &[u16] = &[feature::ROOT, feature::ONBOARD_PROFILES];
        let fake = super::super::tests::fake_mouse(FEATURES, |_, _| vec![0; 16]);
        let written = fake.written.clone();
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        assert!(dev.set_onboard_mode(1).is_err());
        assert!(dev.set_active_profile(1).is_err());
        dev.onboard_mode().unwrap();
        // Every request went to slot 1 as HID++ 2.0 (a feature index), none as a receiver register
        // (sub ids 0x80–0x83 to the receiver itself, index 0xFF).
        for request in written.lock().unwrap().iter() {
            assert_eq!(request[1], 1);
            assert!(!(0x80..=0x83).contains(&request[2]));
        }
    }
}

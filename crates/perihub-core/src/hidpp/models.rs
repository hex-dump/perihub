//! Logitech mice that PeriHub recognizes, with a USB cable or through a LIGHTSPEED receiver.
//!
//! The list comes from libratbag's device files (MIT, commit 8235b5b): only G-series
//! mice and the MX518, which store their settings in onboard profiles (HID++ 0x8100).
//! Models that have not been tested on real hardware are read-only: see [`MouseModel::verified`].

/// HID++ device index for devices connected directly over USB (no receiver).
pub const WIRED_INDEX: u8 = 0xFF;

pub const G502_HERO: u16 = 0xC08B;
pub const G502_X_WIRELESS: u16 = 0xC098;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseModel {
    /// USB product id with the cable. Mice without one use their wireless id here.
    pub product_id: u16,
    /// Id the mouse reports through a LIGHTSPEED receiver (libratbag's second `DeviceMatch`).
    pub wireless_id: Option<u16>,
    /// Id the mouse has over Bluetooth Low Energy, where it is a Bluetooth HID device of its own (libratbag's
    /// `bluetooth:` `DeviceMatch`). PeriHub only reads a mouse connected that way
    /// (`docs/protocols/logitech-onboard-profiles.md`).
    pub bluetooth_id: Option<u16>,
    pub name: &'static str,
    /// Device index the mouse answers on (libratbag's `DeviceIndex`).
    pub device_index: u8,
    /// Tested with PeriHub on a real device. Unverified models only get read commands.
    pub verified: bool,
}

impl MouseModel {
    /// Whether the mouse also connects with a USB cable, as its own USB device.
    pub fn has_cable(&self) -> bool {
        self.wireless_id != Some(self.product_id)
    }

    /// Folder name for backups. The G502 HERO keeps the one used before other models existed.
    pub fn backup_dir(&self) -> String {
        if self.product_id == G502_HERO { "g502".into() } else { format!("logitech-{:04x}", self.product_id) }
    }
}

const fn model(product_id: u16, name: &'static str) -> MouseModel {
    MouseModel { product_id, wireless_id: None, bluetooth_id: None, name, device_index: WIRED_INDEX, verified: false }
}

/// A mouse that connects with a cable and through a LIGHTSPEED receiver.
const fn wireless(product_id: u16, wireless_id: u16, name: &'static str) -> MouseModel {
    MouseModel { wireless_id: Some(wireless_id), ..model(product_id, name) }
}

/// A mouse without a data cable: only through a LIGHTSPEED receiver.
const fn receiver_only(wireless_id: u16, name: &'static str) -> MouseModel {
    wireless(wireless_id, wireless_id, name)
}

pub const MODELS: &[MouseModel] = &[
    MouseModel { verified: true, ..model(G502_HERO, "G502 HERO") },
    model(0xC07D, "G502 Proteus Core"),
    model(0xC332, "G502 Proteus Spectrum"),
    wireless(0xC08D, 0x407F, "G502 HERO Wireless"),
    model(0xC099, "G502 X"),
    MouseModel { verified: true, ..wireless(G502_X_WIRELESS, 0x409F, "G502 X Wireless") },
    wireless(0xC095, 0x4099, "G502 X PLUS"),
    model(0xC085, "G Pro"),
    model(0xC08C, "G Pro"),
    MouseModel { device_index: 1, ..wireless(0xC088, 0x4079, "G Pro Wireless") },
    wireless(0xC094, 0x4093, "G Pro X Superlight"),
    model(0xC084, "G102/G103/G203"),
    model(0xC092, "G102/G103/G203"),
    model(0xC09D, "G102/G103/G203"),
    model(0xC07F, "G302"),
    model(0xC080, "G303"),
    // Wireless id from a test report (issue #34); libratbag lists the cable id only.
    wireless(0xC097, 0x409E, "G303 Shroud Edition"),
    model(0xC07E, "G402"),
    model(0xC083, "G403"),
    model(0xC08F, "G403 HERO"),
    wireless(0xC082, 0x405D, "G403 Wireless"),
    wireless(0xC087, 0x4070, "G703"),
    wireless(0xC090, 0x4086, "G703 HERO"),
    wireless(0xC096, 0x409D, "G705"),
    wireless(0xC081, 0x4053, "G900"),
    wireless(0xC086, 0x4067, "G903"),
    wireless(0xC091, 0x4087, "G903 HERO"),
    model(0xC08E, "MX518"),
    // The two PRO mice below come from test reports (issues #33 and #32), not from libratbag: its device
    // files (commit 8235b5b) do not list them. Open pull request #1676 there names the first one and
    // its cable id 0xC09A. They use profile format 7, which PeriHub only reads.
    wireless(0xC09A, 0x40A8, "PRO 2 LIGHTSPEED"),
    wireless(0xC0A0, 0x40B8, "PRO X 2 DEX"),
    receiver_only(0x4074, "G305"),
    receiver_only(0x406C, "G603"),
    // libratbag's device file for the G604 lists its Bluetooth id too; it is the id the request of issue #43 shows.
    MouseModel { bluetooth_id: Some(0xB024), ..receiver_only(0x4085, "G604") },
];

/// The model of a mouse connected with its USB cable.
pub fn find(product_id: u16) -> Option<&'static MouseModel> {
    MODELS.iter().find(|m| m.product_id == product_id && m.has_cable())
}

/// The model of a mouse behind a LIGHTSPEED receiver, from the id it reports there.
pub fn find_wireless(wireless_id: u16) -> Option<&'static MouseModel> {
    MODELS.iter().find(|m| m.wireless_id == Some(wireless_id))
}

/// The model of a mouse connected over Bluetooth Low Energy, from the product id it has there.
pub fn find_bluetooth(bluetooth_id: u16) -> Option<&'static MouseModel> {
    MODELS.iter().find(|m| m.bluetooth_id == Some(bluetooth_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_ids_are_unique() {
        for (i, a) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|b| b.product_id != a.product_id), "{:04x}", a.product_id);
        }
    }

    #[test]
    fn wireless_ids_are_unique_and_not_usb_ids() {
        let ids: Vec<u16> = MODELS.iter().filter_map(|m| m.wireless_id).collect();
        for (i, a) in ids.iter().enumerate() {
            assert!(!ids[i + 1..].contains(a), "{a:04x}");
            assert!(find(*a).is_none(), "{a:04x} must not match a USB device");
        }
        assert_eq!(find_wireless(0x4093).unwrap().name, "G Pro X Superlight");
        assert_eq!(find_wireless(0x4074).unwrap().name, "G305");
        assert_eq!(find(0xC094).unwrap().name, "G Pro X Superlight");
        // Wireless ids read from the mice themselves (issues #32, #33, #34).
        assert_eq!(find_wireless(0x409E).unwrap().name, "G303 Shroud Edition");
        assert_eq!(find_wireless(0x40A8).unwrap().name, "PRO 2 LIGHTSPEED");
        assert_eq!(find_wireless(0x40B8).unwrap().name, "PRO X 2 DEX");
        assert_eq!(find(0xC0A0).unwrap().name, "PRO X 2 DEX");
    }

    #[test]
    fn bluetooth_ids_are_unique_and_belong_to_no_other_connection() {
        let ids: Vec<u16> = MODELS.iter().filter_map(|m| m.bluetooth_id).collect();
        for (i, a) in ids.iter().enumerate() {
            assert!(!ids[i + 1..].contains(a), "{a:04x}");
            assert!(find(*a).is_none() && find_wireless(*a).is_none(), "{a:04x} is the id of another connection");
            assert!(!super::super::receiver::is_receiver(*a), "{a:04x}");
        }
        // The id of issue #43, and the receiver id the same mouse reports through its LIGHTSPEED receiver.
        let g604 = find_bluetooth(0xB024).unwrap();
        assert_eq!(g604.name, "G604");
        assert_eq!(find_wireless(0x4085).unwrap().name, g604.name);
        assert!(!g604.has_cable(), "the G604 has no data cable");
        assert!(find_bluetooth(0x4085).is_none() && find_bluetooth(0xC08B).is_none());
    }

    #[test]
    fn verified_models() {
        let verified: Vec<_> = MODELS.iter().filter(|m| m.verified).map(|m| m.product_id).collect();
        assert_eq!(verified, [G502_HERO, G502_X_WIRELESS]);
        assert_eq!(find(G502_HERO).unwrap().backup_dir(), "g502");
        assert_eq!(find(G502_X_WIRELESS).unwrap().backup_dir(), "logitech-c098");
        assert_eq!(find(0xC094).unwrap().backup_dir(), "logitech-c094");
    }
}

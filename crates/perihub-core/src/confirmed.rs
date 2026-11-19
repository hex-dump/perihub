//! What testers confirmed on real hardware, as data: the one table `cargo xtask confirm` edits.
//!
//! - A device of another brand or a Corsair device is **confirmed** by its USB id: the catalogue
//!   lists it as supported and the interface shows it as tested (`tested` on its info).
//! - A Logitech mouse is confirmed by the memory format it reports (profile format and sector
//!   size), not by its model: any mouse with a confirmed format counts as tested (see
//!   `mouse::support`).
//!
//! Every entry names the GitHub issue whose report confirmed it; 0 means before issues were used.
//! The tables are sorted and one entry per line (`cargo xtask confirm` keeps them so).

/// A USB id a tester confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbConfirmation {
    pub vendor_id: u16,
    pub product_id: u16,
    pub issue: u32,
}

/// A Logitech memory format a tester saved to on a real mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryConfirmation {
    pub profile_format: u8,
    pub sector_size: u16,
    pub issue: u32,
}

// Unused while no id is confirmed: the table below gets its entries from `cargo xtask confirm`.
#[allow(dead_code)]
const fn usb(vendor_id: u16, product_id: u16, issue: u32) -> UsbConfirmation {
    UsbConfirmation { vendor_id, product_id, issue }
}

const fn memory(profile_format: u8, sector_size: u16, issue: u32) -> MemoryConfirmation {
    MemoryConfirmation { profile_format, sector_size, issue }
}

/// Devices of other brands and Corsair devices, by USB id (vendor, product).
#[rustfmt::skip]
pub const USB: &[UsbConfirmation] = &[
];

/// Memories PeriHub has saved to on real mice, with their USB cable: format 2 in 256-byte sectors on
/// the G502 HERO (before issues were used), format 3 in 255-byte sectors on the G502 X Wireless of
/// issue #15. The memory format decides how a save is laid out, not the model: any mouse whose
/// memory reports one of these counts as tested. The same model can change format with its
/// firmware, so it is read from the mouse every time.
#[rustfmt::skip]
pub const MEMORY: &[MemoryConfirmation] = &[
    memory(2, 256, 0),
    memory(3, 255, 15),
];

/// Whether a tester confirmed the device with this USB id.
pub fn usb_confirmed(vendor_id: u16, product_id: u16) -> bool {
    USB.iter().any(|c| c.vendor_id == vendor_id && c.product_id == product_id)
}

/// Whether a tester confirmed a model: any of its USB ids (a wireless mouse has one on its cable and
/// one through its receiver).
pub fn model_confirmed(vendor_id: u16, product_ids: &[u16]) -> bool {
    product_ids.iter().any(|&id| usb_confirmed(vendor_id, id))
}

/// Whether a Logitech memory format (profile format, sector size) was saved to on a real mouse.
pub fn memory_tested(profile_format: u8, sector_size: u16) -> bool {
    MEMORY.iter().any(|c| c.profile_format == profile_format && c.sector_size == sector_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corsair::{self, models as corsair_models};
    use crate::peripheral;

    #[test]
    fn the_two_memories_tested_before_the_table_are_in_it() {
        // Later confirmations add entries: these two stay.
        assert!(memory_tested(2, 256) && memory_tested(3, 255));
        assert!(!memory_tested(2, 0) && !memory_tested(0, 256));
        let issues: Vec<_> = MEMORY
            .iter()
            .filter(|c| (c.profile_format, c.sector_size) == (2, 256) || (c.profile_format, c.sector_size) == (3, 255))
            .map(|c| c.issue)
            .collect();
        assert_eq!(issues, [0, 15]);
    }

    #[test]
    fn tables_are_sorted_without_repeats() {
        let usb: Vec<_> = USB.iter().map(|c| (c.vendor_id, c.product_id)).collect();
        assert!(usb.windows(2).all(|w| w[0] < w[1]), "{usb:04x?}");
        let memory: Vec<_> = MEMORY.iter().map(|c| (c.profile_format, c.sector_size)).collect();
        assert!(memory.windows(2).all(|w| w[0] < w[1]), "{memory:?}");
    }

    #[test]
    fn every_confirmed_id_is_a_device_peripheral_or_corsair_knows() {
        for c in USB {
            let known = peripheral::find(c.vendor_id, c.product_id).is_some()
                || (c.vendor_id == corsair::CORSAIR
                    && (corsair_models::MODELS.iter().any(|m| m.product_id == c.product_id)
                        || corsair_models::RECEIVERS.iter().any(|&(id, _)| id == c.product_id)));
            assert!(known, "{:04x}:{:04x} is in no driver table", c.vendor_id, c.product_id);
            assert!(c.issue > 0, "{:04x}:{:04x} has no issue", c.vendor_id, c.product_id);
        }
    }
}

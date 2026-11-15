//! The AppStream metadata's `<provides>`: the USB ids of the devices PeriHub supports, so that
//! software centres suggest it when one of them is plugged in.

use crate::catalog::{Access, Device, Level};
use crate::select::{block, fill, select};

pub const PATH: &str = "packaging/linux/io.github.hex_dump.perihub.metainfo.xml";

pub fn render(old: &str, devices: &[Device]) -> String {
    fill(old, PATH, |kind, selector, indent| match kind {
        "modalias" => {
            let mut ids: Vec<(u16, u16)> = select(devices, selector)
                .into_iter()
                // A device PeriHub only names, or does not open on Linux, is no reason to suggest it.
                .filter(|d| d.level != Level::Recognized && d.access != Access::Closed)
                .flat_map(|d| d.product_ids.iter().map(|&id| (d.vendor_id, id)))
                .collect();
            ids.sort_unstable();
            ids.dedup();
            block(ids.into_iter().map(|(v, p)| format!("<modalias>usb:v{v:04X}p{p:04X}d*</modalias>")), indent)
        }
        _ => panic!("{PATH}: unknown generated block {kind:?}"),
    })
}

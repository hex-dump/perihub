//! USB recordings of what a device's official software sends it, for a device request
//! (docs/captures.md, "Let PeriHub record it").
//!
//! The capture is the system's own: USBPcap on Windows, installed only for the recording and removed
//! after it, and usbmon on Linux (see [`record`]). This module reads what they give ([`pcap`]),
//! keeps only the device asked for ([`Collector`] while recording, [`summary`] at the end), and
//! writes its transfers as short lines, step by step. Left out, always: every other device on the
//! bus, sound and video (isochronous transfers), the string descriptors that can hold the serial
//! number, and the reports a device sends that are not vendor-defined (key presses, mouse
//! movements), also when what an interface carries is not known.

pub mod descriptors;
pub mod packet;
pub mod pcap;
pub mod record;
pub mod summary;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use packet::Kind;
pub use pcap::{Frame, Link};
pub use summary::{Step, Summary, Target, summarize};

/// The most a recording keeps in memory; beyond it the rest is dropped and the text says so.
pub const MAX_BYTES: usize = 64 << 20;

/// What is kept while recording, as frames arrive: the device's frames, and before the device is
/// known only control transfers (where its device descriptor comes), never another device's
/// reports. Sound and video are dropped at once.
#[derive(Debug)]
pub struct Collector {
    link: Link,
    vendor_id: u16,
    product_id: u16,
    /// Bus and address of the device, as the system said or as found in the recording.
    pub addresses: BTreeSet<(u16, u16)>,
    pub frames: Vec<Frame>,
    bytes: usize,
    /// More than [`MAX_BYTES`] came: the end is missing.
    pub cut: bool,
}

impl Collector {
    pub fn new(link: Link, vendor_id: u16, product_id: u16, addresses: impl IntoIterator<Item = (u16, u16)>) -> Self {
        Self {
            link,
            vendor_id,
            product_id,
            addresses: addresses.into_iter().collect(),
            frames: Vec::new(),
            bytes: 0,
            cut: false,
        }
    }

    pub fn push(&mut self, frame: Frame) {
        let Some(p) = packet::parse(self.link, &frame) else { return };
        if p.kind == Kind::Isochronous {
            return;
        }
        // A device descriptor with the device's ids, from the device (also after it was plugged in again).
        if p.kind == Kind::Control
            && p.completion
            && let Some(d) = descriptors::device_descriptor(p.data)
            && (d.vendor_id, d.product_id) == (self.vendor_id, self.product_id)
        {
            self.addresses.insert((p.bus, p.address));
        }
        let mine = self.addresses.contains(&(p.bus, p.address));
        let keep = mine || (p.kind == Kind::Control && self.addresses.is_empty());
        if !keep {
            return;
        }
        if self.bytes + frame.data.len() > MAX_BYTES {
            self.cut = true;
            return;
        }
        self.bytes += frame.data.len();
        self.frames.push(frame);
    }

    /// The frames of the device only (and the control frames before it was found).
    pub fn finish(mut self) -> (Vec<Frame>, BTreeSet<(u16, u16)>, bool) {
        self.frames.sort_by_key(|f| f.time_us);
        (self.frames, self.addresses, self.cut)
    }
}

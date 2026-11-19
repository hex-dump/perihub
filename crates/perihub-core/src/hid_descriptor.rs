//! A summary of a HID report descriptor, for a device request: which report ids each top-level
//! collection has, and how long its input, output and feature reports are. Only the descriptor is
//! read (it says what a device sends and accepts, never what is typed on it), so a maintainer sees
//! the shape of a device's reports without asking its owner to run anything.

use std::collections::BTreeMap;

use serde::Serialize;

/// The longest descriptor read: the HID specification's limit for a descriptor is 65535 bytes, and
/// real ones are well under 1 KiB.
pub const MAX_DESCRIPTOR: usize = 4096;

/// The lengths of one report id, in bytes, with the id byte when there is one (what a HID library
/// reads and writes). 0: the report does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Reports {
    pub id: u8,
    pub input: u32,
    pub output: u32,
    pub feature: u32,
}

/// One top-level (application) collection: its usage and its reports, by report id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopCollection {
    pub usage_page: u16,
    pub usage: u16,
    pub reports: Vec<Reports>,
}

/// The globals a push and a pop save and restore.
#[derive(Clone, Copy, Default)]
struct Globals {
    page: u16,
    size: u32,
    count: u32,
    id: u8,
}

/// The report ids of every top-level collection, or [None] when the descriptor is empty, cut short
/// or does not balance its collections.
pub fn summarize(descriptor: &[u8]) -> Option<Vec<TopCollection>> {
    let mut found: Vec<(TopCollection, BTreeMap<u8, [u32; 3]>)> = Vec::new();
    let (mut globals, mut stack) = (Globals::default(), Vec::new());
    let mut usage: Option<(u16, u16)> = None;
    let mut depth = 0u32;
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xFE {
            // A long item: its size is in the next byte.
            i += 3 + *descriptor.get(i + 1)? as usize;
            if i > descriptor.len() {
                return None;
            }
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let data = descriptor.get(i + 1..i + 1 + size)?;
        let value = data.iter().rev().fold(0u32, |v, &b| (v << 8) | u32::from(b));
        i += 1 + size;
        let tag = prefix >> 4;
        match (prefix >> 2) & 0x03 {
            // Main
            0 => {
                match tag {
                    // Input, Output, Feature
                    0x8 | 0x9 | 0xB => {
                        if let Some((_, bits)) = found.last_mut().filter(|_| depth > 0) {
                            let slot = bits.entry(globals.id).or_default();
                            slot[[0x8, 0x9, 0xB].iter().position(|&t| t == tag)?] += globals.size * globals.count;
                        }
                    }
                    0xA => {
                        if depth == 0 {
                            let (usage_page, usage) = usage.unwrap_or((globals.page, 0));
                            found.push((TopCollection { usage_page, usage, reports: Vec::new() }, BTreeMap::new()));
                        }
                        depth += 1;
                    }
                    0xC => depth = depth.checked_sub(1)?,
                    _ => {}
                }
                usage = None;
            }
            // Global
            1 => match tag {
                0x0 => globals.page = u16::try_from(value).ok()?,
                0x7 => globals.size = value,
                0x8 => globals.id = u8::try_from(value).ok()?,
                0x9 => globals.count = value,
                0xA => stack.push(globals),
                0xB => globals = stack.pop()?,
                _ => {}
            },
            // Local: the usage, with its page in the high bytes when it has four
            2 if tag == 0x0 => {
                usage = Some(if size == 4 {
                    ((value >> 16) as u16, value as u16)
                } else {
                    (globals.page, u16::try_from(value).ok()?)
                })
            }
            _ => {}
        }
    }
    if depth != 0 || found.is_empty() {
        return None;
    }
    Some(
        found
            .into_iter()
            .map(|(mut top, bits)| {
                let bytes = |id: u8, bits: u32| if bits == 0 { 0 } else { bits.div_ceil(8) + u32::from(id != 0) };
                top.reports = bits
                    .into_iter()
                    .map(|(id, [input, output, feature])| Reports {
                        id,
                        input: bytes(id, input),
                        output: bytes(id, output),
                        feature: bytes(id, feature),
                    })
                    .filter(|r| r.input + r.output + r.feature > 0)
                    .collect();
                top
            })
            .collect(),
    )
}

/// Bytes as lowercase hex without spaces.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boot mouse of the HID specification (appendix B.2): three buttons, X and Y, no report id.
    const BOOT_MOUSE: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00,
        0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30,
        0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06, 0xC0, 0xC0,
    ];

    /// A vendor page (0xFF00) with three report ids of 6, 19 and 63 bytes, in and out, as gaming
    /// mice and receivers have, and a feature report with id 0x20.
    const VENDOR: &[u8] = &[
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0x75, 0x08, 0x95, 0x06, 0x15, 0x00, 0x26, 0xFF, 0x00,
        0x09, 0x01, 0x81, 0x00, 0x09, 0x01, 0x91, 0x00, 0x85, 0x11, 0x95, 0x13, 0x09, 0x02, 0x81, 0x00, 0x09, 0x02,
        0x91, 0x00, 0x85, 0x12, 0x96, 0x3F, 0x00, 0x09, 0x03, 0x81, 0x00, 0x09, 0x03, 0x91, 0x00, 0x85, 0x20, 0x95,
        0x02, 0x09, 0x04, 0xB1, 0x02, 0xC0,
    ];

    #[test]
    fn a_boot_mouse_has_one_report_without_an_id() {
        let top = summarize(BOOT_MOUSE).unwrap();
        assert_eq!(
            top,
            [TopCollection {
                usage_page: 1,
                usage: 2,
                reports: vec![Reports { id: 0, input: 3, output: 0, feature: 0 }]
            }]
        );
    }

    #[test]
    fn vendor_reports_are_told_apart_by_id_and_count_the_id_byte() {
        let top = summarize(VENDOR).unwrap();
        assert_eq!((top[0].usage_page, top[0].usage), (0xFF00, 1));
        assert_eq!(
            top[0].reports,
            [
                Reports { id: 0x10, input: 7, output: 7, feature: 0 },
                Reports { id: 0x11, input: 20, output: 20, feature: 0 },
                Reports { id: 0x12, input: 64, output: 64, feature: 0 },
                Reports { id: 0x20, input: 0, output: 0, feature: 3 },
            ]
        );
    }

    #[test]
    fn two_collections_are_summarized_apart() {
        let both: Vec<u8> = [BOOT_MOUSE, VENDOR].concat();
        let top = summarize(&both).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!((top[0].usage_page, top[0].usage), (1, 2));
        assert_eq!((top[1].usage_page, top[1].usage), (0xFF00, 1));
    }

    #[test]
    fn a_pushed_report_size_comes_back_after_the_pop() {
        // Report size 8, push, report size 1, pop, count 2, input: two bytes, not two bits.
        let d = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x75, 0x08, 0xA4, 0x75, 0x01, 0xB4, 0x95, 0x02, 0x81, 0x02, 0xC0];
        assert_eq!(summarize(&d).unwrap()[0].reports[0].input, 2);
    }

    #[test]
    fn a_descriptor_that_does_not_make_sense_is_not_summarized() {
        assert_eq!(summarize(&[]), None);
        // Cut in the middle of an item, and a collection never closed.
        assert_eq!(summarize(&BOOT_MOUSE[..BOOT_MOUSE.len() - 3]), None);
        assert_eq!(summarize(&BOOT_MOUSE[..BOOT_MOUSE.len() - 1]), None);
        assert_eq!(summarize(&[0x05]), None);
        // A pop with nothing pushed.
        assert_eq!(summarize(&[0xB4]), None);
    }

    #[test]
    fn hex_has_two_digits_per_byte() {
        assert_eq!(hex(&[0x05, 0x01, 0xFF]), "0501ff");
    }
}

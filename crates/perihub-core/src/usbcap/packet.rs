//! The packets of a capture, whatever tool made it, and the transfers they make up.

use std::collections::HashMap;

use super::pcap::{Frame, Link};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Isochronous,
    Interrupt,
    Control,
    Bulk,
}

impl Kind {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Isochronous),
            1 => Some(Self::Interrupt),
            2 => Some(Self::Control),
            3 => Some(Self::Bulk),
            _ => None,
        }
    }
}

/// One packet, as both tools describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet<'a> {
    /// What ties a request to its completion: USBPcap's IRP id, usbmon's URB id.
    pub id: u64,
    pub time_us: i64,
    pub bus: u16,
    pub address: u16,
    /// With the direction bit (0x80: towards the host).
    pub endpoint: u8,
    pub kind: Kind,
    /// The request coming back (usbmon's 'C' or 'E', USBPcap's "from the device" bit).
    pub completion: bool,
    /// The 8 bytes of a control request, on the packet that sends it.
    pub setup: Option<[u8; 8]>,
    pub data: &'a [u8],
    pub ok: bool,
}

/// USBPcap's stages of a control transfer (`USBPCAP_CONTROL_STAGE_*`).
const STAGE_SETUP: u8 = 0;

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

/// The packet in `frame`, or [None] for a frame too short or of a kind not known.
pub fn parse(link: Link, frame: &Frame) -> Option<Packet<'_>> {
    match link {
        Link::UsbPcap => parse_usbpcap(frame),
        Link::UsbLinux => parse_usbmon(frame, 48),
        Link::UsbLinuxMmapped => parse_usbmon(frame, 64),
    }
}

/// USBPcap's `USBPCAP_BUFFER_PACKET_HEADER` (27 bytes, packed, little-endian): header length,
/// IRP id, USBD status, URB function, info (bit 0: from the device), bus (root hub), device
/// address, endpoint, transfer type, data length; a control transfer adds its stage.
fn parse_usbpcap(frame: &Frame) -> Option<Packet<'_>> {
    let d = &frame.data;
    if d.len() < 27 {
        return None;
    }
    let header = usize::from(u16_at(d, 0));
    if header < 27 || header > d.len() {
        return None;
    }
    let kind = Kind::from_code(d[22])?;
    let length = u32_at(d, 23) as usize;
    let mut data = &d[header..];
    data = &data[..length.min(data.len())];
    let mut setup = None;
    if kind == Kind::Control && header >= 28 && d[27] == STAGE_SETUP && data.len() >= 8 {
        setup = Some(data[..8].try_into().unwrap());
        data = &data[8..];
    }
    Some(Packet {
        id: u64::from_le_bytes(d[2..10].try_into().unwrap()),
        time_us: frame.time_us,
        bus: u16_at(d, 17),
        address: u16_at(d, 19),
        endpoint: d[21],
        kind,
        completion: d[16] & 1 != 0,
        setup,
        data,
        ok: u32_at(d, 10) == 0,
    })
}

/// usbmon's `struct mon_bin_hdr` (Documentation/usb/usbmon.rst): URB id, type ('S', 'C', 'E'),
/// transfer type, endpoint, device, bus, setup flag (0: the setup bytes are there), data flag (0:
/// data follows), seconds, microseconds, status, URB length, captured length, setup bytes. The
/// data follows the header, 48 or 64 bytes long.
fn parse_usbmon(frame: &Frame, header: usize) -> Option<Packet<'_>> {
    let d = &frame.data;
    if d.len() < header {
        return None;
    }
    let kind = Kind::from_code(d[9])?;
    let submit = d[8] == b'S';
    let captured = u32_at(d, 36) as usize;
    let data = if d[15] == 0 { &d[header..header + captured.min(d.len() - header)] } else { &[][..] };
    let status = i32::from_le_bytes(d[28..32].try_into().unwrap());
    Some(Packet {
        id: u64::from_le_bytes(d[..8].try_into().unwrap()),
        time_us: frame.time_us,
        bus: u16_at(d, 12),
        address: u16::from(d[11]),
        endpoint: d[10],
        kind,
        completion: !submit,
        setup: (submit && kind == Kind::Control && d[14] == 0).then(|| d[40..48].try_into().unwrap()),
        data,
        ok: submit || (d[8] == b'C' && status == 0),
    })
}

/// A whole transfer: a control request with its data, or one report or block of data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub time_us: i64,
    pub bus: u16,
    pub address: u16,
    pub endpoint: u8,
    pub kind: Kind,
    pub setup: Option<[u8; 8]>,
    /// The data that went the transfer's way: sent for an OUT transfer, received for an IN one.
    pub data: Vec<u8>,
    pub ok: bool,
    /// The device answered (a control request still waiting when the recording stopped did not).
    pub answered: bool,
}

impl Transfer {
    /// Towards the host.
    pub fn is_in(&self) -> bool {
        match self.setup {
            Some(setup) => setup[0] & 0x80 != 0,
            None => self.endpoint & 0x80 != 0,
        }
    }
}

/// The transfers of `packets`, in the order they ended: a control transfer once its completion
/// is seen, an OUT report when it is sent, an IN report when it arrives. Packets with no data
/// (requests for the next IN report, completions of OUT reports) make no transfer.
pub fn transfers<'a>(packets: impl IntoIterator<Item = Packet<'a>>) -> Vec<Transfer> {
    let mut open: HashMap<(u16, u16, u64), Transfer> = HashMap::new();
    let mut out = Vec::new();
    for p in packets {
        let key = (p.bus, p.address, p.id);
        match p.kind {
            Kind::Control if !p.completion => {
                if let Some(setup) = p.setup {
                    let data = if setup[0] & 0x80 == 0 { p.data.to_vec() } else { Vec::new() };
                    open.insert(
                        key,
                        Transfer {
                            time_us: p.time_us,
                            bus: p.bus,
                            address: p.address,
                            endpoint: p.endpoint,
                            kind: p.kind,
                            setup: Some(setup),
                            data,
                            ok: true,
                            answered: false,
                        },
                    );
                } else if let Some(t) = open.get_mut(&key) {
                    // USBPcap's data stage of an OUT request, when it comes apart from the setup.
                    if !t.is_in() {
                        t.data.extend_from_slice(p.data);
                    }
                }
            }
            Kind::Control => {
                if let Some(mut t) = open.remove(&key) {
                    if t.is_in() {
                        t.data.extend_from_slice(p.data);
                    }
                    t.ok = p.ok;
                    t.answered = true;
                    t.time_us = p.time_us;
                    out.push(t);
                }
            }
            _ => {
                let towards_host = p.endpoint & 0x80 != 0;
                if p.data.is_empty() || towards_host != p.completion {
                    continue;
                }
                out.push(Transfer {
                    time_us: p.time_us,
                    bus: p.bus,
                    address: p.address,
                    endpoint: p.endpoint,
                    kind: p.kind,
                    setup: None,
                    data: p.data.to_vec(),
                    ok: p.ok,
                    answered: true,
                });
            }
        }
    }
    let mut waiting: Vec<Transfer> = open.into_values().collect();
    waiting.sort_by_key(|t| t.time_us);
    out.extend(waiting);
    out.sort_by_key(|t| t.time_us);
    out
}

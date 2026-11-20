//! pcap files, and the streams the two capture tools write, read and written without a library.
//!
//! Three link types are read: USBPcap's (249, Windows), and usbmon's with its 48-byte header (189,
//! what `read(2)` on `/dev/usbmonN` gives) or its 64-byte one (220, the memory-mapped interface,
//! what Wireshark saves on Linux). Only classic pcap: a pcapng file has to be saved as pcap first.

/// What the bytes of each frame are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// USBPcap (Windows): its own header, then the packet's data.
    UsbPcap,
    /// usbmon's 48-byte header, then the data (`LINKTYPE_USB_LINUX`).
    UsbLinux,
    /// usbmon's 64-byte header, then the data (`LINKTYPE_USB_LINUX_MMAPPED`).
    UsbLinuxMmapped,
}

impl Link {
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            249 => Some(Self::UsbPcap),
            189 => Some(Self::UsbLinux),
            220 => Some(Self::UsbLinuxMmapped),
            _ => None,
        }
    }

    pub fn code(self) -> u32 {
        match self {
            Self::UsbPcap => 249,
            Self::UsbLinux => 189,
            Self::UsbLinuxMmapped => 220,
        }
    }
}

/// One captured packet: when, in microseconds since 1970, and its bytes (header included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub time_us: i64,
    pub data: Vec<u8>,
}

/// A frame longer than this is taken for a damaged file. usbmon gives at most a fifth of its
/// buffer (about 60 KiB), USBPcap at most its snapshot length (65535 by default).
pub const MAX_FRAME: usize = 1 << 20;

const MAGIC_US: u32 = 0xA1B2_C3D4;
const MAGIC_NS: u32 = 0xA1B2_3C4D;
const PCAPNG: u32 = 0x0A0D_0D0A;

/// How the file's numbers are written, from its first four bytes.
#[derive(Debug, Clone, Copy)]
struct Order {
    swapped: bool,
    nanos: bool,
}

impl Order {
    fn from_magic(bytes: &[u8]) -> Result<Self, String> {
        let le = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        match le {
            MAGIC_US => Ok(Self { swapped: false, nanos: false }),
            MAGIC_NS => Ok(Self { swapped: false, nanos: true }),
            _ if le.swap_bytes() == MAGIC_US => Ok(Self { swapped: true, nanos: false }),
            _ if le.swap_bytes() == MAGIC_NS => Ok(Self { swapped: true, nanos: true }),
            PCAPNG => {
                Err("this is a pcapng file: save it as pcap (Wireshark: File, Save As, Wireshark/tcpdump - pcap)"
                    .into())
            }
            _ => Err("not a pcap file".into()),
        }
    }

    fn u32(self, bytes: &[u8]) -> u32 {
        let v = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        if self.swapped { v.swap_bytes() } else { v }
    }
}

/// The link type and the frames of a whole pcap file. A last frame cut short (a capture stopped
/// while it was writing) is left out.
pub fn read(bytes: &[u8]) -> Result<(Link, Vec<Frame>), String> {
    let mut stream = PcapStream::default();
    let frames = stream.push(bytes);
    if let Some(error) = stream.error {
        return Err(error);
    }
    let link = stream.link.ok_or("the pcap file is cut short")?;
    Ok((link, frames))
}

/// A pcap file of `frames`, little-endian, with microseconds.
pub fn write(link: Link, frames: &[Frame]) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + frames.iter().map(|f| 16 + f.data.len()).sum::<usize>());
    out.extend_from_slice(&MAGIC_US.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&65535u32.to_le_bytes());
    out.extend_from_slice(&link.code().to_le_bytes());
    for frame in frames {
        let len = frame.data.len() as u32;
        out.extend_from_slice(&(frame.time_us.div_euclid(1_000_000) as u32).to_le_bytes());
        out.extend_from_slice(&(frame.time_us.rem_euclid(1_000_000) as u32).to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&frame.data);
    }
    out
}

/// A pcap file that arrives a piece at a time, as USBPcapCMD writes it into a pipe.
#[derive(Debug, Default)]
pub struct PcapStream {
    buf: Vec<u8>,
    order: Option<Order>,
    link: Option<Link>,
    /// Set when the stream is not a pcap of a USB link type: nothing more is read.
    pub error: Option<String>,
}

impl PcapStream {
    pub fn link(&self) -> Option<Link> {
        self.link
    }

    /// The frames complete with `bytes`.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        if self.error.is_some() {
            return Vec::new();
        }
        self.buf.extend_from_slice(bytes);
        let mut at = 0;
        let order = match self.order {
            Some(order) => order,
            None => {
                let order = match self.buf.len() {
                    0..4 => return Vec::new(),
                    _ => match Order::from_magic(&self.buf) {
                        Ok(order) => order,
                        Err(error) => {
                            self.error = Some(error);
                            return Vec::new();
                        }
                    },
                };
                if self.buf.len() < 24 {
                    return Vec::new();
                }
                let code = order.u32(&self.buf[20..24]) & 0x0FFF_FFFF;
                match Link::from_code(code) {
                    Some(link) => self.link = Some(link),
                    None => {
                        self.error = Some(format!("not a USB capture (link type {code})"));
                        return Vec::new();
                    }
                }
                self.order = Some(order);
                at = 24;
                order
            }
        };
        let mut frames = Vec::new();
        while self.buf.len() - at >= 16 {
            let head = &self.buf[at..at + 16];
            let (secs, frac, len) = (order.u32(head), order.u32(&head[4..]), order.u32(&head[8..]) as usize);
            if len > MAX_FRAME {
                self.error = Some("the pcap file is damaged (a frame is too long)".into());
                break;
            }
            if self.buf.len() - at - 16 < len {
                break;
            }
            let micros = if order.nanos { frac / 1000 } else { frac };
            frames.push(Frame {
                time_us: i64::from(secs) * 1_000_000 + i64::from(micros),
                data: self.buf[at + 16..at + 16 + len].to_vec(),
            });
            at += 16 + len;
        }
        self.buf.drain(..at);
        frames
    }
}

/// The size of usbmon's header as `read(2)` gives it (`PKT_SZ_API0` in drivers/usb/mon/mon_bin.c).
pub const USBMON_HEADER: usize = 48;

/// What `read(2)` on `/dev/usbmonN` gives: each event's 48-byte header, then as many bytes of data
/// as its `len_cap` says, back to back (a read that stops inside an event goes on with it in the
/// next one). Each event becomes a frame of link type [`Link::UsbLinux`].
#[derive(Debug, Default)]
pub struct UsbmonStream {
    buf: Vec<u8>,
}

impl UsbmonStream {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(bytes);
        let mut at = 0;
        let mut frames = Vec::new();
        while self.buf.len() - at >= USBMON_HEADER {
            let head = &self.buf[at..at + USBMON_HEADER];
            let len = u32::from_le_bytes(head[36..40].try_into().unwrap()) as usize;
            let total = USBMON_HEADER + len.min(MAX_FRAME);
            if self.buf.len() - at < total {
                break;
            }
            let secs = i64::from_le_bytes(head[16..24].try_into().unwrap());
            let micros = i32::from_le_bytes(head[24..28].try_into().unwrap());
            frames
                .push(Frame { time_us: secs * 1_000_000 + i64::from(micros), data: self.buf[at..at + total].to_vec() });
            at += total;
        }
        self.buf.drain(..at);
        frames
    }
}

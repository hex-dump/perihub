//! Corsair mice that speak the protocol described in `docs/protocols/corsair-bragi.md`, written
//! from that page only. Nothing here has been tried on a real Corsair device yet: PeriHub only
//! reads them, and changes their lighting only when the user turns on experimental lighting for
//! the session.

pub mod device;
pub mod models;

use std::time::{Duration, Instant};

use crate::color::Rgb;
use crate::error::{Error, Result};
use crate::hidpp::Transport;

pub const CORSAIR: u16 = 0x1B1C;
/// HID interface the mice take requests on.
pub const CONTROL_INTERFACE: i32 = 1;
/// Packet size of most mice; a few use 128 bytes (see the model table).
pub const PACKET: usize = 64;
const MARKER: u8 = 0x08;
const TIMEOUT: Duration = Duration::from_millis(1000);

mod command {
    pub const SET: u8 = 0x01;
    pub const GET: u8 = 0x02;
    pub const CLOSE: u8 = 0x05;
    pub const WRITE: u8 = 0x06;
    pub const CONTINUE: u8 = 0x07;
    pub const OPEN: u8 = 0x0D;
    pub const KEEP_ALIVE: u8 = 0x12;
}

pub mod property {
    pub const POLLING: u8 = 0x01;
    pub const PRODUCT_ID: u8 = 0x12;
    /// On a receiver: bit n set while the device on channel n is connected.
    pub const CHANNELS: u8 = 0x36;
    pub const MODE: u8 = 0x03;
    pub const FIRMWARE: u8 = 0x13;
    pub const DPI_X: u8 = 0x21;
    pub const DPI_Y: u8 = 0x22;
}

pub const MODE_HARDWARE: u8 = 1;
pub const MODE_SOFTWARE: u8 = 2;

const LIGHTING_HANDLE: u8 = 0;
const LIGHTING_RESOURCE: u8 = 0x01;
/// Argument of the close command before the handle.
const CLOSE_ONE: u8 = 0x01;

const STATUS_OK: u8 = 0x00;
const STATUS_ALREADY_OPEN: u8 = 0x03;
const STATUS_UNSUPPORTED: u8 = 0x05;

/// Where the data starts in the first packet of a write, and in the packets that continue it.
const WRITE_DATA_AT: usize = 7;
const CONTINUE_DATA_AT: usize = 3;

/// Whether a packet may reach the mouse. Reading a property is always allowed. Anything that
/// changes the mouse only while experimental lighting is on, and only what lighting needs: the
/// mode (hardware or software, nothing else), handle 0 on the lighting resource, the data written
/// to it, and the keep-alive. Every other command, property or resource is refused, whatever the
/// rest of the code asks for.
pub fn allowed(packet: &[u8], lighting: bool) -> bool {
    // The marker, with the channel of a device behind a receiver in its low bits.
    if packet.len() < 6 || packet[0] & !0x07 != MARKER {
        return false;
    }
    match packet[1] {
        command::GET => true,
        command::SET => {
            lighting
                && packet[2] == property::MODE
                && packet[3] == 0
                && matches!(packet[4], MODE_HARDWARE | MODE_SOFTWARE)
                && packet[5] == 0
        }
        command::OPEN => lighting && packet[2] == LIGHTING_HANDLE && packet[3] == LIGHTING_RESOURCE && packet[4] == 0,
        command::CLOSE => lighting && packet[2] == CLOSE_ONE && packet[3] == LIGHTING_HANDLE,
        command::WRITE | command::CONTINUE => lighting && packet[2] == LIGHTING_HANDLE,
        command::KEEP_ALIVE => lighting,
        _ => false,
    }
}

/// The lighting data: all red values, then all green, then all blue.
pub fn lighting_data(colors: &[Rgb]) -> Vec<u8> {
    let mut data: Vec<u8> = colors.iter().map(|c| c.r).collect();
    data.extend(colors.iter().map(|c| c.g));
    data.extend(colors.iter().map(|c| c.b));
    data
}

/// The packets of `size` bytes that write `data` to the lighting handle: the first one carries the
/// length, the others continue it.
fn write_packets(data: &[u8], size: usize, marker: u8) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut first = vec![0u8; size];
    first[0] = marker;
    first[1] = command::WRITE;
    first[2] = LIGHTING_HANDLE;
    first[3..7].copy_from_slice(&(data.len() as u32).to_le_bytes());
    let (head, mut rest) = data.split_at(data.len().min(size - WRITE_DATA_AT));
    first[WRITE_DATA_AT..WRITE_DATA_AT + head.len()].copy_from_slice(head);
    packets.push(first);
    while !rest.is_empty() {
        let mut next = vec![0u8; size];
        next[0] = marker;
        next[1] = command::CONTINUE;
        next[2] = LIGHTING_HANDLE;
        let (chunk, left) = rest.split_at(rest.len().min(size - CONTINUE_DATA_AT));
        next[CONTINUE_DATA_AT..CONTINUE_DATA_AT + chunk.len()].copy_from_slice(chunk);
        packets.push(next);
        rest = left;
    }
    packets
}

/// A Corsair mouse on its control interface.
pub struct BragiDevice {
    dev: Box<dyn Transport>,
    /// Packet size of this device.
    size: usize,
    /// Channel of the device behind a receiver (1 to 7), or 0 for the device on the cable or the
    /// receiver itself.
    channel: u8,
    /// Experimental lighting is on: the commands it needs may be sent (see [`allowed`]).
    lighting: bool,
}

impl BragiDevice {
    /// `size`: the packet size of the model, 64 or 128 bytes.
    pub fn new(dev: impl Transport + 'static, size: usize) -> Self {
        Self { dev: Box::new(dev), size, channel: 0, lighting: false }
    }

    pub fn set_lighting_allowed(&mut self, on: bool) {
        self.lighting = on;
    }

    /// Talks to the device on this channel of a receiver from now on.
    pub fn set_channel(&mut self, channel: u8) {
        self.channel = channel & 0x07;
    }

    /// On a receiver: the channels whose device is connected now.
    pub fn connected_channels(&mut self) -> Result<Vec<u8>> {
        let mut packet = self.packet(command::GET, &[property::CHANNELS]);
        packet[0] = MARKER;
        let reply = self.request(&packet)?;
        Ok((1..=7).filter(|n| reply[3] >> n & 1 == 1).collect())
    }

    /// USB product id the device reports (behind a receiver, the same as on its cable).
    pub fn product_id(&mut self) -> Result<u16> {
        let id = self.property(property::PRODUCT_ID)? as u16;
        if id == 0xFFFF { Err(Error::Unreachable) } else { Ok(id) }
    }

    /// Sends a packet and returns the reply to it, whatever its status.
    fn exchange(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        if !allowed(packet, self.lighting) {
            return Err(Error::Invalid(format!("command {:#04x} is not allowed on this mouse", packet[1])));
        }
        let mut out = vec![0u8; self.size + 1];
        out[1..=packet.len()].copy_from_slice(packet);
        self.dev.write(&out)?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout);
            }
            let mut reply = vec![0u8; self.size];
            let n = self.dev.read_timeout(&mut reply, left.as_millis().max(1) as i32)?;
            if n == 0 {
                return Err(Error::Timeout);
            }
            // Other reports can arrive on the same interface: only the reply to this command counts.
            if n >= 7 && reply[1] == packet[1] {
                return Ok(reply);
            }
        }
    }

    /// Sends a packet and fails unless the mouse reports success.
    fn request(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        let reply = self.exchange(packet)?;
        match reply[2] {
            STATUS_OK => Ok(reply),
            STATUS_UNSUPPORTED => Err(Error::Protocol(format!("command {:#04x} is not supported", packet[1]))),
            status => Err(Error::Protocol(format!("command {:#04x} failed with status {status:#04x}", packet[1]))),
        }
    }

    fn packet(&self, command: u8, args: &[u8]) -> Vec<u8> {
        let mut packet = vec![0u8; self.size];
        packet[0] = MARKER | self.channel;
        packet[1] = command;
        packet[2..2 + args.len()].copy_from_slice(args);
        packet
    }

    /// Reads a property: three bytes, little endian.
    pub fn property(&mut self, property: u8) -> Result<u32> {
        let reply = self.request(&self.packet(command::GET, &[property]))?;
        Ok(u32::from_le_bytes([reply[3], reply[4], reply[5], 0]))
    }

    /// Firmware version as "major.minor.build".
    pub fn firmware(&mut self) -> Result<String> {
        let reply = self.request(&self.packet(command::GET, &[property::FIRMWARE]))?;
        Ok(format!("{}.{}.{}", reply[3], reply[4], u16::from_le_bytes([reply[5], reply[6]])))
    }

    pub fn set_mode(&mut self, mode: u8) -> Result<()> {
        self.request(&self.packet(command::SET, &[property::MODE, 0, mode, 0])).map(|_| ())
    }

    /// Opens handle 0 on the lighting resource, closing it first if it was left open.
    pub fn open_lighting(&mut self) -> Result<()> {
        let open = self.packet(command::OPEN, &[LIGHTING_HANDLE, LIGHTING_RESOURCE, 0]);
        let mut reply = self.exchange(&open)?;
        if reply[2] == STATUS_ALREADY_OPEN {
            self.close_lighting()?;
            reply = self.exchange(&open)?;
        }
        match reply[2] {
            STATUS_OK => Ok(()),
            status => Err(Error::Protocol(format!("opening the lighting failed with status {status:#04x}"))),
        }
    }

    pub fn close_lighting(&mut self) -> Result<()> {
        self.request(&self.packet(command::CLOSE, &[CLOSE_ONE, LIGHTING_HANDLE])).map(|_| ())
    }

    pub fn write_lighting(&mut self, colors: &[Rgb]) -> Result<()> {
        for packet in write_packets(&lighting_data(colors), self.size, MARKER | self.channel) {
            self.request(&packet)?;
        }
        Ok(())
    }

    pub fn keep_alive(&mut self) -> Result<()> {
        self.request(&self.packet(command::KEEP_ALIVE, &[])).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(bytes: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; PACKET];
        p[..bytes.len()].copy_from_slice(bytes);
        p
    }

    #[test]
    fn reads_are_always_allowed_changes_only_with_experimental_lighting() {
        let get = packet(&[0x08, 0x02, 0x13]);
        assert!(allowed(&get, false));
        let software = packet(&[0x08, 0x01, 0x03, 0x00, 0x02, 0x00]);
        assert!(!allowed(&software, false));
        assert!(allowed(&software, true));
        assert!(!allowed(&packet(&[0x08, 0x12]), false));
        assert!(allowed(&packet(&[0x08, 0x12]), true));
    }

    #[test]
    fn only_the_lighting_commands_get_through() {
        // Other properties: polling rate, brightness, DPI.
        for property in [0x01, 0x02, 0x1E, 0x21, 0x22] {
            assert!(!allowed(&packet(&[0x08, 0x01, property, 0x00, 0x01, 0x00]), true), "{property:#x}");
        }
        // Another value for the mode.
        assert!(!allowed(&packet(&[0x08, 0x01, 0x03, 0x00, 0x03, 0x00]), true));
        // Another handle or resource, such as the wireless pairing id (5) and key (6).
        assert!(!allowed(&packet(&[0x08, 0x0D, 0x01, 0x01, 0x00]), true));
        assert!(!allowed(&packet(&[0x08, 0x0D, 0x00, 0x05, 0x00]), true));
        assert!(!allowed(&packet(&[0x08, 0x0D, 0x00, 0x06, 0x00]), true));
        assert!(!allowed(&packet(&[0x08, 0x06, 0x01]), true));
        assert!(!allowed(&packet(&[0x08, 0x05, 0x01, 0x01]), true));
        // Commands PeriHub never uses, and packets without the marker.
        for command in [0x00, 0x03, 0x04, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0E, 0x13, 0xFF] {
            assert!(!allowed(&packet(&[0x08, command]), true), "{command:#x}");
        }
        assert!(!allowed(&packet(&[0x07, 0x02, 0x13]), true));
        assert!(!allowed(&packet(&[0x18, 0x02, 0x13]), true));
        // A device behind a receiver: the channel in the marker.
        assert!(allowed(&packet(&[0x09, 0x02, 0x13]), false));
        assert!(!allowed(&packet(&[0x09, 0x01, 0x01, 0x00, 0x01, 0x00]), true));
    }

    #[test]
    fn lighting_is_all_red_then_green_then_blue() {
        let colors = [Rgb { r: 1, g: 2, b: 3 }, Rgb { r: 4, g: 5, b: 6 }];
        assert_eq!(lighting_data(&colors), [1, 4, 2, 5, 3, 6]);
    }

    #[test]
    fn a_short_write_is_one_packet_with_the_length() {
        let packets = write_packets(&[9; 15], PACKET, MARKER);
        assert_eq!(packets.len(), 1);
        assert_eq!(&packets[0][..7], &[0x08, 0x06, 0x00, 15, 0, 0, 0]);
        assert_eq!(&packets[0][7..22], &[9; 15]);
        assert!(packets[0][22..].iter().all(|&b| b == 0));
    }

    #[test]
    fn a_long_write_continues_in_more_packets() {
        let data: Vec<u8> = (0..130).map(|i| i as u8).collect();
        let packets = write_packets(&data, PACKET, MARKER);
        // 57 bytes in the first packet, 61 in each of the others.
        assert_eq!(packets.len(), 3);
        // With 128-byte packets: 121, then 125.
        let big = write_packets(&data, 128, MARKER);
        assert_eq!(big.len(), 2);
        assert!(big.iter().all(|p| p.len() == 128));
        assert_eq!(big[1][3], 121);
        assert_eq!(&packets[1][..3], &[0x08, 0x07, 0x00]);
        assert_eq!(packets[1][3], 57);
        assert_eq!(packets[2][3], 118);
        assert!(packets.iter().all(|p| allowed(p, true)));
    }

    /// A simulated mouse that answers every packet with the same command and a fixed status.
    struct Echo {
        sent: std::sync::Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
        status: u8,
    }

    impl Transport for Echo {
        fn write(&self, data: &[u8]) -> Result<()> {
            self.sent.lock().unwrap().push(data.to_vec());
            Ok(())
        }

        fn read_timeout(&self, buf: &mut [u8], _millis: i32) -> Result<usize> {
            let sent = self.sent.lock().unwrap();
            let last = sent.last().unwrap();
            buf[0] = 0;
            buf[1] = last[2];
            buf[2] = self.status;
            buf[3] = 4;
            buf[4] = 2;
            buf[5] = 7;
            Ok(PACKET)
        }
    }

    #[test]
    fn a_refused_command_never_reaches_the_mouse() {
        let sent = Default::default();
        let mut dev = BragiDevice::new(Echo { sent: std::sync::Arc::clone(&sent), status: 0 }, PACKET);
        assert!(dev.set_mode(MODE_SOFTWARE).is_err());
        assert!(dev.keep_alive().is_err());
        assert!(sent.lock().unwrap().is_empty());
        assert_eq!(dev.firmware().unwrap(), "4.2.7");
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        // Report id 0, then the packet.
        assert_eq!(&sent[0][..4], &[0x00, 0x08, 0x02, 0x13]);
        assert_eq!(sent[0].len(), PACKET + 1);
    }

    #[test]
    fn a_device_behind_a_receiver_gets_its_channel_in_every_packet() {
        let sent = Default::default();
        let mut dev = BragiDevice::new(Echo { sent: std::sync::Arc::clone(&sent), status: 0 }, PACKET);
        // The receiver answers 4, 2, 7 to every read: bits 2 (0b100) for property 0x36.
        assert_eq!(dev.connected_channels().unwrap(), [2]);
        dev.set_channel(2);
        dev.firmware().unwrap();
        dev.set_lighting_allowed(true);
        dev.write_lighting(&[Rgb { r: 1, g: 2, b: 3 }]).unwrap();
        let sent = sent.lock().unwrap();
        assert_eq!(sent[0][1], 0x08, "the receiver itself");
        assert_eq!(sent[1][1], 0x0A, "channel 2");
        assert_eq!(sent[2][1], 0x0A);
    }

    #[test]
    fn an_error_status_is_an_error() {
        let mut dev = BragiDevice::new(Echo { sent: Default::default(), status: STATUS_UNSUPPORTED }, PACKET);
        assert!(dev.property(property::POLLING).is_err());
    }
}

//! The readers and the privacy filter, on a real recording: a G502 HERO (046d:c08b) in a Windows 11
//! virtual machine, USBPcap 1.5.4.0 capturing root hub 3 while G HUB-style requests went to it
//! (three HID++ pings by SET_REPORT and their answers on endpoint 0x82, and string descriptor
//! reads). Its serial number string was replaced with zeros.

use super::Collector;
use super::descriptors::{self, Inputs, Interface};
use super::packet::{self, Kind};
use super::pcap::{self, Frame, Link, PcapStream, UsbmonStream};
use super::summary::{self, Step, Target, hex_short, summarize};

const G502: &[u8] = include_bytes!("../../tests/fixtures/usbpcap-g502-hero.pcap");

/// Shaped like the G502 HERO's configuration: interface 0 a boot mouse on endpoint 0x81,
/// interface 1 a HID interface on endpoint 0x82.
const CONFIG: &[u8] = &[
    0x09, 0x02, 0x3B, 0x00, 0x02, 0x01, 0x00, 0xA0, 0x96, // configuration
    0x09, 0x04, 0x00, 0x00, 0x01, 0x03, 0x01, 0x02, 0x00, // interface 0: HID, boot, mouse
    0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x43, 0x00, // HID
    0x07, 0x05, 0x81, 0x03, 0x08, 0x00, 0x01, // endpoint 0x81, interrupt
    0x09, 0x04, 0x01, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, // interface 1: HID
    0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x4E, 0x00, // HID
    0x07, 0x05, 0x82, 0x03, 0x14, 0x00, 0x01, // endpoint 0x82, interrupt
];

/// Interface 1's report descriptor, shaped like a gaming mouse's: a keyboard with report id 1
/// (what its buttons type), and a vendor page with ids 0x10 and 0x11 (its configuration).
const REPORTS_IF1: &[u8] = &[
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75,
    0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x06, 0x75, 0x08, 0x26, 0xFF, 0x00, 0x19, 0x00, 0x2A, 0xFF, 0x00, 0x81, 0x00,
    0xC0, // keyboard, id 1
    0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0x75, 0x08, 0x95, 0x06, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x09,
    0x01, 0x81, 0x00, 0x09, 0x01, 0x91, 0x00, 0x85, 0x11, 0x95, 0x13, 0x09, 0x02, 0x81, 0x00, 0x09, 0x02, 0x91, 0x00,
    0xC0, // vendor page, ids 0x10 and 0x11
];

fn g502() -> (Link, Vec<Frame>) {
    pcap::read(G502).unwrap()
}

fn target(addresses: Vec<(u16, u16)>) -> Target {
    Target { vendor_id: 0x046D, product_id: 0xC08B, addresses, ..Default::default() }
}

/// The G502 as the system would describe it: its configuration and interface 1's reports.
fn described() -> Target {
    let mut device = vec![
        0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40, 0x6D, 0x04, 0x8B, 0xC0, 0x00, 0x27, 0x01, 0x02, 0x03, 0x01,
    ];
    device.extend_from_slice(CONFIG);
    Target {
        usb_descriptors: Some(device),
        report_descriptors: vec![(1, REPORTS_IF1.to_vec())],
        ..target(vec![(3, 2)])
    }
}

#[test]
fn the_usbpcap_recording_is_read() {
    let (link, frames) = g502();
    assert_eq!(link, Link::UsbPcap);
    assert_eq!(frames.len(), 124);
    let first = packet::parse(link, &frames[0]).unwrap();
    assert_eq!((first.bus, first.address, first.endpoint, first.kind), (3, 2, 0x00, Kind::Control));
    assert_eq!(first.setup, Some([0x21, 0x09, 0x11, 0x02, 0x01, 0x00, 0x14, 0x00]));
    assert!(!first.completion);
    // The pcap is read the same arriving in pieces, as from USBCapCMD's pipe.
    let mut stream = PcapStream::default();
    let pieces: Vec<Frame> = G502.chunks(37).flat_map(|c| stream.push(c)).collect();
    assert_eq!(pieces, frames);
    assert_eq!(pcap::read(&pcap::write(link, &frames)).unwrap(), (link, frames));
}

#[test]
fn transfers_pair_requests_with_their_answers() {
    let (link, frames) = g502();
    let all = packet::transfers(frames.iter().filter_map(|f| packet::parse(link, f)));
    let ping = all.iter().find(|t| t.data == [0x10, 0xFF, 0x00, 0x1A, 0x00, 0x00, 0x5A]).unwrap();
    assert_eq!(ping.setup.unwrap()[..2], [0x21, 0x09]);
    assert!(ping.ok && ping.answered && !ping.is_in());
    let answer = all
        .iter()
        .position(|t| t.endpoint == 0x82 && t.data[..7] == [0x11, 0xFF, 0x00, 0x1A, 0x04, 0x02, 0x5A])
        .unwrap();
    assert!(all[answer].is_in() && all[answer].data.len() == 20);
    // A string read: the request's index, the answer's data.
    let product = all.iter().find(|t| t.setup.is_some_and(|s| s[1] == 6 && s[3] == 3 && s[2] == 2)).unwrap();
    assert_eq!(&product.data[..4], &[0x2E, 0x03, b'G', 0x00]);
}

#[test]
fn without_its_descriptors_only_control_transfers_are_kept() {
    let (link, frames) = g502();
    let text = summarize(link, &frames, &target(vec![(3, 2)]), &[], "USBPcap").text;
    assert!(text.contains("SET_REPORT output id 10 if1: 10ff001a00005a"), "{text}");
    assert!(text.contains("SET_REPORT output id 11 if1: 11ff001c00005ac33c(00×11)"), "{text}");
    // Interface 1's answers: what the interface carries is not known, so they are left out.
    assert!(!text.contains("← in ep82"), "{text}");
    assert!(text.contains("Interfaces: not known"));
    assert!(text.contains("reports from ep82"));
    // No device descriptor: every string but the language list is left out, the serial's too.
    assert!(!text.contains("GET_DESCRIPTOR string"), "{text}");
    assert!(text.contains("54 string descriptors"), "{text}");
}

#[test]
fn vendor_reports_are_kept_and_key_presses_left_out() {
    let (link, mut frames) = g502();
    // A key press of interface 1's keyboard (report id 1), made from one of its answers.
    let answer = frames
        .iter()
        .position(|f| packet::parse(link, f).is_some_and(|p| p.endpoint == 0x82 && !p.data.is_empty()))
        .unwrap();
    let mut key = frames[answer].clone();
    let header = usize::from(u16::from_le_bytes([key.data[0], key.data[1]]));
    key.data.truncate(header);
    key.data[23..27].copy_from_slice(&8u32.to_le_bytes());
    key.data.extend_from_slice(&[0x01, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
    key.time_us += 1;
    frames.insert(answer + 1, key);
    let summary = summarize(link, &frames, &described(), &[], "USBPcap");
    let text = &summary.text;
    assert!(text.contains("← in ep82 if1: 11ff001a04025a(00×13)"), "{text}");
    assert!(text.contains("← in ep82 if1: 11ff001c04025a(00×13)"), "{text}");
    assert!(!text.contains("0102000400"), "{text}");
    assert!(text.contains("1 reports from if1"), "{text}");
    assert!(text.contains("Interface 0 (HID boot mouse): ep81; its reports left out"), "{text}");
    assert!(text.contains("Interface 1 (HID): ep82; reports 10 11 kept (vendor-defined)"), "{text}");
    // Now the strings are known: the product's is shown, the serial number's never.
    assert!(text.contains("← GET_DESCRIPTOR string 2: 2e03470035003000"), "{text}");
    assert!(!text.contains("GET_DESCRIPTOR string 3"), "{text}");
    assert!(text.contains("18 string descriptors (they can hold the serial number)"), "{text}");
}

#[test]
fn steps_split_the_recording_and_repeats_are_folded() {
    let (link, frames) = g502();
    let start_ms = frames[0].time_us / 1000;
    let ping_ms = frames
        .iter()
        .find(|f| packet::parse(link, f).is_some_and(|p| p.data.starts_with(&[0x10, 0xFF, 0x00, 0x1A])))
        .unwrap()
        .time_us
        / 1000;
    let steps = [
        Step { at_ms: start_ms, name: "Open the software".into() },
        Step { at_ms: ping_ms, name: "DPI 800 to 1600".into() },
    ];
    let text = summarize(link, &frames, &described(), &steps, "USBPcap").text;
    let first = text.find("Step 1: Open the software (at 0.0 s)").unwrap();
    let second = text.find("Step 2: DPI 800 to 1600 (at ").unwrap();
    let ping = text.find("10ff001a00005a").unwrap();
    assert!(first < second && second < ping, "{text}");
    // G HUB asked for the same strings again and again: one line with a count.
    assert!(text.contains("[the next 2 lines ×6]") || text.contains("×6"), "{text}");
    assert!(!text.contains("Before the first step"));
}

#[test]
fn a_recording_without_the_device_says_so_and_shows_nothing() {
    let (link, frames) = g502();
    let other = Target { vendor_id: 0x1234, product_id: 0x5678, ..Default::default() };
    let summary = summarize(link, &frames, &other, &[], "USBPcap");
    assert!(!summary.found);
    assert!(summary.text.contains("1234:5678 was not found"), "{}", summary.text);
    assert!(!summary.text.contains("ff00"));
}

#[test]
fn the_collector_keeps_the_device_and_drops_the_rest() {
    let (link, frames) = g502();
    // Known address: everything of it is kept.
    let mut known = Collector::new(link, 0x046D, 0xC08B, [(3, 2)]);
    frames.iter().cloned().for_each(|f| known.push(f));
    assert_eq!(known.finish().0.len(), 124);
    // Another device's address: its reports are dropped, only control frames wait for the
    // device to be found, and a device descriptor with the ids finds it.
    let mut unknown = Collector::new(link, 0x046D, 0xC08B, [(3, 9)]);
    frames.iter().cloned().for_each(|f| unknown.push(f));
    assert!(unknown.finish().0.is_empty());
    let mut searching = Collector::new(link, 0x046D, 0xC08B, []);
    frames.iter().cloned().for_each(|f| searching.push(f));
    let (kept, addresses, cut) = searching.finish();
    assert!(!cut && addresses.is_empty());
    assert!(kept.iter().all(|f| packet::parse(link, f).unwrap().kind == Kind::Control));
    // The 8 frames of endpoint 0x82 (4 answers, 4 requests for the next one) are not control frames.
    assert_eq!(kept.len(), 124 - 8);
}

/// A usbmon event as `read(2)` gives it: the 48-byte header, then the data.
fn usbmon(id: u64, kind: u8, transfer: u8, endpoint: u8, time_us: i64, setup: Option<[u8; 8]>, data: &[u8]) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(&id.to_le_bytes());
    e.extend_from_slice(&[kind, transfer, endpoint, 7]);
    e.extend_from_slice(&1u16.to_le_bytes());
    e.push(if setup.is_some() { 0 } else { b'-' });
    e.push(if data.is_empty() { b'<' } else { 0 });
    e.extend_from_slice(&(time_us / 1_000_000).to_le_bytes());
    e.extend_from_slice(&((time_us % 1_000_000) as i32).to_le_bytes());
    e.extend_from_slice(&(if kind == b'S' { -115i32 } else { 0 }).to_le_bytes());
    e.extend_from_slice(&(data.len() as u32).to_le_bytes());
    e.extend_from_slice(&(data.len() as u32).to_le_bytes());
    e.extend_from_slice(&setup.unwrap_or_default());
    e.extend_from_slice(data);
    e
}

#[test]
fn usbmon_streams_are_read_and_summarized_like_usbpcap() {
    let t = 1_795_125_655_000_000;
    let ping = [0x10, 0xFF, 0x00, 0x1A, 0x00, 0x00, 0x5A];
    let mut answer = vec![0x11, 0xFF, 0x00, 0x1A, 0x04, 0x02, 0x5A];
    answer.resize(20, 0);
    let stream: Vec<u8> = [
        usbmon(1, b'S', 2, 0x00, t, Some([0x21, 0x09, 0x10, 0x02, 0x01, 0x00, 0x07, 0x00]), &ping),
        usbmon(1, b'C', 2, 0x00, t + 500, None, &[]),
        usbmon(2, b'C', 1, 0x82, t + 900, None, &answer),
        // A key press on the boot mouse's endpoint, and a sound packet: never kept.
        usbmon(3, b'C', 1, 0x81, t + 950, None, &[0x01, 0x00, 0x05, 0x00]),
        usbmon(4, b'C', 0, 0x83, t + 990, None, &[0xAA; 16]),
    ]
    .concat();
    // Read in pieces that cut headers and data anywhere, as a pipe gives them.
    let mut reader = UsbmonStream::default();
    let frames: Vec<Frame> = stream.chunks(13).flat_map(|c| reader.push(c)).collect();
    assert_eq!(frames.len(), 5);
    assert_eq!(frames[2].time_us, t + 900);
    let mut target = described();
    target.addresses = vec![(1, 7)];
    let mut collector = Collector::new(Link::UsbLinux, 0x046D, 0xC08B, target.addresses.clone());
    frames.into_iter().for_each(|f| collector.push(f));
    let (frames, ..) = collector.finish();
    assert_eq!(frames.len(), 4, "the isochronous packet is dropped at once");
    let text = summarize(Link::UsbLinux, &frames, &target, &[], "usbmon").text;
    assert!(text.contains("→ SET_REPORT output id 10 if1: 10ff001a00005a"), "{text}");
    assert!(text.contains("← in ep82 if1: 11ff001a04025a(00×13)"), "{text}");
    assert!(!text.contains("01000500"), "{text}");
    assert!(text.contains("1 reports from if0"), "{text}");
    // The same events in a pcap with the 64-byte header (what Wireshark saves on Linux).
    let mmapped: Vec<Frame> = reader_frames(&stream)
        .into_iter()
        .map(|f| Frame { data: [&f.data[..48], &[0u8; 16], &f.data[48..]].concat(), ..f })
        .collect();
    let (link, back) = pcap::read(&pcap::write(Link::UsbLinuxMmapped, &mmapped)).unwrap();
    let text = summarize(link, &back, &target, &[], "usbmon").text;
    assert!(text.contains("← in ep82 if1: 11ff001a04025a(00×13)"), "{text}");
}

fn reader_frames(stream: &[u8]) -> Vec<Frame> {
    UsbmonStream::default().push(stream)
}

#[test]
fn what_an_interface_may_keep_follows_its_class_and_collections() {
    let hid = |subclass, protocol| Interface { number: 0, class: 3, subclass, protocol, ..Default::default() };
    assert_eq!(descriptors::inputs(&hid(1, 1), Some(REPORTS_IF1)), Inputs::Nothing, "a boot keyboard, always");
    assert_eq!(descriptors::inputs(&hid(0, 0), None), Inputs::Nothing, "not known");
    assert_eq!(descriptors::inputs(&hid(0, 0), Some(REPORTS_IF1)), Inputs::Ids([0x10, 0x11].into()));
    assert_eq!(descriptors::inputs(&hid(0, 0), Some(&REPORTS_IF1[39..])), Inputs::All, "vendor page only");
    let vendor = Interface { class: 0xFF, ..Default::default() };
    assert_eq!(descriptors::inputs(&vendor, None), Inputs::All);
    let audio = Interface { class: 1, ..Default::default() };
    assert_eq!(descriptors::inputs(&audio, None), Inputs::Nothing);
    // A keyboard without report ids next to a vendor collection: nothing tells them apart.
    let no_id: Vec<u8> = [&REPORTS_IF1[..6], &REPORTS_IF1[8..39], &REPORTS_IF1[39..]].concat();
    assert_eq!(descriptors::inputs(&hid(0, 0), Some(&no_id)), Inputs::Nothing);
    let interfaces = descriptors::interfaces(CONFIG);
    assert_eq!(interfaces.len(), 2);
    assert_eq!((interfaces[1].number, interfaces[1].endpoints.iter().copied().collect::<Vec<_>>()), (1, vec![0x82]));
}

#[test]
fn hex_is_short_where_it_ends_in_zeros() {
    assert_eq!(hex_short(&[0x11, 0xFF, 0, 0, 0, 0]), "11ff(00×4)");
    assert_eq!(hex_short(&[0x11, 0, 0, 0]), "11000000");
    assert_eq!(hex_short(&[]), "");
}

#[test]
fn a_pcapng_file_is_refused_with_a_way_out() {
    assert!(pcap::read(&[0x0A, 0x0D, 0x0D, 0x0A, 0, 0, 0, 0]).unwrap_err().contains("pcapng"));
    let mut ethernet = pcap::write(Link::UsbPcap, &[]);
    ethernet[20] = 1;
    assert!(pcap::read(&ethernet).unwrap_err().contains("not a USB capture"));
}

#[test]
fn the_text_is_capped() {
    let (link, frames) = g502();
    let mut many = Vec::new();
    for k in 0..400 {
        for f in &frames {
            many.push(Frame { time_us: f.time_us + k * 10_000_000, data: f.data.clone() });
        }
    }
    // Different data each time, so that nothing folds.
    for (k, f) in many.iter_mut().enumerate() {
        if let Some(p) = packet::parse(link, f)
            && p.setup.is_some_and(|s| s[1] == 9)
        {
            let at = f.data.len() - 1;
            f.data[at] = k as u8;
        }
    }
    let steps: Vec<Step> = (0..400)
        .map(|k| Step { at_ms: (frames[0].time_us + k * 10_000_000) / 1000, name: format!("step {k}") })
        .collect();
    let text = summarize(link, &many, &described(), &steps, "USBPcap").text;
    assert!(text.len() <= summary::MAX_TEXT, "{}", text.len());
    assert!(text.contains("(cut here: about"), "{}", &text[text.len() - 300..]);
}

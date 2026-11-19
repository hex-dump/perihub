//! Recording on Linux: usbmon, read as root through pkexec, which shows the system's password window.
//!
//! The root part is a short shell script, not PeriHub itself: an AppImage's files are on a mount
//! that root cannot read. It loads usbmon if it is not loaded, copies `/dev/usbmonN` (the
//! device's bus) to its output with `cat` until a line (or the end) comes on its input, then
//! unloads usbmon if it loaded it. `read(2)` on that file gives each event's 48-byte header and its
//! data back to back (Documentation/usb/usbmon.rst, drivers/usb/mon/mon_bin.c), which PeriHub reads
//! as it comes, keeping only the device. If PeriHub stops, its end of the input closes and the
//! script cleans up all the same. No polkit action of PeriHub's own is installed: the window names
//! /bin/sh, as for the udev rules.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{Event, Events, Recorded, Request};
use crate::usbcap::pcap::UsbmonStream;
use crate::usbcap::{Collector, Link, Target};

/// Run by pkexec as `sh -c SCRIPT perihub-usbmon BUS`. Exit codes: 3 usbmon could not be loaded,
/// 4 its device file did not appear.
const SCRIPT: &str = r#"bus="$1"
loaded=0
if [ ! -d /sys/module/usbmon ]; then
  modprobe usbmon || exit 3
  loaded=1
fi
i=0
while [ ! -e "/dev/usbmon$bus" ] && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
if [ ! -e "/dev/usbmon$bus" ]; then
  [ "$loaded" = 1 ] && modprobe -r usbmon
  exit 4
fi
cat "/dev/usbmon$bus" &
reader=$!
echo recording >&2
read -r line || true
kill "$reader" 2>/dev/null
wait "$reader" 2>/dev/null
if [ "$loaded" = 1 ]; then
  modprobe -r usbmon || echo "usbmon could not be unloaded" >&2
fi
exit 0
"#;

pub fn unavailable() -> Option<String> {
    if Path::new("/.flatpak-info").exists() {
        return Some(
            "PeriHub runs as a Flatpak, whose sandbox cannot read USB traffic: record with the AppImage or the \
             .deb version, or by hand with the capture guide"
                .into(),
        );
    }
    if !["/usr/bin/pkexec", "/bin/pkexec"].iter().any(|p| Path::new(p).exists()) {
        return Some("pkexec (polkit), which asks for the password, is not installed".into());
    }
    None
}

/// A USB device as sysfs shows it.
#[derive(Debug)]
struct SysDevice {
    bus: u16,
    address: u16,
    descriptors: Vec<u8>,
}

fn find(vendor_id: u16, product_id: u16) -> Vec<SysDevice> {
    let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") else { return Vec::new() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        // Interfaces ("1-2:1.0") have a colon; devices ("1-2") and root hubs ("usb1") do not.
        if entry.file_name().to_string_lossy().contains(':') {
            continue;
        }
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok().map(|s| s.trim().to_string());
        let hex = |name: &str| read(name).and_then(|s| u16::from_str_radix(&s, 16).ok());
        let dec = |name: &str| read(name).and_then(|s| s.parse::<u16>().ok());
        if hex("idVendor") != Some(vendor_id) || hex("idProduct") != Some(product_id) {
            continue;
        }
        if let (Some(bus), Some(address)) = (dec("busnum"), dec("devnum")) {
            let descriptors = std::fs::read(dir.join("descriptors")).unwrap_or_default();
            found.push(SysDevice { bus, address, descriptors });
        }
    }
    found.sort_by_key(|d| (d.bus, d.address));
    found
}

pub struct Handle {
    stdin: Mutex<Option<ChildStdin>>,
    cancelled: Arc<AtomicBool>,
}

impl Handle {
    pub fn stop(&self, cancel: bool) {
        if cancel {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        // The script goes on at the line, or at the end of its input when the line cannot be written.
        if let Some(mut stdin) = self.stdin.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = stdin.write_all(b"stop\n");
        }
    }
}

fn failure(code: Option<i32>, said: &[String]) -> String {
    match code {
        Some(126) => "the password window was closed: nothing was recorded".into(),
        Some(127) => "not authorized: the password window could not be shown (is a polkit agent running?)".into(),
        Some(3) => "the usbmon kernel module could not be loaded".into(),
        Some(4) => "usbmon's device file for the bus did not appear".into(),
        code => {
            let said = said.join("; ");
            format!(
                "the recording stopped (exit code {code:?}){}",
                if said.is_empty() { String::new() } else { format!(": {said}") }
            )
        }
    }
}

pub fn start(request: Request, events: Events) -> Result<Handle, String> {
    if let Some(reason) = unavailable() {
        return Err(reason);
    }
    let devices = find(request.vendor_id, request.product_id);
    let Some(first) = devices.first() else {
        return Err("the device was not found among the USB devices (a Bluetooth device cannot be recorded)".into());
    };
    // One bus, or usbmon0, which holds every bus.
    let bus = if devices.iter().all(|d| d.bus == first.bus) { first.bus } else { 0 };
    let target = Target {
        vendor_id: request.vendor_id,
        product_id: request.product_id,
        addresses: devices.iter().map(|d| (d.bus, d.address)).collect(),
        usb_descriptors: Some(first.descriptors.clone()).filter(|d| !d.is_empty()),
        report_descriptors: request.report_descriptors,
    };
    events(Event::Status("Waiting for your password…".into()));
    let mut child = Command::new("pkexec")
        .args(["/bin/sh", "-c", SCRIPT, "perihub-usbmon", &bus.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run pkexec: {e}"))?;
    let (Some(stdin), Some(mut stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("cannot talk to pkexec".into());
    };
    let addresses = target.addresses.clone();
    let reader = std::thread::spawn(move || {
        let mut stream = UsbmonStream::default();
        let mut collector = Collector::new(Link::UsbLinux, request.vendor_id, request.product_id, addresses);
        let mut buf = vec![0u8; 1 << 16];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            for frame in stream.push(&buf[..n]) {
                collector.push(frame);
            }
        }
        collector
    });
    let said = Arc::new(Mutex::new(Vec::<String>::new()));
    let (lines, told) = (said.clone(), events.clone());
    let errors = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line == "recording" {
                told(Event::Recording);
            } else if !line.trim().is_empty() {
                log::info!("usbmon recording: {line}");
                lines.lock().unwrap_or_else(|e| e.into_inner()).push(line);
            }
        }
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    std::thread::spawn(move || {
        let collector = reader.join();
        let code = child.wait().ok().and_then(|s| s.code());
        let _ = errors.join();
        let said = said.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if flag.load(Ordering::SeqCst) {
            events(Event::Cancelled);
            return;
        }
        match (code, collector) {
            (Some(0), Ok(collector)) => {
                let (frames, addresses, cut) = collector.finish();
                let target = Target { addresses: addresses.into_iter().collect(), ..target };
                for line in said {
                    events(Event::Cleanup(line));
                }
                events(Event::Finished(Box::new(Recorded {
                    link: Link::UsbLinux,
                    frames,
                    target,
                    source: "usbmon",
                    cut,
                })));
            }
            (code, _) => events(Event::Failed(failure(code, &said))),
        }
    });
    Ok(Handle { stdin: Mutex::new(Some(stdin)), cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_unloads_only_what_it_loaded_and_ends_with_its_input() {
        assert!(SCRIPT.contains("if [ ! -d /sys/module/usbmon ]"));
        assert!(SCRIPT.contains("modprobe -r usbmon") && SCRIPT.matches("loaded\" = 1").count() == 2);
        assert!(SCRIPT.contains("read -r line || true"));
    }

    #[test]
    fn pkexec_codes_are_told_in_words() {
        assert!(failure(Some(126), &[]).contains("password window was closed"));
        assert!(failure(Some(3), &[]).contains("could not be loaded"));
        assert_eq!(failure(Some(1), &["oops".into()]), "the recording stopped (exit code Some(1)): oops");
    }
}

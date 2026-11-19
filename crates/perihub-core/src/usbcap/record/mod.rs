//! Making a recording: on Windows with USBPcap, installed by an elevated copy of PeriHub for the
//! recording only ([`windows`]); on Linux with usbmon, read as root through pkexec ([`linux`]).
//! Either way the interface gets [`Event`]s, and at the end the device's frames.

#[cfg(target_os = "linux")]
mod linux;
mod usbpcap;
#[cfg(windows)]
mod windows;

use std::sync::Arc;

use super::{Frame, Link, Target};

/// The device to record.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Its name, as the system gives it: how USBPcap's device list is searched.
    pub product: Option<String>,
    /// The HID report descriptor of each interface, as the system gives it.
    pub report_descriptors: Vec<(u8, Vec<u8>)>,
}

/// What a finished recording holds: the device's frames and what is known of it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub link: Link,
    pub frames: Vec<Frame>,
    pub target: Target,
    /// The tool, for the text: "USBPcap 1.5.4.0" or "usbmon".
    pub source: &'static str,
    /// More came than PeriHub keeps: the end is missing.
    pub cut: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// What is being done (downloading, installing, removing…).
    Status(String),
    /// The capture runs: the user can go through the steps.
    Recording,
    /// What the removal of USBPcap left (Windows).
    Cleanup(String),
    Finished(Box<Recorded>),
    Cancelled,
    Failed(String),
    /// A removal of what an unfinished recording left is over (Windows).
    Ended,
}

pub type Events = Arc<dyn Fn(Event) + Send + Sync>;

/// A recording under way.
pub struct Recording {
    #[cfg(target_os = "linux")]
    inner: linux::Handle,
    #[cfg(windows)]
    inner: windows::Handle,
}

impl Recording {
    /// Ends the recording and keeps it.
    pub fn stop(&self) {
        #[cfg(any(target_os = "linux", windows))]
        self.inner.stop(false);
    }

    /// Ends the recording and drops it.
    pub fn cancel(&self) {
        #[cfg(any(target_os = "linux", windows))]
        self.inner.stop(true);
    }
}

/// Why this system cannot record, if it cannot.
pub fn unavailable() -> Option<String> {
    #[cfg(target_os = "linux")]
    return linux::unavailable();
    #[cfg(windows)]
    return None;
    #[cfg(not(any(target_os = "linux", windows)))]
    Some("recording is available on Windows and Linux only".into())
}

/// The tool a recording uses here, for the interface.
pub const TOOL: &str = if cfg!(windows) { "USBPcap" } else { "usbmon" };

/// Starts recording `request`'s device. `events` is called from other threads, the last time with
/// [`Event::Finished`], [`Event::Cancelled`] or [`Event::Failed`].
pub fn start(request: Request, events: Events) -> Result<Recording, String> {
    #[cfg(target_os = "linux")]
    return linux::start(request, events).map(|inner| Recording { inner });
    #[cfg(windows)]
    return windows::start(request, events).map(|inner| Recording { inner });
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (request, events);
        Err(unavailable().unwrap_or_default())
    }
}

/// An earlier recording did not finish and left USBPcap installed (Windows: PeriHub crashed, or the
/// computer was turned off, before it removed it).
pub fn leftover() -> bool {
    #[cfg(windows)]
    return windows::leftover();
    #[cfg(not(windows))]
    false
}

/// Removes what an unfinished recording left, with one administrator prompt (Windows).
pub fn clean_leftover(events: Events) -> Result<Recording, String> {
    #[cfg(windows)]
    return windows::clean_leftover(events).map(|inner| Recording { inner });
    #[cfg(not(windows))]
    {
        let _ = events;
        Err("nothing to remove on this system".into())
    }
}

/// The hidden argument that makes PeriHub the elevated helper of a recording (Windows).
pub const HELPER_ARG: &str = "--usb-record-helper";

/// The elevated helper, started by [`start`] with [`HELPER_ARG`]: returns the process's exit code.
#[cfg(windows)]
pub fn helper_main(args: &[String]) -> i32 {
    windows::helper_main(args)
}

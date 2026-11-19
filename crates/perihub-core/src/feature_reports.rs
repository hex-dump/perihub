//! The current contents of the feature reports a device declares, for a device request: so that a
//! maintainer can recognize a known controller chip and its protocol (a firmware version, a chip
//! id, a table of settings) without asking anyone for a USB capture.
//!
//! # Why this is safe
//!
//! A feature report is a small block of state a device keeps and hands over on request (HID
//! specification, "Get_Report", report type Feature). Reading one is the standard way to ask a
//! device what it holds; it does not change it. Still, the rules below keep it from reaching
//! anything it should not:
//!
//! - **Only GET_FEATURE** (hidapi's get_feature_report) is ever sent. Never SET_FEATURE, never an
//!   output report, never a write of any kind, and no input report is read: nothing typed or
//!   moved on the device can be captured, since that comes in input reports, which are not read.
//! - **Only ids the descriptor declares**, with the declared length, once each, and only from
//!   collections on a vendor-defined usage page (0xFF00 and above). Those carry a manufacturer's
//!   own settings and information, not keys or movement.
//! - **A node with a mouse, keyboard, pointer or keypad collection is never opened**
//!   (Generic Desktop usages 1, 2, 6 and 7), also when it carries a vendor collection too.
//! - **No extra rights**: the node is opened as hidapi opens any device. On Linux that works only
//!   when the hidraw node is already open to the user (a udev rule): when it is not, nothing is
//!   asked for and nothing is read. On Windows the collection is opened by hidapi without read or
//!   write access when the system keeps it to itself.
//! - **Stop at the first error** of an interface: the rest of its ids are not tried. A reply that
//!   does not come within [REPORT_TIMEOUT] stops it too, and the handle is closed as soon as the
//!   call returns.
//! - **Bounded**: at most [MAX_REPORTS] ids of at most [MAX_REPORT_LENGTH] bytes, [MAX_BYTES] bytes
//!   in all, per interface.
//! - **Only on the "Send a report" page**: the manager reads these while that page is open, never
//!   in the background, and once per interface until the device is unplugged.

use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

use crate::hid_descriptor::{self, TopCollection};

/// The first vendor-defined usage page: below it the usages are defined by the HID specification.
pub const VENDOR_PAGE: u16 = 0xFF00;
/// At most this many feature report ids are asked for, per interface.
pub const MAX_REPORTS: usize = 32;
/// A feature report longer than this (declared, with the id byte) is not asked for.
pub const MAX_REPORT_LENGTH: u32 = 512;
/// The sum of the declared lengths asked for, per interface.
pub const MAX_BYTES: u32 = 1024;
/// How long one reply is waited for.
pub const REPORT_TIMEOUT: Duration = Duration::from_millis(1500);

/// One feature report to ask for: its id and its declared length, with the id byte when the id is
/// not 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub id: u8,
    pub length: u32,
}

/// The Generic Desktop usages of what is typed or moved: pointer, mouse, keyboard, keypad.
fn is_input_device(c: &TopCollection) -> bool {
    c.usage_page == 0x01 && matches!(c.usage, 0x01 | 0x02 | 0x06 | 0x07)
}

/// The feature reports to ask for, from a descriptor: those declared by collections on a vendor
/// page, and none at all when the descriptor does not make sense or has a collection a person types
/// or moves with (then the node is not opened).
pub fn plan(descriptor: &[u8]) -> Vec<Request> {
    let Some(top) = hid_descriptor::summarize(descriptor) else { return Vec::new() };
    if top.iter().any(is_input_device) {
        return Vec::new();
    }
    let (mut requests, mut total) = (Vec::<Request>::new(), 0u32);
    for report in top.iter().filter(|c| c.usage_page >= VENDOR_PAGE).flat_map(|c| &c.reports) {
        let length = report.feature;
        if length == 0 || length > MAX_REPORT_LENGTH || requests.iter().any(|r| r.id == report.id) {
            continue;
        }
        if requests.len() == MAX_REPORTS || total + length > MAX_BYTES {
            break;
        }
        total += length;
        requests.push(Request { id: report.id, length });
    }
    requests
}

/// How reading one interface's feature reports went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// No vendor-defined feature report is declared (or the descriptor was not read, or the node
    /// is one a person types or moves with): nothing was opened.
    NothingToRead,
    /// The node could not be opened without extra rights.
    NotOpened,
    /// Every id was asked for once.
    Read,
    /// An id got no answer: the ones after it were not asked for.
    StoppedAfterError,
    /// An id did not answer in time: the ones after it were not asked for.
    TimedOut,
}

/// What one id answered. [None]: the device did not answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub id: u8,
    /// The declared length, with the id byte when the id is not 0.
    pub length: u32,
    /// What came back, as hex, as hidapi returns it: the first byte is the report id (0 when the
    /// device has none).
    pub hex: Option<String>,
}

/// The feature reports of one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureRead {
    pub outcome: Outcome,
    pub reports: Vec<Report>,
}

impl FeatureRead {
    pub fn new(outcome: Outcome) -> Self {
        Self { outcome, reports: Vec::new() }
    }
}

/// A GET_FEATURE that failed (the device did not answer, or the system refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetFailed;

/// What a feature report is read through: a hidapi device, or a stand-in in the tests. Only GET is
/// possible, on purpose: this trait has no way to write.
pub trait Source: Send + 'static {
    /// GET_FEATURE: the first byte of the buffer is the report id. Returns the bytes read.
    fn get_feature(&mut self, buffer: &mut [u8]) -> Result<usize, GetFailed>;
}

impl Source for hidapi::HidDevice {
    fn get_feature(&mut self, buffer: &mut [u8]) -> Result<usize, GetFailed> {
        self.get_feature_report(buffer).map_err(|_| GetFailed)
    }
}

/// Asks for each request once, in order, on a thread of its own, so that a device that never
/// answers cannot hold PeriHub up: after [REPORT_TIMEOUT] without a reply the thread is left to
/// finish (it closes the handle when its call returns) and nothing more is asked.
pub fn read<S: Source>(source: S, requests: &[Request], timeout: Duration) -> FeatureRead {
    if requests.is_empty() {
        return FeatureRead::new(Outcome::NothingToRead);
    }
    let (sender, receiver) = mpsc::channel();
    let asked = requests.to_vec();
    let spawned = std::thread::Builder::new().name("perihub-feature-reports".into()).spawn(move || {
        let mut source = source;
        for request in asked {
            // hidapi wants the id byte in front, also for id 0 (whose declared length has none).
            let size = request.length as usize + usize::from(request.id == 0);
            let mut buffer = vec![0u8; size];
            buffer[0] = request.id;
            let answer = source.get_feature(&mut buffer).map(|n| buffer[..n.min(size)].to_vec());
            let failed = answer.is_err();
            // The receiver is gone after a timeout: stop asking.
            if sender.send(answer).is_err() || failed {
                break;
            }
        }
    });
    if spawned.is_err() {
        return FeatureRead::new(Outcome::NotOpened);
    }
    let mut result = FeatureRead::new(Outcome::Read);
    for request in requests {
        match receiver.recv_timeout(timeout) {
            Ok(Ok(bytes)) => result.reports.push(Report {
                id: request.id,
                length: request.length,
                hex: Some(hid_descriptor::hex(&bytes)),
            }),
            Ok(Err(GetFailed)) => {
                result.reports.push(Report { id: request.id, length: request.length, hex: None });
                result.outcome = Outcome::StoppedAfterError;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                result.reports.push(Report { id: request.id, length: request.length, hex: None });
                result.outcome = Outcome::TimedOut;
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                result.outcome = Outcome::StoppedAfterError;
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A vendor page (0xFF00) with input and output reports (ids 0x10 and 0x11) and feature reports
    /// with ids 0x20 (2 bytes) and 0x21 (8 bytes), then a collection on page 0xFF01 with a feature
    /// report with id 0x30 (4 bytes).
    const VENDOR: &[u8] = &[
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0x75, 0x08, 0x95, 0x06, 0x09, 0x01, 0x81, 0x00, 0x85,
        0x11, 0x95, 0x13, 0x09, 0x02, 0x91, 0x00, 0x85, 0x20, 0x95, 0x02, 0x09, 0x04, 0xB1, 0x02, 0x85, 0x21, 0x95,
        0x08, 0x09, 0x05, 0xB1, 0x02, 0xC0, 0x06, 0x01, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x30, 0x75, 0x08, 0x95,
        0x04, 0x09, 0x01, 0xB1, 0x02, 0xC0,
    ];
    /// A vendor collection whose only feature report has no id (4 bytes).
    const NO_ID: &[u8] =
        &[0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x95, 0x04, 0x09, 0x01, 0xB1, 0x02, 0xC0];
    /// A boot mouse and a keyboard, each with a feature report of its own (id 1, one byte).
    const MOUSE_WITH_FEATURE: &[u8] =
        &[0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x01, 0x75, 0x08, 0x95, 0x01, 0x09, 0x01, 0xB1, 0x02, 0xC0];

    #[test]
    fn the_declared_feature_report_ids_and_lengths_of_vendor_collections_are_planned() {
        assert_eq!(
            plan(VENDOR),
            [Request { id: 0x20, length: 3 }, Request { id: 0x21, length: 9 }, Request { id: 0x30, length: 5 },]
        );
        // No id: the length has no id byte.
        assert_eq!(plan(NO_ID), [Request { id: 0, length: 4 }]);
    }

    #[test]
    fn nothing_is_planned_for_input_and_output_reports_or_for_a_descriptor_that_does_not_make_sense() {
        // Only input and output reports declared.
        let io = [
            0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x95, 0x04, 0x09, 0x01, 0x81, 0x00, 0x09, 0x01, 0x91,
            0x00, 0xC0,
        ];
        assert!(plan(&io).is_empty());
        assert!(plan(&[]).is_empty());
        assert!(plan(&VENDOR[..VENDOR.len() - 1]).is_empty());
    }

    #[test]
    fn a_node_with_a_mouse_keyboard_pointer_or_keypad_is_never_planned() {
        assert!(plan(MOUSE_WITH_FEATURE).is_empty());
        for usage in [0x01u8, 0x02, 0x06, 0x07] {
            let mut d = MOUSE_WITH_FEATURE.to_vec();
            d[3] = usage;
            assert!(plan(&d).is_empty(), "usage {usage}");
        }
        // The same vendor collection in a node that also has a mouse is not read either.
        let both: Vec<u8> = [MOUSE_WITH_FEATURE, VENDOR].concat();
        assert!(plan(&both).is_empty());
        // A consumer-control collection is not typing: its vendor sibling is read.
        let consumer = [0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x95, 0x01, 0x09, 0x01, 0x81, 0x00, 0xC0];
        assert_eq!(plan(&[&consumer[..], VENDOR].concat()).len(), 3);
    }

    #[test]
    fn a_vendor_page_is_ff00_and_above() {
        let mut d = VENDOR.to_vec();
        // The first collection moves to page 0xFE00, which the HID specification defines: only the
        // second collection is on a vendor page.
        d[2] = 0xFE;
        assert_eq!(plan(&d), [Request { id: 0x30, length: 5 }]);
    }

    #[test]
    fn the_plan_is_bounded() {
        // 40 ids of 1 byte, then a report of 513 bytes.
        let mut d = vec![0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x95, 0x01];
        for id in 1..=40u8 {
            d.extend([0x85, id, 0x09, 0x01, 0xB1, 0x02]);
        }
        d.push(0xC0);
        assert_eq!(plan(&d).len(), MAX_REPORTS);
        // 3 reports of 400 bytes: the third goes over 1024 bytes in all.
        let mut big = vec![0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x96, 0x8F, 0x01];
        for id in 1..=3u8 {
            big.extend([0x85, id, 0x09, 0x01, 0xB1, 0x02]);
        }
        big.push(0xC0);
        assert_eq!(plan(&big).len(), 2);
        // One of 513 bytes (512 and the id byte is the limit).
        let mut long = vec![0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x96, 0x00, 0x02, 0x85, 1];
        long.extend([0x09, 0x01, 0xB1, 0x02, 0xC0]);
        assert!(plan(&long).is_empty());
    }

    /// A device that answers from a list, and remembers what it was asked.
    struct Fake {
        answers: Vec<Result<Vec<u8>, GetFailed>>,
        asked: Asked,
        delay: Duration,
    }

    impl Source for Fake {
        fn get_feature(&mut self, buffer: &mut [u8]) -> Result<usize, GetFailed> {
            std::thread::sleep(self.delay);
            self.asked.lock().unwrap().push((buffer[0], buffer.len()));
            let answer = self.answers.remove(0)?;
            buffer[..answer.len()].copy_from_slice(&answer);
            Ok(answer.len())
        }
    }

    type Asked = Arc<Mutex<Vec<(u8, usize)>>>;

    fn fake(answers: Vec<Result<Vec<u8>, GetFailed>>, delay: Duration) -> (Fake, Asked) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        (Fake { answers, asked: asked.clone(), delay }, asked)
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn each_declared_id_is_asked_once_with_its_length_and_the_bytes_are_recorded_as_hex() {
        let (device, asked) = fake(vec![Ok(vec![0x20, 0xAB, 0x01]), Ok(vec![0x00, 1, 2, 3, 4])], Duration::ZERO);
        let requests = [Request { id: 0x20, length: 3 }, Request { id: 0, length: 4 }];
        let read = read(device, &requests, SECOND);
        assert_eq!(read.outcome, Outcome::Read);
        assert_eq!(
            read.reports,
            [
                Report { id: 0x20, length: 3, hex: Some("20ab01".into()) },
                Report { id: 0, length: 4, hex: Some("0001020304".into()) },
            ]
        );
        // The id 0 buffer has room for the id byte on top of its 4 bytes.
        assert_eq!(*asked.lock().unwrap(), [(0x20, 3), (0, 5)]);
    }

    #[test]
    fn it_stops_at_the_first_error() {
        let (device, asked) = fake(vec![Ok(vec![0x20, 1, 2]), Err(GetFailed), Ok(vec![0x22, 0])], Duration::ZERO);
        let requests =
            [Request { id: 0x20, length: 3 }, Request { id: 0x21, length: 9 }, Request { id: 0x22, length: 2 }];
        let read = read(device, &requests, SECOND);
        assert_eq!(read.outcome, Outcome::StoppedAfterError);
        assert_eq!(read.reports.len(), 2);
        assert_eq!(read.reports[1], Report { id: 0x21, length: 9, hex: None });
        assert_eq!(asked.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_device_that_does_not_answer_in_time_does_not_hold_it_up() {
        let (device, asked) = fake(vec![Ok(vec![0x20, 1, 2]), Ok(vec![0x21])], Duration::from_millis(400));
        let requests = [Request { id: 0x20, length: 3 }, Request { id: 0x21, length: 1 }];
        let started = std::time::Instant::now();
        let read = read(device, &requests, Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_millis(300));
        assert_eq!(read.outcome, Outcome::TimedOut);
        assert_eq!(read.reports, [Report { id: 0x20, length: 3, hex: None }]);
        // The thread finishes its one call, finds nobody listening and asks nothing more.
        std::thread::sleep(Duration::from_millis(900));
        assert_eq!(asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn with_nothing_to_ask_nothing_is_opened() {
        let (device, asked) = fake(vec![], Duration::ZERO);
        assert_eq!(read(device, &[], SECOND), FeatureRead::new(Outcome::NothingToRead));
        assert!(asked.lock().unwrap().is_empty());
    }

    #[test]
    fn the_outcome_is_serialized_as_a_word() {
        let json = serde_json::to_value(FeatureRead::new(Outcome::StoppedAfterError)).unwrap();
        assert_eq!(json["outcome"], "stopped_after_error");
    }
}

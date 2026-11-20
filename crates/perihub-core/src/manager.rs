//! Detects supported devices when they are plugged in or unplugged.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi};
use serde::Serialize;

use crate::apex::Apex3Tkl;
use crate::config::KeyboardConfig;
use crate::conflicts::{self, Conflict};
use crate::corsair::device::{CorsairInfo, CorsairTask};
use crate::corsair::{self, BragiDevice};
use crate::error::{Error, Result};
use crate::g300::{self, G300Info, G300Task};
use crate::hidpp::direct::{self, DirectModel};
use crate::hidpp::host::Look;
use crate::hidpp::models::{self, MouseModel};
use crate::hidpp::receiver::{self, Presence, UnsupportedDevice};
use crate::hidpp::{HidppDevice, Transport};
use crate::keyboard::{KeyInterfaces, KeyboardInfo, KeyboardTask};
use crate::logitech::LightsMemory;
use crate::logitech::LogitechTask;
use crate::mouse::{MouseInfo, MouseTask, ReceiverLink};
use crate::peripheral::qmk::{self, Found};
use crate::peripheral::task::{Identity, PeripheralTask};
use crate::peripheral::{self, PeripheralInfo, Protocol};
use crate::report_devices::{self, ReportDevice};
use crate::softmacro::KeyboardTriggers;
use crate::sync::LockExt;
use crate::worker::Worker;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// The Send a report page counts as open this long after it last asked for the devices (it asks every
/// two seconds).
const REPORT_PAGE_OPEN: Duration = Duration::from_secs(6);
/// Start of the error shown while the system's HID access cannot be opened.
const HID_UNAVAILABLE: &str = "HID unavailable";
/// The wait before the first new try to open the system's HID access, and the longest wait.
const HID_RETRY_FIRST: Duration = Duration::from_secs(2);
const HID_RETRY_MAX: Duration = Duration::from_secs(30);

/// The wait after the `attempt`-th failure (1 for the first): 2 s, 4 s, 8 s, 16 s, then 30 s.
fn retry_delay(attempt: u32) -> Duration {
    HID_RETRY_FIRST.saturating_mul(1u32 << attempt.saturating_sub(1).min(5)).min(HID_RETRY_MAX)
}

/// Calls `attempt` until it works, waiting longer after each failure (see [`retry_delay`]).
/// `failed` is told the number of the failure, the error and the wait that follows.
fn retry_with_backoff<T, E>(
    mut attempt: impl FnMut() -> std::result::Result<T, E>,
    mut failed: impl FnMut(u32, &E, Duration),
    mut sleep: impl FnMut(Duration),
) -> T {
    let mut failures = 0u32;
    loop {
        match attempt() {
            Ok(value) => return value,
            Err(error) => {
                failures = failures.saturating_add(1);
                let wait = retry_delay(failures);
                failed(failures, &error, wait);
                sleep(wait);
            }
        }
    }
}

/// Runs one step of a detection pass; a panic in it is logged and counts as "nothing changed", so
/// the other steps and the next passes still run. A mutex the step held is recovered by
/// [`LockExt::lock_recover`].
fn guarded(what: &str, step: impl FnOnce() -> bool) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(step)) {
        Ok(changed) => changed,
        Err(_) => {
            log::error!("detection of {what} panicked: skipped for this pass");
            false
        }
    }
}
/// Conflicting programs are looked for every this many detection passes (10 seconds).
/// How often a Corsair receiver without a connected device is asked again.
const CORSAIR_RECEIVER_RETRY: Duration = Duration::from_secs(10);
const CONFLICT_CHECK_EVERY: u32 = 5;
/// While a mouse is open, the receivers are asked for the other devices paired with them every this
/// many passes (10 seconds).
const RECEIVER_SCAN_EVERY: u32 = 5;

const STEELSERIES: u16 = 0x1038;
pub const APEX_3_TKL: u16 = 0x1622;
/// Recognized but not supported yet: never opened, see issue #9.
pub const APEX_3: u16 = 0x161A;
const LOGITECH: u16 = 0x046D;
/// Shown while a mouse PeriHub has not seen yet sleeps behind its receiver.
const WIRELESS_MOUSE: &str = "Wireless mouse";

fn is_keyboard_config_interface(d: &DeviceInfo) -> bool {
    d.vendor_id() == STEELSERIES
        && d.product_id() == APEX_3_TKL
        && d.interface_number() == 1
        && ((d.usage_page() == 0xFFC0 && d.usage() == 1) || d.usage_page() == 0)
}

/// "Long" HID++ collection (report 0x11) of a known mouse. On Windows it has a separate path
/// from the short one.
fn mouse_hidpp_interface(d: &DeviceInfo) -> Option<&'static MouseModel> {
    let long_collection =
        (d.usage_page() == 0xFF00 && d.usage() == 2) || (d.usage_page() == 0 && d.interface_number() == 1);
    if d.vendor_id() == LOGITECH && long_collection { models::find(d.product_id()) } else { None }
}

/// The control interfaces of the Corsair devices PeriHub knows, one per device. On Windows an
/// interface can have more than one collection: the vendor-defined one is preferred (not verified
/// on a real device yet).
fn corsair_control_interfaces(devices: &[DeviceInfo]) -> Vec<(DeviceInfo, &'static corsair::models::CorsairModel)> {
    pick_collections(devices, |d| corsair::models::find(d.product_id()))
}

/// The control interfaces of the Corsair wireless receivers PeriHub knows.
fn corsair_receiver_interfaces(devices: &[DeviceInfo]) -> Vec<(DeviceInfo, ())> {
    pick_collections(devices, |d| corsair::models::is_receiver(d.product_id()).then_some(()))
}

/// One collection of interface 1 per Corsair device that `known` accepts.
fn pick_collections<T: Copy>(devices: &[DeviceInfo], known: impl Fn(&DeviceInfo) -> Option<T>) -> Vec<(DeviceInfo, T)> {
    let mut found: Vec<(DeviceInfo, T)> = Vec::new();
    for d in devices {
        let Some(model) = (d.vendor_id() == corsair::CORSAIR && d.interface_number() == corsair::CONTROL_INTERFACE)
            .then(|| known(d))
            .flatten()
        else {
            continue;
        };
        // Collections of the same interface share the serial number and product id.
        let same =
            found.iter().position(|(o, _)| o.product_id() == d.product_id() && o.serial_number() == d.serial_number());
        match same {
            Some(i) if found[i].0.usage_page() < 0xFF00 && d.usage_page() >= 0xFF00 => found[i] = (d.clone(), model),
            Some(_) => {}
            None => found.push((d.clone(), model)),
        }
    }
    found
}

/// The configuration collection of each device of another brand PeriHub knows, one per device: the
/// one with the usage page its driver talks to, or else the interface a source names. Devices of a
/// brand PeriHub has no driver for, and models missing from the tables, are never opened.
fn peripheral_collections(devices: &[DeviceInfo]) -> Vec<(DeviceInfo, &'static peripheral::Model)> {
    let same = |a: &DeviceInfo, b: &DeviceInfo| {
        a.vendor_id() == b.vendor_id() && a.product_id() == b.product_id() && a.serial_number() == b.serial_number()
    };
    let mut found: Vec<(DeviceInfo, &'static peripheral::Model)> = Vec::new();
    // On Windows an interface can have several collections: a vendor-defined one goes first.
    let mut ordered: Vec<&DeviceInfo> = devices.iter().collect();
    ordered.sort_by_key(|d| d.usage_page() < 0xFF00);
    for by_usage in [true, false] {
        for d in ordered.iter().copied() {
            let Some(model) = peripheral::find(d.vendor_id(), d.product_id()) else { continue };
            let matches = if by_usage {
                model.usage_page == Some(d.usage_page())
            } else {
                model.interface == Some(d.interface_number())
            };
            if matches && !found.iter().any(|(o, _)| same(o, d)) {
                found.push((d.clone(), model));
            }
        }
    }
    found
}

/// The HID++ collection with `usage` (2 long, 1 short) of the same interface as `of`, a Logitech
/// HID++ 1.0 mouse. On Windows each has a path of its own; on Linux they are the same node.
fn hidpp10_collection<'a>(devices: &'a [DeviceInfo], of: &DeviceInfo, usage: u16) -> Option<&'a DeviceInfo> {
    devices.iter().find(|d| {
        d.vendor_id() == of.vendor_id()
            && d.product_id() == of.product_id()
            && d.serial_number() == of.serial_number()
            && d.interface_number() == of.interface_number()
            && d.usage_page() == 0xFF00
            && d.usage() == usage
    })
}

/// HID++ collection of a receiver: usage 2 is the long one, 1 the short one. On Linux both are the
/// same hidraw node.
fn receiver_collection(d: &DeviceInfo, usage: u16) -> bool {
    d.vendor_id() == LOGITECH
        && receiver::is_receiver(d.product_id())
        && ((d.usage_page() == 0xFF00 && d.usage() == usage) || (d.usage_page() == 0 && d.interface_number() == 2))
}

/// A Logitech device PeriHub reads on its own USB connection (see [`direct`]).
struct DirectPaths {
    model: &'static DirectModel,
    /// What it is kept under: the path of its first HID++ collection, or of its first collection.
    key: CString,
    /// Its HID++ collections, in the order they are tried.
    collections: Vec<DeviceInfo>,
    /// Every collection of the device, for the test report.
    listed: Vec<String>,
    /// Why PeriHub does not open it, if it does not.
    closed: Option<String>,
}

/// While a device on its own USB connection has not answered, it is asked again this often: each
/// try waits up to a second for each collection and index.
const DIRECT_RETRY: Duration = Duration::from_secs(30);
/// How often a device that answered before, and whose thread ended, is opened again.
const DIRECT_REOPEN: Duration = Duration::from_secs(10);

fn collection_text(d: &DeviceInfo) -> String {
    format!("interface {}, usage page {:04x}, usage {:04x}", d.interface_number(), d.usage_page(), d.usage())
}

/// The Logitech devices of [`direct::DIRECT`] connected, one per device. A collection whose path
/// also carries a keyboard's typing is never opened: on Linux one node holds every collection of an
/// interface, and hidapi lists the keyboard collection with the same path (or, when it reads no
/// usage, cannot tell).
fn direct_devices(devices: &[DeviceInfo]) -> Vec<DirectPaths> {
    let mut found: Vec<DirectPaths> = Vec::new();
    let mut seen: Vec<(u16, Option<String>)> = Vec::new();
    for d in devices {
        let Some(model) = (d.vendor_id() == LOGITECH).then(|| direct::find(d.product_id())).flatten() else { continue };
        let id = (d.product_id(), d.serial_number().map(str::to_owned));
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let same: Vec<&DeviceInfo> = devices
            .iter()
            .filter(|o| o.vendor_id() == LOGITECH && (o.product_id(), o.serial_number().map(str::to_owned)) == id)
            .collect();
        let mut listed: Vec<String> = same.iter().map(|o| collection_text(o)).collect();
        listed.sort();
        listed.dedup();
        let typing = |o: &DeviceInfo| {
            same.iter().any(|k| k.path() == o.path() && k.usage_page() == 1 && k.usage() == 6)
                || (model.typing && o.usage_page() == 0)
        };
        let hidpp: Vec<&DeviceInfo> = same
            .iter()
            .copied()
            .filter(|o| o.interface_number() == model.interface && direct::hidpp_collection(o.usage_page()))
            .collect();
        let mut collections: Vec<DeviceInfo> = Vec::new();
        for o in hidpp.iter().filter(|o| !typing(o)) {
            if !collections.iter().any(|c| c.path() == o.path()) {
                collections.push((*o).clone());
            }
        }
        collections.sort_by_key(|c| direct::try_order(c.usage_page(), c.usage()));
        let closed = if !collections.is_empty() {
            None
        } else if hidpp.is_empty() {
            Some(format!("no HID++ collection (usage page ff43 or ff00) on interface {}", model.interface))
        } else {
            Some("its HID++ collection shares a device node with the keyboard's typing (Linux): PeriHub does not open it".into())
        };
        let key = collections.first().map_or_else(|| d.path().to_owned(), |c| c.path().to_owned());
        found.push(DirectPaths { model, key, collections, listed, closed });
    }
    found
}

/// What PeriHub knows of a device on its own USB connection.
struct DirectState {
    /// What it read, once the device answered.
    device: Option<UnsupportedDevice>,
    /// The collection and device index it answered on.
    reached: Option<(CString, u8)>,
    /// What each collection and index answered, for the test report.
    answers: Vec<String>,
    last_try: Option<Instant>,
    /// The system refused to open it (on Linux, no udev rule gives access to it).
    denied: bool,
}

/// A receiver's long HID++ collection, and its short one when that has a path of its own (Windows).
type ReceiverPaths = (DeviceInfo, Option<DeviceInfo>);

/// What a receiver's slots hold.
#[derive(Default)]
struct Probe {
    /// Slots whose device answers.
    online: Vec<u8>,
    /// Slots whose paired device is turned off or asleep.
    asleep: Vec<u8>,
    /// What each slot answered, for test reports.
    answers: Vec<String>,
}

fn open_channel(api: &HidApi, info: &DeviceInfo) -> Result<hidapi::HidDevice> {
    info.open_device(api).map_err(Error::from)
}

fn probe(api: &HidApi, (long, short): &ReceiverPaths) -> Result<Probe> {
    let long_dev = open_channel(api, long)?;
    let short_dev = short.as_ref().map(|d| open_channel(api, d)).transpose()?;
    // On Windows the ping goes to the short collection, and the answer is read on both.
    let (channel, long): (&dyn Transport, Option<&dyn Transport>) = match &short_dev {
        Some(dev) => (dev, Some(&long_dev)),
        None => (&long_dev, None),
    };
    let mut found = Probe::default();
    let mut timeouts = 0;
    for slot in receiver::SLOTS {
        let (presence, answer) = receiver::ping_answer(channel, long, slot)?;
        found.answers.push(format!("slot {slot}: {answer}"));
        match presence {
            Some(Presence::Online) => found.online.push(slot),
            Some(Presence::Asleep) => found.asleep.push(slot),
            Some(Presence::Empty) => {}
            None => {
                // A receiver that does not answer twice: nothing more to ask it in this pass.
                timeouts += 1;
                if timeouts == 2 {
                    break;
                }
            }
        }
    }
    Ok(found)
}

fn open_behind(api: &HidApi, (long, short): &ReceiverPaths, slot: u8, read_only: bool) -> Result<HidppDevice> {
    let short_dev = match short {
        Some(info) => Some(Box::new(open_channel(api, info)?) as Box<dyn Transport>),
        None => None,
    };
    HidppDevice::behind_receiver(open_channel(api, long)?, short_dev, slot, read_only)
}

/// The model of the device in `slot`, or a description of a device PeriHub does not support.
fn identify(
    api: &HidApi,
    paths: &ReceiverPaths,
    slot: u8,
) -> Result<std::result::Result<&'static MouseModel, UnsupportedDevice>> {
    let mut dev = open_behind(api, paths, slot, true)?;
    let model = match dev.wireless_id() {
        Ok(id) => id.and_then(models::find_wireless),
        Err(Error::Unreachable) => return Err(Error::Unreachable),
        // Without the list of ids (on a Logi Bolt receiver it can lack the LIGHTSPEED one) the
        // device is not one PeriHub knows: the test report shows what it has.
        Err(_) => None,
    };
    match model {
        Some(model) => Ok(Ok(model)),
        None => Ok(Err(UnsupportedDevice::read_recorded(&mut dev, slot, paths.0.product_id())?)),
    }
}

/// The HID collections of every Logitech receiver connected, by product id, for test reports:
/// they show why PeriHub cannot reach a receiver.
fn receiver_collections(api: &HidApi) -> Vec<(u16, Vec<String>)> {
    let mut found: Vec<(u16, Vec<String>)> = Vec::new();
    for d in api.device_list().filter(|d| d.vendor_id() == LOGITECH && receiver::is_receiver(d.product_id())) {
        let collection =
            format!("interface {}, usage page {:04x}, usage {:04x}", d.interface_number(), d.usage_page(), d.usage());
        match found.iter_mut().find(|(id, _)| *id == d.product_id()) {
            Some((_, list)) if list.contains(&collection) => {}
            Some((_, list)) => list.push(collection),
            None => found.push((d.product_id(), vec![collection])),
        }
    }
    for (_, list) in &mut found {
        list.sort();
    }
    found.sort_by_key(|(id, _)| *id);
    found
}

/// A Logitech receiver that is connected, for the test report that helps support what is paired
/// with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiverReport {
    pub product_id: u16,
    /// "LIGHTSPEED receiver", "Logi Bolt receiver" or "Unifying receiver".
    pub name: &'static str,
    /// Its HID collections, such as "interface 2, usage page ff00, usage 0002".
    pub collections: Vec<String>,
    /// A Logi Bolt receiver: nothing paired with it is supported yet.
    pub bolt: bool,
    /// A Unifying receiver: nothing paired with it is supported yet, and on Linux PeriHub's udev
    /// rule does not open it (see [`receiver::UNIFYING`]).
    pub unifying: bool,
    /// PeriHub found the collection it talks to the paired devices through.
    pub reachable: bool,
    /// The system refused to open it (on Linux, no udev rule gives access to it).
    pub denied: bool,
    /// What each of its slots answered the last time PeriHub asked, or why it could not ask.
    pub slots: Vec<String>,
    /// Slots whose device PeriHub has open by its features: its DPI can be read and changed now.
    pub open_slots: Vec<u8>,
    /// Paired devices that answer and that PeriHub does not support yet.
    pub unsupported: Vec<UnsupportedDevice>,
    /// Not a receiver: a device on its own USB connection (see [`direct`]). `name` says how it
    /// connects, `slots` what its HID++ collections answered, and `unsupported` holds the device
    /// itself, always, with what PeriHub could read of it; its "slot" is the HID++ device index.
    pub direct: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceStatus {
    pub keyboard: Option<KeyboardInfo>,
    pub mouse: Option<MouseInfo>,
    /// Last connection error, to show to the user (e.g. missing permissions).
    pub keyboard_error: Option<String>,
    pub mouse_error: Option<String>,
    /// A mouse paired with a connected LIGHTSPEED receiver that is turned off or asleep: its
    /// model, if PeriHub has seen it since it started.
    pub mouse_asleep: Option<String>,
    /// A keyboard PeriHub recognizes but does not support yet: it is not opened.
    pub unsupported_keyboard: Option<String>,
    /// Running programs that take control of the devices, such as Logitech G HUB.
    pub conflicts: Vec<String>,
    /// A Logitech G300.
    pub g300: Option<G300Info>,
    pub g300_error: Option<String>,
    /// Corsair mice and keyboards (experimental support).
    pub corsair: Vec<CorsairInfo>,
    pub corsair_error: Option<String>,
    /// Mice and keyboards of other brands, from published protocol documentation (experimental).
    pub peripherals: Vec<PeripheralInfo>,
    pub peripheral_error: Option<String>,
    /// Keyboards with QMK firmware found by their raw HID collection, open or not.
    pub qmk: Vec<QmkKeyboard>,
    /// Connected Logitech receivers (LIGHTSPEED and Logi Bolt).
    pub receivers: Vec<ReceiverReport>,
    /// Running programs that set their own DPI and other settings on Logitech devices (Solaar,
    /// logiops): they may undo what PeriHub sets on the devices it knows by their features.
    pub logitech_programs: Vec<String>,
    /// PeriHub runs in a Flatpak sandbox and cannot see the other programs running (see
    /// [`conflicts::sandboxed`]).
    pub sandboxed: bool,
    /// The first detection pass is over. Until then the lists above are empty because nothing was looked
    /// at yet, not because nothing is connected: the interface must not offer a report for it.
    pub scanned: bool,
}

/// A keyboard with QMK firmware: PeriHub reads Vial keyboards by itself, and other keyboards with a raw
/// HID collection only once the user turns them on as VIA keyboards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QmkKeyboard {
    /// "vvvv:pppp", as in the peripherals.
    pub key: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub brand: String,
    pub name: String,
    pub vial: bool,
    /// PeriHub may open it: a Vial keyboard, or one the user turned on.
    pub enabled: bool,
    /// Open now, with a page of its own.
    pub open: bool,
    /// The system refused to open it (on Linux, no rule opens its raw HID interface).
    pub denied: bool,
}

#[derive(Default)]
struct Slots {
    keyboard: Option<(Worker<KeyboardTask>, KeyboardInfo)>,
    mouse: Option<(Worker<MouseTask>, MouseInfo)>,
    /// The Logitech G300, by the path of its vendor collection.
    g300: Option<(CString, Worker<G300Task>, G300Info)>,
    /// G300 that failed to open: not retried until it is plugged in again.
    g300_failed: Option<CString>,
    g300_error: Option<String>,
    /// Open Corsair devices, by the path of their control interface.
    corsair: Vec<(CString, Worker<CorsairTask>, CorsairInfo)>,
    corsair_error: Option<String>,
    /// Corsair devices that failed to open: not retried until they are plugged in again.
    corsair_failed: Vec<CString>,
    /// Corsair receivers without a connected device, and when they were last asked.
    corsair_waiting: HashMap<CString, std::time::Instant>,
    /// Open devices of other brands, by the path of their configuration collection.
    peripherals: Vec<(CString, Worker<PeripheralTask>, PeripheralInfo)>,
    peripheral_error: Option<String>,
    /// Devices of other brands that failed to open: not retried until they are plugged in again.
    peripheral_failed: Vec<CString>,
    /// Devices that did not answer (asleep, or away from their receiver), and when they were asked.
    peripheral_waiting: HashMap<CString, std::time::Instant>,
    /// QMK keyboards found, and the paths of those the system refused to open (asked again at every
    /// pass: a rule may have been installed since).
    qmk: Vec<QmkKeyboard>,
    qmk_denied: HashSet<CString>,
    /// Set by the mouse's thread while another program controls the mouse.
    mouse_host_mode: Option<Arc<AtomicBool>>,
    reported_host_mode: bool,
    conflicts: Vec<Conflict>,
    keyboard_error: Option<String>,
    mouse_error: Option<String>,
    /// Path of the open mouse, to notice when it goes away while another one is connected.
    mouse_path: Option<CString>,
    /// Unverified mouse that failed to open: not retried until it is plugged in again.
    mouse_failed: Option<CString>,
    mouse_asleep: Option<String>,
    /// What the receivers have that PeriHub cannot open, shown while no mouse is open.
    receiver_note: Option<String>,
    unsupported_keyboard: Option<String>,
    /// Model last seen behind each receiver (by path), to name it while it sleeps.
    receiver_models: HashMap<CString, &'static str>,
    /// Devices PeriHub does not support, by receiver and slot: not identified again.
    receiver_unsupported: HashMap<(CString, u8), UnsupportedDevice>,
    /// Connected Logitech receivers, for test reports.
    receivers: Vec<ReceiverReport>,
    /// What each receiver's slots answered (by path), for test reports.
    receiver_answers: HashMap<CString, Vec<String>>,
    /// Receivers the system refused to open (by path).
    receiver_denied: HashSet<CString>,
    /// Slots that answered the last time each receiver was asked (by path).
    receiver_online: HashMap<CString, Vec<u8>>,
    /// Devices PeriHub knows by their features, open on their own thread, by receiver path and slot,
    /// with the receiver's product id. A device on its own USB connection is here too, by the path of
    /// its HID++ collection and its device index, with its own product id.
    logitech: HashMap<(CString, u8), (Worker<LogitechTask>, u16)>,
    /// Logitech devices on their own USB connection, by [`DirectPaths::key`].
    direct: HashMap<CString, DirectState>,
    logitech_programs: Vec<&'static str>,
    /// Interfaces of the connected keyboard that report key presses.
    key_interfaces: KeyInterfaces,
    /// The connected devices PeriHub does not know, for a device request.
    report_devices: Vec<ReportDevice>,
    /// The first detection pass is over.
    scanned: bool,
    /// What was read from the nodes of those devices (their report descriptors), once each.
    report_details: report_devices::DetailsCache,
    /// The Send a report page asked for the list of devices this recently: only then are the feature
    /// reports of those devices read (see [`crate::feature_reports`]), never in the background.
    report_page_seen: Option<std::time::Instant>,
    /// Keys of the connected keyboard's map that start software macros, kept by its thread.
    keyboard_macro_keys: Option<Arc<Mutex<Vec<u8>>>>,
}

pub type Listener = Box<dyn Fn(&DeviceStatus) + Send + Sync>;

#[derive(Clone)]
pub struct Manager {
    slots: Arc<Mutex<Slots>>,
    keyboard_config: Arc<Mutex<KeyboardConfig>>,
    /// Mice whose memory format is untested may be saved to (see [`MouseTask::open`]).
    untested_saves: bool,
    /// The DPI to set on the devices PeriHub knows by their features, by settings key.
    logitech_dpi: Arc<Mutex<HashMap<String, u16>>>,
    /// The lights of the keyboards PeriHub knows by their features, by settings key: the look it
    /// shows again every time such a keyboard connects (this session only), the last look shown
    /// (saved in the settings), and how host mode went.
    logitech_lights: Arc<Mutex<HashMap<String, LightsMemory>>>,
    /// Keyboards with a raw HID collection the user turned on as VIA keyboards, by "vvvv:pppp".
    qmk_enabled: Arc<Mutex<HashSet<String>>>,
}

impl Manager {
    /// Starts detection in the background. `listener` is called on every change.
    /// `untested_saves`: a mouse whose memory format is untested may be saved to, each save undone
    /// unless the user keeps it. Only the interface asks that; the command line never saves to one.
    /// `logitech_dpi`: the DPI to set on the devices PeriHub knows by their features every time they
    /// connect, by settings key (see [`Manager::set_logitech_dpi`]). `qmk_keyboards`: the keyboards
    /// the user turned on as VIA keyboards (see [`Manager::set_qmk_enabled`]). `logitech_looks`: the
    /// last look shown on each keyboard whose lights PeriHub drove; `logitech_drive`: the keyboards
    /// whose lights the user wants PeriHub to drive, taken with that look whenever they connect.
    pub fn start(
        keyboard_config: KeyboardConfig,
        untested_saves: bool,
        logitech_dpi: HashMap<String, u16>,
        logitech_looks: HashMap<String, Look>,
        logitech_drive: Vec<String>,
        qmk_keyboards: Vec<String>,
        listener: Listener,
    ) -> Self {
        let manager = Self {
            slots: Arc::new(Mutex::new(Slots::default())),
            keyboard_config: Arc::new(Mutex::new(keyboard_config)),
            untested_saves,
            logitech_dpi: Arc::new(Mutex::new(logitech_dpi)),
            logitech_lights: Arc::new(Mutex::new(
                logitech_looks
                    .into_iter()
                    .map(|(key, look)| {
                        let wanted = logitech_drive.contains(&key);
                        let memory = LightsMemory { saved: Some(look), ..LightsMemory::default() };
                        memory.wanted.store(wanted, std::sync::atomic::Ordering::SeqCst);
                        (key, memory)
                    })
                    .collect(),
            )),
            qmk_enabled: Arc::new(Mutex::new(qmk_keyboards.into_iter().collect())),
        };
        let background = manager.clone();
        thread::Builder::new()
            .name("perihub-hotplug".into())
            .spawn(move || background.hotplug_loop(listener))
            .expect("failed to start device detection");
        manager
    }

    fn hotplug_loop(&self, listener: Listener) {
        // At login (autostart) hidraw or its permissions may not be ready yet: try again, slowly,
        // instead of leaving PeriHub without detection until it is restarted.
        let mut api = retry_with_backoff(
            HidApi::new,
            |attempt, error, wait| {
                log::warn!("HID unavailable (attempt {attempt}), trying again in {} s: {error}", wait.as_secs());
                let mut slots = self.slots.lock_recover();
                slots.keyboard_error = Some(format!("{HID_UNAVAILABLE}, trying again: {error}"));
                slots.mouse_error = slots.keyboard_error.clone();
                slots.scanned = true;
                drop(slots);
                listener(&self.status());
            },
            thread::sleep,
        );
        {
            // The message of the failed attempts goes away with the first success.
            let mut guard = self.slots.lock_recover();
            let slots = &mut *guard;
            for error in [&mut slots.keyboard_error, &mut slots.mouse_error] {
                if error.as_deref().is_some_and(|e| e.starts_with(HID_UNAVAILABLE)) {
                    *error = None;
                }
            }
            drop(guard);
            listener(&self.status());
        }
        let mut pass = 0u32;
        loop {
            // A panic in the code for one kind of device must not end the detection of all.
            let mut changed =
                guarded("receivers and Logitech", || self.poll(&mut api, pass.is_multiple_of(RECEIVER_SCAN_EVERY)));
            changed |= guarded("Corsair", || self.poll_corsair(&api));
            changed |= guarded("G300", || self.poll_g300(&api));
            changed |= guarded("other brands", || self.poll_peripherals(&api));
            if pass.is_multiple_of(CONFLICT_CHECK_EVERY) {
                changed |= guarded("conflicts", || self.check_conflicts());
            }
            // The first pass is always announced, also when it finds nothing: that is the answer.
            let first = !std::mem::replace(&mut self.slots.lock_recover().scanned, true);
            if changed || first {
                listener(&self.status());
            }
            pass = pass.wrapping_add(1);
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// One detection pass. Returns `true` if anything changed.
    /// `scan`: ask the receivers for their devices even while a mouse is open.
    fn poll(&self, api: &mut HidApi, scan: bool) -> bool {
        if let Err(error) = api.refresh_devices() {
            log::warn!("HID enumeration failed: {error}");
            return false;
        }
        // Reading descriptors can take a while (Windows opens each node): not under the lock.
        let (mut cache, page_open) = {
            let mut slots = self.slots.lock_recover();
            let open = slots.report_page_seen.is_some_and(|seen| seen.elapsed() < REPORT_PAGE_OPEN);
            (std::mem::take(&mut slots.report_details), open)
        };
        let report = report_devices::unknown_devices(report_devices::entries(api, &mut cache, page_open));
        let mut slots = self.slots.lock_recover();
        slots.report_devices = report;
        slots.report_details = cache;
        drop(slots);
        let keyboard = api.device_list().find(|d| is_keyboard_config_interface(d)).cloned();
        let key_interface = |number| {
            api.device_list()
                .find(|d| {
                    d.vendor_id() == STEELSERIES && d.product_id() == APEX_3_TKL && d.interface_number() == number
                })
                .map(|d| d.path().to_owned())
        };
        let key_interfaces = KeyInterfaces { boot: key_interface(0), bitmap: key_interface(2) };
        let mice: Vec<(DeviceInfo, &'static MouseModel)> =
            api.device_list().filter_map(|d| mouse_hidpp_interface(d).map(|m| (d.clone(), m))).collect();
        let receivers: Vec<ReceiverPaths> = api
            .device_list()
            .filter(|d| receiver_collection(d, 2))
            .map(|long| {
                let short = api.device_list().find(|d| {
                    receiver_collection(d, 1) && d.product_id() == long.product_id() && d.path() != long.path()
                });
                (long.clone(), short.cloned())
            })
            .collect();
        let unsupported_keyboard = api
            .device_list()
            .any(|d| d.vendor_id() == STEELSERIES && d.product_id() == APEX_3)
            .then(|| "SteelSeries Apex 3".to_string());
        let mut changed = false;
        {
            let mut slots = self.slots.lock_recover();
            if slots.unsupported_keyboard != unsupported_keyboard {
                if let Some(name) = &unsupported_keyboard {
                    log::info!("{name} connected: not supported yet, so PeriHub does not open it");
                }
                slots.unsupported_keyboard = unsupported_keyboard;
                changed = true;
            }
        }

        let keyboard_present = {
            let mut slots = self.slots.lock_recover();
            if slots.keyboard.as_ref().is_some_and(|(w, _)| !w.is_alive() || keyboard.is_none()) {
                slots.keyboard = None;
                log::info!("keyboard disconnected");
                slots.key_interfaces = KeyInterfaces::default();
                slots.keyboard_macro_keys = None;
                changed = true;
            }
            slots.keyboard.is_some()
        };
        if let (false, Some(info)) = (keyboard_present, &keyboard) {
            let config = self.keyboard_config.lock_recover().clone();
            let result = info
                .open_device(api)
                .map_err(Error::from)
                .and_then(|dev| KeyboardTask::open(Apex3Tkl::new(dev), &config, key_interfaces.clone()));
            let mut slots = self.slots.lock_recover();
            let error = match result {
                Ok(task) => {
                    let info = task.info.clone();
                    log::info!("keyboard connected: {}, firmware {}", info.name, info.firmware);
                    slots.keyboard_macro_keys = Some(task.macro_keys_handle());
                    slots.keyboard = Some((Worker::spawn("perihub-keyboard", task), info));
                    slots.key_interfaces = key_interfaces;
                    None
                }
                Err(error) => Some(open_error_message(error)),
            };
            changed |= slots.keyboard.is_some() || slots.keyboard_error != error;
            if let Some(message) = error.as_ref().filter(|_| slots.keyboard_error != error) {
                log::warn!("keyboard: {message}");
            }
            slots.keyboard_error = error;
        }

        let connected = |path: &Option<CString>| {
            mice.iter().any(|(d, _)| Some(d.path()) == path.as_deref())
                || receivers.iter().any(|(d, _)| Some(d.path()) == path.as_deref())
        };
        let candidate = {
            let mut slots = self.slots.lock_recover();
            if slots.mouse.as_ref().is_some_and(|(w, _)| !w.is_alive()) || !connected(&slots.mouse_path) {
                if slots.mouse.take().is_some() {
                    log::info!("mouse disconnected");
                    changed = true;
                }
                slots.mouse_path = None;
            }
            if !connected(&slots.mouse_failed) {
                slots.mouse_failed = None;
            }
            slots.receiver_models.retain(|path, _| receivers.iter().any(|(d, _)| d.path() == path.as_c_str()));
            slots.receiver_answers.retain(|path, _| receivers.iter().any(|(d, _)| d.path() == path.as_c_str()));
            slots.receiver_denied.retain(|path| receivers.iter().any(|(d, _)| d.path() == path.as_c_str()));
            slots.receiver_online.retain(|path, _| receivers.iter().any(|(d, _)| d.path() == path.as_c_str()));
            slots
                .receiver_unsupported
                .retain(|(path, _), _| receivers.iter().any(|(d, _)| d.path() == path.as_c_str()));
            if slots.mouse.is_some() {
                None
            } else {
                // A tested model wins if more than one supported mouse is connected.
                let failed = slots.mouse_failed.clone();
                Some(mice.iter().filter(|(d, _)| Some(d.path()) != failed.as_deref()).max_by_key(|(_, m)| m.verified))
            }
        };
        match candidate {
            Some(Some((info, model))) => {
                let result = info
                    .open_device(api)
                    .map_err(Error::from)
                    // Read-only until the mouse's memory format says more (see `mouse::support`).
                    .and_then(|dev| HidppDevice::new(dev, model.device_index, true))
                    .and_then(|dev| MouseTask::open(dev, model, self.untested_saves));
                changed |= self.install_mouse(result, info.path(), model, None);
            }
            // No mouse with a cable: look behind the receivers.
            Some(None) => changed |= self.watch_receivers(api, &receivers),
            None => {}
        }
        {
            let mut slots = self.slots.lock_recover();
            if slots.mouse.is_some() || receivers.is_empty() {
                changed |= slots.mouse_asleep.take().is_some() | slots.receiver_note.take().is_some();
            }
        }
        let direct =
            direct_devices(&api.device_list().filter(|d| d.vendor_id() == LOGITECH).cloned().collect::<Vec<_>>());
        changed |= self.poll_direct(api, &direct);
        changed |= self.poll_logitech(api, &receivers, scan);
        let mut reports = self.receiver_reports(api, &receivers);
        reports.extend(self.direct_reports(&direct));
        {
            let mut slots = self.slots.lock_recover();
            if slots.receivers != reports {
                for report in reports.iter().filter(|r| !r.reachable && !slots.receivers.contains(r)) {
                    log::info!(
                        "{} {:04x}: no HID++ collection to reach it through ({})",
                        report.name,
                        report.product_id,
                        report.collections.join("; ")
                    );
                }
                slots.receivers = reports;
                changed = true;
            }
        }
        // The mouse's thread notices when another program takes the mouse or gives it back.
        let mut slots = self.slots.lock_recover();
        let host_mode =
            slots.mouse.is_some() && slots.mouse_host_mode.as_ref().is_some_and(|f| f.load(Ordering::SeqCst));
        if host_mode != slots.reported_host_mode {
            slots.reported_host_mode = host_mode;
            changed = true;
        }
        changed
    }

    /// Keeps the mouse that `result` opened, or its error. `via` describes the receiver for the log.
    /// Returns `true` if the status changed.
    fn install_mouse(
        &self,
        result: Result<MouseTask>,
        path: &CStr,
        model: &'static MouseModel,
        via: Option<String>,
    ) -> bool {
        let mut slots = self.slots.lock_recover();
        let error = match result {
            Ok(task) => {
                let mouse = task.info.clone();
                slots.mouse_host_mode = Some(task.host_mode_flag());
                slots.reported_host_mode = mouse.host_mode;
                log::info!(
                    "mouse connected: {} ({}{}), firmware {}",
                    mouse.model,
                    match (mouse.verified, mouse.experimental, mouse.writable) {
                        (true, _, true) => "tested memory format",
                        (true, _, false) => "tested memory format, no saves",
                        (false, true, _) => "experimental saves, untested memory format",
                        (false, false, _) => "read-only",
                    },
                    via.as_deref().map(|via| format!(", through {via}")).unwrap_or_default(),
                    mouse.firmware.iter().map(|f| format!("{} {}", f.name, f.version)).collect::<Vec<_>>().join(", ")
                );
                if mouse.receiver {
                    slots.receiver_models.insert(path.to_owned(), model.name);
                }
                slots.receiver_note = None;
                slots.mouse = Some((Worker::spawn("perihub-mouse", task), mouse));
                slots.mouse_path = Some(path.to_owned());
                None
            }
            // It fell asleep while it was being opened: tried again at the next pass.
            Err(Error::Unreachable) => None,
            Err(error) => {
                if !model.verified && !is_permission_error(&error) {
                    slots.mouse_failed = Some(path.to_owned());
                }
                Some(format!("{}: {}", model.name, open_error_message(error)))
            }
        };
        let changed = slots.mouse.is_some() || slots.mouse_error != error;
        if let Some(message) = error.as_ref().filter(|_| slots.mouse_error != error) {
            log::warn!("mouse: {message}");
        }
        slots.mouse_error = error;
        changed
    }

    /// The connected receivers, with the devices PeriHub found behind them and does not support.
    fn receiver_reports(&self, api: &HidApi, receivers: &[ReceiverPaths]) -> Vec<ReceiverReport> {
        let slots = self.slots.lock_recover();
        receiver_collections(api)
            .into_iter()
            .map(|(product_id, collections)| {
                let paths: Vec<&CStr> =
                    receivers.iter().filter(|(d, _)| d.product_id() == product_id).map(|(d, _)| d.path()).collect();
                let mut unsupported: Vec<UnsupportedDevice> = slots
                    .receiver_unsupported
                    .iter()
                    .filter(|((path, _), _)| paths.contains(&path.as_c_str()))
                    .map(|(_, device)| device.clone())
                    .collect();
                unsupported.sort_by_key(|d| d.slot);
                let answers = slots
                    .receiver_answers
                    .iter()
                    .filter(|(path, _)| paths.contains(&path.as_c_str()))
                    .flat_map(|(_, answers)| answers.iter().cloned())
                    .collect();
                let mut open_slots: Vec<u8> = slots
                    .logitech
                    .keys()
                    .filter(|(path, _)| paths.contains(&path.as_c_str()))
                    .map(|&(_, slot)| slot)
                    .collect();
                open_slots.sort();
                ReceiverReport {
                    open_slots,
                    product_id,
                    name: receiver::name(product_id),
                    collections,
                    bolt: product_id == receiver::BOLT,
                    unifying: receiver::is_unifying(product_id),
                    reachable: !paths.is_empty(),
                    denied: paths.iter().any(|path| slots.receiver_denied.contains(*path)),
                    slots: answers,
                    unsupported,
                    direct: false,
                }
            })
            .collect()
    }

    /// Looks for a mouse behind the receivers and opens the first one that answers,
    /// read-only until its model has been tested. Returns `true` if the status changed.
    fn watch_receivers(&self, api: &HidApi, receivers: &[ReceiverPaths]) -> bool {
        let failed = self.slots.lock_recover().mouse_failed.clone();
        let mut asleep = None;
        let mut note = None;
        for paths in receivers.iter().filter(|(long, _)| Some(long.path()) != failed.as_deref()) {
            let path = paths.0.path().to_owned();
            let found = probe(api, paths);
            let answers = match &found {
                Ok(found) => found.answers.clone(),
                Err(error) => vec![format!("could not ask: {error}")],
            };
            {
                let mut slots = self.slots.lock_recover();
                let online = found.as_ref().map(|f| f.online.clone()).unwrap_or_default();
                let sleeping = found.as_ref().is_ok_and(|f| !f.asleep.is_empty());
                // Once per change: what the slots of a receiver with nothing to open answered.
                let previous = slots.receiver_answers.insert(path.clone(), answers.clone());
                if online.is_empty() && !sleeping && previous.as_ref() != Some(&answers) {
                    let id = paths.0.product_id();
                    log::info!("{} {id:04x}: nothing paired answers ({})", receiver::name(id), answers.join("; "));
                }
                slots.receiver_online.insert(path.clone(), online);
                if matches!(&found, Err(error) if is_permission_error(error)) {
                    slots.receiver_denied.insert(path.clone());
                } else {
                    slots.receiver_denied.remove(&path);
                }
            }
            let found = match found {
                Ok(found) => found,
                // On Linux the udev rule leaves a Unifying receiver closed on purpose: the page says
                // why, instead of asking for the rule.
                Err(error) if receiver::is_unifying(paths.0.product_id()) && is_permission_error(&error) => continue,
                Err(error) => {
                    note = Some(format!("{}: {}", receiver::name(paths.0.product_id()), open_error_message(error)));
                    continue;
                }
            };
            let receiver = format!("{} {:04x}", receiver::name(paths.0.product_id()), paths.0.product_id());
            // A device with a page of its own (a keyboard, say) asleep is not a mouse asleep (issue #30).
            let mouse_asleep = {
                let slots = self.slots.lock_recover();
                found.asleep.iter().any(|&slot| !slots.receiver_unsupported.contains_key(&(path.clone(), slot)))
            };
            if mouse_asleep {
                let seen = self.slots.lock_recover().receiver_models.get(&path).copied();
                if asleep.is_none() {
                    asleep = Some((seen.unwrap_or(WIRELESS_MOUSE).to_string(), receiver.clone()));
                }
            }
            for slot in found.online {
                let known = self.slots.lock_recover().receiver_unsupported.get(&(path.clone(), slot)).cloned();
                let identity = match known {
                    Some(device) => Ok(Err(device)),
                    None => identify(api, paths, slot),
                };
                let model = match identity {
                    Ok(Ok(model)) => model,
                    // It gets a page of its own (see `poll_logitech`): no note on the mouse entry, which
                    // would list a mouse that is not there (issue #30).
                    Ok(Err(device)) => {
                        self.slots.lock_recover().receiver_unsupported.insert((path.clone(), slot), device);
                        continue;
                    }
                    Err(Error::Unreachable) => continue,
                    Err(error) => {
                        note = Some(format!("{receiver}: {}", open_error_message(error)));
                        continue;
                    }
                };
                let wireless_id = model.wireless_id.unwrap_or_default();
                let link = ReceiverLink { receiver_id: paths.0.product_id(), slot, wireless_id };
                // Over the radio `mouse::support` grants experimental saves on a tested memory, RAM changes otherwise.
                let result = open_behind(api, paths, slot, true)
                    .and_then(|dev| MouseTask::open(dev, model, self.untested_saves))
                    .map(|mut task| {
                        task.info.link = Some(link);
                        task
                    });
                let via = format!("{receiver}, slot {slot}, wireless id {wireless_id:04x}");
                return self.install_mouse(result, &path, model, Some(via));
            }
        }
        let mut slots = self.slots.lock_recover();
        let mut changed = false;
        let asleep_name = asleep.as_ref().map(|(name, _)| name.clone());
        if slots.mouse_asleep != asleep_name {
            if let Some((name, receiver)) = &asleep {
                log::info!("{name} behind {receiver}: turned off or asleep");
            }
            slots.mouse_asleep = asleep_name;
            changed = true;
        }
        if slots.receiver_note != note {
            if let Some(message) = &note {
                log::warn!("mouse: {message}");
            }
            slots.receiver_note = note;
            changed = true;
        }
        changed
    }

    /// Opens a thread for every device behind a receiver that PeriHub knows by its features and that
    /// answers, and drops the threads of the devices that stopped answering (they are opened again,
    /// with their DPI set again, when they are back). `watch_receivers` asks the receivers only while
    /// no mouse is open: then they are asked here, when `scan` is set. Returns `true` if a device was
    /// found.
    fn poll_logitech(&self, api: &HidApi, receivers: &[ReceiverPaths], scan: bool) -> bool {
        let present = |path: &CStr| receivers.iter().any(|(d, _)| d.path() == path);
        let mut changed = false;
        let mouse_link = {
            let mut slots = self.slots.lock_recover();
            let direct: Vec<(CString, u8)> = slots.direct.values().filter_map(|s| s.reached.clone()).collect();
            slots.logitech.retain(|key, (worker, _)| worker.is_alive() && (present(&key.0) || direct.contains(key)));
            slots.mouse.as_ref().map(|(_, info)| (slots.mouse_path.clone(), info.link.as_ref().map(|l| l.slot)))
        };
        if scan && let Some((mouse_path, mouse_slot)) = mouse_link {
            for paths in receivers {
                let path = paths.0.path().to_owned();
                let Ok(found) = probe(api, paths) else { continue };
                self.slots.lock_recover().receiver_online.insert(path.clone(), found.online.clone());
                for slot in found.online {
                    let known = self.slots.lock_recover().receiver_unsupported.contains_key(&(path.clone(), slot));
                    let the_mouse = mouse_path.as_deref() == Some(path.as_c_str()) && mouse_slot == Some(slot);
                    if known || the_mouse {
                        continue;
                    }
                    if let Ok(Err(device)) = identify(api, paths, slot) {
                        self.slots.lock_recover().receiver_unsupported.insert((path.clone(), slot), device);
                        changed = true;
                    }
                }
            }
        }
        let to_open: Vec<(ReceiverPaths, u8, UnsupportedDevice)> = {
            let slots = self.slots.lock_recover();
            slots
                .receiver_unsupported
                .iter()
                .filter(|((path, slot), _)| {
                    !slots.logitech.contains_key(&(path.clone(), *slot))
                        && slots.receiver_online.get(path).is_some_and(|online| online.contains(slot))
                })
                .filter_map(|((path, slot), device)| {
                    let paths = receivers.iter().find(|(d, _)| d.path() == path.as_c_str())?;
                    Some((paths.clone(), *slot, device.clone()))
                })
                .collect()
        };
        for (paths, slot, device) in to_open {
            let saved = self.logitech_dpi.lock_recover().get(&device.settings_key()).copied();
            let lights = self.logitech_lights.lock_recover().entry(device.settings_key()).or_default().clone();
            let receiver_id = paths.0.product_id();
            match open_behind(api, &paths, slot, true).and_then(|dev| LogitechTask::open(dev, device, saved, lights)) {
                Ok(task) => {
                    log::info!(
                        "{} connected through the {}, slot {slot} (read by its features)",
                        task.device.label(),
                        receiver::name(receiver_id)
                    );
                    let worker = Worker::spawn("perihub-logitech", task);
                    self.slots.lock_recover().logitech.insert((paths.0.path().to_owned(), slot), (worker, receiver_id));
                }
                // Asleep again: opened when it answers.
                Err(Error::Unreachable) => {}
                // Another device was paired in that slot: read it again before anything is set on it.
                Err(Error::OtherDevice) => {
                    log::info!("{}, slot {slot}: a different device answers now", receiver::name(receiver_id));
                    self.slots.lock_recover().receiver_unsupported.remove(&(paths.0.path().to_owned(), slot));
                    changed = true;
                }
                Err(error) => log::debug!("{}, slot {slot}: {error}", receiver::name(receiver_id)),
            }
        }
        changed
    }

    /// Reads the Logitech devices on their own USB connection that have not answered yet, and opens a
    /// read-only thread for each one that answers (see [`direct`]). Returns `true` if anything changed.
    fn poll_direct(&self, api: &HidApi, direct: &[DirectPaths]) -> bool {
        let mut changed = false;
        {
            let mut slots = self.slots.lock_recover();
            let before = slots.direct.len();
            slots.direct.retain(|key, _| direct.iter().any(|d| d.key == *key));
            changed |= slots.direct.len() != before;
        }
        for paths in direct.iter().filter(|p| !p.collections.is_empty()) {
            let (known, reached) = {
                let mut guard = self.slots.lock_recover();
                let slots = &mut *guard;
                let state = slots.direct.entry(paths.key.clone()).or_insert_with(|| {
                    changed = true;
                    DirectState { device: None, reached: None, answers: Vec::new(), last_try: None, denied: false }
                });
                let open =
                    state.reached.as_ref().and_then(|r| slots.logitech.get(r)).is_some_and(|(w, _)| w.is_alive());
                let wait = if state.device.is_some() { DIRECT_REOPEN } else { DIRECT_RETRY };
                if open || state.last_try.is_some_and(|t| t.elapsed() < wait) {
                    continue;
                }
                state.last_try = Some(Instant::now());
                (state.device.clone(), state.reached.clone())
            };
            let name = paths.model.name;
            let mut answers = Vec::new();
            let mut denied = false;
            let mut found: Option<(CString, HidppDevice, u8)> = None;
            match (&known, &reached) {
                // Read before: opened again where it answered.
                (Some(_), Some((path, index))) => {
                    let info = paths.collections.iter().find(|c| c.path() == path.as_c_str());
                    let opened = info
                        .ok_or(Error::NotConnected)
                        .and_then(|info| open_channel(api, info))
                        .and_then(|dev| HidppDevice::new(dev, *index, true));
                    match opened {
                        Ok(dev) => found = Some((path.clone(), dev, *index)),
                        Err(error) => log::debug!("Logitech {name}: {error}"),
                    }
                }
                _ => {
                    for info in &paths.collections {
                        let label = format!("usage page {:04x}, usage {:04x}", info.usage_page(), info.usage());
                        if let Err(error) = open_channel(api, info) {
                            denied |= is_permission_error(&error);
                            answers.push(format!("{label}: could not open it: {}", open_error_message(error)));
                            continue;
                        }
                        let mut open = || open_channel(api, info).map(|d| Box::new(d) as Box<dyn Transport>);
                        if let Some((dev, index)) = direct::connect(&mut open, &label, &mut answers) {
                            found = Some((info.path().to_owned(), dev, index));
                            break;
                        }
                    }
                }
            }
            let mut device = known;
            let mut opened = None;
            if let Some((path, mut dev, index)) = found {
                if device.is_none() {
                    match UnsupportedDevice::read(&mut dev, index) {
                        Ok(mut read) => {
                            read.read_only = true;
                            log::info!(
                                "Logitech {name} ({:04x}) on its {}: {}, read by its features, read-only",
                                paths.model.product_id,
                                paths.model.connection,
                                read.label()
                            );
                            device = Some(read);
                        }
                        Err(error) => answers.push(format!("reading it: {error}")),
                    }
                }
                if let Some(read) = &device {
                    match LogitechTask::open(dev, read.clone(), None, LightsMemory::default()) {
                        Ok(task) => opened = Some((path.clone(), index, task)),
                        // Another device of the same model: read again before anything else.
                        Err(Error::OtherDevice) => device = None,
                        Err(error) => answers.push(format!("opening it: {error}")),
                    }
                }
                let mut slots = self.slots.lock_recover();
                if let Some(state) = slots.direct.get_mut(&paths.key) {
                    state.reached = Some((path, index));
                }
            }
            let mut slots = self.slots.lock_recover();
            if let Some((path, index, task)) = opened {
                let worker = Worker::spawn("perihub-logitech", task);
                slots.logitech.insert((path, index), (worker, paths.model.product_id));
                changed = true;
            }
            let Some(state) = slots.direct.get_mut(&paths.key) else { continue };
            if state.device != device {
                state.device = device;
                changed = true;
            }
            // A device opened again where it answered keeps what it answered the first time.
            if !answers.is_empty() && state.answers != answers {
                if state.answers.is_empty() || state.device.is_none() {
                    log::info!("Logitech {name}: {}", answers.join("; "));
                }
                state.answers = answers;
                changed = true;
            }
            if state.denied != denied {
                state.denied = denied;
                changed = true;
            }
        }
        changed
    }

    /// The Logitech devices on their own USB connection, in the shape of a receiver's report: the
    /// page and the test report show them as a device behind one.
    fn direct_reports(&self, direct: &[DirectPaths]) -> Vec<ReceiverReport> {
        let slots = self.slots.lock_recover();
        direct
            .iter()
            .map(|paths| {
                let state = slots.direct.get(&paths.key);
                let open = |r: &(CString, u8)| slots.logitech.get(r).is_some_and(|(w, _)| w.is_alive());
                let open_slots = state.and_then(|s| s.reached.clone()).filter(open).map(|(_, i)| vec![i]);
                let mut answers: Vec<String> = paths.closed.iter().cloned().collect();
                answers.extend(state.iter().flat_map(|s| s.answers.iter().cloned()));
                let device = state.and_then(|s| s.device.clone()).unwrap_or_else(|| {
                    let why = paths.closed.clone().unwrap_or_else(|| "no HID++ answer yet".into());
                    direct::not_read(paths.model, why)
                });
                ReceiverReport {
                    product_id: paths.model.product_id,
                    name: paths.model.connection,
                    collections: paths.listed.clone(),
                    bolt: false,
                    unifying: false,
                    reachable: !paths.collections.is_empty(),
                    denied: state.is_some_and(|s| s.denied),
                    slots: answers,
                    open_slots: open_slots.unwrap_or_default(),
                    unsupported: vec![device],
                    direct: true,
                }
            })
            .collect()
    }

    /// The device PeriHub knows by its features in `slot` of a connected receiver.
    pub fn logitech(&self, receiver_id: u16, slot: u8) -> Result<Worker<LogitechTask>> {
        let slots = self.slots.lock_recover();
        slots
            .logitech
            .iter()
            .find(|((_, s), (_, id))| *s == slot && *id == receiver_id)
            .map(|(_, (worker, _))| worker.clone())
            .ok_or(Error::NotConnected)
    }

    /// Keeps the DPI to set on a device every time it connects (`None`: none any more), by
    /// [`UnsupportedDevice::settings_key`].
    pub fn set_logitech_dpi(&self, key: String, dpi: Option<u16>) {
        let mut saved = self.logitech_dpi.lock_recover();
        match dpi {
            Some(dpi) => saved.insert(key, dpi),
            None => saved.remove(&key),
        };
    }

    /// The user drove a keyboard's lights with `look` (`Some`): PeriHub takes them with it every
    /// time the keyboard connects, at every start too. `None`: the user gave them back; PeriHub no
    /// longer takes them (the look stays, for the editor).
    pub fn set_logitech_look(&self, key: String, look: Option<Look>) {
        let mut lights = self.logitech_lights.lock_recover();
        let memory = lights.entry(key).or_default();
        memory.wanted.store(look.is_some(), std::sync::atomic::Ordering::SeqCst);
        if look.is_some() {
            memory.saved = look;
        }
    }

    /// The user wants PeriHub to drive the lights of a keyboard.
    pub fn drives_logitech_lights(&self) -> bool {
        self.logitech_lights.lock_recover().values().any(|m| m.wanted.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Gives the lights of every keyboard PeriHub drives back to the keyboard. `forget` (the tray's
    /// "Turn off experimental lighting"): PeriHub no longer takes them, as with "Give the lights
    /// back". Otherwise (quitting PeriHub, installing an update) they are taken again at the next
    /// start.
    pub fn release_logitech(&self, forget: bool) {
        let workers: Vec<_> = self.slots.lock_recover().logitech.values().map(|(w, _)| w.clone()).collect();
        if forget {
            for memory in self.logitech_lights.lock_recover().values() {
                memory.wanted.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        for worker in workers {
            let _ = if forget {
                worker.call(|d| d.give_back_lights().map(|_| ()))
            } else {
                worker.call(|d| d.give_back_for_now())
            };
        }
    }

    /// Gives the lights of every keyboard PeriHub drives back before read-only mode goes on. PeriHub
    /// takes them again once it is off.
    pub fn release_logitech_for_read_only(&self) {
        let workers: Vec<_> = self.slots.lock_recover().logitech.values().map(|(w, _)| w.clone()).collect();
        for worker in workers {
            let _ = worker.call(|d| d.give_back_for_read_only());
        }
    }

    /// Turns a keyboard with a raw HID collection on or off as a VIA keyboard, by "vvvv:pppp". It is
    /// opened or closed at the next pass.
    pub fn set_qmk_enabled(&self, key: &str, on: bool) {
        let mut enabled = self.qmk_enabled.lock_recover();
        if on {
            enabled.insert(key.to_string());
        } else {
            enabled.remove(key);
        }
    }

    /// Looks for running programs that take control of the devices. Returns `true` if the list changed.
    fn check_conflicts(&self) -> bool {
        let (running, logitech_programs) = conflicts::running_and_logitech_programs();
        let mut slots = self.slots.lock_recover();
        if slots.conflicts == running && slots.logitech_programs == logitech_programs {
            return false;
        }
        log::info!("programs that conflict with PeriHub: {:?}", running.iter().map(|c| c.name()).collect::<Vec<_>>());
        if slots.logitech_programs != logitech_programs {
            log::info!("programs that set Logitech settings: {logitech_programs:?}");
        }
        slots.conflicts = running;
        slots.logitech_programs = logitech_programs;
        true
    }

    /// The connected USB and Bluetooth devices PeriHub does not know, as of the last pass.
    pub fn report_devices(&self) -> Vec<ReportDevice> {
        let mut slots = self.slots.lock_recover();
        slots.report_page_seen = Some(std::time::Instant::now());
        slots.report_devices.clone()
    }

    pub fn status(&self) -> DeviceStatus {
        let slots = self.slots.lock_recover();
        DeviceStatus {
            keyboard: slots.keyboard.as_ref().map(|(_, info)| info.clone()),
            mouse: slots
                .mouse
                .as_ref()
                .map(|(_, info)| MouseInfo { host_mode: slots.reported_host_mode, ..info.clone() }),
            keyboard_error: slots.keyboard_error.clone(),
            mouse_error: slots.mouse_error.clone().or_else(|| slots.receiver_note.clone()),
            mouse_asleep: slots.mouse_asleep.clone(),
            unsupported_keyboard: slots.unsupported_keyboard.clone(),
            conflicts: slots.conflicts.iter().map(|c| c.name().to_string()).collect(),
            corsair: slots.corsair.iter().map(|(_, _, info)| info.clone()).collect(),
            g300: slots.g300.as_ref().map(|(_, _, info)| info.clone()),
            g300_error: slots.g300_error.clone(),
            corsair_error: slots.corsair_error.clone(),
            peripherals: slots.peripherals.iter().map(|(_, _, info)| info.clone()).collect(),
            peripheral_error: slots.peripheral_error.clone(),
            qmk: slots.qmk.clone(),
            receivers: slots.receivers.clone(),
            logitech_programs: slots.logitech_programs.iter().map(|name| name.to_string()).collect(),
            sandboxed: conflicts::sandboxed(),
            scanned: slots.scanned,
        }
    }

    pub fn g300(&self) -> Result<Worker<G300Task>> {
        self.slots.lock_recover().g300.as_ref().map(|(_, w, _)| w.clone()).ok_or(Error::NotConnected)
    }

    /// Opens or forgets the Logitech G300. Returns `true` if the status changed.
    fn poll_g300(&self, api: &HidApi) -> bool {
        let candidates: Vec<&DeviceInfo> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == LOGITECH
                    && d.product_id() == g300::PRODUCT_ID
                    && d.interface_number() == g300::INTERFACE
            })
            .collect();
        // On Windows the interface has a keyboard collection too: the reports are in the vendor one.
        let found = candidates.iter().find(|d| d.usage_page() == g300::USAGE_PAGE).or(candidates.first()).copied();
        let path = found.map(|d| d.path().to_owned());
        let mut slots = self.slots.lock_recover();
        let mut changed = false;
        if slots.g300.as_ref().is_some_and(|(p, w, _)| !w.is_alive() || Some(p) != path.as_ref()) {
            slots.g300 = None;
            log::info!("Logitech G300 disconnected");
            changed = true;
        }
        if slots.g300_failed.is_some() && slots.g300_failed != path {
            slots.g300_failed = None;
        }
        let Some(device) = found else {
            return changed | slots.g300_error.take().is_some();
        };
        if slots.g300.is_some() || slots.g300_failed.as_deref() == Some(device.path()) {
            return changed;
        }
        drop(slots);
        let result = device.open_device(api).map_err(Error::from).and_then(G300Task::open);
        let mut slots = self.slots.lock_recover();
        match result {
            Ok(task) => {
                let info = task.info.clone();
                slots.g300 = Some((device.path().to_owned(), Worker::spawn("perihub-g300", task), info));
                slots.g300_error = None;
                true
            }
            Err(error) => {
                let message = format!("G300: {}", open_error_message(error));
                // Without the udev rule it is tried again at every pass: said once, not every time.
                let new = slots.g300_error.as_ref() != Some(&message);
                if new {
                    log::warn!("{message}");
                }
                if !message.contains("permission denied") {
                    slots.g300_failed = Some(device.path().to_owned());
                }
                slots.g300_error = Some(message);
                changed | new
            }
        }
    }

    /// The open device of another brand with this key ("vvvv:pppp").
    pub fn peripheral(&self, key: &str) -> Result<Worker<PeripheralTask>> {
        let slots = self.slots.lock_recover();
        slots
            .peripherals
            .iter()
            .find(|(_, _, info)| info.key == key)
            .map(|(_, w, _)| w.clone())
            .ok_or(Error::NotConnected)
    }

    /// Keeps the status in step with what a device of another brand shows after a change.
    pub fn peripheral_changed(&self, info: &PeripheralInfo) {
        let mut slots = self.slots.lock_recover();
        if let Some((_, _, current)) = slots.peripherals.iter_mut().find(|(_, _, i)| i.key == info.key) {
            *current = info.clone();
        }
    }

    /// Opens new devices of other brands and forgets the ones that went away. Returns `true` if
    /// the status changed.
    fn poll_peripherals(&self, api: &HidApi) -> bool {
        let qmk_found = |d: &DeviceInfo| qmk::classify(d.usage_page(), d.usage(), d.serial_number());
        let devices: Vec<DeviceInfo> = api
            .device_list()
            .filter(|d| peripheral::vendors().any(|v| v == d.vendor_id()) || qmk_found(d) != Found::No)
            .cloned()
            .collect();
        let mut found = peripheral_collections(&devices);
        // A HID++ 1.0 mouse is opened by its long collection, which gets the long answers.
        for (d, model) in found.iter_mut() {
            if matches!(model.protocol, Protocol::LogitechHidpp10(_))
                && let Some(long) = hidpp10_collection(&devices, d, 2)
            {
                *d = long.clone();
            }
        }
        let enabled = self.qmk_enabled.lock_recover().clone();
        let mut keyboards: Vec<(CString, QmkKeyboard)> = Vec::new();
        for d in &devices {
            let model = match qmk_found(d) {
                Found::Vial => &qmk::VIAL_MODEL,
                Found::Via => &qmk::VIA_MODEL,
                Found::No | Found::Bootloader => continue,
            };
            let key = format!("{:04x}:{:04x}", d.vendor_id(), d.product_id());
            if found.iter().any(|(o, _)| o.path() == d.path()) || keyboards.iter().any(|(_, k)| k.key == key) {
                continue;
            }
            // Never a node that also carries what is typed, whatever rule opened it.
            if !qmk::node_carries_only_raw_hid(&d.path().to_string_lossy()) {
                continue;
            }
            let vial = matches!(model.protocol, Protocol::Qmk(p) if p.vial);
            let who = qmk_identity(d, model);
            let on = vial || enabled.contains(&key);
            keyboards.push((
                d.path().to_owned(),
                QmkKeyboard {
                    key,
                    vendor_id: d.vendor_id(),
                    product_id: d.product_id(),
                    brand: who.brand,
                    name: who.name,
                    vial,
                    enabled: on,
                    open: false,
                    denied: false,
                },
            ));
            if on {
                found.push((d.clone(), model));
            }
        }
        // A keyboard of another brand is opened only where nothing typed arrives (Linux; elsewhere the
        // system keeps typed keys away from programs). QMK keyboards were checked above, more strictly.
        found.retain(|(d, model)| {
            // On Linux a model 70-perihub.rules does not open is not opened at all: the Wraith's collection
            // can carry how far each key is pressed (it will need a rule the user turns on), and most Razer
            // keyboards' interface also carries typed keys.
            if cfg!(target_os = "linux") && !peripheral::opened_on_linux(model) {
                return false;
            }
            let typing = model.kind == peripheral::Kind::Keyboard
                && !matches!(model.protocol, Protocol::Qmk(_))
                && peripheral::node_carries_typing(&d.path().to_string_lossy());
            if typing {
                log::debug!("{} {}: its node also carries typed keys, not opened", model.brand, model.name);
            }
            !typing
        });
        let present = |path: &CStr| found.iter().any(|(d, _)| d.path() == path);
        let mut changed = false;
        let mut slots = self.slots.lock_recover();
        let before = slots.peripherals.len();
        slots.peripherals.retain(|(path, worker, info)| {
            let keep = worker.is_alive() && present(path);
            if !keep {
                log::info!("{} {} disconnected", info.brand, info.model);
            }
            keep
        });
        changed |= slots.peripherals.len() != before;
        slots.peripheral_failed.retain(|path| present(path));
        slots.peripheral_waiting.retain(|path, _| present(path));
        if found.is_empty() && slots.peripheral_error.take().is_some() {
            changed = true;
        }
        let new: Vec<_> = found
            .into_iter()
            .filter(|(d, _)| {
                let path = d.path();
                !slots.peripherals.iter().any(|(p, _, _)| p.as_c_str() == path)
                    && !slots.peripheral_failed.iter().any(|p| p.as_c_str() == path)
                    && slots.peripheral_waiting.get(path).is_none_or(|at| at.elapsed() >= CORSAIR_RECEIVER_RETRY)
            })
            .collect();
        drop(slots);
        for (device, model) in new {
            let changes = self.untested_saves;
            let is_qmk = matches!(model.protocol, Protocol::Qmk(_));
            let who = if is_qmk { qmk_identity(&device, model) } else { Identity::of(model, device.product_id()) };
            let short = match model.protocol {
                Protocol::LogitechHidpp10(_) => {
                    hidpp10_collection(&devices, &device, 1).filter(|d| d.path() != device.path())
                }
                _ => None,
            };
            let result = device.open_device(api).map_err(Error::from).and_then(|dev| {
                let short = match short {
                    Some(d) => Some(Box::new(d.open_device(api)?) as Box<dyn peripheral::Hid>),
                    None => None,
                };
                PeripheralTask::open(Box::new(dev), short, model, who, changes)
            });
            let mut slots = self.slots.lock_recover();
            let path = device.path().to_owned();
            match result {
                Ok(task) => {
                    let info = task.info.clone();
                    slots.peripherals.push((path.clone(), Worker::spawn("perihub-peripheral", task), info));
                    slots.peripheral_waiting.remove(&path);
                    slots.qmk_denied.remove(&path);
                    slots.peripheral_error = None;
                    changed = true;
                }
                // Its page says how to open it: asked again at every pass, without a message.
                Err(error) if is_qmk && is_permission_error(&error) => {
                    if slots.qmk_denied.insert(path) {
                        log::info!(
                            "QMK keyboard {:04x}:{:04x}: permission denied",
                            device.vendor_id(),
                            device.product_id()
                        );
                        changed = true;
                    }
                }
                // Asleep or away from its receiver: asked again later, without a message.
                Err(Error::Unreachable | Error::Timeout) => {
                    slots.peripheral_waiting.insert(path, std::time::Instant::now());
                }
                Err(error) => {
                    let message = format!("{} {}: {}", model.brand, model.name, open_error_message(error));
                    // Without the udev rule it is tried again at every pass: said once, not every time.
                    let new = slots.peripheral_error.as_ref() != Some(&message);
                    if new {
                        log::warn!("{message}");
                    }
                    if !message.contains("permission denied") {
                        slots.peripheral_failed.push(path);
                    }
                    slots.peripheral_error = Some(message);
                    changed |= new;
                }
            }
        }
        let mut slots = self.slots.lock_recover();
        slots.qmk_denied.retain(|path| keyboards.iter().any(|(p, _)| p == path));
        let keyboards: Vec<QmkKeyboard> = keyboards
            .into_iter()
            .map(|(path, k)| QmkKeyboard {
                open: slots.peripherals.iter().any(|(p, _, _)| *p == path),
                denied: k.enabled && slots.qmk_denied.contains(&path),
                ..k
            })
            .collect();
        if slots.qmk != keyboards {
            slots.qmk = keyboards;
            changed = true;
        }
        changed
    }

    /// The open Corsair device with this USB product id.
    pub fn corsair(&self, product_id: u16) -> Result<Worker<CorsairTask>> {
        let slots = self.slots.lock_recover();
        slots
            .corsair
            .iter()
            .find(|(_, _, info)| info.product_id == product_id)
            .map(|(_, w, _)| w.clone())
            .ok_or(Error::NotConnected)
    }

    /// Keeps the status in step with a Corsair device's lighting (it lasts for the session only).
    pub fn corsair_changed(&self, info: &CorsairInfo) {
        let mut slots = self.slots.lock_recover();
        if let Some((_, _, current)) = slots.corsair.iter_mut().find(|(_, _, i)| i.product_id == info.product_id) {
            *current = info.clone();
        }
    }

    /// Opens new Corsair devices and forgets the ones that went away. Returns `true` if the
    /// status changed.
    fn poll_corsair(&self, api: &HidApi) -> bool {
        let devices: Vec<DeviceInfo> = api.device_list().cloned().collect();
        let found = corsair_control_interfaces(&devices);
        let receivers = corsair_receiver_interfaces(&devices);
        let present = |path: &CStr| {
            found.iter().any(|(d, _)| d.path() == path) || receivers.iter().any(|(d, _)| d.path() == path)
        };
        let mut changed = false;
        let mut slots = self.slots.lock_recover();
        let before = slots.corsair.len();
        slots.corsair.retain(|(path, worker, info)| {
            let keep = worker.is_alive() && present(path);
            if !keep {
                log::info!("Corsair {} disconnected", info.model);
            }
            keep
        });
        changed |= slots.corsair.len() != before;
        slots.corsair_failed.retain(|path| present(path));
        slots.corsair_waiting.retain(|path, _| present(path));
        if found.is_empty() && receivers.is_empty() && slots.corsair_error.take().is_some() {
            changed = true;
        }
        let new: Vec<_> = found
            .into_iter()
            .filter(|(d, _)| {
                !slots.corsair.iter().any(|(p, _, _)| p.as_c_str() == d.path())
                    && !slots.corsair_failed.iter().any(|p| p.as_c_str() == d.path())
            })
            .collect();
        let open_receivers: Vec<DeviceInfo> = receivers
            .into_iter()
            .map(|(d, _)| d)
            .filter(|d| {
                let path = d.path();
                !slots.corsair.iter().any(|(p, _, _)| p.as_c_str() == path)
                    && !slots.corsair_failed.iter().any(|p| p.as_c_str() == path)
                    && slots.corsair_waiting.get(path).is_none_or(|at| at.elapsed() >= CORSAIR_RECEIVER_RETRY)
            })
            .collect();
        drop(slots);
        for receiver in open_receivers {
            changed |= self.open_corsair_receiver(api, &receiver);
        }
        for (device, model) in new {
            let result = device
                .open_device(api)
                .map_err(Error::from)
                .and_then(|dev| CorsairTask::open(BragiDevice::new(dev, model.packet), model));
            let mut slots = self.slots.lock_recover();
            match result {
                Ok(task) => {
                    let info = task.info();
                    log::info!("Corsair {} connected (experimental support, read-only)", info.model);
                    slots.corsair.push((device.path().to_owned(), Worker::spawn("perihub-corsair", task), info));
                    slots.corsair_error = None;
                    changed = true;
                }
                Err(error) => {
                    let message = format!("Corsair {}: {}", model.name, open_error_message(error));
                    // Without the udev rule it is tried again at every pass: said once, not every time.
                    let new = slots.corsair_error.as_ref() != Some(&message);
                    if new {
                        log::warn!("{message}");
                    }
                    if !message.contains("permission denied") {
                        slots.corsair_failed.push(device.path().to_owned());
                    }
                    slots.corsair_error = Some(message);
                    changed |= new;
                }
            }
        }
        changed
    }

    /// Opens the first connected device behind a Corsair receiver that PeriHub knows. Returns
    /// `true` if the status changed.
    fn open_corsair_receiver(&self, api: &HidApi, receiver: &DeviceInfo) -> bool {
        let path = receiver.path().to_owned();
        let result = receiver.open_device(api).map_err(Error::from).and_then(|dev| {
            let mut dev = BragiDevice::new(dev, corsair::PACKET);
            for channel in dev.connected_channels()? {
                dev.set_channel(channel);
                let id = match dev.product_id() {
                    Ok(id) => id,
                    Err(e) => {
                        log::debug!("Corsair receiver, channel {channel}: {e}");
                        continue;
                    }
                };
                match corsair::models::find(id) {
                    Some(model) => {
                        return CorsairTask::open_behind(dev, model, receiver.product_id(), channel).map(Some);
                    }
                    None => log::info!("Corsair receiver, channel {channel}: device {id:04x} is not supported yet"),
                }
            }
            Ok(None)
        });
        let mut slots = self.slots.lock_recover();
        match result {
            Ok(Some(task)) => {
                let info = task.info();
                log::info!("Corsair {} connected through its receiver (experimental support, read-only)", info.model);
                slots.corsair.push((path.clone(), Worker::spawn("perihub-corsair", task), info));
                slots.corsair_waiting.remove(&path);
                true
            }
            // Nothing connected (or nothing PeriHub knows): asked again later.
            Ok(None) => {
                slots.corsair_waiting.insert(path, std::time::Instant::now());
                false
            }
            Err(error) => {
                let message = format!("Corsair receiver: {}", open_error_message(error));
                let changed = slots.corsair_error.as_ref() != Some(&message);
                if changed {
                    log::warn!("{message}");
                }
                if message.contains("permission denied") {
                    slots.corsair_waiting.insert(path, std::time::Instant::now());
                } else {
                    slots.corsair_failed.push(path);
                }
                slots.corsair_error = Some(message);
                changed
            }
        }
    }

    /// Gives every Corsair device with experimental lighting back to its own settings (tray menu,
    /// quitting PeriHub, installing an update).
    pub fn release_corsair(&self) {
        let workers: Vec<_> = self.slots.lock_recover().corsair.iter().map(|(_, w, _)| w.clone()).collect();
        for worker in workers {
            if let Ok(info) = worker.call(|m| m.turn_off_lighting()) {
                self.corsair_changed(&info);
            }
        }
    }

    pub fn keyboard(&self) -> Result<Worker<KeyboardTask>> {
        self.slots.lock_recover().keyboard.as_ref().map(|(w, _)| w.clone()).ok_or(Error::NotConnected)
    }

    /// The open mouse: path of its HID interface (the one that also carries its keyboard reports)
    /// and its USB product id. `None` for a mouse behind a receiver, whose keyboard reports arrive
    /// elsewhere: those mice are read-only, so no button of theirs starts a software macro.
    pub fn mouse_id(&self) -> Option<(CString, u16)> {
        let slots = self.slots.lock_recover();
        let (_, info) = slots.mouse.as_ref().filter(|(_, info)| !info.receiver)?;
        Some((slots.mouse_path.clone()?, info.product_id))
    }

    /// The connected keyboard as a source of keys that start software macros.
    pub fn keyboard_triggers(&self) -> Option<KeyboardTriggers> {
        let slots = self.slots.lock_recover();
        slots.keyboard.as_ref()?;
        Some(KeyboardTriggers {
            boot: slots.key_interfaces.boot.clone(),
            bitmap: slots.key_interfaces.bitmap.clone(),
            keys: slots.keyboard_macro_keys.as_ref()?.lock_recover().clone(),
        })
    }

    pub fn mouse(&self) -> Result<Worker<MouseTask>> {
        self.slots.lock_recover().mouse.as_ref().map(|(w, _)| w.clone()).ok_or(Error::NotConnected)
    }

    /// Whether PeriHub may read the keyboard's key presses (for reactive lighting), or `None` if
    /// the keyboard is not connected. Only opens the interface for a moment; nothing is read.
    pub fn key_presses_readable(&self) -> Option<bool> {
        let path = self.slots.lock_recover().key_interfaces.boot.clone()?;
        Some(HidApi::new().and_then(|api| api.open_path(&path)).is_ok())
    }

    /// Settings applied whenever the keyboard is connected.
    pub fn keyboard_config(&self) -> KeyboardConfig {
        self.keyboard_config.lock_recover().clone()
    }

    /// Updates the settings to reapply whenever the keyboard is reconnected.
    pub fn set_keyboard_config(&self, config: KeyboardConfig) {
        *self.keyboard_config.lock_recover() = config;
    }
}

fn is_permission_error(error: &Error) -> bool {
    let text = error.to_string();
    text.contains("Permission denied") || text.contains("ermesso") || text.contains("ccess is denied")
}

/// Who a QMK keyboard is, from its USB strings.
fn qmk_identity(d: &DeviceInfo, model: &peripheral::Model) -> Identity {
    Identity::qmk(model, d.vendor_id(), d.product_id(), d.manufacturer_string(), d.product_string())
}

fn open_error_message(error: Error) -> String {
    if is_permission_error(&error) {
        "permission denied: install the PeriHub udev rule (see the README)".into()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_between_tries_grows_to_30_seconds_and_stays_there() {
        let waits: Vec<u64> = (1..=9).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(waits, [2, 4, 8, 16, 30, 30, 30, 30, 30]);
        // Many failures in a row (a session left open for days) do not overflow.
        assert_eq!(retry_delay(u32::MAX), HID_RETRY_MAX);
    }

    #[test]
    fn detection_tries_again_until_the_hid_access_opens() {
        let mut tries = 0;
        let mut told = Vec::new();
        let mut slept = Vec::new();
        let value = retry_with_backoff(
            || {
                tries += 1;
                if tries < 4 { Err(format!("not ready {tries}")) } else { Ok("opened") }
            },
            |n, error, wait| told.push((n, error.clone(), wait)),
            |wait| slept.push(wait),
        );
        assert_eq!(value, "opened");
        assert_eq!(tries, 4);
        assert_eq!(slept, [2, 4, 8].map(Duration::from_secs));
        assert_eq!(told[0], (1, "not ready 1".to_string(), Duration::from_secs(2)));
        assert_eq!(told.len(), 3);
    }

    #[test]
    fn a_first_success_does_not_wait_or_complain() {
        let value = retry_with_backoff(|| Ok::<_, String>(7), |_, _, _| panic!("no failure"), |_| panic!("no wait"));
        assert_eq!(value, 7);
    }

    #[test]
    fn a_panic_in_one_step_is_skipped_and_the_next_steps_still_run() {
        assert!(!guarded("test", || panic!("a probe went wrong")));
        assert!(guarded("test", || true));
    }

    #[test]
    fn the_slots_stay_usable_after_a_panic_with_them_held() {
        let slots = Arc::new(Mutex::new(Slots::default()));
        let held = slots.clone();
        assert!(!guarded("test", move || {
            let mut slots = held.lock_recover();
            slots.scanned = true;
            panic!("a probe went wrong while holding the slots");
        }));
        assert!(slots.lock().is_err(), "the panic poisoned the lock");
        assert!(slots.lock_recover().scanned);
    }
}

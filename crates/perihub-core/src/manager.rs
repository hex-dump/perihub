//! Detects supported devices when they are plugged in or unplugged.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hidapi::{DeviceInfo, HidApi};
use serde::Serialize;

use crate::apex::Apex3Tkl;
use crate::config::KeyboardConfig;
use crate::conflicts::{self, Conflict};
use crate::corsair::device::{CorsairInfo, CorsairTask};
use crate::corsair::{self, BragiDevice};
use crate::error::{Error, Result};
use crate::g300::{self, G300Info, G300Task};
use crate::hidpp::models::{self, MouseModel};
use crate::hidpp::receiver::{self, Presence, UnsupportedDevice};
use crate::hidpp::{HidppDevice, Transport};
use crate::keyboard::{KeyInterfaces, KeyboardInfo, KeyboardTask};
use crate::logitech::LogitechTask;
use crate::mouse::{MouseInfo, MouseTask, ReceiverLink};
use crate::softmacro::KeyboardTriggers;
use crate::worker::Worker;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Conflicting programs are looked for every this many detection passes (10 seconds).
/// How often a Corsair receiver without a connected device is asked again.
const CORSAIR_RECEIVER_RETRY: Duration = Duration::from_secs(10);
const CONFLICT_CHECK_EVERY: u32 = 5;
/// While a mouse is open, the receivers are asked for the other devices paired with them every this
/// many passes (10 seconds).
const RECEIVER_SCAN_EVERY: u32 = 5;

const STEELSERIES: u16 = 0x1038;
const APEX_3_TKL: u16 = 0x1622;
/// Recognized but not supported yet: never opened, see issue #9.
const APEX_3: u16 = 0x161A;
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

/// HID++ collection of a receiver: usage 2 is the long one, 1 the short one. On Linux both are the
/// same hidraw node.
fn receiver_collection(d: &DeviceInfo, usage: u16) -> bool {
    d.vendor_id() == LOGITECH
        && receiver::is_receiver(d.product_id())
        && ((d.usage_page() == 0xFF00 && d.usage() == usage) || (d.usage_page() == 0 && d.interface_number() == 2))
}

/// A receiver's long HID++ collection, and its short one when that has a path of its own (Windows).
type ReceiverPaths = (DeviceInfo, Option<DeviceInfo>);

/// What a receiver's slots hold.
#[derive(Default)]
struct Probe {
    /// Slots whose device answers.
    online: Vec<u8>,
    /// A paired device is turned off or asleep.
    asleep: bool,
    /// What each slot answered, for test reports.
    answers: Vec<String>,
}

fn open_channel(api: &HidApi, info: &DeviceInfo) -> Result<hidapi::HidDevice> {
    info.open_device(api).map_err(Error::from)
}

fn probe(api: &HidApi, (long, short): &ReceiverPaths) -> Result<Probe> {
    let long_dev = open_channel(api, long)?;
    let short_dev = short.as_ref().map(|d| open_channel(api, d)).transpose()?;
    let channel: &dyn Transport = match &short_dev {
        Some(dev) => dev,
        None => &long_dev,
    };
    let mut found = Probe::default();
    let mut timeouts = 0;
    for slot in receiver::SLOTS {
        match receiver::ping_answer(channel, slot) {
            Ok((presence, answer)) => {
                found.answers.push(format!("slot {slot}: {answer}"));
                match presence {
                    Presence::Online => found.online.push(slot),
                    Presence::Asleep => found.asleep = true,
                    Presence::Empty => {}
                }
            }
            Err(Error::Timeout) => {
                found.answers.push(format!("slot {slot}: no answer"));
                // A receiver that does not answer twice: nothing more to ask it in this pass.
                timeouts += 1;
                if timeouts == 2 {
                    break;
                }
            }
            Err(error) => return Err(error),
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
        None => Ok(Err(UnsupportedDevice::read(&mut dev, slot)?)),
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
    /// Connected Logitech receivers (LIGHTSPEED and Logi Bolt).
    pub receivers: Vec<ReceiverReport>,
    /// Running programs that set their own DPI and other settings on Logitech devices (Solaar,
    /// logiops): they may undo what PeriHub sets on the devices it knows by their features.
    pub logitech_programs: Vec<String>,
    /// PeriHub runs in a Flatpak sandbox and cannot see the other programs running (see
    /// [`conflicts::sandboxed`]).
    pub sandboxed: bool,
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
    /// with the receiver's product id.
    logitech: HashMap<(CString, u8), (Worker<LogitechTask>, u16)>,
    logitech_programs: Vec<&'static str>,
    /// Interfaces of the connected keyboard that report key presses.
    key_interfaces: KeyInterfaces,
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
}

impl Manager {
    /// Starts detection in the background. `listener` is called on every change.
    /// `untested_saves`: a mouse whose memory format is untested may be saved to, each save undone
    /// unless the user keeps it. Only the interface asks that; the command line never saves to one.
    /// `logitech_dpi`: the DPI to set on the devices PeriHub knows by their features every time they
    /// connect, by settings key (see [`Manager::set_logitech_dpi`]).
    pub fn start(
        keyboard_config: KeyboardConfig,
        untested_saves: bool,
        logitech_dpi: HashMap<String, u16>,
        listener: Listener,
    ) -> Self {
        let manager = Self {
            slots: Arc::new(Mutex::new(Slots::default())),
            keyboard_config: Arc::new(Mutex::new(keyboard_config)),
            untested_saves,
            logitech_dpi: Arc::new(Mutex::new(logitech_dpi)),
        };
        let background = manager.clone();
        thread::Builder::new()
            .name("perihub-hotplug".into())
            .spawn(move || background.hotplug_loop(listener))
            .expect("failed to start device detection");
        manager
    }

    fn hotplug_loop(&self, listener: Listener) {
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(error) => {
                let mut slots = self.slots.lock().unwrap();
                slots.keyboard_error = Some(format!("HID unavailable: {error}"));
                slots.mouse_error = slots.keyboard_error.clone();
                drop(slots);
                listener(&self.status());
                return;
            }
        };
        let mut pass = 0u32;
        loop {
            let mut changed = self.poll(&mut api, pass.is_multiple_of(RECEIVER_SCAN_EVERY));
            changed |= self.poll_corsair(&api);
            changed |= self.poll_g300(&api);
            if pass.is_multiple_of(CONFLICT_CHECK_EVERY) {
                changed |= self.check_conflicts();
            }
            if changed {
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
            let mut slots = self.slots.lock().unwrap();
            if slots.unsupported_keyboard != unsupported_keyboard {
                if let Some(name) = &unsupported_keyboard {
                    log::info!("{name} connected: not supported yet, so PeriHub does not open it");
                }
                slots.unsupported_keyboard = unsupported_keyboard;
                changed = true;
            }
        }

        let keyboard_present = {
            let mut slots = self.slots.lock().unwrap();
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
            let config = self.keyboard_config.lock().unwrap().clone();
            let result = info
                .open_device(api)
                .map_err(Error::from)
                .and_then(|dev| KeyboardTask::open(Apex3Tkl::new(dev), &config, key_interfaces.clone()));
            let mut slots = self.slots.lock().unwrap();
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
            let mut slots = self.slots.lock().unwrap();
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
            let mut slots = self.slots.lock().unwrap();
            if slots.mouse.is_some() || receivers.is_empty() {
                changed |= slots.mouse_asleep.take().is_some() | slots.receiver_note.take().is_some();
            }
        }
        changed |= self.poll_logitech(api, &receivers, scan);
        let reports = self.receiver_reports(api, &receivers);
        {
            let mut slots = self.slots.lock().unwrap();
            if slots.receivers != reports {
                slots.receivers = reports;
                changed = true;
            }
        }
        // The mouse's thread notices when another program takes the mouse or gives it back.
        let mut slots = self.slots.lock().unwrap();
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
        let mut slots = self.slots.lock().unwrap();
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
        let slots = self.slots.lock().unwrap();
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
                }
            })
            .collect()
    }

    /// Looks for a mouse behind the receivers and opens the first one that answers,
    /// read-only until its model has been tested. Returns `true` if the status changed.
    fn watch_receivers(&self, api: &HidApi, receivers: &[ReceiverPaths]) -> bool {
        let failed = self.slots.lock().unwrap().mouse_failed.clone();
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
                let mut slots = self.slots.lock().unwrap();
                slots.receiver_answers.insert(path.clone(), answers);
                let online = found.as_ref().map(|f| f.online.clone()).unwrap_or_default();
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
            if found.asleep {
                let seen = self.slots.lock().unwrap().receiver_models.get(&path).copied();
                if asleep.is_none() {
                    asleep = Some((seen.unwrap_or(WIRELESS_MOUSE).to_string(), receiver.clone()));
                }
            }
            for slot in found.online {
                let known = self.slots.lock().unwrap().receiver_unsupported.get(&(path.clone(), slot)).cloned();
                let identity = match known {
                    Some(device) => Ok(Err(device)),
                    None => identify(api, paths, slot),
                };
                let model = match identity {
                    Ok(Ok(model)) => model,
                    Ok(Err(device)) => {
                        note = Some(format!("{receiver}, slot {slot}: {} is not supported yet", device.label()));
                        self.slots.lock().unwrap().receiver_unsupported.insert((path.clone(), slot), device);
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
                // Nothing is saved over the radio: `mouse::support` grants at most RAM changes here.
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
        let mut slots = self.slots.lock().unwrap();
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
            let mut slots = self.slots.lock().unwrap();
            slots.logitech.retain(|(path, _), (worker, _)| worker.is_alive() && present(path));
            slots.mouse.as_ref().map(|(_, info)| (slots.mouse_path.clone(), info.link.as_ref().map(|l| l.slot)))
        };
        if scan && let Some((mouse_path, mouse_slot)) = mouse_link {
            for paths in receivers {
                let path = paths.0.path().to_owned();
                let Ok(found) = probe(api, paths) else { continue };
                self.slots.lock().unwrap().receiver_online.insert(path.clone(), found.online.clone());
                for slot in found.online {
                    let known = self.slots.lock().unwrap().receiver_unsupported.contains_key(&(path.clone(), slot));
                    let the_mouse = mouse_path.as_deref() == Some(path.as_c_str()) && mouse_slot == Some(slot);
                    if known || the_mouse {
                        continue;
                    }
                    if let Ok(Err(device)) = identify(api, paths, slot) {
                        self.slots.lock().unwrap().receiver_unsupported.insert((path.clone(), slot), device);
                        changed = true;
                    }
                }
            }
        }
        let to_open: Vec<(ReceiverPaths, u8, UnsupportedDevice)> = {
            let slots = self.slots.lock().unwrap();
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
            let saved = self.logitech_dpi.lock().unwrap().get(&device.settings_key()).copied();
            let receiver_id = paths.0.product_id();
            match open_behind(api, &paths, slot, true).and_then(|dev| LogitechTask::open(dev, device, saved)) {
                Ok(task) => {
                    log::info!(
                        "{} connected through the {}, slot {slot} (read by its features)",
                        task.device.label(),
                        receiver::name(receiver_id)
                    );
                    let worker = Worker::spawn("perihub-logitech", task);
                    self.slots
                        .lock()
                        .unwrap()
                        .logitech
                        .insert((paths.0.path().to_owned(), slot), (worker, receiver_id));
                }
                // Asleep again: opened when it answers.
                Err(Error::Unreachable) => {}
                // Another device was paired in that slot: read it again before anything is set on it.
                Err(Error::OtherDevice) => {
                    log::info!("{}, slot {slot}: a different device answers now", receiver::name(receiver_id));
                    self.slots.lock().unwrap().receiver_unsupported.remove(&(paths.0.path().to_owned(), slot));
                    changed = true;
                }
                Err(error) => log::debug!("{}, slot {slot}: {error}", receiver::name(receiver_id)),
            }
        }
        changed
    }

    /// The device PeriHub knows by its features in `slot` of a connected receiver.
    pub fn logitech(&self, receiver_id: u16, slot: u8) -> Result<Worker<LogitechTask>> {
        let slots = self.slots.lock().unwrap();
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
        let mut saved = self.logitech_dpi.lock().unwrap();
        match dpi {
            Some(dpi) => saved.insert(key, dpi),
            None => saved.remove(&key),
        };
    }

    /// Looks for running programs that take control of the devices. Returns `true` if the list changed.
    fn check_conflicts(&self) -> bool {
        let running = conflicts::running();
        let logitech_programs = conflicts::logitech_programs_running();
        let mut slots = self.slots.lock().unwrap();
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

    pub fn status(&self) -> DeviceStatus {
        let slots = self.slots.lock().unwrap();
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
            receivers: slots.receivers.clone(),
            logitech_programs: slots.logitech_programs.iter().map(|name| name.to_string()).collect(),
            sandboxed: conflicts::sandboxed(),
        }
    }

    pub fn g300(&self) -> Result<Worker<G300Task>> {
        self.slots.lock().unwrap().g300.as_ref().map(|(_, w, _)| w.clone()).ok_or(Error::NotConnected)
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
        let mut slots = self.slots.lock().unwrap();
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
        let mut slots = self.slots.lock().unwrap();
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

    /// The open Corsair device with this USB product id.
    pub fn corsair(&self, product_id: u16) -> Result<Worker<CorsairTask>> {
        let slots = self.slots.lock().unwrap();
        slots
            .corsair
            .iter()
            .find(|(_, _, info)| info.product_id == product_id)
            .map(|(_, w, _)| w.clone())
            .ok_or(Error::NotConnected)
    }

    /// Keeps the status in step with a Corsair device's lighting (it lasts for the session only).
    pub fn corsair_changed(&self, info: &CorsairInfo) {
        let mut slots = self.slots.lock().unwrap();
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
        let mut slots = self.slots.lock().unwrap();
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
            let mut slots = self.slots.lock().unwrap();
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
        let mut slots = self.slots.lock().unwrap();
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
        let workers: Vec<_> = self.slots.lock().unwrap().corsair.iter().map(|(_, w, _)| w.clone()).collect();
        for worker in workers {
            if let Ok(info) = worker.call(|m| m.turn_off_lighting()) {
                self.corsair_changed(&info);
            }
        }
    }

    pub fn keyboard(&self) -> Result<Worker<KeyboardTask>> {
        self.slots.lock().unwrap().keyboard.as_ref().map(|(w, _)| w.clone()).ok_or(Error::NotConnected)
    }

    /// The open mouse: path of its HID interface (the one that also carries its keyboard reports)
    /// and its USB product id. `None` for a mouse behind a receiver, whose keyboard reports arrive
    /// elsewhere: those mice are read-only, so no button of theirs starts a software macro.
    pub fn mouse_id(&self) -> Option<(CString, u16)> {
        let slots = self.slots.lock().unwrap();
        let (_, info) = slots.mouse.as_ref().filter(|(_, info)| !info.receiver)?;
        Some((slots.mouse_path.clone()?, info.product_id))
    }

    /// The connected keyboard as a source of keys that start software macros.
    pub fn keyboard_triggers(&self) -> Option<KeyboardTriggers> {
        let slots = self.slots.lock().unwrap();
        slots.keyboard.as_ref()?;
        Some(KeyboardTriggers {
            boot: slots.key_interfaces.boot.clone(),
            bitmap: slots.key_interfaces.bitmap.clone(),
            keys: slots.keyboard_macro_keys.as_ref()?.lock().unwrap().clone(),
        })
    }

    pub fn mouse(&self) -> Result<Worker<MouseTask>> {
        self.slots.lock().unwrap().mouse.as_ref().map(|(w, _)| w.clone()).ok_or(Error::NotConnected)
    }

    /// Whether PeriHub may read the keyboard's key presses (for reactive lighting), or `None` if
    /// the keyboard is not connected. Only opens the interface for a moment; nothing is read.
    pub fn key_presses_readable(&self) -> Option<bool> {
        let path = self.slots.lock().unwrap().key_interfaces.boot.clone()?;
        Some(HidApi::new().and_then(|api| api.open_path(&path)).is_ok())
    }

    /// Settings applied whenever the keyboard is connected.
    pub fn keyboard_config(&self) -> KeyboardConfig {
        self.keyboard_config.lock().unwrap().clone()
    }

    /// Updates the settings to reapply whenever the keyboard is reconnected.
    pub fn set_keyboard_config(&self, config: KeyboardConfig) {
        *self.keyboard_config.lock().unwrap() = config;
    }
}

fn is_permission_error(error: &Error) -> bool {
    let text = error.to_string();
    text.contains("Permission denied") || text.contains("ermesso") || text.contains("ccess is denied")
}

fn open_error_message(error: Error) -> String {
    if is_permission_error(&error) {
        "permission denied: install the PeriHub udev rule (see the README)".into()
    } else {
        error.to_string()
    }
}

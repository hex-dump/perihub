//! Detects supported devices when they are plugged in or unplugged.

use std::collections::HashMap;
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
use crate::error::{Error, Result};
use crate::hidpp::models::{self, MouseModel};
use crate::hidpp::receiver::{self, Presence};
use crate::hidpp::{HidppDevice, Transport};
use crate::keyboard::{KeyInterfaces, KeyboardInfo, KeyboardTask};
use crate::mouse::{MouseInfo, MouseTask};
use crate::softmacro::KeyboardTriggers;
use crate::worker::Worker;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Conflicting programs are looked for every this many detection passes (10 seconds).
const CONFLICT_CHECK_EVERY: u32 = 5;

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

/// HID++ collection of a LIGHTSPEED receiver: usage 2 is the long one, 1 the short one. On Linux
/// both are the same hidraw node.
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
    for slot in receiver::SLOTS {
        match receiver::ping(channel, slot) {
            Ok(Presence::Online) => found.online.push(slot),
            Ok(Presence::Asleep) => found.asleep = true,
            Ok(Presence::Empty) => {}
            // A receiver that does not answer: nothing more to ask it in this pass.
            Err(Error::Timeout) => break,
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
fn identify(api: &HidApi, paths: &ReceiverPaths, slot: u8) -> Result<std::result::Result<&'static MouseModel, String>> {
    let mut dev = open_behind(api, paths, slot, true)?;
    let id = dev.wireless_id()?;
    Ok(id.and_then(models::find_wireless).ok_or_else(|| {
        let name = dev.device_name().unwrap_or_else(|_| "a device".into());
        match id {
            Some(id) => format!("{name} ({id:04x})"),
            None => name,
        }
    }))
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
}

#[derive(Default)]
struct Slots {
    keyboard: Option<(Worker<KeyboardTask>, KeyboardInfo)>,
    mouse: Option<(Worker<MouseTask>, MouseInfo)>,
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
    receiver_unsupported: HashMap<(CString, u8), String>,
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
}

impl Manager {
    /// Starts detection in the background. `listener` is called on every change.
    pub fn start(keyboard_config: KeyboardConfig, listener: Listener) -> Self {
        let manager = Self {
            slots: Arc::new(Mutex::new(Slots::default())),
            keyboard_config: Arc::new(Mutex::new(keyboard_config)),
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
            let mut changed = self.poll(&mut api);
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
    fn poll(&self, api: &mut HidApi) -> bool {
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
                    .and_then(|dev| HidppDevice::new(dev, model.device_index, !model.verified))
                    .and_then(|dev| MouseTask::open(dev, model));
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
                    if mouse.verified { "tested" } else { "read-only" },
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

    /// Looks for a mouse behind the LIGHTSPEED receivers and opens the first one that answers,
    /// read-only until its model has been tested. Returns `true` if the status changed.
    fn watch_receivers(&self, api: &HidApi, receivers: &[ReceiverPaths]) -> bool {
        let failed = self.slots.lock().unwrap().mouse_failed.clone();
        let mut asleep = None;
        let mut note = None;
        for paths in receivers.iter().filter(|(long, _)| Some(long.path()) != failed.as_deref()) {
            let path = paths.0.path().to_owned();
            let found = match probe(api, paths) {
                Ok(found) => found,
                Err(error) => {
                    note = Some(format!("LIGHTSPEED receiver: {}", open_error_message(error)));
                    continue;
                }
            };
            let receiver = format!("LIGHTSPEED receiver {:04x}", paths.0.product_id());
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
                        note = Some(format!("{receiver}, slot {slot}: {device} is not supported yet"));
                        self.slots.lock().unwrap().receiver_unsupported.insert((path.clone(), slot), device);
                        continue;
                    }
                    Err(Error::Unreachable) => continue,
                    Err(error) => {
                        note = Some(format!("LIGHTSPEED receiver: {}", open_error_message(error)));
                        continue;
                    }
                };
                let result = open_behind(api, paths, slot, !model.verified).and_then(|dev| MouseTask::open(dev, model));
                let wireless_id = model.wireless_id.unwrap_or_default();
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

    /// Looks for running programs that take control of the devices. Returns `true` if the list changed.
    fn check_conflicts(&self) -> bool {
        let running = conflicts::running();
        let mut slots = self.slots.lock().unwrap();
        if slots.conflicts == running {
            return false;
        }
        log::info!("programs that conflict with PeriHub: {:?}", running.iter().map(|c| c.name()).collect::<Vec<_>>());
        slots.conflicts = running;
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

//! "Record the official software" on the Send a report page: a USB recording of what the device's
//! official software sends it (see `perihub_core::usbcap`), turned into text for the device
//! request. Nothing is sent anywhere: the text goes into the request the user sends themselves.

use std::sync::{Arc, Mutex};

use perihub_core::usbcap::record::{self, Event, Events, Recording, Request};
use perihub_core::usbcap::{Step, summarize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// The recording under way, and the user's steps once it is stopped.
#[derive(Default)]
pub struct UsbRecorder {
    current: Mutex<Option<Recording>>,
    steps: Mutex<Vec<Step>>,
}

/// What the interface is told, as the "usb-record" event.
#[derive(Serialize, Clone)]
struct Progress {
    /// status, recording, cleanup, done, cancelled, failed or ended.
    state: &'static str,
    text: String,
}

#[derive(Serialize)]
pub struct RecorderInfo {
    available: bool,
    reason: Option<String>,
    /// "USBPcap" or "usbmon".
    tool: &'static str,
    windows: bool,
    /// An earlier recording left USBPcap installed (Windows).
    leftover: bool,
}

#[tauri::command]
pub fn usb_record_info() -> RecorderInfo {
    let reason = record::unavailable();
    RecorderInfo {
        available: reason.is_none(),
        reason,
        tool: record::TOOL,
        windows: cfg!(windows),
        leftover: record::leftover(),
    }
}

/// A report descriptor of the device, as the Send a report page has it.
#[derive(Deserialize)]
pub struct Descriptor {
    interface: i32,
    hex: Option<String>,
}

#[derive(Deserialize)]
pub struct StepMark {
    at_ms: i64,
    name: String,
}

fn from_hex(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()).collect()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn events(app: &AppHandle) -> Events {
    let app = app.clone();
    Arc::new(move |event| {
        let emit = |state: &'static str, text: String| {
            let _ = app.emit("usb-record", Progress { state, text });
        };
        let recorder = app.state::<UsbRecorder>();
        match event {
            Event::Status(text) => emit("status", text),
            Event::Recording => emit("recording", String::new()),
            Event::Cleanup(text) => emit("cleanup", text),
            Event::Finished(recorded) => {
                let steps = std::mem::take(&mut *lock(&recorder.steps));
                let summary = summarize(recorded.link, &recorded.frames, &recorded.target, &steps, recorded.source);
                let mut text = summary.text;
                if recorded.cut {
                    text.push_str("\n(The recording was too large: its end is missing.)");
                }
                log::info!("USB recording finished: {} transfers kept", summary.transfers);
                *lock(&recorder.current) = None;
                emit("done", text);
            }
            Event::Cancelled => {
                *lock(&recorder.current) = None;
                emit("cancelled", String::new());
            }
            Event::Failed(error) => {
                log::warn!("USB recording: {error}");
                *lock(&recorder.current) = None;
                emit("failed", error);
            }
            Event::Ended => {
                *lock(&recorder.current) = None;
                emit("ended", String::new());
            }
        }
    })
}

/// Async, and on a thread of its own: Windows's administrator prompt waits for the user.
#[tauri::command]
pub async fn usb_record_start(
    app: AppHandle,
    vendor_id: u16,
    product_id: u16,
    product: Option<String>,
    descriptors: Vec<Descriptor>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start(&app, vendor_id, product_id, product, descriptors))
        .await
        .map_err(|e| e.to_string())?
}

fn start(
    app: &AppHandle,
    vendor_id: u16,
    product_id: u16,
    product: Option<String>,
    descriptors: Vec<Descriptor>,
) -> Result<(), String> {
    let state = app.state::<UsbRecorder>();
    let mut current = lock(&state.current);
    if current.is_some() {
        return Err("a recording is already under way".into());
    }
    let report_descriptors = descriptors
        .iter()
        .filter_map(|d| Some((u8::try_from(d.interface).ok()?, from_hex(d.hex.as_deref()?)?)))
        .collect();
    let request = Request { vendor_id, product_id, product, report_descriptors };
    lock(&state.steps).clear();
    log::info!("USB recording of {vendor_id:04x}:{product_id:04x} asked");
    *current = Some(record::start(request, events(app))?);
    Ok(())
}

/// Ends the recording and keeps it; `steps` are the user's steps with the times they started.
#[tauri::command]
pub fn usb_record_stop(state: State<'_, UsbRecorder>, steps: Vec<StepMark>) -> Result<(), String> {
    *lock(&state.steps) = steps.into_iter().map(|s| Step { at_ms: s.at_ms, name: s.name }).collect();
    match lock(&state.current).as_ref() {
        Some(recording) => {
            recording.stop();
            Ok(())
        }
        None => Err("no recording is under way".into()),
    }
}

/// Async: while the administrator prompt of a start is open, the start holds the recording.
#[tauri::command]
pub async fn usb_record_cancel(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if let Some(recording) = lock(&app.state::<UsbRecorder>().current).as_ref() {
            recording.cancel();
        }
    })
    .await;
}

/// Ends a recording when its window is closed: nobody can go through its steps any more. Never waits
/// (a start may be holding the recording while Windows's administrator prompt is open).
pub fn cancel_on_close(app: &AppHandle) {
    if let Ok(current) = app.state::<UsbRecorder>().current.try_lock()
        && let Some(recording) = current.as_ref()
    {
        recording.cancel();
    }
}

/// Removes the USBPcap an unfinished recording left (Windows, one administrator prompt).
#[tauri::command]
pub async fn usb_record_clean_leftover(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<UsbRecorder>();
        let mut current = lock(&state.current);
        if current.is_some() {
            return Err("a recording is under way".to_string());
        }
        *current = Some(record::clean_leftover(events(&app))?);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_come_in_hex() {
        assert_eq!(from_hex("0501ff"), Some(vec![0x05, 0x01, 0xFF]));
        assert_eq!(from_hex(""), Some(vec![]));
        assert_eq!(from_hex("zz"), None);
    }
}

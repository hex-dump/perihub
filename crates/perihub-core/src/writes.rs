//! Read-only mode and the write log.
//!
//! Read-only mode is one switch for the whole program: while it is on, nothing that changes a
//! device is sent to it, whoever asks (the interface, an app profile, an undo, the command line).
//! It is checked in every protocol at the point where a change and a read can be told apart, just
//! before the bytes go out, so it applies at once to the devices already open, without a restart.
//!
//! Requests that only read still go out: HID sends the request half of a read like any other
//! report, and PeriHub needs those reads to find the devices and show them.
//!
//! The write log keeps the last changes sent to the devices, and the ones refused, for this
//! session only: in memory, never on disk.

use std::collections::VecDeque;
#[cfg(not(test))]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, RwLockReadGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::{Error, Result};

/// How many writes the log keeps.
pub const LOG_MAX: usize = 500;
/// How many bytes of each write the log shows.
const SHOWN_BYTES: usize = 16;
/// The result of a write refused by read-only mode.
pub const REFUSED: &str = "refused (read-only mode)";

static READ_ONLY: AtomicBool = AtomicBool::new(false);
/// Held by a device thread for each request it runs (see [`operation`]).
static OPERATIONS: RwLock<()> = RwLock::new(());
#[cfg(not(test))]
static LOG: Mutex<VecDeque<WriteEntry>> = Mutex::new(VecDeque::new());

/// The log of the program; in the tests, the log of the test thread (see [`test`]).
fn with_log<R>(f: impl FnOnce(&mut VecDeque<WriteEntry>) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut LOG.lock().unwrap_or_else(|e| e.into_inner()));
    #[cfg(test)]
    return test::LOG.with(|log| f(&mut log.borrow_mut()));
}

/// One write to a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteEntry {
    /// Milliseconds since 1970.
    pub time: u64,
    pub device: String,
    /// What the write does: a command name, a HID++ feature and function…
    pub kind: String,
    /// The first bytes, in hex.
    pub data: String,
    /// How many bytes were sent.
    pub bytes: usize,
    /// "ok", [`REFUSED`], or what went wrong.
    pub result: String,
}

/// Held while a device thread runs one request or one periodic task: read-only mode changes
/// between two of them, never in the middle of one, so a save of several sectors, with its read
/// back and its undo, goes out whole or not at all.
pub fn operation() -> RwLockReadGuard<'static, ()> {
    OPERATIONS.read().unwrap_or_else(|e| e.into_inner())
}

/// Turns read-only mode on or off, for every device at once. Waits for the requests the device
/// threads are running to end (a few seconds at most, for a save).
pub fn set_read_only(on: bool) {
    let _between_requests = OPERATIONS.write().unwrap_or_else(|e| e.into_inner());
    if READ_ONLY.swap(on, Ordering::SeqCst) != on {
        log::info!("read-only mode {}", if on { "on: nothing is sent to the devices but reads" } else { "off" });
    }
}

pub fn read_only() -> bool {
    READ_ONLY.load(Ordering::SeqCst) || test::read_only()
}

/// [`Error::ReadOnly`] while read-only mode is on.
pub fn refuse_if_read_only() -> Result<()> {
    if read_only() { Err(Error::ReadOnly) } else { Ok(()) }
}

/// Called just before a change goes to a device: refused, and kept in the log, while read-only
/// mode is on.
pub fn allow(device: &str, kind: &str, data: &[u8]) -> Result<()> {
    if read_only() {
        note(device, kind, data, REFUSED);
        return Err(Error::ReadOnly);
    }
    Ok(())
}

/// Keeps a change that was sent, with how it went.
pub fn sent<T>(device: &str, kind: &str, data: &[u8], result: &Result<T>) {
    match result {
        Ok(_) => note(device, kind, data, "ok"),
        Err(error) => note(device, kind, data, &error.to_string()),
    }
}

/// Sends a change with `send`, unless read-only mode is on, and keeps it in the log.
pub fn send<T>(device: &str, kind: &str, data: &[u8], send: impl FnOnce() -> Result<T>) -> Result<T> {
    allow(device, kind, data)?;
    let result = send();
    sent(device, kind, data, &result);
    result
}

/// Adds a line to the log, the oldest one going once it is full.
pub fn note(device: &str, kind: &str, data: &[u8], result: &str) {
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut shown = crate::peripheral::hex(&data[..data.len().min(SHOWN_BYTES)]);
    if data.len() > SHOWN_BYTES {
        shown.push_str(" …");
    }
    let entry = WriteEntry {
        time,
        device: device.to_string(),
        kind: kind.to_string(),
        data: shown,
        bytes: data.len(),
        result: result.to_string(),
    };
    with_log(|log| {
        if log.len() >= LOG_MAX {
            log.pop_front();
        }
        log.push_back(entry);
    });
}

/// The writes of this session, oldest first.
pub fn entries() -> Vec<WriteEntry> {
    with_log(|log| log.iter().cloned().collect())
}

pub fn clear() {
    with_log(VecDeque::clear);
}

/// The tests run side by side in one process: read-only mode is turned on for one test thread
/// only, so the tests that write are not refused, and each test thread has a log of its own.
#[cfg(test)]
pub(crate) mod test {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    thread_local! {
        static ON: Cell<bool> = const { Cell::new(false) };
        pub(super) static LOG: RefCell<VecDeque<super::WriteEntry>> = const { RefCell::new(VecDeque::new()) };
    }

    pub fn read_only() -> bool {
        ON.with(Cell::get)
    }

    /// Read-only mode on for this test thread, until the guard is dropped.
    pub struct ReadOnly;

    impl Drop for ReadOnly {
        fn drop(&mut self) {
            ON.with(|on| on.set(false));
        }
    }

    pub fn read_only_mode() -> ReadOnly {
        ON.with(|on| on.set(true));
        ReadOnly
    }

    /// The log lines of one device.
    pub fn entries_of(device: &str) -> Vec<super::WriteEntry> {
        super::entries().into_iter().filter(|e| e.device == device).collect()
    }
}

#[cfg(not(test))]
mod test {
    pub fn read_only() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_is_refused_and_logged_in_read_only_mode() {
        let device = "writes test device";
        let _on = test::read_only_mode();
        assert!(read_only());
        let mut called = false;
        let result = send(device, "change", &[1, 2, 3], || {
            called = true;
            Ok(())
        });
        assert!(matches!(result, Err(Error::ReadOnly)));
        assert!(!called, "nothing may be sent");
        assert_eq!(result.unwrap_err().to_string(), "Read-only mode is on");
        let log = test::entries_of(device);
        assert_eq!(log.len(), 1);
        assert_eq!(
            (log[0].kind.as_str(), log[0].data.as_str(), log[0].result.as_str()),
            ("change", "01 02 03", REFUSED)
        );
    }

    #[test]
    fn a_write_goes_out_and_is_logged_with_its_result_when_read_only_mode_is_off() {
        let device = "writes test device 2";
        assert!(!read_only());
        assert!(refuse_if_read_only().is_ok());
        send(device, "colors", &[0xAB; 20], || Ok(())).unwrap();
        let failed: Result<()> = send(device, "save", &[0x11], || Err(Error::Timeout));
        assert!(failed.is_err());
        let log = test::entries_of(device);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].data, format!("{} …", ["ab"; 16].join(" ")));
        assert_eq!((log[0].bytes, log[0].result.as_str()), (20, "ok"));
        assert_eq!(log[1].result, Error::Timeout.to_string());
    }

    #[test]
    fn the_mode_changes_only_between_two_requests() {
        let (started, wait) = std::sync::mpsc::channel();
        let request = std::thread::spawn(move || {
            let _operation = operation();
            started.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::time::Instant::now()
        });
        wait.recv().unwrap();
        // Off, as it already is: the tests running beside this one are not affected.
        set_read_only(false);
        let changed = std::time::Instant::now();
        assert!(changed >= request.join().unwrap(), "the change waited for the request to end");
    }

    #[test]
    fn read_only_mode_of_one_test_thread_stays_there() {
        let _on = test::read_only_mode();
        let other = std::thread::spawn(read_only).join().unwrap();
        assert!(read_only() && !other);
    }

    #[test]
    fn the_log_keeps_only_the_last_writes() {
        let device = "writes test device 3";
        for i in 0..LOG_MAX + 5 {
            note(device, "change", &[i as u8], "ok");
        }
        let all = entries();
        assert_eq!(all.len(), LOG_MAX);
        assert_eq!(all[0].data, "05");
        assert_eq!(all.last().unwrap().data, format!("{:02x}", (LOG_MAX + 4) as u8));
        clear();
        assert!(entries().is_empty());
    }
}

//! A Corsair mouse on its own thread: what PeriHub reads from it, and experimental lighting.
//!
//! Experimental lighting lasts for the session only. It puts the mouse in software mode, which
//! PeriHub keeps with a keep-alive, and gives it back to its own settings (hardware mode) when it
//! is turned off, when the thread stops (unplugged, PeriHub quitting) and as soon as a command
//! fails, so that PeriHub never holds a mouse it can no longer talk to.

use std::time::{Duration, Instant};

use serde::Serialize;

use super::models::CorsairModel;
use super::{BragiDevice, MODE_HARDWARE, MODE_SOFTWARE, property};
use crate::color::Rgb;
use crate::error::{Error, Result};
use crate::worker::DeviceTask;

/// How often the keep-alive is sent while PeriHub drives the lighting.
const KEEP_ALIVE: Duration = Duration::from_secs(20);
/// Nothing to do while experimental lighting is off: the thread only wakes up for requests.
const IDLE: Duration = Duration::from_secs(60);
/// After lighting is turned on, the mouse is given back unless the user confirms that it still
/// works (see [`CorsairMouseTask::keep_lighting`]). The interface counts down 15 seconds and turns
/// it off itself; this is the fallback if the window was closed or stopped answering.
const CONFIRM_WITHIN: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
pub struct CorsairMouseInfo {
    pub product_id: u16,
    pub model: String,
    pub leds: u8,
    pub firmware: Option<String>,
    /// 1 = hardware (its own settings), 2 = software (a program drives it), as read at connection.
    pub mode: Option<u8>,
    pub polling_hz: Option<u16>,
    pub dpi: Option<u16>,
    /// What could not be read, for test reports.
    pub read_errors: Vec<String>,
    /// The colour PeriHub shows on every LED, while experimental lighting is on.
    pub lighting: Option<Rgb>,
}

pub struct CorsairMouseTask {
    dev: BragiDevice,
    pub info: CorsairMouseInfo,
    last_keep_alive: Instant,
    /// When lighting is turned off unless the user confirms that the mouse still works.
    confirm_by: Option<Instant>,
    closed: bool,
}

fn polling_hz(value: u32) -> Option<u16> {
    match value {
        1 => Some(125),
        2 => Some(250),
        3 => Some(500),
        4 => Some(1000),
        _ => None,
    }
}

impl CorsairMouseTask {
    /// Reads what it can without changing anything. A mouse that answers nothing at all is an error.
    pub fn open(mut dev: BragiDevice, model: &'static CorsairModel) -> Result<Self> {
        let mut errors = Vec::new();
        let mut answered = false;
        let mut note = |what: &str, error: Error, answered: &mut bool| {
            if !matches!(error, Error::Timeout | Error::Hid(_)) {
                *answered = true;
            }
            errors.push(format!("{what}: {error}"));
        };
        let firmware = match dev.firmware() {
            Ok(v) => Some(v).inspect(|_| answered = true),
            Err(e) => {
                note("firmware", e, &mut answered);
                None
            }
        };
        let mut read = |what: &str, prop: u8, answered: &mut bool| match dev.property(prop) {
            Ok(v) => {
                *answered = true;
                Some(v)
            }
            Err(e) => {
                note(what, e, answered);
                None
            }
        };
        let mode = read("mode", property::MODE, &mut answered).map(|v| v as u8);
        let polling = read("polling rate", property::POLLING, &mut answered);
        let dpi = read("DPI", property::DPI_X, &mut answered);
        if !answered {
            return Err(Error::Protocol(format!("the mouse did not answer ({})", errors.join("; "))));
        }
        let info = CorsairMouseInfo {
            product_id: model.product_id,
            model: model.name.into(),
            leds: model.leds,
            firmware,
            mode,
            polling_hz: polling.and_then(polling_hz),
            dpi: dpi.map(|v| v as u16),
            read_errors: errors,
            lighting: None,
        };
        log::info!(
            "Corsair {}: firmware {}, mode {:?}, {:?} Hz, {:?} DPI, unreadable: {:?}",
            info.model,
            info.firmware.as_deref().unwrap_or("?"),
            info.mode,
            info.polling_hz,
            info.dpi,
            info.read_errors
        );
        Ok(Self { dev, info, last_keep_alive: Instant::now(), confirm_by: None, closed: false })
    }

    /// Shows one colour on every LED (experimental). The first time it takes the mouse into
    /// software mode. If anything fails the mouse is given back at once.
    pub fn set_lighting(&mut self, color: Rgb) -> Result<CorsairMouseInfo> {
        if self.info.lighting.is_none() {
            let others = crate::conflicts::corsair_programs_running();
            if !others.is_empty() {
                return Err(Error::Invalid(format!(
                    "quit {} first: two programs driving the same Corsair mouse can leave it unable to click",
                    others.join(" and ")
                )));
            }
        }
        let first = self.info.lighting.is_none();
        let result = (|| {
            if first {
                self.dev.set_lighting_allowed(true);
                self.dev.set_mode(MODE_SOFTWARE)?;
                self.dev.open_lighting()?;
                self.last_keep_alive = Instant::now();
                log::info!("Corsair {}: experimental lighting on", self.info.model);
            }
            self.dev.write_lighting(&vec![color; self.info.leds as usize])
        })();
        match result {
            Ok(()) => {
                self.info.lighting = Some(color);
                if first {
                    self.confirm_by = Some(Instant::now() + CONFIRM_WITHIN);
                }
                Ok(self.info.clone())
            }
            Err(error) => {
                self.release();
                Err(Error::Protocol(format!("{error}. Experimental lighting was turned off.")))
            }
        }
    }

    /// The user checked that the mouse still works with PeriHub driving it.
    pub fn keep_lighting(&mut self) {
        self.confirm_by = None;
    }

    /// Gives the mouse back to its own settings.
    pub fn turn_off_lighting(&mut self) -> Result<CorsairMouseInfo> {
        self.release();
        Ok(self.info.clone())
    }

    /// Back to hardware mode, best effort: the mouse may be gone already.
    fn release(&mut self) {
        self.confirm_by = None;
        if self.info.lighting.take().is_some() || self.dev_lighting_allowed() {
            if let Err(e) = self.dev.close_lighting() {
                log::debug!("Corsair: closing the lighting: {e}");
            }
            match self.dev.set_mode(MODE_HARDWARE) {
                Ok(()) => {
                    log::info!("Corsair {}: experimental lighting off, mouse back to its own settings", self.info.model)
                }
                Err(e) => log::warn!("Corsair {}: switching back to hardware mode failed: {e}", self.info.model),
            }
        }
        self.dev.set_lighting_allowed(false);
    }

    fn dev_lighting_allowed(&self) -> bool {
        self.dev.lighting
    }

    /// Before the manager drops the mouse: give it back, then stop the thread.
    pub fn close(&mut self) -> Result<()> {
        self.release();
        self.closed = true;
        Ok(())
    }

    pub fn info(&self) -> CorsairMouseInfo {
        self.info.clone()
    }
}

impl DeviceTask for CorsairMouseTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.closed {
            return Err(Error::NotConnected);
        }
        if self.info.lighting.is_none() {
            return Ok(IDLE);
        }
        if self.confirm_by.is_some_and(|at| Instant::now() >= at) {
            log::warn!("Corsair {}: lighting was not confirmed: giving the mouse back", self.info.model);
            self.release();
            return Ok(IDLE);
        }
        if self.last_keep_alive.elapsed() >= KEEP_ALIVE {
            let others = crate::conflicts::corsair_programs_running();
            if !others.is_empty() {
                log::warn!("Corsair {}: {} started: giving the mouse back", self.info.model, others.join(", "));
                self.release();
                return Ok(IDLE);
            }
            self.last_keep_alive = Instant::now();
            if let Err(e) = self.dev.keep_alive() {
                log::warn!("Corsair {}: keep-alive failed ({e}): giving the mouse back", self.info.model);
                self.release();
                return Err(e);
            }
        }
        let next = KEEP_ALIVE.saturating_sub(self.last_keep_alive.elapsed());
        Ok(match self.confirm_by {
            Some(at) => next.min(at.saturating_duration_since(Instant::now())),
            None => next,
        })
    }
}

impl Drop for CorsairMouseTask {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::corsair::models;
    use crate::hidpp::Transport;

    /// A mouse that answers every command with success and records what it got.
    #[derive(Clone, Default)]
    struct FakeMouse {
        sent: Arc<Mutex<Vec<Vec<u8>>>>,
        /// Commands answered with this status instead of success.
        fail: Arc<Mutex<Option<(u8, u8)>>>,
    }

    impl Transport for FakeMouse {
        fn write(&self, data: &[u8]) -> Result<()> {
            self.sent.lock().unwrap().push(data[1..].to_vec());
            Ok(())
        }

        fn read_timeout(&self, buf: &mut [u8], _millis: i32) -> Result<usize> {
            let sent = self.sent.lock().unwrap();
            let last = sent.last().unwrap();
            buf.fill(0);
            buf[1] = last[1];
            if let Some((command, status)) = *self.fail.lock().unwrap()
                && command == last[1]
            {
                buf[2] = status;
            }
            if last[1] == 0x02 {
                // mode 1 (hardware), 1000 Hz (4), 1600 DPI, firmware 1.2.3
                buf[3..7].copy_from_slice(match last[2] {
                    0x03 => &[1, 0, 0, 0],
                    0x01 => &[4, 0, 0, 0],
                    0x21 => &[0x40, 0x06, 0, 0],
                    _ => &[1, 2, 3, 0],
                });
            }
            Ok(64)
        }
    }

    impl FakeMouse {
        fn commands(&self) -> Vec<(u8, u8)> {
            self.sent.lock().unwrap().iter().map(|p| (p[1], p[2])).collect()
        }
    }

    fn open(fake: &FakeMouse) -> CorsairMouseTask {
        CorsairMouseTask::open(BragiDevice::new(fake.clone()), models::find(0x1B70).unwrap()).unwrap()
    }

    #[test]
    fn opening_only_reads() {
        let fake = FakeMouse::default();
        let task = open(&fake);
        assert!(fake.commands().iter().all(|&(c, _)| c == 0x02), "{:?}", fake.commands());
        assert_eq!(task.info.firmware.as_deref(), Some("1.2.3"));
        assert_eq!(task.info.mode, Some(1));
        assert_eq!(task.info.polling_hz, Some(1000));
        assert_eq!(task.info.dpi, Some(1600));
    }

    #[test]
    fn lighting_takes_the_mouse_and_gives_it_back() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        fake.sent.lock().unwrap().clear();
        task.set_lighting(Rgb { r: 255, g: 0, b: 0 }).unwrap();
        let commands = fake.commands();
        assert_eq!(commands[0], (0x01, 0x03), "software mode first");
        assert_eq!(commands[1], (0x0D, 0x00), "then the lighting handle");
        assert_eq!(commands[2], (0x06, 0x00));
        // Two LEDs: red, red, green, green, blue, blue.
        assert_eq!(&fake.sent.lock().unwrap()[2][2..13], &[0, 6, 0, 0, 0, 255, 255, 0, 0, 0, 0]);
        fake.sent.lock().unwrap().clear();
        task.turn_off_lighting().unwrap();
        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(&sent.last().unwrap()[..5], &[0x08, 0x01, 0x03, 0x00, 0x01], "hardware mode last");
        assert!(task.info.lighting.is_none());
    }

    #[test]
    fn a_failure_gives_the_mouse_back_at_once() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        *fake.fail.lock().unwrap() = Some((0x06, 0x09));
        assert!(task.set_lighting(Rgb { r: 1, g: 2, b: 3 }).is_err());
        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(&sent.last().unwrap()[..5], &[0x08, 0x01, 0x03, 0x00, 0x01]);
        assert!(task.info.lighting.is_none());
        // Nothing but reads can reach the mouse any more.
        assert!(task.dev.set_mode(MODE_SOFTWARE).is_err());
    }

    #[test]
    fn lighting_that_is_not_confirmed_is_turned_off_by_the_thread() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        task.set_lighting(Rgb { r: 1, g: 2, b: 3 }).unwrap();
        task.confirm_by = Some(Instant::now());
        task.tick().unwrap();
        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(&sent.last().unwrap()[..5], &[0x08, 0x01, 0x03, 0x00, 0x01]);
        assert!(task.info.lighting.is_none());
    }

    #[test]
    fn confirmed_lighting_stays_on() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        task.set_lighting(Rgb { r: 1, g: 2, b: 3 }).unwrap();
        task.keep_lighting();
        task.tick().unwrap();
        assert!(task.info.lighting.is_some());
    }

    #[test]
    fn dropping_the_task_gives_the_mouse_back() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        task.set_lighting(Rgb { r: 1, g: 2, b: 3 }).unwrap();
        drop(task);
        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(&sent.last().unwrap()[..5], &[0x08, 0x01, 0x03, 0x00, 0x01]);
    }

    #[test]
    fn a_closed_task_without_lighting_sends_nothing() {
        let fake = FakeMouse::default();
        let mut task = open(&fake);
        fake.sent.lock().unwrap().clear();
        task.close().unwrap();
        drop(task);
        assert!(fake.sent.lock().unwrap().is_empty());
    }
}

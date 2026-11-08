//! A Corsair mouse or keyboard on its own thread: what PeriHub reads from it, and experimental lighting.
//!
//! Experimental lighting lasts for the session only. It puts the device in software mode, which
//! PeriHub keeps with a keep-alive, and gives it back to its own settings (hardware mode) when it
//! is turned off, when the thread stops (unplugged, PeriHub quitting) and as soon as a command
//! fails, so that PeriHub never holds a device it can no longer talk to.

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
/// After lighting is turned on, the device is given back unless the user confirms that it still
/// works (see [`CorsairTask::keep_lighting`]). The interface counts down 15 seconds and turns
/// it off itself; this is the fallback if the window was closed or stopped answering.
const CONFIRM_WITHIN: Duration = Duration::from_secs(20);
/// How often PeriHub checks that a device behind a receiver is still connected.
const PRESENCE_CHECK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
pub struct CorsairInfo {
    pub product_id: u16,
    pub model: String,
    pub kind: super::models::Kind,
    /// Connected through its wireless receiver: the receiver's USB product id.
    pub receiver: Option<u16>,
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

pub struct CorsairTask {
    dev: BragiDevice,
    pub info: CorsairInfo,
    last_keep_alive: Instant,
    /// When lighting is turned off unless the user confirms that the device still works.
    confirm_by: Option<Instant>,
    /// Behind a receiver: its channel, and when PeriHub last checked that it is still connected.
    channel: Option<u8>,
    last_presence_check: Instant,
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

impl CorsairTask {
    /// Reads what it can without changing anything. A device that answers nothing at all is an error.
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
        // Keyboards have no DPI.
        let dpi =
            if model.kind == super::models::Kind::Mouse { read("DPI", property::DPI_X, &mut answered) } else { None };
        if !answered {
            return Err(Error::Protocol(format!("the device did not answer ({})", errors.join("; "))));
        }
        let info = CorsairInfo {
            product_id: model.product_id,
            model: model.name.into(),
            kind: model.kind,
            receiver: None,
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
        Ok(Self {
            dev,
            info,
            last_keep_alive: Instant::now(),
            confirm_by: None,
            channel: None,
            last_presence_check: Instant::now(),
            closed: false,
        })
    }

    /// Opens the device on `channel` of a receiver (see [`CorsairTask::open`]).
    pub fn open_behind(mut dev: BragiDevice, model: &'static CorsairModel, receiver: u16, channel: u8) -> Result<Self> {
        dev.set_channel(channel);
        let mut task = Self::open(dev, model)?;
        task.info.receiver = Some(receiver);
        task.channel = Some(channel);
        Ok(task)
    }

    /// Shows one colour on every LED (experimental). The first time it takes the device into
    /// software mode. If anything fails the device is given back at once.
    pub fn set_lighting(&mut self, color: Rgb) -> Result<CorsairInfo> {
        if self.info.leds == 0 {
            return Err(Error::Invalid(format!("the {} has no lighting", self.info.model)));
        }
        if self.info.lighting.is_none() {
            let others = crate::conflicts::corsair_programs_running();
            if !others.is_empty() {
                return Err(Error::Invalid(format!(
                    "quit {} first: two programs driving the same Corsair device can leave it unable to click or type",
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

    /// The user checked that the device still works with PeriHub driving it.
    pub fn keep_lighting(&mut self) {
        self.confirm_by = None;
    }

    /// Gives the device back to its own settings.
    pub fn turn_off_lighting(&mut self) -> Result<CorsairInfo> {
        self.release();
        Ok(self.info.clone())
    }

    /// Back to hardware mode, best effort: the device may be gone already.
    fn release(&mut self) {
        self.confirm_by = None;
        if self.info.lighting.take().is_some() || self.dev_lighting_allowed() {
            if let Err(e) = self.dev.close_lighting() {
                log::debug!("Corsair: closing the lighting: {e}");
            }
            match self.dev.set_mode(MODE_HARDWARE) {
                Ok(()) => {
                    log::info!(
                        "Corsair {}: experimental lighting off, device back to its own settings",
                        self.info.model
                    )
                }
                Err(e) => log::warn!("Corsair {}: switching back to hardware mode failed: {e}", self.info.model),
            }
        }
        self.dev.set_lighting_allowed(false);
    }

    fn dev_lighting_allowed(&self) -> bool {
        self.dev.lighting
    }

    /// Before the manager drops the device: give it back, then stop the thread.
    pub fn close(&mut self) -> Result<()> {
        self.release();
        self.closed = true;
        Ok(())
    }

    pub fn info(&self) -> CorsairInfo {
        self.info.clone()
    }
}

impl DeviceTask for CorsairTask {
    fn tick(&mut self) -> Result<Duration> {
        if self.closed {
            return Err(Error::NotConnected);
        }
        // Behind a receiver the device can be turned off or fall asleep: the thread stops, and the
        // manager finds it again when it is back.
        if let Some(channel) = self.channel
            && self.last_presence_check.elapsed() >= PRESENCE_CHECK
        {
            self.last_presence_check = Instant::now();
            if !self.dev.connected_channels()?.contains(&channel) {
                log::info!("Corsair {}: turned off or asleep behind its receiver", self.info.model);
                self.release();
                return Err(Error::Unreachable);
            }
        }
        let idle = if self.channel.is_some() { PRESENCE_CHECK } else { IDLE };
        if self.info.lighting.is_none() {
            return Ok(idle);
        }
        if self.confirm_by.is_some_and(|at| Instant::now() >= at) {
            log::warn!("Corsair {}: lighting was not confirmed: giving the device back", self.info.model);
            self.release();
            return Ok(idle);
        }
        if self.last_keep_alive.elapsed() >= KEEP_ALIVE {
            let others = crate::conflicts::corsair_programs_running();
            if !others.is_empty() {
                log::warn!("Corsair {}: {} started: giving the device back", self.info.model, others.join(", "));
                self.release();
                return Ok(idle);
            }
            self.last_keep_alive = Instant::now();
            if let Err(e) = self.dev.keep_alive() {
                log::warn!("Corsair {}: keep-alive failed ({e}): giving the device back", self.info.model);
                self.release();
                return Err(e);
            }
        }
        let next = KEEP_ALIVE.saturating_sub(self.last_keep_alive.elapsed()).min(idle);
        Ok(match self.confirm_by {
            Some(at) => next.min(at.saturating_duration_since(Instant::now())),
            None => next,
        })
    }
}

impl Drop for CorsairTask {
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
        /// Behind a receiver: the channels connected (property 0x36).
        channels: Arc<Mutex<u8>>,
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
                let channels = *self.channels.lock().unwrap();
                let value: [u8; 4] = match last[2] {
                    0x03 => [1, 0, 0, 0],
                    0x01 => [4, 0, 0, 0],
                    0x21 => [0x40, 0x06, 0, 0],
                    0x36 => [channels, 0, 0, 0],
                    _ => [1, 2, 3, 0],
                };
                buf[3..7].copy_from_slice(&value);
            }
            Ok(64)
        }
    }

    impl FakeMouse {
        fn commands(&self) -> Vec<(u8, u8)> {
            self.sent.lock().unwrap().iter().map(|p| (p[1], p[2])).collect()
        }
    }

    fn open(fake: &FakeMouse) -> CorsairTask {
        CorsairTask::open(BragiDevice::new(fake.clone(), 64), models::find(0x1B70).unwrap()).unwrap()
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
    fn a_device_behind_a_receiver_stops_when_it_is_turned_off() {
        let fake = FakeMouse::default();
        *fake.channels.lock().unwrap() = 0b10;
        let model = models::find(0x1B4C).unwrap();
        let mut task = CorsairTask::open_behind(BragiDevice::new(fake.clone(), 64), model, 0x1BA6, 1).unwrap();
        assert_eq!(task.info.receiver, Some(0x1BA6));
        // Every request after opening went to channel 1.
        assert!(fake.sent.lock().unwrap().iter().all(|p| p[0] == 0x09));
        task.last_presence_check = Instant::now() - PRESENCE_CHECK;
        task.tick().unwrap();
        // The check asks the receiver itself.
        assert_eq!(&fake.sent.lock().unwrap().last().unwrap()[..3], &[0x08, 0x02, 0x36]);
        *fake.channels.lock().unwrap() = 0;
        task.last_presence_check = Instant::now() - PRESENCE_CHECK;
        assert!(matches!(task.tick(), Err(Error::Unreachable)));
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

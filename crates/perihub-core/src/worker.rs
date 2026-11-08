//! One thread per device: every request goes through here, so the lighting animation
//! and the interface never overlap on the same HID connection.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};

pub trait DeviceTask: Send + 'static {
    /// Periodic work (e.g. one frame of the lighting effect). Returns how long until it runs again.
    fn tick(&mut self) -> Result<Duration>;
}

type Job<T> = Box<dyn FnOnce(&mut T) + Send>;

pub struct Worker<T> {
    tx: Sender<Job<T>>,
    alive: Arc<AtomicBool>,
}

impl<T> Clone for Worker<T> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), alive: self.alive.clone() }
    }
}

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

impl<T: DeviceTask> Worker<T> {
    pub fn spawn(name: &str, mut task: T) -> Self {
        let (tx, rx) = mpsc::channel::<Job<T>>();
        let alive = Arc::new(AtomicBool::new(true));
        let flag = alive.clone();
        thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut wait = Duration::ZERO;
                loop {
                    match rx.recv_timeout(wait) {
                        Ok(job) => job(&mut task),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    match task.tick() {
                        Ok(next) => wait = next,
                        Err(error) => {
                            log::warn!("device unreachable: {error}");
                            break;
                        }
                    }
                }
                flag.store(false, Ordering::SeqCst);
            })
            .expect("failed to start the device thread");
        Self { tx, alive }
    }

    /// Both handles talk to the same thread.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.alive, &other.alive)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Runs `f` on the device's thread and waits for its result.
    pub fn call<R: Send + 'static>(&self, f: impl FnOnce(&mut T) -> Result<R> + Send + 'static) -> Result<R> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Box::new(move |task| {
                let _ = reply_tx.send(f(task));
            }))
            .map_err(|_| Error::NotConnected)?;
        reply_rx.recv_timeout(CALL_TIMEOUT).map_err(|_| Error::Timeout)?
    }
}

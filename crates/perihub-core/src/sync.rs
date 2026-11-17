//! Locks that survive a panic elsewhere.

use std::sync::{Mutex, MutexGuard};

/// A lock that is taken even when another thread panicked while holding it.
///
/// `Mutex::lock().unwrap()` turns one panic into a panic at every later use of the same mutex, so
/// one bad answer from one device would end the detection of every device until PeriHub restarts.
/// For state that is only shown (which devices were found, their errors) the data left by the
/// panic is still good to use: the next pass looks at the devices again and rewrites it. Do not use
/// it for a state in which a half-done change would be dangerous.
pub trait LockExt<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            log::warn!("a lock was poisoned by a panic in another thread: using its state as it is");
            poisoned.into_inner()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_poisoned_lock_is_still_taken_with_its_data() {
        let lock = Arc::new(Mutex::new(vec![1, 2]));
        let other = lock.clone();
        let panicked = std::thread::spawn(move || {
            let mut guard = other.lock().unwrap();
            guard.push(3);
            panic!("a probe went wrong");
        })
        .join();
        assert!(panicked.is_err());
        assert!(lock.lock().is_err(), "the lock is poisoned");
        assert_eq!(*lock.lock_recover(), vec![1, 2, 3]);
        // And it keeps working afterwards.
        lock.lock_recover().push(4);
        assert_eq!(lock.lock_recover().len(), 4);
    }
}

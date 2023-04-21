use crate::base::metrics_util::{measure_time, warn_if_takes_too_long};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Attempts to lock the given mutex.
///
/// Returns the guard even if mutex is poisoned.
///
/// # Panics
///
/// Panics in debug builds if already locked (blocks in release builds).
pub fn non_blocking_lock<'a, T>(
    mutex: &'a Mutex<T>,
    description: &'static str,
) -> MutexGuard<'a, T> {
    // TODO-high-performance This panics when pressing play. Check it out!
    // #[cfg(debug_assertions)]
    // match mutex.try_lock() {
    //     Ok(g) => g,
    //     Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
    //     Err(std::sync::TryLockError::WouldBlock) => {
    //         panic!("locking mutex would block: {}", description)
    //     }
    // }
    // #[cfg(not(debug_assertions))]
    blocking_lock(mutex, description)
}

/// Locks the given mutex.
///
/// Returns the guard even if mutex is poisoned.
pub fn blocking_lock_arc<'a, T>(
    mutex: &'a Arc<Mutex<T>>,
    description: &'static str,
) -> MutexGuard<'a, T> {
    blocking_lock(&*mutex, description)
}

/// Locks the given mutex.
///
/// Returns the guard even if mutex is poisoned.
pub fn blocking_lock<'a, T>(mutex: &'a Mutex<T>, description: &'static str) -> MutexGuard<'a, T> {
    warn_if_takes_too_long(description, Duration::from_millis(30), || {
        match mutex.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        }
    })
}

use std::sync::{Mutex, MutexGuard};

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
    #[cfg(debug_assertions)]
    match mutex.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            panic!("locking mutex would block: {}", description)
        }
    }
    #[cfg(not(debug_assertions))]
    match mutex.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

/// Locks the given mutex, potentially blocking.
///
/// Returns the guard even if mutex is poisoned.
pub fn blocking_lock<T>(mutex: &Mutex<T>) -> MutexGuard<T> {
    match mutex.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

use std::sync::{Mutex, MutexGuard, TryLockError};

/// Attempts to lock the given mutex.
///
/// Returns the guard even if mutex is poisoned.
///
/// # Panics
///
/// Panics if already locked.
pub fn non_blocking_lock<'a, T>(
    mutex: &'a Mutex<T>,
    description: &'static str,
) -> MutexGuard<'a, T> {
    match mutex.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
        Err(TryLockError::WouldBlock) => panic!("locking mutex would block: {}", description),
    }
}

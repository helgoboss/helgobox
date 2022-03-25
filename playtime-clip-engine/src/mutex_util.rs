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
        // TODO-medium When going into production, we should probably acquire the lock anyway.
        //  It's just a good error catching tool for the pre-release phase.
        Err(TryLockError::WouldBlock) => panic!("locking mutex would block: {}", description),
    }
}

use std::ops::{Deref, DerefMut};

/// Claims that it doesn't matter for the wrapped value not implementing Send and Sync.
///
/// Of course this is unsafe and should only be used if it really doesn't matter.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct SendOrSyncWhatever<T>(T);

impl<T> SendOrSyncWhatever<T> {
    pub unsafe fn new(value: T) -> Self {
        Self(value)
    }

    pub fn get(&self) -> &T {
        &self.0
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Deref for SendOrSyncWhatever<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> DerefMut for SendOrSyncWhatever<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

unsafe impl<T> Send for SendOrSyncWhatever<T> {}
unsafe impl<T> Sync for SendOrSyncWhatever<T> {}

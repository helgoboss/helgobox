use std::ffi::c_void;

/// This utility provides a way to pass a trait object reference that is neither `Send` nor
/// `'static` into functions that require these traits.
///
/// Dangerous stuff and rarely necessary! You go down to C level with this.
pub struct Trafficker {
    thin_ptr: *const c_void,
}

unsafe impl Send for Trafficker {}

impl Trafficker {
    /// Put a reference to a trait object reference in here (`&&dyn ...`).
    ///
    /// We need a reference to a reference here because
    pub fn new<T: Copy>(thin_ref: &T) -> Self {
        let thin_ptr = thin_ref as *const _ as *const c_void;
        Self { thin_ptr }
    }

    /// Get it out again.
    ///
    /// Make sure you use the same type as in `new`! We can't make `T` a type parameter of the
    /// struct because otherwise the borrow checker would complain that things go out of scope.
    ///
    /// # Safety
    ///
    /// If you don't provide the proper type or the reference passed to `new` went out of scope,
    /// things crash horribly.
    pub unsafe fn get<T: Copy>(&self) -> T {
        *(self.thin_ptr as *const T)
    }
}

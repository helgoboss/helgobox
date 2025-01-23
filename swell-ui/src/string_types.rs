use reaper_low::raw;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

pub struct SwellStringArg<'a>(Cow<'a, CStr>);

impl SwellStringArg<'_> {
    pub fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr()
    }

    pub(super) fn as_lparam(&self) -> raw::LPARAM {
        self.0.as_ptr() as _
    }
}

impl<'a> From<&'a CStr> for SwellStringArg<'a> {
    fn from(s: &'a CStr) -> Self {
        SwellStringArg(s.into())
    }
}

impl<'a> From<&'a str> for SwellStringArg<'a> {
    fn from(s: &'a str) -> Self {
        // Requires copying
        SwellStringArg(
            CString::new(s)
                .expect("Rust string too exotic for REAPER")
                .into(),
        )
    }
}

impl From<String> for SwellStringArg<'_> {
    fn from(s: String) -> Self {
        // Doesn't require copying because we own the string now
        SwellStringArg(
            CString::new(s)
                .expect("Rust string too exotic for REAPER")
                .into(),
        )
    }
}

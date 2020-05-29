use reaper_low::{raw, Swell};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::process::id;
use std::ptr::{null_mut, NonNull};

pub struct Clipboard(());

impl Clipboard {
    pub fn new() -> Clipboard {
        Swell::get().OpenClipboard(null_mut());
        Clipboard(())
    }

    pub fn write_text(&self, text: &str) {
        let swell = Swell::get();
        let bytes = text.as_bytes();
        let length = bytes.len() + 1;
        let data = swell.GlobalAlloc(raw::GMEM_MOVEABLE as _, length as _);
        let locker = Locker::new(data);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes as *const [u8] as *const _, locker.lock, length);
        }
        swell.EmptyClipboard();
        swell.SetClipboardData(swell.CF_TEXT(), data);
    }

    pub fn read_text(&self) -> Result<String, &'static str> {
        let swell = Swell::get();
        let data =
            NonNull::new(swell.GetClipboardData(swell.CF_TEXT())).ok_or("clipboard empty")?;
        let locker = Locker::new(data.as_ptr());
        let c_str = unsafe { CStr::from_ptr(locker.lock as *const c_char) };
        c_str
            .to_owned()
            .into_string()
            .map_err(|_| "clipboard content not valid UTF-8")
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        Swell::get().CloseClipboard();
    }
}

struct Locker {
    lock: *mut c_void,
    data: raw::HANDLE,
}

impl Locker {
    fn new(data: raw::HANDLE) -> Locker {
        Locker {
            lock: Swell::get().GlobalLock(data),
            data,
        }
    }
}

impl Drop for Locker {
    fn drop(&mut self) {
        Swell::get().GlobalUnlock(self.data)
    }
}

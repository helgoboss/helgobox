use crate::infrastructure::common::win32::{GetDlgItem, SetWindowText, HWND};
use std::ffi::CString;

/// Represents a window (in the win32 sense where windows are not only top-level windows but also
/// embedded components)
#[derive(Clone, Copy, Debug)]
pub struct Window {
    hwnd: HWND,
}

impl Window {
    pub fn new(hwnd: HWND) -> Window {
        Window { hwnd }
    }

    pub fn get_hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn find_control(&self, control_id: u32) -> Option<Window> {
        let hwnd = unsafe { GetDlgItem(self.hwnd, control_id as i32) };
        if hwnd.is_null() {
            return None;
        }
        Some(Window::new(hwnd))
    }

    pub fn set_text(&self, text: &str) {
        use std::ffi::OsStr;
        use std::iter::once;
        let c_str = CString::new(text).expect("string too exotic");
        unsafe { SetWindowText(self.hwnd, c_str.as_ptr()) };
    }
}

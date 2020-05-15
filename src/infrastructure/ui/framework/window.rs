use reaper_low::raw::WM_CLOSE;
use reaper_low::{raw, Swell};
use std::ffi::CString;

/// Represents a window (in the win32 sense where windows are not only top-level windows but also
/// embedded components)
#[derive(Clone, Copy, Debug)]
pub struct Window {
    hwnd: raw::HWND,
}

impl Window {
    pub fn new(hwnd: raw::HWND) -> Window {
        Window { hwnd }
    }

    pub fn get_hwnd(&self) -> raw::HWND {
        self.hwnd
    }

    pub fn find_control(&self, control_id: u32) -> Option<Window> {
        let hwnd = unsafe { Swell::get().GetDlgItem(self.hwnd, control_id as i32) };
        if hwnd.is_null() {
            return None;
        }
        Some(Window::new(hwnd))
    }

    pub fn close(&self) {
        Swell::get().SendMessage(self.hwnd, WM_CLOSE, 0, 0);
    }

    pub fn set_text(&self, text: &str) {
        let c_str = CString::new(text).expect("string too exotic");
        unsafe { Swell::get().SetWindowText(self.hwnd, c_str.as_ptr()) };
    }
}

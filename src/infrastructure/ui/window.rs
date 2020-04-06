use winapi::shared::windef::HWND;

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
        use winapi::um::winuser::GetDlgItem;
        let hwnd = unsafe { GetDlgItem(self.hwnd, control_id as i32) };
        if hwnd.is_null() {
            return None;
        }
        Some(Window::new(hwnd))
    }

    pub fn set_text(&self, text: &str) {
        use std::ffi::OsStr;
        use std::iter::once;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use winapi::um::winuser::SetWindowTextW;
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(once(0)).collect();
        unsafe { SetWindowTextW(self.hwnd, wide.as_ptr()) };
    }
}

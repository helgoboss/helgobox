use crate::infrastructure::ui::framework::{DialogUnits, Dimensions, Pixels};
use reaper_low::raw::WM_CLOSE;
use reaper_low::{raw, Swell};
use std::ffi::CString;

/// Represents a window (in the win32 sense where windows are not only top-level windows but also
/// embedded components)
#[derive(Clone, Copy, Debug)]
pub struct Window {
    raw: raw::HWND,
}

impl Window {
    pub fn new(hwnd: raw::HWND) -> Option<Window> {
        if hwnd.is_null() {
            return None;
        }
        Some(Window { raw: hwnd })
    }

    pub fn raw(&self) -> raw::HWND {
        self.raw
    }

    pub fn find_control(&self, control_id: u32) -> Option<Window> {
        let hwnd = unsafe { Swell::get().GetDlgItem(self.raw, control_id as i32) };
        Window::new(hwnd)
    }

    pub fn close(&self) {
        Swell::get().SendMessage(self.raw, WM_CLOSE, 0, 0);
    }

    pub fn set_text(&self, text: &str) {
        let c_str = CString::new(text).expect("string too exotic");
        unsafe { Swell::get().SetWindowText(self.raw, c_str.as_ptr()) };
    }

    pub fn parent(&self) -> Option<Window> {
        Window::new(Swell::get().GetParent(self.raw))
    }

    /// Converts the given dialog unit dimensions to pixels with window information.
    ///
    /// Makes difference on Windows. On Windows the calculation is based on HiDPI settings. The
    /// given window must be a dialog window, otherwise it returns the wrong value
    ///
    /// On other systems the calculation just uses a constant factor.
    pub fn dimensions_to_pixels(&self, dimensions: Dimensions<DialogUnits>) -> Dimensions<Pixels> {
        #[cfg(target_family = "windows")]
        {
            use crate::infrastructure::common::bindings::root::*;
            let mut rect = tagRECT {
                left: 0,
                top: 0,
                right: dimensions.width.as_raw(),
                bottom: dimensions.height.as_raw(),
            };
            unsafe {
                MapDialogRect(self.raw as _, &mut rect as _);
            }
            Dimensions {
                width: Pixels(rect.right as u32),
                height: Pixels(rect.bottom as u32),
            }
        }
        #[cfg(target_family = "unix")]
        self.to_pixels()
    }
}

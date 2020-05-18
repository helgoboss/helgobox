use crate::bindings::root;
use crate::{DialogUnits, Dimensions, Pixels, Point};
use reaper_low::{raw, Swell};
use std::ffi::CString;
use std::ptr::null_mut;

/// Represents a window.
///
/// _Window_ is meant in the win32 sense, where windows are not only top-level windows but also
/// embedded components such as buttons or text fields.
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

    pub fn require_control(&self, control_id: u32) -> Window {
        self.find_control(control_id)
            .expect("required control not found")
    }

    pub fn is_checked(&self) -> bool {
        Swell::get().SendMessage(self.raw, raw::BM_GETCHECK, 0, 0) == raw::BST_CHECKED as isize
    }

    pub fn close(&self) {
        Swell::get().SendMessage(self.raw, raw::WM_CLOSE, 0, 0);
    }

    pub fn set_text(&self, text: &str) {
        let c_str = CString::new(text).expect("string too exotic");
        unsafe { Swell::get().SetWindowText(self.raw, c_str.as_ptr()) };
    }

    pub fn parent(&self) -> Option<Window> {
        Window::new(Swell::get().GetParent(self.raw))
    }

    pub fn show(&self) {
        Swell::get().ShowWindow(self.raw, raw::SW_SHOW);
    }

    pub fn destroy(&self) {
        Swell::get().DestroyWindow(self.raw);
    }

    pub fn move_to(&self, point: Point<DialogUnits>) {
        let point: Point<_> = self.convert_to_pixels(point);
        Swell::get().SetWindowPos(
            self.raw,
            null_mut(),
            point.x.as_raw(),
            point.y.as_raw(),
            0,
            0,
            raw::SWP_NOSIZE,
        );
    }

    /// Converts the given dialog unit point or dimensions to a pixels point or dimensions by using
    /// window information.
    ///
    /// Makes difference on Windows. On Windows the calculation is based on HiDPI settings. The
    /// given window must be a dialog window, otherwise it returns the wrong value
    ///
    /// On other systems the calculation just uses a constant factor.
    pub fn convert_to_pixels<T: From<Point<Pixels>>>(
        &self,
        point: impl Into<Point<DialogUnits>>,
    ) -> T {
        let point = point.into();
        #[cfg(target_family = "windows")]
        {
            let mut rect = root::tagRECT {
                left: 0,
                top: 0,
                right: point.x.as_raw(),
                bottom: point.y.as_raw(),
            };
            unsafe {
                root::MapDialogRect(self.raw as _, &mut rect as _);
            }
            Point {
                x: Pixels(rect.right as u32),
                y: Pixels(rect.bottom as u32),
            }
            .into()
        }
        #[cfg(target_family = "unix")]
        point.in_pixels().into()
    }
}

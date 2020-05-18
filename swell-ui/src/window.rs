use crate::bindings::root;
use crate::{DialogUnits, Dimensions, Pixels, Point};
use reaper_low::{raw, Swell};
use std::ffi::{CStr, CString};
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

    pub fn set_checked(&self, is_checked: bool) {
        Swell::get().SendMessage(
            self.raw,
            raw::BM_SETCHECK,
            if is_checked {
                raw::BST_CHECKED
            } else {
                raw::BST_UNCHECKED
            } as usize,
            0,
        );
    }

    pub fn check(&self) {
        self.set_checked(true);
    }

    pub fn uncheck(&self) {
        self.set_checked(false);
    }

    pub fn is_checked(&self) -> bool {
        Swell::get().SendMessage(self.raw, raw::BM_GETCHECK, 0, 0) == raw::BST_CHECKED as isize
    }

    pub fn add_combo_box_item(&self, label: &CStr) {
        Swell::get().SendMessage(self.raw, raw::CB_ADDSTRING, 0, label.as_ptr() as _);
    }

    pub fn set_combo_box_item_data(&self, index: u32, data: isize) {
        Swell::get().SendMessage(self.raw, raw::CB_SETITEMDATA, index as _, data);
    }

    pub fn combo_box_item_data(&self, index: u32) -> isize {
        Swell::get().SendMessage(self.raw, raw::CB_GETITEMDATA, index as _, 0)
    }

    pub fn clear_combo_box(&self) {
        Swell::get().SendMessage(self.raw, raw::CB_RESETCONTENT, 0, 0);
    }

    pub fn select_combo_box_item(&self, index: u32) {
        Swell::get().SendMessage(self.raw, raw::CB_SETCURSEL, index as _, 0);
    }

    pub fn select_combo_box_item_or_unknown_by_data(
        &self,
        start_index: u32,
        item_data: isize,
        unknown_label: &CStr,
    ) {
        let item_count = self.combo_box_item_count();
        let found_item = (start_index..item_count).any(|index| {
            if self.combo_box_item_data(index) == item_data {
                self.select_combo_box_item(index);
                true
            } else {
                false
            }
        });
        if !found_item {
            self.add_combo_box_item(unknown_label);
            self.select_combo_box_item(item_count);
        }
    }

    pub fn combo_box_item_count(&self) -> u32 {
        Swell::get().SendMessage(self.raw, raw::CB_GETCOUNT, 0, 0) as _
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

    pub fn set_visible(&self, is_shown: bool) {
        Swell::get().ShowWindow(self.raw, if is_shown { raw::SW_SHOW } else { raw::SW_HIDE });
    }

    pub fn show(&self) {
        self.set_visible(true);
    }

    pub fn hide(&self) {
        self.set_visible(false);
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        Swell::get().EnableWindow(self.raw, is_enabled.into());
    }

    pub fn enable(&self) {
        self.set_enabled(true);
    }

    pub fn disable(&self) {
        self.set_enabled(false);
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

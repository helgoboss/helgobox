use crate::{DialogUnits, Menu, Pixels, Point, SwellStringArg};
use reaper_low::raw::RECT;
use reaper_low::{raw, Swell};
use std::ffi::CString;
use std::fmt::Display;
use std::os::raw::c_char;
use std::ptr::{null, null_mut, NonNull};
use std::time::Duration;

/// Represents a window.
///
/// _Window_ is meant in the win32 sense, where windows are not only top-level windows but also
/// embedded components such as buttons or text fields.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Window {
    raw: raw::HWND,
}

impl Window {
    pub fn new(hwnd: raw::HWND) -> Option<Window> {
        NonNull::new(hwnd).map(Window::from_non_null)
    }

    pub fn cursor_pos() -> Point<Pixels> {
        let mut point = raw::POINT { x: 0, y: 0 };
        unsafe { Swell::get().GetCursorPos(&mut point as _) };
        Point::new(Pixels(point.x as _), Pixels(point.y as _))
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn dark_mode_is_enabled() -> bool {
        #[cfg(target_os = "macos")]
        {
            Swell::get().SWELL_osx_is_dark_mode(1)
        }
        #[cfg(target_os = "windows")]
        unsafe {
            use palette::{IntoColor, Srgb};
            let color = winapi::um::uxtheme::GetThemeSysColor(
                null_mut(),
                winapi::um::winuser::COLOR_WINDOW,
            );
            let (r, g, b) = (
                Swell::GetRValue(color),
                Swell::GetGValue(color),
                Swell::GetBValue(color),
            );
            let rgb: Srgb = palette::Srgb::new(r, g, b).into_format();
            let luma = rgb.into_luma();
            luma.luma < 0.5
        }
    }

    pub fn from_non_null(hwnd: NonNull<raw::HWND__>) -> Window {
        Window { raw: hwnd.as_ptr() }
    }

    pub fn raw(self) -> raw::HWND {
        self.raw
    }

    pub fn raw_non_null(self) -> NonNull<raw::HWND__> {
        unsafe { NonNull::new_unchecked(self.raw) }
    }

    pub fn find_control(self, control_id: u32) -> Option<Window> {
        let hwnd = unsafe { Swell::get().GetDlgItem(self.raw, control_id as i32) };
        Window::new(hwnd)
    }

    pub fn require_control(self, control_id: u32) -> Window {
        self.find_control(control_id)
            .expect("required control not found")
    }

    /// Attention: This blocks the thread but continues the event loop, so you shouldn't have
    /// anything borrowed while calling this unless you want errors due to reentrancy.
    pub fn alert<'a>(
        self,
        caption: impl Into<SwellStringArg<'a>>,
        msg: impl Into<SwellStringArg<'a>>,
    ) {
        unsafe {
            Swell::get().MessageBox(
                self.raw,
                msg.into().as_ptr(),
                caption.into().as_ptr(),
                raw::MB_OK as _,
            );
        }
    }

    /// Attention: This blocks the thread but continues the event loop, so you shouldn't have
    /// anything borrowed while calling this unless you want errors due to reentrancy.
    pub fn confirm<'a>(
        self,
        caption: impl Into<SwellStringArg<'a>>,
        msg: impl Into<SwellStringArg<'a>>,
    ) -> bool {
        let result = unsafe {
            Swell::get().MessageBox(
                self.raw,
                msg.into().as_ptr(),
                caption.into().as_ptr(),
                raw::MB_YESNO as _,
            )
        };
        result == raw::IDYES as _
    }

    /// Attention: This blocks the thread but continues the event loop, so you shouldn't have
    /// anything borrowed while calling this unless you want errors due to reentrancy.
    pub fn ask_yes_no_or_cancel<'a>(
        self,
        caption: impl Into<SwellStringArg<'a>>,
        msg: impl Into<SwellStringArg<'a>>,
    ) -> Option<bool> {
        let result = unsafe {
            Swell::get().MessageBox(
                self.raw,
                msg.into().as_ptr(),
                caption.into().as_ptr(),
                raw::MB_YESNOCANCEL as _,
            )
        };
        if result == raw::IDYES as _ {
            Some(true)
        } else if result == raw::IDNO as _ {
            Some(false)
        } else {
            None
        }
    }

    pub fn set_checked_or_hide(self, is_checked: Option<bool>) {
        if let Some(state) = is_checked {
            self.show();
            self.set_checked(state);
        } else {
            self.hide();
        }
    }

    pub fn set_checked(self, is_checked: bool) {
        unsafe {
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
    }

    pub fn check(self) {
        self.set_checked(true);
    }

    pub fn uncheck(self) {
        self.set_checked(false);
    }

    pub fn is_checked(self) -> bool {
        let result = unsafe { Swell::get().SendMessage(self.raw, raw::BM_GETCHECK, 0, 0) };
        result == raw::BST_CHECKED as isize
    }

    pub fn fill_combo_box_with_data_vec<I: Display>(self, items: Vec<(isize, I)>) {
        self.fill_combo_box_with_data(items.into_iter())
    }

    // TODO-low Check if we can take the items by reference. Probably wouldn't make a big
    //  difference because moving is not a problem in all known cases.
    pub fn fill_combo_box_with_data<I: Display>(
        self,
        items: impl Iterator<Item = (isize, I)> + ExactSizeIterator,
    ) {
        self.clear_combo_box();
        self.maybe_init_combo_box_storage(items.len());
        for (i, (data, item)) in items.enumerate() {
            self.insert_combo_box_item_with_data(i, data, item.to_string());
        }
    }

    /// Okay to use if approximately less than 100 items, otherwise might become slow.
    pub fn fill_combo_box_with_data_small<I: Display>(
        self,
        items: impl Iterator<Item = (isize, I)>,
    ) {
        self.clear_combo_box();
        self.fill_combo_box_with_data_internal(items);
    }

    pub fn fill_combo_box_indexed<I: Display>(
        self,
        items: impl Iterator<Item = I> + ExactSizeIterator,
    ) {
        self.clear_combo_box();
        self.maybe_init_combo_box_storage(items.len());
        for item in items {
            self.add_combo_box_item(item.to_string());
        }
    }

    pub fn fill_combo_box_indexed_vec<I: Display>(self, items: Vec<I>) {
        self.fill_combo_box_indexed(items.into_iter());
    }

    pub fn fill_combo_box_small<I: Display>(self, items: impl Iterator<Item = I>) {
        self.clear_combo_box();
        for item in items {
            self.add_combo_box_item(item.to_string());
        }
    }

    /// Reserves some capacity if there are many items.
    ///
    /// See https://docs.microsoft.com/en-us/windows/win32/controls/cb-initstorage.
    fn maybe_init_combo_box_storage(self, item_count: usize) {
        if item_count > 100 {
            // The 32 is just a rough estimate.
            self.init_combo_box_storage(item_count, 32);
        }
    }

    fn fill_combo_box_with_data_internal<I: Display>(
        self,
        items: impl Iterator<Item = (isize, I)>,
    ) {
        for (i, (data, item)) in items.enumerate() {
            self.insert_combo_box_item_with_data(i, data, item.to_string());
        }
    }

    pub fn init_combo_box_storage(self, item_count: usize, item_size: usize) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::CB_INITSTORAGE, item_count, item_size as _);
        }
    }

    pub fn insert_combo_box_item_with_data<'a>(
        &self,
        index: usize,
        data: isize,
        label: impl Into<SwellStringArg<'a>>,
    ) {
        self.insert_combo_box_item(index, label);
        self.set_combo_box_item_data(index, data);
    }

    pub fn selected_combo_box_item_index(self) -> usize {
        let result = unsafe { Swell::get().SendMessage(self.raw, raw::CB_GETCURSEL, 0, 0) };
        result as usize
    }

    pub fn selected_combo_box_item_data(self) -> isize {
        let index = self.selected_combo_box_item_index();
        self.combo_box_item_data(index)
    }

    pub fn insert_combo_box_item<'a>(self, index: usize, label: impl Into<SwellStringArg<'a>>) {
        unsafe {
            Swell::get().SendMessage(
                self.raw,
                raw::CB_INSERTSTRING,
                index,
                label.into().as_lparam(),
            );
        }
    }

    pub fn add_combo_box_item<'a>(self, label: impl Into<SwellStringArg<'a>>) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::CB_ADDSTRING, 0, label.into().as_lparam());
        }
    }

    pub fn set_combo_box_item_data(self, index: usize, data: isize) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::CB_SETITEMDATA, index, data);
        }
    }

    pub fn combo_box_item_data(self, index: usize) -> isize {
        unsafe { Swell::get().SendMessage(self.raw, raw::CB_GETITEMDATA, index, 0) }
    }

    pub fn clear_combo_box(self) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::CB_RESETCONTENT, 0, 0);
        }
    }

    pub fn select_combo_box_item_by_index(self, index: usize) -> Result<(), &'static str> {
        if index >= self.combo_box_item_count() {
            return Err("index for combo box selection out of bound");
        }
        self.select_combo_box_item_by_index_internal(index);
        Ok(())
    }

    fn select_combo_box_item_by_index_internal(self, index: usize) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::CB_SETCURSEL, index, 0);
        }
    }

    pub fn select_combo_box_item_by_data(self, item_data: isize) -> Result<(), &'static str> {
        let item_index = (0..self.combo_box_item_count())
            .find(|index| self.combo_box_item_data(*index) == item_data)
            .ok_or("couldn't find combo box item by item data")?;
        self.select_combo_box_item_by_index_internal(item_index);
        Ok(())
    }

    pub fn select_only_combo_box_item<'a>(self, label: impl Into<SwellStringArg<'a>>) {
        self.clear_combo_box();
        self.select_new_combo_box_item(label);
    }

    pub fn select_new_combo_box_item<'a>(self, label: impl Into<SwellStringArg<'a>>) {
        self.add_combo_box_item(label);
        self.select_combo_box_item_by_index_internal(self.combo_box_item_count() - 1);
    }

    pub fn combo_box_item_count(self) -> usize {
        let result = unsafe { Swell::get().SendMessage(self.raw, raw::CB_GETCOUNT, 0, 0) };
        result as _
    }

    pub fn close(self) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::WM_CLOSE, 0, 0);
        }
    }

    pub fn set_timer(self, id: usize, duration: Duration) {
        unsafe {
            Swell::get().SetTimer(self.raw, id, duration.as_millis() as _, None);
        }
    }

    pub fn kill_timer(self, id: usize) {
        unsafe {
            Swell::get().KillTimer(self.raw, id);
        }
    }

    pub fn redraw(self) {
        let swell = Swell::get();
        unsafe {
            let mut rect = RECT::default();
            swell.GetClientRect(self.raw, &mut rect as _);
            swell.InvalidateRect(self.raw, &rect as _, 0);
            swell.UpdateWindow(self.raw);
        }
    }

    pub fn has_focus(&self) -> bool {
        Swell::get().GetFocus() == self.raw
    }

    pub fn focus(&self) {
        unsafe {
            Swell::get().SetFocus(self.raw);
        }
    }

    pub fn set_slider_range(&self, min: u32, max: u32) {
        unsafe {
            Swell::get().SendMessage(self.raw, raw::TBM_SETRANGE, 0, make_long(min, max));
        }
    }

    pub fn set_slider_value(&self, value: u32) {
        if self.has_focus() && (key_is_down(raw::VK_LBUTTON) || key_is_down(raw::VK_RBUTTON)) {
            // No need to set value if this is the slider which we are currently tracking.
            return;
        }
        unsafe {
            Swell::get().SendMessage(self.raw, raw::TBM_SETPOS, 1, value as _);
        }
    }

    pub fn slider_value(&self) -> u32 {
        let result = unsafe { Swell::get().SendMessage(self.raw, raw::TBM_GETPOS, 0, 0) };
        result as _
    }

    /// Returns up to 256 bytes and doesn't do line break conversion.
    pub fn text(self) -> Result<String, &'static str> {
        self.text_internal(256)
    }

    /// Returns up to 20000 bytes. Also normalizes line breaks to simple unix-style linefeeds.
    pub fn multi_line_text(self) -> Result<String, &'static str> {
        let text = self.text_internal(20_000)?;
        let fixed_text = if cfg!(windows) {
            text.replace("\r\n", "\n")
        } else {
            text
        };
        Ok(fixed_text)
    }

    fn text_internal(self, max_size: u32) -> Result<String, &'static str> {
        let (text, result) = with_string_buffer(max_size, |buffer, max_size| unsafe {
            Swell::get().GetWindowText(self.raw, buffer, max_size)
        });
        if result == 0 {
            return Err("handle not found or doesn't support text");
        }
        text.into_string().map_err(|_| "non UTF-8")
    }

    pub fn set_text<'a>(self, text: impl Into<SwellStringArg<'a>>) {
        unsafe { Swell::get().SetWindowText(self.raw, text.into().as_ptr()) };
    }

    /// Converts line breaks to Windows-style if necessary.
    pub fn set_multi_line_text(self, text: impl AsRef<str>) {
        #[cfg(windows)]
        {
            let fixed_text = text.as_ref().replace('\n', "\r\n");
            self.set_text(fixed_text)
        }
        #[cfg(unix)]
        {
            self.set_text(text.as_ref());
        }
    }

    pub fn set_text_or_hide<'a>(self, text: Option<impl Into<SwellStringArg<'a>>>) {
        if let Some(t) = text {
            self.set_text(t);
            self.show();
        } else {
            self.hide();
        }
    }

    pub fn parent(self) -> Option<Window> {
        Window::new(unsafe { Swell::get().GetParent(self.raw) })
    }

    pub fn set_visible(self, is_shown: bool) {
        unsafe {
            Swell::get().ShowWindow(self.raw, if is_shown { raw::SW_SHOW } else { raw::SW_HIDE });
        }
    }

    pub fn show(self) {
        self.set_visible(true);
    }

    pub fn hide(self) {
        self.set_visible(false);
    }

    pub fn set_enabled(self, is_enabled: bool) {
        unsafe {
            Swell::get().EnableWindow(self.raw, is_enabled.into());
        }
    }

    pub fn enable(self) {
        self.set_enabled(true);
    }

    pub fn disable(self) {
        self.set_enabled(false);
    }

    pub fn destroy(self) {
        unsafe {
            Swell::get().DestroyWindow(self.raw);
        }
    }

    pub fn open_popup_menu(self, menu: Menu, location: Point<Pixels>) -> Option<u32> {
        let swell = Swell::get();
        let result = unsafe {
            swell.TrackPopupMenu(
                menu.raw(),
                raw::TPM_RETURNCMD as _,
                location.x.get() as _,
                location.y.get() as _,
                0,
                self.raw(),
                null(),
            )
        };
        if result == 0 {
            return None;
        }
        Some(result as _)
    }

    pub fn move_to(self, point: Point<DialogUnits>) {
        let point: Point<_> = self.convert_to_pixels(point);
        unsafe {
            Swell::get().SetWindowPos(
                self.raw,
                null_mut(),
                point.x.as_raw(),
                point.y.as_raw(),
                0,
                0,
                raw::SWP_NOSIZE as _,
            );
        }
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
            let mut rect = winapi::shared::windef::RECT {
                left: 0,
                top: 0,
                right: point.x.as_raw(),
                bottom: point.y.as_raw(),
            };
            unsafe {
                winapi::um::winuser::MapDialogRect(self.raw as _, &mut rect as _);
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

fn with_string_buffer<T>(
    max_size: u32,
    fill_buffer: impl FnOnce(*mut c_char, i32) -> T,
) -> (CString, T) {
    let vec: Vec<u8> = vec![1; max_size as usize];
    let c_string = unsafe { CString::from_vec_unchecked(vec) };
    let raw = c_string.into_raw();
    let result = fill_buffer(raw, max_size as i32);
    let string = unsafe { CString::from_raw(raw) };
    (string, result)
}

fn make_long(lo: u32, hi: u32) -> isize {
    ((lo & 0xffff) | ((hi & 0xffff) << 16)) as _
}

fn key_is_down(key: u32) -> bool {
    Swell::get().GetAsyncKeyState(key as _) & 0x8000 != 0
}

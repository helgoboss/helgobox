use crate::{menu_tree, DialogUnits, Dimensions, Menu, MenuBar, Pixels, Point, SwellStringArg};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use reaper_low::raw::RECT;
use reaper_low::{raw, Swell};
use reaper_medium::Hwnd;
use std::ffi::CString;
use std::fmt::Display;
use std::os::raw::c_char;
use std::ptr::{null, null_mut};
use std::time::Duration;

/// Represents a window.
///
/// _Window_ is meant in the win32 sense, where windows are not only top-level windows but also
/// embedded components such as buttons or text fields.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Window {
    raw: raw::HWND,
}

unsafe impl Send for Window {}

impl Window {
    pub fn new(hwnd: raw::HWND) -> Option<Window> {
        Hwnd::new(hwnd).map(Window::from_hwnd)
    }

    pub fn from_raw_window_handle(handle: RawWindowHandle) -> Result<Self, &'static str> {
        let window_ptr = match handle {
            RawWindowHandle::AppKit(h) => h.ns_view,
            RawWindowHandle::Win32(h) => h.hwnd,
            _ => return Err("unsupported raw window handle"),
        };
        Self::new(window_ptr as _).ok_or("couldn't obtain window from raw window handle")
    }

    pub fn screen_size() -> Dimensions<Pixels> {
        Dimensions::new(Self::screen_width(), Self::screen_height())
    }

    pub fn screen_width() -> Pixels {
        Pixels(Swell::get().GetSystemMetrics(raw::SM_CXSCREEN) as _)
    }

    pub fn screen_height() -> Pixels {
        Pixels(Swell::get().GetSystemMetrics(raw::SM_CYSCREEN) as _)
    }

    pub fn cursor_pos() -> Point<Pixels> {
        let mut point = raw::POINT { x: 0, y: 0 };
        unsafe { Swell::get().GetCursorPos(&mut point as _) };
        Point::new(Pixels(point.x as _), Pixels(point.y as _))
    }

    /// On Linux, this always returns `false` at the moment.
    pub fn dark_mode_is_enabled() -> bool {
        #[cfg(target_os = "macos")]
        {
            Swell::get().SWELL_osx_is_dark_mode(0)
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
        #[cfg(target_os = "linux")]
        {
            false
        }
    }

    pub fn from_hwnd(hwnd: Hwnd) -> Window {
        Window { raw: hwnd.as_ptr() }
    }

    pub fn focused() -> Option<Window> {
        Self::new(Swell::get().GetFocus())
    }

    pub fn raw(self) -> raw::HWND {
        self.raw
    }

    pub fn size(self) -> Dimensions<Pixels> {
        let rect = self.rect();
        Dimensions::new(
            Pixels(rect.right as u32 - rect.left as u32),
            Pixels(rect.bottom as u32 - rect.top as u32),
        )
    }

    pub fn rect(self) -> raw::RECT {
        let mut rect = RECT::default();
        unsafe { Swell::get().GetClientRect(self.raw, &mut rect) };
        rect
    }

    pub fn raw_hwnd(self) -> Hwnd {
        Hwnd::new(self.raw).expect("raw hwnd is null")
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

    /// This return the first child *window* (not subview) on macOS.
    #[cfg(target_os = "macos")]
    pub fn first_child_window(&self) -> Option<Window> {
        unsafe {
            let view = &*(self.raw as *const crate::macos::NSView);
            let view_window = view.window()?;
            let child_content_view = view_window.child_windows().first_object()?.content_view()?;
            Window::new(child_content_view.as_ref() as *const _ as raw::HWND)
        }
    }

    /// Lets this window process the current app event if this is not a text field.
    ///
    /// Returns whether it was a text field or not.
    #[cfg(target_os = "macos")]
    pub fn process_current_app_event_if_no_text_field(self) -> bool {
        unsafe {
            let view = &*(self.raw as *const crate::macos::NSView);
            if view.is_kind_of_class(objc2::class!(NSTextField))
                || view.is_kind_of_class(objc2::class!(NSTextView))
            {
                return false;
            }
            let app = crate::macos::ns_app();
            if let Some(current_event) = app.current_event() {
                if let Some(window) = view.window() {
                    window.send_event(&current_event);
                }
            }
            true
        }
    }

    /// Also invokes TranslateMessage.
    #[cfg(target_family = "windows")]
    pub fn process_raw_message(self, msg: raw::MSG) {
        unsafe {
            winapi::um::winuser::TranslateMessage(&msg as *const _ as _);
            Swell::get().SendMessage(self.raw, msg.message, msg.wParam, msg.lParam);
        }
    }

    #[cfg(target_family = "unix")]
    pub fn process_raw_message(self, msg: raw::MSG) {
        unsafe {
            Swell::get().SendMessage(self.raw, msg.message, msg.wParam, msg.lParam);
        }
    }

    /// Attention: Doesn't invoke TranslateMessage message on Windows.
    pub fn process_raw_message_flexible(
        self,
        msg: raw::UINT,
        wparam: raw::WPARAM,
        lparam: raw::LPARAM,
    ) -> raw::LRESULT {
        unsafe { Swell::get().SendMessage(self.raw, msg, wparam, lparam) }
    }

    /// Attention: On macOS, this returns the first subview, not the first child window.
    pub fn first_child(&self) -> Option<Window> {
        let swell = Swell::get();
        let ptr = unsafe { swell.GetWindow(self.raw, raw::GW_CHILD as _) };
        Window::new(ptr)
    }

    #[cfg(target_os = "linux")]
    pub fn get_xlib_handle(&self) -> Result<raw_window_handle::XlibHandle, &'static str> {
        // This function actually works correctly we don't have egui enabled on Linux currently
        // anyway (full-text search for "wonky" to get an explanation). We could have just let
        // this function unused, but I had to comment out gdk_sys crate dependencies because
        // cross v2.5 uses Ubuntu < v20, which has an old glib that's not compatible with the
        // gdk_sys dependencies. So it wouldn't compile anymore, that's why I commented it out.
        todo!()
        // let swell = Swell::get();
        // swell.pointers().SWELL_GetOSWindow.ok_or(
        //     "Couldn't load function SWELL_GetOSWindow. Please use an up-to-date REAPER version!",
        // )?;
        // let gdk_window = unsafe {
        //     swell.SWELL_GetOSWindow(
        //         self.raw,
        //         reaper_medium::reaper_str!("GdkWindow").as_c_str().as_ptr(),
        //     )
        // } as *mut gdk_sys::GdkWindow;
        // if gdk_window.is_null() {
        //     return Err("Couldn't get OS window from SWELL window");
        // }
        // let gdk_display = unsafe { gdk_sys::gdk_window_get_display(gdk_window) };
        // if gdk_display.is_null() {
        //     return Err("Couldn't get GDK display from SWELL OS window");
        // }
        // let x_display = unsafe { gdk_x11_sys::gdk_x11_display_get_xdisplay(gdk_display as _) };
        // if x_display.is_null() {
        //     return Err("Couldn't get X display from GDK display");
        // }
        // let x_window = unsafe { gdk_x11_sys::gdk_x11_window_get_xid(gdk_window as _) };
        // if x_window == 0 {
        //     return Err("Couldn't get X window from GDK window");
        // }
        // let mut handle = raw_window_handle::XlibHandle::empty();
        // handle.window = x_window as _;
        // handle.display = x_display as _;
        // Ok(handle)
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

    pub fn resize_first_child_according_to_parent(self) {
        unsafe {
            extern "C" fn resize_proc(arg1: raw::HWND, _arg2: raw::LPARAM) -> raw::BOOL {
                if let Some(win) = Window::new(arg1) {
                    win.resize(win.parent().unwrap().size());
                }
                0
            }
            Swell::get().EnumChildWindows(self.raw, Some(resize_proc), 0);
        }
    }

    pub fn focus_first_child(self) {
        unsafe {
            extern "C" fn focus_proc(arg1: raw::HWND, _arg2: raw::LPARAM) -> raw::BOOL {
                if let Some(win) = Window::new(arg1) {
                    win.focus();
                }
                0
            }
            Swell::get().EnumChildWindows(self.raw, Some(focus_proc), 0);
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

    pub fn is_visible(self) -> bool {
        unsafe { Swell::get().IsWindowVisible(self.raw) }
    }

    pub fn is_open(self) -> bool {
        unsafe { Swell::get().IsWindow(self.raw) }
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

    pub fn open_simple_popup_menu<T>(
        self,
        mut pure_menu: menu_tree::Menu<T>,
        location: Point<Pixels>,
    ) -> Option<T> {
        let menu_bar = MenuBar::new_popup_menu();
        pure_menu.index(1);
        menu_tree::fill_menu(menu_bar.menu(), &pure_menu);
        let result_index = self.open_popup_menu(menu_bar.menu(), location)?;
        let res = pure_menu
            .find_item_by_id(result_index)
            .expect("selected menu item not found")
            .result;
        Some(res)
    }

    pub fn move_to_pixels(self, point: Point<Pixels>) {
        unsafe {
            Swell::get().SetWindowPos(
                self.raw,
                null_mut(),
                point.x.as_raw(),
                point.y.as_raw(),
                0,
                0,
                (raw::SWP_NOSIZE | raw::SWP_NOZORDER) as _,
            );
        }
    }

    pub fn move_to_dialog_units(self, point: Point<DialogUnits>) {
        self.move_to_pixels(self.convert_to_pixels(point));
    }

    pub fn resize(self, dimensions: Dimensions<Pixels>) {
        unsafe {
            Swell::get().SetWindowPos(
                self.raw,
                null_mut(),
                0,
                0,
                dimensions.width.as_raw(),
                dimensions.height.as_raw(),
                // (raw::SWP_NOMOVE | raw::SWP_NOZORDER) as _,
                0,
            );
        }
    }

    pub fn taborder_first(self) {
        /// zorder is used to set taborder,
        /// note HWND_BOTTOM should be drawn as the first (to be the last in zorder),
        /// and so it is the first child (and so the first in taborder)

        /// to check: I have not found HWND_xxx constants in reaper-low bindings...
        const HWND_BOTTOM: raw::HWND = 0x1 as raw::HWND;
        unsafe {
            Swell::get().SetWindowPos(
                self.raw,
                HWND_BOTTOM,
                0,
                0,
                0,
                0,
                (raw::SWP_NOSIZE | raw::SWP_NOMOVE) as _,
            );
        }
    }

    #[cfg(target_family = "windows")]
    pub fn dpi_scaling_factor(&self) -> f64 {
        self.dpi() as f64 / 96.0
    }

    #[cfg(target_family = "windows")]
    pub fn dpi(&self) -> u32 {
        let dynamic_win_api = crate::win::DynamicWinApi::load();
        if let Some(f) = dynamic_win_api.get_dpi_for_window() {
            f(self.raw as _)
        } else {
            96
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

    /// The reverse of `convert_to_pixels`.
    pub fn convert_to_dialog_units<T: From<Point<DialogUnits>>>(
        &self,
        point: impl Into<Point<Pixels>>,
    ) -> T {
        // Because Windows doesn't provide the reverse of MapDialogRect, we calculate the
        // scaling factors ourselves.
        let (x_factor, y_factor) = self.dialog_unit_to_pixel_scaling_factors();
        let point = point.into();
        Point::new(
            DialogUnits((point.x.get() as f64 / x_factor).round() as u32),
            DialogUnits((point.y.get() as f64 / y_factor).round() as u32),
        )
        .into()
    }

    /// Calculates the dialog-unit to pixel scaling factors for each axis.
    fn dialog_unit_to_pixel_scaling_factors(&self) -> (f64, f64) {
        let du_rect = Point::new(DialogUnits(1000), DialogUnits(1000));
        let px_rect: Point<_> = self.convert_to_pixels(du_rect);
        let x_factor = px_rect.x.get() as f64 / du_rect.x.get() as f64;
        let y_factor = px_rect.y.get() as f64 / du_rect.y.get() as f64;
        (x_factor, y_factor)
    }
}

unsafe impl HasRawWindowHandle for Window {
    fn raw_window_handle(&self) -> RawWindowHandle {
        #[cfg(target_os = "macos")]
        {
            let mut handle = raw_window_handle::AppKitHandle::empty();
            handle.ns_view = self.raw as *mut core::ffi::c_void;
            RawWindowHandle::AppKit(handle)
        }
        #[cfg(target_os = "windows")]
        {
            let mut handle = raw_window_handle::Win32Handle::empty();
            handle.hwnd = self.raw as *mut core::ffi::c_void;
            RawWindowHandle::Win32(handle)
        }
        #[cfg(target_os = "linux")]
        {
            let handle = self.get_xlib_handle().expect("couldn't get xlib handle");
            RawWindowHandle::Xlib(handle)
        }
    }
}

/// We use this on Linux to create a parented baseview/egui window (OpenGL) inside.
#[cfg(target_os = "linux")]
pub struct XBridgeWindow {
    /// Our parent. A normal SWELL window as we have many in ReaLearn. Usually an empty window.
    parent_window: Window,
    /// This is a SWELL window created by SWELL_CreateXBridgeWindow, but it's not the X window!
    /// Plus, it's not a normal SWELL window, it's a *special* one. E.g. it can't
    /// give us a GdkWindow when queried via GetOSWindow()! The parent window needs to be used
    /// for that.
    _bridge_window: Window,
    /// ID of the X window. This is what it is all about! This is the X window inside which we
    /// fire up the OpenGL context.
    x_window_id: core::ffi::c_ulong,
}

#[cfg(target_os = "linux")]
impl XBridgeWindow {
    /// Creates an X bridge window inside the given parent window.
    ///
    /// The parent window must be a normal SWELL window as we have many of them in ReaLearn.
    /// Usually an empty window.
    pub fn create(parent_window: Window) -> Result<Self, &'static str> {
        let mut x_window_id: core::ffi::c_ulong = 0;
        let rect = parent_window.rect();
        let x_bridge_hwnd = unsafe {
            Swell::get().SWELL_CreateXBridgeWindow(
                parent_window.raw(),
                &mut x_window_id as *mut core::ffi::c_ulong as *mut *mut core::ffi::c_void,
                &rect as *const _,
            )
        };
        let bridge_window =
            Window::new(x_bridge_hwnd).ok_or("SWELL_CreateXBridgeWindow returned null")?;
        if x_window_id == 0 {
            return Err("SWELL_CreateXBridgeWindow didn't assign the X window ref");
        }
        unsafe {
            let prop_key = reaper_medium::reaper_str!("SWELL_XBRIDGE_KBHOOK_CHECK");
            let msg = raw::WM_USER as i32 + 100;
            Swell::get().SetProp(
                bridge_window.raw(),
                prop_key.as_c_str().as_ptr(),
                msg as *mut core::ffi::c_void,
            );
        }
        let x_bridge_window = Self {
            parent_window,
            _bridge_window: bridge_window,
            x_window_id,
        };
        Ok(x_bridge_window)
    }
}

#[cfg(target_os = "linux")]
unsafe impl HasRawWindowHandle for XBridgeWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        let mut handle = raw_window_handle::XlibHandle::empty();
        // It's important to get the X display from the parent. The bridge_window itself is a
        // special SWELL window which doesn't give us the GdkWindow and therefore we can't obtain
        // the xlib handle from it and as a consequence also no the X display.
        let parent_xlib_handle = self
            .parent_window
            .get_xlib_handle()
            .expect("couldn't get xlib handle of x bridge parent");
        handle.window = self.x_window_id;
        handle.display = parent_xlib_handle.display;
        RawWindowHandle::Xlib(handle)
    }
}

pub enum SwellWindow {
    Normal(Window),
    #[cfg(target_os = "linux")]
    XBridge(XBridgeWindow),
}

unsafe impl HasRawWindowHandle for SwellWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match self {
            SwellWindow::Normal(w) => w.raw_window_handle(),
            #[cfg(target_os = "linux")]
            SwellWindow::XBridge(w) => w.raw_window_handle(),
        }
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

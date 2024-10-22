use crate::{create_window, DeviceContext, Pixels, Point, SharedView, Window};
use reaper_low::raw;
use rxrust::prelude::*;

use reaper_medium::Hbrush;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;

/// Represents a displayable logical part of the UI, such as a panel.
///
/// Each view has a 1:0..1 relationship to a window. One can say that a view is
/// implemented/displayed by a window. A window (= HWND) is the more low-level technical
/// implementation concept which has lots of possible behavior (see `Window` struct) whereas a view
/// is a higher-level logical concept which uses window methods to implement very particular and
/// aptly-named logic which makes sense for that part of the UI.
///
/// All views have a few things in common, e.g. views can be opened (window gets created) and closed
/// (window gets destroyed). These common things are modeled by this trait. In addition to a common
/// interface, this trait provides default implementations for that common behavior.
///
/// An other important part of this trait are window events/callbacks, which implementors can
/// handle.
///
/// # Design
///
/// ## Why do view callback methods take self not as mutable reference?
/// win32 window procedures can be *reentered*, see the win32 docs! Now let's assume we would take
/// self as mutable reference (`&mut self`). If we would have a borrow checker (`RefCell`), it would
/// complain on reentry by panicking. Rightly so. Without `RefCell` things would get very unsafe and
/// we wouldn't even get notified about it. I think the only correct way is to never let the window
/// procedure call view methods in a mutable context. Make all view handler methods take an
/// immutable reference. The same strategy which we are using with `IReaperControlSurface` in
/// `reaper-rs`, because this is reentrant as well.
///
/// ## Why are there no exceptions?
/// One could argue that e.g. `WM_INITDIALOG` is not reentered and we could therefore make an
/// exception. But not only the win32 window procedure might call our view, also our own code. Just
/// think of a `close()` method which takes `&mut self` and calls `DestroyWindow()`. Windows would
/// send a `WM_INITDIALOG` message while we are still in the `close()` method, et voilà ... we would
/// have 2 mutable accesses. It's just not safe and would cause a false feeling of security!
///
/// ## So how do we mutate things in the callback methods?
/// Everything which needs to be mutable needs to be wrapped with a `RefCell`. We need to pursue the
/// fine-granular `RefCell` approach because reentrancy is unavoidable. We just need to make sure
/// not to write to the same data member non-exclusively. If we fail to achieve that, at least
/// the panic lets us know about the issue.
///
/// ## Why do view callback methods take self as `SharedView<Self>`?
/// Given the above mentioned safety measures and knowing that we must keep views as `Rc`s anyway
/// (for lifetime reasons, see `ViewManager`), it is possible to take self as `SharedView<Self>`
/// without sacrificing anything. The obvious advantage we have is that it gives us an easy way to
/// access view methods in subscribe closures without running into lifetime problems (such as &self
/// disappearing while still being used in the closure).
pub trait View: Debug {
    // Data providers (implementation required, used internally)
    // =========================================================

    /// ID of the dialog resource to look up when creating the window.
    ///
    /// The dialog resource basically defines the window's initial look.
    fn dialog_resource_id(&self) -> u32;

    /// Returns the current window, if any.
    ///
    /// In order to implement behavior common to views, the `View` trait needs mutable access to
    /// this context.
    fn view_context(&self) -> &ViewContext;

    // Event handlers (implementation optional)
    // =================================================

    /// WM_INITDIALOG.
    ///
    /// Should return `true` if you want the window to be actually shown when it's created.
    fn show_window_on_init(&self) -> bool {
        true
    }

    /// WM_INITDIALOG.
    ///
    /// Should return `true` if keyboard focus is desired.
    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        false
    }

    /// WM_CLOSE.
    ///
    /// Should return `true` if the window must not be destroyed.
    fn close_requested(self: SharedView<Self>) -> bool {
        false
    }

    /// WM_DESTROY.
    fn on_destroy(self: SharedView<Self>, _window: Window) {}

    /// WM_SHOWWINDOW.
    ///
    /// Should return `true` if processed.
    fn shown_or_hidden(self: SharedView<Self>, shown: bool) -> bool {
        let _ = shown;
        false
    }

    /// WM_COMMAND, HIWORD(wparam) == 0.
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        let _ = resource_id;
    }

    /// WM_COMMAND, HIWORD(wparam) == CBN_SELCHANGE
    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        let _ = resource_id;
    }

    /// WM_VSCROLL, LOWORD(wparam).
    ///
    /// Should return `true` if processed.
    fn scrolled_vertically(self: SharedView<Self>, _code: u32) -> bool {
        false
    }

    /// WM_HSCROLL, lparam (!= 0).
    fn slider_moved(self: SharedView<Self>, _slider: Window) {}

    /// Should return `true` if processed.
    fn resized(self: SharedView<Self>) -> bool {
        false
    }

    /// Should return `true` if processed.
    fn focused(self: SharedView<Self>) -> bool {
        false
    }

    /// WM_MOUSEWHEEL, HIWORD(wparam).
    ///
    /// Should return `true` if processed.
    fn mouse_wheel_turned(self: SharedView<Self>, distance: i32) -> bool {
        let _ = distance;
        false
    }

    /// WM_MOUSEMOVE.
    ///
    /// Should return `true` if processed.
    fn mouse_moved(self: SharedView<Self>, position: Point<i32>) -> bool {
        let _ = position;
        false
    }

    /// WM_NCHITTEST.
    ///
    /// Should return `true` if processed.
    fn mouse_test(self: SharedView<Self>, position: Point<i32>) -> bool {
        let _ = position;
        false
    }

    /// WM_KEYDOWN.
    ///
    /// Should return `true` if processed.
    fn key_down(self: SharedView<Self>, key_code: u8) -> bool {
        let _ = key_code;
        false
    }

    /// WM_KEYUP.
    ///
    /// On macOS, a multi-line text field fires this instead of edit_control_changed.
    /// But it's not fired on Windows!
    ///
    /// Should return `true` if processed.
    fn key_up(self: SharedView<Self>, key_code: u8) -> bool {
        let _ = key_code;
        false
    }

    /// EN_CHANGE, LOWORD(wparam).
    ///
    /// Should return `true` if processed.
    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        let _ = resource_id;
        false
    }

    /// EN_SETFOCUS, LOWORD(wparam).
    ///
    /// Should return `true` if processed.
    fn edit_control_focus_set(self: SharedView<Self>, resource_id: u32) -> bool {
        let _ = resource_id;
        false
    }

    /// EN_KILLFOCUS, LOWORD(wparam).
    ///
    /// Should return `true` if processed.
    ///
    /// Currently not fired on Linux!
    fn edit_control_focus_killed(self: SharedView<Self>, _resource_id: u32) -> bool {
        false
    }

    /// WM_CONTEXTMENU
    ///
    /// Should return `true` if processed in order to prevent the context menu request going up to higher layers.
    fn context_menu_wanted(self: SharedView<Self>, _location: Point<Pixels>) -> bool {
        false
    }

    /// WM_PAINT
    ///
    /// Should return `true` if processed.
    fn paint(self: SharedView<Self>) -> bool {
        false
    }

    /// WM_ERASEBKGND
    ///
    /// Should return `true` if processed.
    fn erase_background(self: SharedView<Self>, device_context: DeviceContext) -> bool {
        let _ = device_context;
        false
    }

    /// WM_CTLCOLORSTATIC
    ///
    /// Can return a custom background brush for painting that control.
    fn control_color_static(
        self: SharedView<Self>,
        device_context: DeviceContext,
        window: Window,
    ) -> Option<Hbrush> {
        let _ = device_context;
        let _ = window;
        None
    }

    /// WM_CTLCOLORDLG
    ///
    /// Can return a custom background brush for painting that dialog.
    fn control_color_dialog(
        self: SharedView<Self>,
        device_context: DeviceContext,
        window: Window,
    ) -> Option<Hbrush> {
        let _ = device_context;
        let _ = window;
        None
    }

    /// Timer with the given ID fires.
    ///
    /// Should return `true` if processed.
    fn timer(&self, id: usize) -> bool {
        let _ = id;
        false
    }

    /// Called whenever the DialogProc (not WindowProc!!!) is called, before any other callback
    /// method.
    ///
    /// Return `None` to indicate that processing should continue, that is, the other callback
    /// methods should be called accordingly.
    fn process_raw(
        &self,
        window: Window,
        msg: raw::UINT,
        wparam: raw::WPARAM,
        lparam: raw::LPARAM,
    ) -> Option<raw::INT_PTR> {
        let _ = window;
        let _ = msg;
        let _ = wparam;
        let _ = lparam;
        None
    }

    fn get_keyboard_event_receiver(&self, focused_window: Window) -> Option<Window> {
        Some(focused_window)
    }

    /// If `true`, `RealearnAccelerator` will forward raw keyboard events to this window.
    ///
    /// - Absolutely necessary for egui containers (so that egui receives keyboard events)
    /// - For normal dialogs this can be bad (at least on Windows) because tabbing through text fields is not possible
    ///   anymore (https://github.com/helgoboss/helgobox/issues/1213)
    fn wants_raw_keyboard_input(&self) -> bool {
        false
    }

    // Public methods (intended to be used by consumers)
    // =================================================

    /// Opens this view in the given parent window.
    fn open(self: SharedView<Self>, parent_window: Window) -> Option<Window>
    where
        Self: Sized + 'static,
    {
        let resource_id = self.dialog_resource_id();
        create_window(self, resource_id, Some(parent_window))
    }

    /// Opens this view in a free window.
    fn open_without_parent(self: SharedView<Self>) -> Option<Window>
    where
        Self: Sized + 'static,
    {
        let resource_id = self.dialog_resource_id();
        create_window(self, resource_id, None)
    }

    /// Closes this view.
    fn close(&self) {
        if let Some(window) = self.view_context().window.get() {
            window.destroy();
        }
    }

    /// Returns whether this view is currently open.
    fn is_open(&self) -> bool {
        self.view_context().window.get().is_some()
    }
}

/// Context data of a view.
///
/// If Rust traits could provide data in the form of fields, this would be it.
#[derive(Clone, Default, Debug)]
pub struct ViewContext {
    pub(crate) window: Cell<Option<Window>>,
    pub(crate) closed_subject: RefCell<LocalSubject<'static, (), ()>>,
}

impl ViewContext {
    /// Returns the current window associated with this view if this view is open.
    pub fn window(&self) -> Option<Window> {
        self.window.get()
    }

    /// Returns the current window associated with this view.
    ///
    /// # Panics
    ///
    /// Panics if the window doesn't exist (the view is not open).
    pub fn require_window(&self) -> Window {
        self.window().expect("window not found but required")
    }

    /// Returns the control with the given resource ID.
    ///
    /// # Panics
    ///
    /// Panics if the window or control doesn't exist.
    pub fn require_control(&self, resource_id: u32) -> Window {
        self.require_window().require_control(resource_id)
    }

    /// Fires when the window is closed.
    pub fn closed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.closed_subject.borrow().clone()
    }
}

use crate::{create_window, Window};
use rxrust::prelude::*;
use std::borrow::BorrowMut;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt::Debug;
use std::rc::{Rc, Weak};

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
/// ## Why do view callback methods take self as `Rc<Self>`?
/// Given the above mentioned safety measures and knowing that we must keep views as `Rc`s anyway
/// (for lifetime reasons, see `ViewManager`), it is possible to take self as `Rc<Self>` without
/// sacrificing anything. The obvious advantage we have is that it gives us an easy way to access
/// view methods in subscribe closures without running into lifetime problems (such as &self
/// disappearing while still being used in the closure).
pub trait View: Debug {
    /// ID of the dialog resource to look up when creating the window.
    ///
    /// The dialog resource basically defines the window's initial look.
    fn dialog_resource_id(&self) -> u32;

    /// Returns the current window, if any.
    ///
    /// In order to implement behavior common to views, the `View` trait needs mutable access to
    /// this context.
    fn view_context(&self) -> &ViewContext;

    /// Opens this view in the given parent window.
    fn open(self: Rc<Self>, parent_window: Window)
    where
        Self: Sized + 'static,
    {
        let resource_id = self.dialog_resource_id();
        create_window(self, resource_id, parent_window);
    }

    /// Closes this view.
    fn close(&self) {
        self.view_context().require_window().close();
    }

    /// Returns whether this view is currently open.
    fn is_open(&self) -> bool {
        self.view_context().window.get().is_some()
    }

    fn opened_internal(self: Rc<Self>, window: Window) -> bool {
        self.view_context().window.replace(Some(window));
        self.opened(window)
    }

    fn closed_internal(self: Rc<Self>) {
        self.clone().closed();
        self.view_context().closed_subject.borrow_mut().next(());
        self.view_context().window.replace(None);
    }

    /// WM_INITDIALOG
    ///
    /// Returns true if keyboard focus is desired.
    fn opened(self: Rc<Self>, window: Window) -> bool {
        false
    }

    /// WM_DESTROY
    fn closed(self: Rc<Self>) {}

    /// WM_COMMAND, HIWORD(wparam) == 0
    fn button_clicked(self: Rc<Self>, resource_id: u32) {}

    /// WM_COMMAND, HIWORD(wparam) == CBN_SELCHANGE
    fn option_selected(self: Rc<Self>, resource_id: u32) {}
}

/// Context data of a view.
///
/// If Rust traits could provide data in the form of fields, this would be it.
#[derive(Clone, Default, Debug)]
pub struct ViewContext {
    window: Cell<Option<Window>>,
    closed_subject: RefCell<LocalSubject<'static, (), ()>>,
}

impl ViewContext {
    /// Returns the current window associated with this view.
    ///
    /// # Panics
    ///
    /// Panics if the window doesn't exist (the view is not open).
    pub fn require_window(&self) -> Window {
        self.window.get().expect("window not found but required")
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
    pub fn closed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> {
        self.closed_subject.borrow().clone()
    }
}

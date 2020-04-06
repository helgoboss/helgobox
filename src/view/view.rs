use crate::view::Window;
use std::fmt::Debug;
use winapi::_core::mem::zeroed;
use winapi::_core::ptr::null_mut;
use winapi::shared::minwindef::HINSTANCE;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::wingdi::TextOutA;
use winapi::um::winuser::MAKEINTRESOURCEA;
use winapi::um::winuser::{
    BeginPaint, CreateDialogParamA, DefWindowProcW, PostQuitMessage, SW_SHOWDEFAULT, WM_COMMAND,
    WM_DESTROY, WM_INITDIALOG, WM_PAINT,
};

/// UI component. Has a 1:1 relationship with a window handle (as in HWND).
///
/// # Design
///
/// ## Why do view callback methods take self as immutable reference (`&self`)?
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
/// TODO Rename to ViewListener or WindowHandler or anything in-between
pub trait View: Debug {
    fn opened(&self, window: Window) {}

    fn closed(&self) {}

    fn button_clicked(&self, resource_id: u32) {}
}

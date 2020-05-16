use super::Window;
use std::fmt::Debug;
use std::rc::Rc;

/// Trait whose methods represent window events, which implementors can handle.
///
/// Has a 1:1 relationship with a window handle (HWND).
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
pub trait WindowListener: Debug {
    /// WM_INITDIALOG
    fn opened(self: Rc<Self>, _window: Window) {}

    /// WM_DESTROY
    fn closed(self: Rc<Self>) {}

    /// WM_COMMAND TODO
    fn button_clicked(self: Rc<Self>, _resource_id: u32) {}
}

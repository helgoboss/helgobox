use std::io::Error;
use std::os::raw::c_void;

use vst::editor::Editor;
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

use crate::model::RealearnSession;
use crate::view::bindings::root::{ID_IMPORT_BUTTON, ID_MAIN_DIALOG, ID_MAPPINGS_DIALOG};
use crate::view::views::MainView;
use crate::view::{open_view, View};
use std::cell::RefCell;
use std::rc::Rc;

pub struct RealearnEditor<'a> {
    open: bool,
    // TODO Is it possible to get a trait object with View interface even when having the specific
    //  MainView type here? Or even without Box!???
    main_view: Box<dyn View>,
    // `Plugin#get_editor()` must return a Box of something 'static, so it's impossible to take a
    // reference here. Why? Because a reference needs a lifetime. Any non-static lifetime would not
    // satisfy the 'static requirement. Why not require a 'static reference then? Simply because we
    // don't have a session object with static lifetime. The session object is owned by the
    // `Plugin` object, which itself doesn't have a static lifetime. The only way to get a 'static
    // session would be to not let the plugin object own the session but to define a static global.
    // This, however, would be a far worse design than just using a smart pointer here. So using a
    // smart pointer is the best we can do really.
    //
    // This is not the only reason why taking a reference here is not feasible. During the
    // lifecycle of a ReaLearn session we need mutable access to the session both from the
    // editor (of course) and from the plugin (e.g. when REAPER wants us to load some data).
    // When using references, Rust's borrow checker wouldn't let that happen. We can't do anything
    // about this multiple-access requirement, it's just how the VST plugin API works (and
    // many other similar plugin interfaces as well - for good reasons). And being a plugin we have
    // to conform.
    //
    // Fortunately, we know that actually both DAW-plugin interaction (such as loading data) and UI
    // interaction happens in the main thread, in the so called main loop. So there's no need for
    // using a thread-safe smart pointer here. We even can and also should satisfy the borrow
    // checker, meaning that if the session is mutably accessed at a given point in time, it is not
    // accessed from another point as well. This can happen even in a single-threaded environment
    // because functions can call other functions and thereby accessing the same data - just in
    // different stack positions. Just think of reentrancy. Fortunately this is something we can
    // control. And we should, because when this kind of parallel access happens, this can lead to
    // strange bugs which are particularly hard to find.
    //
    // Unfortunately we can't make use of Rust's compile time borrow checker because there's no way
    // that the compiler understands what's going on here. Why? For one thing, because of the VST
    // plugin API design. But first and foremost because we use the FFI, which means we interface
    // with non-Rust code, so Rust couldn't get the complete picture even if the plugin system
    // would be designed in a different way. However, we *can* use Rust's runtime borrow checker
    // `RefCell`. And we should, because it gives us fail-fast behavior. It will let us know
    // immediately when we violated that safety rule.
    // TODO We must take care, however, that REAPER will not crash as a result, that would be very
    //  bad.  See https://github.com/RustAudio/vst-rs/issues/122
    session: Rc<RefCell<RealearnSession<'a>>>,
}

impl<'a> RealearnEditor<'a> {
    pub fn new(session: Rc<RefCell<RealearnSession>>) -> RealearnEditor {
        RealearnEditor {
            open: false,
            session,
            main_view: Box::new(MainView::new()),
        }
    }
}

impl<'a> Editor for RealearnEditor<'a> {
    fn size(&self) -> (i32, i32) {
        (1200, 600)
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        open_view(self.main_view.as_ref(), ID_MAIN_DIALOG, parent as HWND);
        self.open = true;
        true
    }

    fn is_open(&mut self) -> bool {
        self.open
    }
}

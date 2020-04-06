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
use crate::view::{open_view, ViewListener};
use std::cell::RefCell;
use std::rc::Rc;

pub struct RealearnEditor {
    open: bool,
    main_view: Rc<MainView>,
}

impl RealearnEditor {
    pub fn new(session: Rc<RefCell<RealearnSession<'static>>>) -> RealearnEditor {
        RealearnEditor {
            open: false,
            main_view: Rc::new(MainView::new(session)),
        }
    }
}

impl Editor for RealearnEditor {
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
        open_view(self.main_view.clone(), ID_MAIN_DIALOG, parent as HWND);
        self.open = true;
        true
    }

    fn is_open(&mut self) -> bool {
        self.open
    }
}

use crate::domain::RealearnSession;
use crate::infrastructure::ui::views::MainView;
use std::cell::RefCell;

use std::os::raw::c_void;
use std::rc::Rc;
use vst::editor::Editor;
#[cfg(target_os = "windows")]
use winapi::shared::windef::HWND;

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
        self.main_view.clone().open(parent as HWND);
        self.open = true;
        true
    }

    fn is_open(&mut self) -> bool {
        self.open
    }
}

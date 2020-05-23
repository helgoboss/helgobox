use crate::domain::Session;
use crate::infrastructure::ui::MainPanel;
use std::cell::RefCell;

use crate::infrastructure::common::SharedSession;
use lazycell::LazyCell;
use reaper_low::raw::HWND;
use std::borrow::Borrow;
use std::os::raw::c_void;
use std::rc::Rc;
use swell_ui::{Pixels, SharedView, View, Window};
use vst::editor::Editor;

pub struct RealearnEditor {
    session: Rc<LazyCell<SharedSession>>,
    main_panel: LazyCell<SharedView<MainPanel>>,
}

impl RealearnEditor {
    pub fn new(session: Rc<LazyCell<SharedSession>>) -> RealearnEditor {
        RealearnEditor {
            session,
            main_panel: LazyCell::new(),
        }
    }

    fn require_session(&self) -> &SharedSession {
        (*self.session)
            .borrow()
            .expect("session not yet initialized")
    }

    fn main_panel(&self) -> &SharedView<MainPanel> {
        self.main_panel.borrow_with(|| {
            let session = self.require_session().clone();
            Rc::new(MainPanel::new(session))
        })
    }
}

impl Editor for RealearnEditor {
    fn size(&self) -> (i32, i32) {
        self.main_panel().dimensions().to_vst()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        self.main_panel().close()
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        self.main_panel()
            .clone()
            .open_with_resize(Window::new(parent as HWND).expect("no parent window"));
        true
    }

    fn is_open(&mut self) -> bool {
        self.main_panel().is_open()
    }
}

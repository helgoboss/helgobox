use crate::domain::Session;
use crate::infrastructure::ui::MainPanel;
use std::cell::RefCell;

use reaper_low::raw::HWND;
use std::os::raw::c_void;
use std::rc::Rc;
use swell_ui::{Pixels, View, Window};
use vst::editor::Editor;

pub struct RealearnEditor {
    main_panel: Rc<MainPanel>,
}

impl RealearnEditor {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> RealearnEditor {
        RealearnEditor {
            main_panel: Rc::new(MainPanel::new(session)),
        }
    }
}

impl Editor for RealearnEditor {
    fn size(&self) -> (i32, i32) {
        self.main_panel.dimensions().to_vst()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        self.main_panel.close()
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        self.main_panel
            .clone()
            .open_with_resize(Window::new(parent as HWND).expect("no parent window"));
        true
    }

    fn is_open(&mut self) -> bool {
        self.main_panel.is_open()
    }
}

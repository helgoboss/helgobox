use crate::infrastructure::ui::MainPanel;

use reaper_low::firewall;
use reaper_low::raw::HWND;

use std::os::raw::c_void;

use swell_ui::{SharedView, View, Window};
use vst::editor::Editor;

pub struct RealearnEditor {
    main_panel: SharedView<MainPanel>,
}

impl RealearnEditor {
    pub fn new(main_panel: SharedView<MainPanel>) -> RealearnEditor {
        RealearnEditor { main_panel }
    }
}

impl Editor for RealearnEditor {
    fn size(&self) -> (i32, i32) {
        firewall(|| self.main_panel.dimensions().to_vst()).unwrap_or_default()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        firewall(|| self.main_panel.close());
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        firewall(|| {
            self.main_panel
                .clone()
                .open_with_resize(Window::new(parent as HWND).expect("no parent window"));
            true
        })
        .unwrap_or(false)
    }

    fn is_open(&mut self) -> bool {
        firewall(|| self.main_panel.is_open()).unwrap_or(false)
    }
}

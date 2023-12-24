use reaper_low::firewall;
use reaper_low::raw::HWND;

use std::os::raw::c_void;

use crate::infrastructure::ui::instance_panel::InstancePanel;
use swell_ui::{SharedView, View, Window};
use vst::editor::Editor;

pub struct InstanceEditor {
    unit_panel: SharedView<InstancePanel>,
}

impl InstanceEditor {
    pub fn new(unit_panel: SharedView<InstancePanel>) -> Self {
        Self { unit_panel }
    }
}

impl Editor for InstanceEditor {
    fn size(&self) -> (i32, i32) {
        firewall(|| self.unit_panel.dimensions().to_vst()).unwrap_or_default()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        firewall(|| self.unit_panel.close());
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        firewall(|| {
            self.unit_panel
                .clone()
                .open_with_resize(Window::new(parent as HWND).expect("no parent window"));
            true
        })
        .unwrap_or(false)
    }

    fn is_open(&mut self) -> bool {
        firewall(|| self.unit_panel.is_open()).unwrap_or(false)
    }
}

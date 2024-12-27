use reaper_low::raw::HWND;

use std::os::raw::c_void;

use crate::infrastructure::ui::instance_panel::InstancePanel;
use swell_ui::{SharedView, View, Window};
use vst::editor::Editor;

pub struct HelgoboxPluginEditor {
    instance_panel: SharedView<InstancePanel>,
}

impl HelgoboxPluginEditor {
    pub fn new(unit_panel: SharedView<InstancePanel>) -> Self {
        Self {
            instance_panel: unit_panel,
        }
    }
}

impl Editor for HelgoboxPluginEditor {
    fn size(&self) -> (i32, i32) {
        self.instance_panel.dimensions().to_vst()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        self.instance_panel.close();
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        self.instance_panel
            .clone()
            .open_with_resize(Window::new(parent as HWND).expect("no parent window"));
        true
    }

    fn is_open(&mut self) -> bool {
        self.instance_panel.is_open()
    }
}

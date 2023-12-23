use crate::infrastructure::ui::InstancePanel;
use std::cell::{Cell, OnceCell};

use reaper_low::firewall;
use reaper_low::raw::HWND;

use std::os::raw::c_void;
use std::rc::Rc;

use swell_ui::{SharedView, View, Window};
use vst::editor::Editor;

#[derive(Clone)]
pub struct SharedUnitPanel(Rc<UnitPanel>);

impl SharedUnitPanel {
    pub fn new() -> Self {
        Self(Rc::new(UnitPanel::new()))
    }
}

pub struct UnitPanel {
    // TODO-high CONTINUE This should hold multiple panels, one for each instance. Then the
    //  UI state of each instance is memorized.
    main_instance_panel: OnceCell<SharedView<InstancePanel>>,
    open_parent_window: Cell<Option<Window>>,
}

impl UnitPanel {
    pub fn new() -> UnitPanel {
        UnitPanel {
            main_instance_panel: OnceCell::new(),
            open_parent_window: Cell::new(None),
        }
    }
}

impl SharedUnitPanel {
    pub fn notify_main_instance_panel_available(&self, panel: SharedView<InstancePanel>) {
        if let Some(parent_window) = self.0.open_parent_window.get() {
            panel.clone().open_with_resize(parent_window);
        }
        self.0
            .main_instance_panel
            .set(panel)
            .expect("main instance panel already set");
    }
}

impl Editor for SharedUnitPanel {
    fn size(&self) -> (i32, i32) {
        firewall(|| {
            crate::infrastructure::ui::util::main_panel_dimensions()
                .in_pixels()
                .to_vst()
        })
        .unwrap_or_default()
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        firewall(|| {
            self.0.open_parent_window.set(None);
            if let Some(panel) = self.0.main_instance_panel.get() {
                panel.close();
            }
        });
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        firewall(|| {
            let parent_window = Window::new(parent as HWND).expect("parent window not open");
            self.0.open_parent_window.set(Some(parent_window));
            if let Some(panel) = self.0.main_instance_panel.get() {
                panel.clone().open_with_resize(parent_window);
            }
            true
        })
        .unwrap_or(false)
    }

    fn is_open(&mut self) -> bool {
        self.0.open_parent_window.get().is_some()
    }
}

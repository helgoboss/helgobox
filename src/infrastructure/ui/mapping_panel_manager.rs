use crate::domain::{MappingModel, SharedMappingModel};
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::MappingPanel;
use reaper_high::Reaper;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use swell_ui::{SharedView, View, Window};

/// Responsible for managing the currently open top-level mapping panels.
pub struct MappingPanelManager {
    session: SharedSession,
    open_panels: HashMap<*const MappingModel, SharedView<MappingPanel>>,
}

impl MappingPanelManager {
    pub fn new(session: SharedSession) -> MappingPanelManager {
        Self {
            session,
            open_panels: Default::default(),
        }
    }

    /// Opens a panel for editing the given mapping.
    ///
    /// If the window is already open, it will be closed and reopened.
    pub fn edit_mapping(&mut self, mapping: &SharedMappingModel) {
        let session = self.session.clone();
        let panel = self
            .open_panels
            .entry(mapping.as_ptr())
            .or_insert_with(move || {
                let p = MappingPanel::new(session.clone(), mapping.clone());
                SharedView::new(p)
            });
        if panel.is_open() {
            panel.close();
        }
        let reaper_main_window = Window::from_non_null(Reaper::get().main_window());
        panel.clone().open(reaper_main_window);
    }

    /// Closes and removes panels of mappings which don't exist anymore.
    pub fn close_orphan_panels(&mut self) {
        let session = self.session.clone();
        self.open_panels.retain(move |address, panel| {
            if session.borrow().has_mapping(*address) {
                true
            } else {
                panel.close();
                false
            }
        });
    }
}

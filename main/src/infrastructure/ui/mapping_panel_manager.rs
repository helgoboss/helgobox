use crate::domain::SharedMapping;
use crate::domain::WeakSession;
use crate::infrastructure::ui::{MainPanel, MappingPanel};
use reaper_high::Reaper;
use slog::debug;

use swell_ui::{SharedView, View, WeakView, Window};

const MAX_PANEL_COUNT: u32 = 4;

/// Responsible for managing the currently open top-level mapping panels.
#[derive(Debug)]
pub struct MappingPanelManager {
    session: WeakSession,
    main_panel: WeakView<MainPanel>,
    open_panels: Vec<SharedView<MappingPanel>>,
}

impl MappingPanelManager {
    pub fn new(session: WeakSession, main_panel: WeakView<MainPanel>) -> MappingPanelManager {
        Self {
            session,
            main_panel,
            open_panels: Default::default(),
        }
    }

    /// Opens a panel for editing the given mapping.
    ///
    /// If the window is already open, it will be closed and reopened.
    pub fn edit_mapping(&mut self, mapping: &SharedMapping) {
        let existing_panel = self
            .open_panels
            .iter()
            .find(|p| p.mapping_ptr() == mapping.as_ptr());
        if let Some(p) = existing_panel {
            // There's a panel already which show's this mapping.
            p.bring_to_foreground();
            return;
        }
        let panel = self.request_panel();
        panel.show(mapping.clone());
    }

    /// Closes and removes panels of mappings which don't exist anymore.
    pub fn close_orphan_panels(&mut self) {
        let shared_session = self.session.upgrade().expect("session gone");
        let session = shared_session.borrow();
        self.open_panels.retain(|p| {
            if session.has_mapping(p.mapping_ptr()) {
                true
            } else {
                p.close();
                false
            }
        });
    }

    /// Closes and removes all mapping panels
    pub fn close_all(&mut self) {
        for p in &self.open_panels {
            p.close()
        }
        self.open_panels.clear();
    }

    fn request_panel(&mut self) -> SharedView<MappingPanel> {
        self.find_free_panel()
            .or_else(|| self.create_new_panel_if_not_exhausted())
            .unwrap_or_else(|| self.hijack_existing_panel())
    }

    fn find_free_panel(&self) -> Option<SharedView<MappingPanel>> {
        self.open_panels.iter().find(|p| p.is_free()).cloned()
    }

    fn create_new_panel_if_not_exhausted(&mut self) -> Option<SharedView<MappingPanel>> {
        if self.open_panels.len() < MAX_PANEL_COUNT as _ {
            Some(self.create_new_panel())
        } else {
            None
        }
    }

    fn create_new_panel(&mut self) -> SharedView<MappingPanel> {
        let panel = SharedView::new(MappingPanel::new(
            self.session.clone(),
            self.main_panel.clone(),
        ));
        let panel_clone_1 = panel.clone();
        let panel_clone_2 = panel.clone();
        self.open_panels.push(panel);
        let reaper_main_window = Window::from_non_null(Reaper::get().main_window());
        panel_clone_1.open(reaper_main_window);
        panel_clone_2
    }

    fn hijack_existing_panel(&self) -> SharedView<MappingPanel> {
        self.open_panels.first().expect("no existing panel").clone()
    }
}

impl Drop for MappingPanelManager {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping panel manager...");
        // Those are (intentionally) REAPER child windows, not ReaLearn child windows. So we need to
        // close them manually as soon as ReaLearn is unloaded.
        self.close_all();
    }
}

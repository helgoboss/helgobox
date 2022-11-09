use crate::infrastructure::ui::{MainPanel, MappingPanel, SessionMessagePanel};
use reaper_high::Reaper;
use slog::debug;

use crate::application::{Affected, Session, SessionProp, SharedMapping, WeakSession};
use crate::domain::{
    Compartment, MappingId, MappingMatchedEvent, TargetControlEvent, TargetValueChangedEvent,
};
use swell_ui::{SharedView, View, WeakView, Window};

const MAX_PANEL_COUNT: u32 = 4;

/// Responsible for managing the currently open top-level mapping panels.
#[derive(Debug)]
pub struct IndependentPanelManager {
    session: WeakSession,
    main_panel: WeakView<MainPanel>,
    mapping_panels: Vec<SharedView<MappingPanel>>,
    message_panel: SharedView<SessionMessagePanel>,
}

impl IndependentPanelManager {
    pub fn new(session: WeakSession, main_panel: WeakView<MainPanel>) -> IndependentPanelManager {
        Self {
            session: session.clone(),
            main_panel,
            mapping_panels: Default::default(),
            message_panel: SharedView::new(SessionMessagePanel::new(session)),
        }
    }

    pub fn handle_changed_target_value(&self, event: TargetValueChangedEvent) {
        self.do_with_mapping_panel(event.compartment, event.mapping_id, |p| {
            p.handle_changed_target_value(event.targets, event.new_value)
        });
    }

    pub fn handle_matched_mapping(&self, event: MappingMatchedEvent) {
        self.do_with_mapping_panel(event.compartment, event.mapping_id, |p| {
            p.handle_matched_mapping();
        });
    }

    pub fn handle_target_control_event(&self, event: TargetControlEvent) {
        self.do_with_mapping_panel(event.id.compartment, event.id.id, |p| {
            p.handle_target_control_event(event);
        });
    }

    pub fn handle_affected(&self, affected: &Affected<SessionProp>, initiator: Option<u32>) {
        for p in self.mapping_panels.iter().filter(|p| p.is_open()) {
            p.handle_affected(affected, initiator);
        }
    }

    fn do_with_mapping_panel(
        &self,
        compartment: Compartment,
        mapping_id: MappingId,
        f: impl Fn(SharedView<MappingPanel>),
    ) {
        for p in &self.mapping_panels {
            if let Some(m) = p.displayed_mapping() {
                let is_our_mapping = {
                    let m = m.borrow();
                    m.compartment() == compartment && m.id() == mapping_id
                };
                if is_our_mapping {
                    f(p.clone());
                }
            }
        }
    }

    pub fn handle_changed_parameters(&self, session: &Session) {
        for p in &self.mapping_panels {
            let _ = p.clone().notify_parameters_changed(session);
        }
    }

    pub fn open_message_panel(&self) {
        self.message_panel.clone().open(reaper_main_window());
    }

    pub fn close_message_panel(&self) {
        self.message_panel.clone().close();
    }

    /// Opens a panel for editing the given mapping.
    ///
    /// If the window is already open, it will be closed and reopened.
    pub fn edit_mapping(&mut self, mapping: &SharedMapping) {
        let existing_panel = self
            .mapping_panels
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

    /// Hides panels of mappings which don't exist anymore.
    pub fn close_orphan_panels(&mut self) {
        let shared_session = self.session.upgrade().expect("session gone");
        let session = shared_session.borrow();
        for p in &self.mapping_panels {
            if !session.has_mapping(p.mapping_ptr()) {
                p.hide();
            }
        }
    }

    /// Closes and removes all independent panels
    fn destroy(&mut self) {
        self.message_panel.close();
        for p in &self.mapping_panels {
            p.close()
        }
        self.mapping_panels.clear();
    }

    fn request_panel(&mut self) -> SharedView<MappingPanel> {
        self.find_free_panel()
            .or_else(|| self.create_new_panel_if_not_exhausted())
            .unwrap_or_else(|| self.hijack_existing_panel())
    }

    fn find_free_panel(&self) -> Option<SharedView<MappingPanel>> {
        self.mapping_panels.iter().find(|p| p.is_free()).cloned()
    }

    fn create_new_panel_if_not_exhausted(&mut self) -> Option<SharedView<MappingPanel>> {
        if self.mapping_panels.len() < MAX_PANEL_COUNT as _ {
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
        self.mapping_panels.push(panel);
        panel_clone_1.open(reaper_main_window());
        panel_clone_2
    }

    fn hijack_existing_panel(&self) -> SharedView<MappingPanel> {
        self.mapping_panels
            .first()
            .expect("no existing panel")
            .clone()
    }
}

impl Drop for IndependentPanelManager {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping panel manager...");
        // Those are (intentionally) REAPER child windows, not ReaLearn child windows. So we need to
        // close them manually as soon as ReaLearn is unloaded.
        self.destroy();
    }
}

fn reaper_main_window() -> Window {
    Window::from_non_null(Reaper::get().main_window())
}

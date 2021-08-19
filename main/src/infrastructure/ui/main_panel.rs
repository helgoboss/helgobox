use crate::infrastructure::ui::{
    bindings::root, util, HeaderPanel, IndependentPanelManager, MappingRowsPanel,
    SharedIndependentPanelManager, SharedMainState,
};

use lazycell::LazyCell;
use reaper_high::Reaper;

use slog::debug;
use std::cell::{Cell, RefCell};

use crate::application::{Session, SessionUi, WeakSession};
use crate::base::when;
use crate::domain::{
    MappingCompartment, MappingId, MappingMatchedEvent, ProjectionFeedbackValue,
    TargetValueChangedEvent,
};
use crate::infrastructure::plugin::{App, RealearnPluginParameters};
use crate::infrastructure::server::send_projection_feedback_to_subscribed_clients;
use rxrust::prelude::*;
use std::rc::{Rc, Weak};
use std::sync;
use swell_ui::{DialogUnits, Dimensions, Pixels, Point, SharedView, View, ViewContext, Window};

/// The complete ReaLearn panel containing everything.
// TODO-low Maybe call this SessionPanel
#[derive(Debug)]
pub struct MainPanel {
    view: ViewContext,
    active_data: LazyCell<ActiveData>,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    state: SharedMainState,
    plugin_parameters: sync::Weak<RealearnPluginParameters>,
}

#[derive(Debug)]
struct ActiveData {
    session: WeakSession,
    header_panel: SharedView<HeaderPanel>,
    mapping_rows_panel: SharedView<MappingRowsPanel>,
    panel_manager: SharedIndependentPanelManager,
}

impl MainPanel {
    pub fn new(plugin_parameters: sync::Weak<RealearnPluginParameters>) -> Self {
        Self {
            view: Default::default(),
            active_data: LazyCell::new(),
            dimensions: None.into(),
            state: Default::default(),
            plugin_parameters,
        }
    }

    pub fn notify_session_is_available(self: Rc<Self>, session: WeakSession) {
        // Finally, the session is available. First, save its reference and create sub panels.
        let panel_manager = IndependentPanelManager::new(session.clone(), Rc::downgrade(&self));
        let panel_manager = Rc::new(RefCell::new(panel_manager));
        let active_data = ActiveData {
            session: session.clone(),
            header_panel: HeaderPanel::new(
                session.clone(),
                self.state.clone(),
                self.plugin_parameters.clone(),
                Rc::downgrade(&panel_manager),
            )
            .into(),
            mapping_rows_panel: MappingRowsPanel::new(
                session,
                Rc::downgrade(&panel_manager),
                self.state.clone(),
                Point::new(DialogUnits(0), DialogUnits(124)),
            )
            .into(),
            panel_manager,
        };
        self.active_data.fill(active_data).unwrap();
        // If the plug-in window is currently open, open the sub panels as well. Now we are talking!
        if let Some(window) = self.view.window() {
            self.open_sub_panels(window);
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| util::MAIN_PANEL_DIMENSIONS.in_pixels())
    }

    pub fn open_with_resize(self: SharedView<Self>, parent_window: Window) {
        #[cfg(target_family = "windows")]
        {
            // On Windows, the first time opening the dialog window is just to determine the best
            // dimensions based on HiDPI settings.
            // TODO-low If we skip this, the dimensions would be saved. Wouldn't that be better?
            //  I guess if there are multiple screens, keeping this line is better. Then it's a
            //  matter of reopening the GUI to improve scaling. Test it!
            self.dimensions.replace(None);
        }
        self.open(parent_window)
    }

    pub fn force_scroll_to_mapping(&self, mapping_id: MappingId) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel.force_scroll_to_mapping(mapping_id);
        }
    }

    pub fn edit_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel
                .edit_mapping(compartment, mapping_id);
        }
    }

    fn open_sub_panels(&self, window: Window) {
        if let Some(data) = self.active_data.borrow() {
            data.header_panel.clone().open(window);
            data.mapping_rows_panel.clone().open(window);
        }
    }

    fn invalidate_status_text(&self) {
        let state = self.state.borrow();
        self.view
            .require_control(root::ID_MAIN_PANEL_STATUS_TEXT)
            .set_text(state.status_msg.get_ref().as_str());
    }

    fn invalidate_version_text(&self) {
        self.view
            .require_control(root::ID_MAIN_PANEL_VERSION_TEXT)
            .set_text(format!("ReaLearn {}", App::detailed_version_label()));
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_version_text();
        self.invalidate_status_text();
    }

    fn register_listeners(self: SharedView<Self>) {
        let state = self.state.borrow();
        self.when(state.status_msg.changed(), |view| {
            view.invalidate_status_text();
        });
    }

    fn handle_changed_target_value(&self, event: TargetValueChangedEvent) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_changed_target_value(event);
        }
    }

    fn handle_matched_mapping(&self, event: MappingMatchedEvent) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager.borrow().handle_matched_mapping(event);
            if self.is_open() {
                data.mapping_rows_panel.handle_matched_mapping(event);
            }
        }
    }

    fn handle_changed_parameters(&self, session: &Session) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_changed_parameters(session);
        }
    }

    fn when(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = (), Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_async(move |panel, _| reaction(panel));
    }
}

impl View for MainPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAIN_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "windows")]
        if self.dimensions.get().is_none() {
            // The dialog has been opened by user request but the optimal dimensions have not yet
            // been figured out. Figure them out now.
            self.dimensions
                .replace(Some(window.convert_to_pixels(util::MAIN_PANEL_DIMENSIONS)));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.destroy();
            self.open(parent_window);
            return false;
        }
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        self.open_sub_panels(window);
        self.invalidate_all_controls();
        self.register_listeners();
        true
    }
}

impl SessionUi for Weak<MainPanel> {
    fn show_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        upgrade_panel(self).edit_mapping(compartment, mapping_id);
    }

    fn target_value_changed(&self, event: TargetValueChangedEvent) {
        upgrade_panel(self).handle_changed_target_value(event);
    }

    fn parameters_changed(&self, session: &Session) {
        upgrade_panel(self).handle_changed_parameters(session);
    }

    fn send_projection_feedback(&self, session: &Session, value: ProjectionFeedbackValue) {
        let _ = send_projection_feedback_to_subscribed_clients(session.id(), value);
    }

    fn mapping_matched(&self, event: MappingMatchedEvent) {
        upgrade_panel(self).handle_matched_mapping(event);
    }
}

fn upgrade_panel(panel: &Weak<MainPanel>) -> Rc<MainPanel> {
    panel.upgrade().expect("main panel not existing anymore")
}

impl Drop for MainPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping main panel...");
    }
}

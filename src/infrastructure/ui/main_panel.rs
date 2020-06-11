use crate::core::{prop, Prop};
use crate::domain::{MappingModel, Session};
use crate::domain::{ReaperTarget, SharedSession};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::ui::{constants, HeaderPanel, MappingRowsPanel};
use c_str_macro::c_str;
use helgoboss_learn::MidiSource;
use lazycell::LazyCell;
use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::rc::Rc;
use swell_ui::{Dimensions, Pixels, SharedView, View, ViewContext, Window};

/// The complete ReaLearn panel containing everything.
// TODO Maybe call this SessionPanel
pub struct MainPanel {
    pub target_filter: RefCell<Prop<Option<ReaperTarget>>>,
    pub is_learning_target_filter: RefCell<Prop<bool>>,
    pub source_filter: RefCell<Prop<Option<MidiSource>>>,
    pub is_learning_source_filter: RefCell<Prop<bool>>,
    view: ViewContext,
    active_data: LazyCell<ActiveData>,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
}

struct ActiveData {
    session: SharedSession,
    header_panel: SharedView<HeaderPanel>,
    mapping_rows_panel: SharedView<MappingRowsPanel>,
}

impl Default for MainPanel {
    fn default() -> Self {
        Self {
            view: Default::default(),
            active_data: LazyCell::new(),
            dimensions: None.into(),
            target_filter: prop(None).into(),
            is_learning_target_filter: prop(false).into(),
            source_filter: prop(None).into(),
            is_learning_source_filter: prop(false).into(),
        }
    }
}

impl MainPanel {
    pub fn new() -> MainPanel {
        Default::default()
    }

    pub fn notify_session_is_available(self: Rc<Self>, session: SharedSession) {
        // Finally, the session is available. First, save its reference and create sub panels.
        let active_data = ActiveData {
            session: session.clone(),
            header_panel: HeaderPanel::new(session.clone(), self.clone()).into(),
            mapping_rows_panel: MappingRowsPanel::new(session.clone(), self.clone()).into(),
        };
        self.active_data.fill(active_data);
        // If the plug-in window is currently open, open the sub panels as well. Now we are talking!
        if let Some(window) = self.view.window() {
            self.open_sub_panels(window);
        }
    }

    pub fn clear_filters(&self) {
        self.clear_source_filter();
        self.clear_target_filter();
    }

    pub fn clear_source_filter(&self) {
        self.source_filter.borrow_mut().set(None)
    }

    pub fn clear_target_filter(&self) {
        self.target_filter.borrow_mut().set(None)
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| constants::MAIN_PANEL_DIMENSIONS.in_pixels())
    }

    pub fn open_with_resize(self: SharedView<Self>, parent_window: Window) {
        #[cfg(target_family = "windows")]
        {
            // On Windows, the first time opening the dialog window is just to determine the best
            // dimensions based on HiDPI settings.
            // TODO If we skip this, the dimensions would be saved. Wouldn't that be better?
            self.dimensions.replace(None);
        }
        self.open(parent_window)
    }

    pub fn scroll_to_mapping(&self, mapping: *const MappingModel) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel.scroll_to_mapping(mapping);
        }
    }

    fn open_sub_panels(&self, window: Window) {
        if let Some(data) = self.active_data.borrow() {
            data.header_panel.clone().open(window);
            data.mapping_rows_panel.clone().open(window);
        }
    }
}

impl View for MainPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAIN_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "windows")]
        if self.dimensions.get().is_none() {
            // The dialog has been opened by user request but the optimal dimensions have not yet
            // been figured out. Figure them out now.
            self.dimensions.replace(Some(
                window.convert_to_pixels(constants::MAIN_PANEL_DIMENSIONS),
            ));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.destroy();
            self.open(parent_window);
            return false;
        }
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        self.open_sub_panels(window);
        true
    }
}

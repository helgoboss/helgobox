use crate::domain::Session;
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::ui::{constants, HeaderPanel, MappingRowsPanel};
use c_str_macro::c_str;
use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::rc::Rc;
use swell_ui::{Dimensions, Pixels, View, Window};

/// The complete ReaLearn panel containing everything.
#[derive(Debug)]
pub struct MainPanel {
    window: Cell<Option<Window>>,
    header_panel: Rc<HeaderPanel>,
    mapping_rows_panel: Rc<MappingRowsPanel>,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    session: Rc<RefCell<Session<'static>>>,
}

impl MainPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> MainPanel {
        MainPanel {
            window: None.into(),
            header_panel: HeaderPanel::new(session.clone()).into(),
            mapping_rows_panel: MappingRowsPanel::new(session.clone()).into(),
            dimensions: None.into(),
            session,
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| constants::MAIN_PANEL_DIMENSIONS.in_pixels())
    }

    pub fn open_with_resize(self: Rc<Self>, parent_window: Window) {
        #[cfg(target_family = "windows")]
        {
            // On Windows, the first time opening the dialog window is just to determine the best
            // dimensions based on HiDPI settings.
            // TODO If we skip this, the dimensions would be saved. Wouldn't that be better?
            self.dimensions.replace(None);
        }
        self.open(parent_window)
    }
}

impl View for MainPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAIN_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) -> bool {
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
            window.close();
            self.open(parent_window);
            return false;
        }
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        self.header_panel.clone().open(window);
        self.mapping_rows_panel.clone().open(window);
        true
    }
}

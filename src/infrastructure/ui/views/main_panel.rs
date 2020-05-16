use crate::domain::Session;
use crate::infrastructure::common::bindings::root::{
    ID_MAIN_DIALOG, ID_MAPPINGS_DIALOG, ID_MAPPING_ROWS_DIALOG,
};
use crate::infrastructure::ui::framework::{create_window, Dimensions, Pixels, View, Window};
use crate::infrastructure::ui::views::constants::MAIN_PANEL_DIMENSIONS;
use crate::infrastructure::ui::views::{HeaderPanel, MappingRowsPanel};
use c_str_macro::c_str;
use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::rc::Rc;

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
            header_panel: Rc::new(HeaderPanel::new(session.clone())),
            mapping_rows_panel: Rc::new(MappingRowsPanel::new(session.clone())),
            dimensions: None.into(),
            session,
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| MAIN_PANEL_DIMENSIONS.in_pixels())
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
        ID_MAIN_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) {
        #[cfg(target_family = "windows")]
        if self.dimensions.get().is_none() {
            // The dialog has been opened by user request but the optimal dimensions have not yet
            // been figured out. Figure them out now.
            self.dimensions
                .replace(Some(window.convert_to_pixels(MAIN_PANEL_DIMENSIONS)));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.close();
            self.open(parent_window);
            return;
        }
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        self.header_panel.clone().open(window);
        self.mapping_rows_panel.clone().open(window);
    }
}

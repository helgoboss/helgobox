use crate::infrastructure::ui::{util, UnitPanel};
use std::cell::{Cell, OnceCell};
use std::fmt::Debug;

use crate::infrastructure::ui::bindings::root;
use swell_ui::{Dimensions, Pixels, SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct InstancePanel {
    view: ViewContext,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    // TODO-high CONTINUE This should hold multiple panels, one for each unit. Then the
    //  UI state of each unit is memorized.
    main_unit_panel: OnceCell<SharedView<UnitPanel>>,
}

impl InstancePanel {
    pub fn new() -> InstancePanel {
        InstancePanel {
            view: Default::default(),
            main_unit_panel: OnceCell::new(),
            dimensions: None.into(),
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| util::main_panel_dimensions().in_pixels())
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

    pub fn notify_main_unit_panel_available(&self, panel: SharedView<UnitPanel>) {
        if let Some(window) = self.view.window() {
            panel.clone().open(window);
        }
        self.main_unit_panel
            .set(panel)
            .expect("main instance panel already set");
    }
}

impl View for InstancePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_INSTANCE_PANEL
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
                window.convert_to_pixels(util::main_panel_dimensions()),
            ));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.destroy();
            self.open(parent_window);
            return false;
        }
        // Add main instance panel if already available
        if let Some(p) = self.main_unit_panel.get() {
            p.clone().open(window);
        }
        true
    }
}

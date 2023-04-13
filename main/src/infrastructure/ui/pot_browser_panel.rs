use crate::domain::pot::SharedRuntimePotUnit;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use crate::infrastructure::ui::egui_views::pot_browser_panel::{run_ui, State};
use derivative::Derivative;
use raw_window_handle::HasRawWindowHandle;
use reaper_low::raw;
use std::cell::Cell;
use swell_ui::{Dimensions, Point, SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct PotBrowserPanel {
    view: ViewContext,
    child_window: Cell<Option<Window>>,
    pot_unit: SharedRuntimePotUnit,
}

impl PotBrowserPanel {
    pub fn new(pot_unit: SharedRuntimePotUnit) -> Self {
        Self {
            view: Default::default(),
            child_window: Default::default(),
            pot_unit,
        }
    }
}

impl View for PotBrowserPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_EMPTY_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        let screen_size = Window::screen_size();
        let window_size = Dimensions::new(screen_size.width * 0.75, screen_size.height * 0.75);
        window.resize(window_size);
        window.move_to_pixels(Point::new(
            (screen_size.width - window_size.width) * 0.5,
            (screen_size.height - window_size.height) * 0.5,
        ));
        let child_window_handle = egui_views::open(
            window,
            "Pot browser",
            State::new(self.pot_unit.clone()),
            run_ui,
        );
        if let Ok(child_window) =
            Window::from_raw_window_handle(child_window_handle.raw_window_handle())
        {
            self.child_window.set(Some(child_window));
        }
        true
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }

    // fn resized(self: SharedView<Self>) -> bool {
    //     // TODO-high CONTINUE This doesn't work yet. Maybe even not necessary?
    //     if let Some(child_window) = self.child_window.get() {
    //         let new_size = self.view.require_window().size();
    //         child_window.resize(new_size);
    //     }
    //     true
    // }
}

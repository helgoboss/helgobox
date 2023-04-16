use crate::domain::pot::SharedRuntimePotUnit;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use crate::infrastructure::ui::egui_views::pot_browser_panel::{run_ui, State};
use derivative::Derivative;
use reaper_low::raw;
use swell_ui::{Dimensions, Point, SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct PotBrowserPanel {
    view: ViewContext,
    pot_unit: SharedRuntimePotUnit,
}

impl PotBrowserPanel {
    pub fn new(pot_unit: SharedRuntimePotUnit) -> Self {
        Self {
            view: Default::default(),
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
        egui_views::open(
            window,
            "Pot browser",
            State::new(self.pot_unit.clone(), window),
            run_ui,
        );
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

    fn resized(self: SharedView<Self>) -> bool {
        egui_views::on_parent_window_resize(self.view.require_window())
    }
}

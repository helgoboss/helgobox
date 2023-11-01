use crate::application::get_track_label;
use crate::domain::{AnyThreadBackboneState, BackboneState};
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use derivative::Derivative;
use pot::{CurrentPreset, PotFavorites, PotFilterExcludes, SharedRuntimePotUnit};
use pot_browser::{run_ui, PotBrowserIntegration, State};
use reaper_high::{Fx, Track};
use reaper_low::raw;
use std::path::Path;
use std::sync::RwLock;
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
            "Pot Browser",
            State::new(self.pot_unit.clone(), window),
            |context, state| {
                run_ui(context, state, &RealearnPotBrowserIntegration);
            },
        );
        true
    }

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

struct RealearnPotBrowserIntegration;

impl PotBrowserIntegration for RealearnPotBrowserIntegration {
    fn get_track_label(&self, track: &Track) -> String {
        get_track_label(track)
    }

    fn pot_preview_template_path(&self) -> Option<&'static Path> {
        App::realearn_pot_preview_template_path()
    }

    fn pot_favorites(&self) -> &'static RwLock<PotFavorites> {
        &AnyThreadBackboneState::get().pot_favorites
    }

    fn with_current_fx_preset(&self, fx: &Fx, f: impl FnOnce(Option<&CurrentPreset>)) {
        let target_state = BackboneState::target_state().borrow();
        f(target_state.current_fx_preset(fx));
    }

    fn with_pot_filter_exclude_list(&self, f: impl FnOnce(&PotFilterExcludes)) {
        f(&BackboneState::get().pot_filter_exclude_list());
    }
}

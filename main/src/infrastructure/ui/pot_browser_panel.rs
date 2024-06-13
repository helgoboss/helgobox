use crate::application::get_track_label;
use crate::domain::{AnyThreadBackboneState, Backbone};
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use camino::Utf8Path;
use derivative::Derivative;
use pot::{CurrentPreset, PotFavorites, PotFilterExcludes, SharedRuntimePotUnit};
use pot_browser::{run_ui, PotBrowserIntegration, State};
use reaper_high::{Fx, Track};
use reaper_low::raw;
use std::sync::RwLock;
use swell_ui::{SharedView, View, ViewContext, Window};

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
        window.size_and_center_on_screen(0.75, 0.75);
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

    fn pot_preview_template_path(&self) -> Option<&'static Utf8Path> {
        BackboneShell::realearn_pot_preview_template_path()
    }

    fn pot_favorites(&self) -> &'static RwLock<PotFavorites> {
        &AnyThreadBackboneState::get().pot_favorites
    }

    fn with_current_fx_preset(&self, fx: &Fx, f: impl FnOnce(Option<&CurrentPreset>)) {
        let target_state = Backbone::target_state().borrow();
        f(target_state.current_fx_preset(fx));
    }

    fn with_pot_filter_exclude_list(&self, f: impl FnOnce(&PotFilterExcludes)) {
        f(&Backbone::get().pot_filter_exclude_list());
    }
}

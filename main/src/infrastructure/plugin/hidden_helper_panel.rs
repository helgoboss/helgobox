use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug, Default)]
pub struct HiddenHelperPanel {
    view: ViewContext,
}

const PLAYTIME_ENGINE_STATS_TIMER_ID: usize = 322;
const HELGOBOX_TOOLBAR_CHECK_TIMER_ID: usize = 323;

impl HiddenHelperPanel {
    pub fn new() -> Self {
        Self::default()
    }
}

impl View for HiddenHelperPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_HIDDEN_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn show_window_on_init(&self) -> bool {
        false
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.set_timer(PLAYTIME_ENGINE_STATS_TIMER_ID, Duration::from_millis(200));
        window.set_timer(HELGOBOX_TOOLBAR_CHECK_TIMER_ID, Duration::from_millis(3000));
        false
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        BackboneShell::get().shutdown();
    }

    fn timer(&self, id: usize) -> bool {
        match id {
            PLAYTIME_ENGINE_STATS_TIMER_ID => {
                #[cfg(feature = "playtime")]
                {
                    BackboneShell::get()
                        .proto_hub()
                        .notify_engine_stats_changed();
                }
                true
            }
            HELGOBOX_TOOLBAR_CHECK_TIMER_ID => {
                BackboneShell::get().disable_manually_removed_dynamic_toolbar_buttons();
                true
            }
            _ => false,
        }
    }
}

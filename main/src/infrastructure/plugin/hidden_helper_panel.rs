use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug, Default)]
pub struct HiddenHelperPanel {
    view: ViewContext,
}

const TIMER_ID: usize = 322;

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
        window.set_timer(TIMER_ID, Duration::from_millis(200));
        false
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        BackboneShell::get().shutdown();
    }

    fn timer(&self, id: usize) -> bool {
        if id != TIMER_ID {
            return false;
        }
        #[cfg(feature = "playtime")]
        {
            BackboneShell::get()
                .proto_hub()
                .notify_engine_stats_changed()
        }
        true
    }
}

use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use base::metrics_util::measure_time;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug, Default)]
pub struct ShutdownDetectionPanel {
    view: ViewContext,
}

impl ShutdownDetectionPanel {
    pub fn new() -> Self {
        Self::default()
    }
}

impl View for ShutdownDetectionPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_HIDDEN_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        _window.set_timer(989, Duration::from_millis(1));
        false
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        BackboneShell::get().shutdown();
    }

    fn timer(&self, id: usize) -> bool {
        if id == 989 {
            metrics::counter!("fast_timer").increment(1);
            BackboneShell::get().run();
        }
        true
    }
}

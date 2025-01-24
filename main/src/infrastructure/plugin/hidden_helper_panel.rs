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
        // Switch lights off. Essential to call this here and not later on drop!
        BackboneShell::get().shutdown();
        if cfg!(target_os = "windows") {
            // On Windows, we traditionally executed destroy hooks. Mainly because REAPER for
            // Windows provides the preference "VST => Allow complete unload of VST plug-ins".
            // On other OS, this option is not available. Therefore, cleaning up highly static
            // resources is not really necessary or even possible.
            // Well, even on Windows, lately "complete unload" leads to crashes. Not sure why.
            // But anyway, we still try our best.
            //
            // We execute those hooks latest on DLL_PROCESS_DETACH.
            // But executing the plug-in destroy hooks **here already** has the advantage that arbitrary tear-down
            // code can be run. The code that can be called on Windows when dropping everything when the
            // DLL gets detached is limited (if not following this rule, panics may occur and that
            // would abort REAPER because the panic occurs in drop).
            // Still, ideally, the tear-down code itself should be safe to execute on DLL_PROCESS_DETACH
            // as well.
            tracing::info!("Executing plug-in destroy hooks from hidden helper panel...");
            reaper_low::execute_plugin_destroy_hooks();
        }
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

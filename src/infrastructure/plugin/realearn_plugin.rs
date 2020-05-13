use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin::{HostCallback, Info, Plugin};

use super::RealearnEditor;
use crate::domain::RealearnSession;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{reaper_vst_plugin, ReaperPluginContext, Swell};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

reaper_vst_plugin!();

#[derive(Default)]
pub struct RealearnPlugin {
    host: HostCallback,
    session: Rc<RefCell<RealearnSession<'static>>>,
    reaper_guard: Option<Arc<ReaperGuard>>,
}

impl Plugin for RealearnPlugin {
    fn new(host: HostCallback) -> Self {
        Self {
            host,
            ..Default::default()
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "realearn-rs".to_string(),
            unique_id: 2964,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        let guard = Reaper::guarded(|| {
            let context = ReaperPluginContext::from_vst_plugin(
                &self.host,
                reaper_vst_plugin::static_context(),
            )
            .unwrap();
            Swell::make_available_globally(Swell::load(context));
            Reaper::setup_with_defaults(context, "info@helgoboss.org");
            let reaper = Reaper::get();
            reaper.activate();
            reaper.show_console_msg(c_str!("Loaded realearn-rs VST plugin\n"));
        });
        self.reaper_guard = Some(guard);
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(RealearnEditor::new(self.session.clone())))
    }
}

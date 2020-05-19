use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin::{CanDo, HostCallback, Info, Plugin};

use super::RealearnEditor;
use crate::domain::Session;
use crate::infrastructure::common::SharedSession;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{reaper_vst_plugin, ReaperPluginContext, Swell};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use vst::api::Supported;

reaper_vst_plugin!();

#[derive(Default)]
pub struct RealearnPlugin {
    host: HostCallback,
    session: SharedSession,
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

    fn can_do(&self, can_do: CanDo) -> Supported {
        use CanDo::*;
        use Supported::*;
        match can_do {
            // If we don't do this, REAPER won't give us a SWELL parent window, which leads to a
            // horrible crash when doing CreateDialogParam.
            Other(s) if s == "hasCockosViewAsConfig" => Custom(0xbeef_0000),
            _ => Maybe,
        }
    }
}

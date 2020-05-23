use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin::{CanDo, HostCallback, Info, Plugin};

use super::RealearnEditor;
use crate::domain::Session;
use crate::infrastructure::common::SharedSession;
use reaper_high::{Fx, Project, Reaper, ReaperGuard, Take, Track};
use reaper_low::{reaper_vst_plugin, PluginContext, Swell};
use reaper_medium::TypeSpecificPluginContext;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
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
        self.reaper_guard = Some(self.setup_reaper());
        let fx = self.get_containing_fx();
        self.session.borrow_mut().set_containing_fx(fx);
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        let session = self.session.clone();
        Some(Box::new(RealearnEditor::new(session)))
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

impl RealearnPlugin {
    fn setup_reaper(&mut self) -> Arc<ReaperGuard> {
        Reaper::guarded(|| {
            // Done once for all ReaLearn instances
            let context =
                PluginContext::from_vst_plugin(&self.host, reaper_vst_plugin::static_context())
                    .unwrap();
            Swell::make_available_globally(Swell::load(context));
            Reaper::setup_with_defaults(context, "info@helgoboss.org");
            let reaper = Reaper::get();
            reaper.activate();
        })
    }

    /// Calling this in the `new()` method is too early. The containing FX can't generally be found
    /// when we just open a REAPER project. We must wait for `init()` to be called.
    fn get_containing_fx(&self) -> Fx {
        let reaper = Reaper::get();
        let aeffect = NonNull::new(self.host.raw_effect()).expect("must not be null");
        let plugin_context = reaper.medium_reaper().plugin_context();
        let vst_context = match plugin_context.type_specific() {
            TypeSpecificPluginContext::Vst(ctx) => ctx,
            _ => unreachable!(),
        };
        if let Some(track) = unsafe { vst_context.request_containing_track(aeffect) } {
            let project = unsafe { vst_context.request_containing_project(aeffect) };
            let track = Track::new(track, Some(project));
            // TODO Fix this! This is just wrong and super temporary. Right now we are interested in
            // track only.
            track.normal_fx_chain().fx_by_index_untracked(0)
        } else if let Some(take) = unsafe { vst_context.request_containing_take(aeffect) } {
            let take = Take::new(take);
            // TODO Fix this!
            take.fx_chain().fx_by_index_untracked(0)
        } else {
            // TODO Fix this!
            reaper.monitoring_fx_chain().fx_by_index_untracked(0)
        }
    }
}

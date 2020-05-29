use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin::{CanDo, HostCallback, Info, Plugin, PluginParameters};

use super::RealearnEditor;
use crate::domain::Session;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::plugin::realearn_plugin_parameters::RealearnPluginParameters;
use crate::infrastructure::ui::MainPanel;
use lazycell::LazyCell;
use reaper_high::{Fx, Project, Reaper, ReaperGuard, Take, Track};
use reaper_low::{reaper_vst_plugin, PluginContext, Swell};
use reaper_medium::TypeSpecificPluginContext;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use swell_ui::SharedView;
use vst::api::Supported;

reaper_vst_plugin!();

pub struct RealearnPlugin {
    // This will be filled right at construction time. It won't have a session yet though.
    main_panel: SharedView<MainPanel>,
    // This will be set on `new()`.
    host: HostCallback,
    // This will be set as soon as the containing FX is known (one main loop cycle after `init()`).
    session: Rc<LazyCell<SharedSession>>,
    // We need to keep that here in order to notify it as soon as the session becomes available.
    plugin_parameters: Arc<RealearnPluginParameters>,
    // This will be set on `init()`.
    reaper_guard: Option<Arc<ReaperGuard>>,
}

impl Default for RealearnPlugin {
    fn default() -> Self {
        Self {
            host: Default::default(),
            session: Rc::new(LazyCell::new()),
            main_panel: Default::default(),
            reaper_guard: None,
            plugin_parameters: Default::default(),
        }
    }
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
            preset_chunks: true,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        firewall(|| {
            self.reaper_guard = Some(self.ensure_reaper_setup());
            self.schedule_session_creation();
        });
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(RealearnEditor::new(self.main_panel.clone())))
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        use CanDo::*;
        use Supported::*;
        match can_do {
            // If we don't do this, REAPER for Linux won't give us a SWELL plug-in window, which
            // leads to a horrible crash when doing CreateDialogParam. In our UI we use SWELL
            // to put controls into the plug-in window. SWELL assumes that the parent window for
            // controls is also a SWELL window.
            Other(s) if s == "hasCockosViewAsConfig" => Custom(0xbeef_0000),
            _ => Maybe,
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        self.plugin_parameters.clone()
    }
}

impl RealearnPlugin {
    fn ensure_reaper_setup(&mut self) -> Arc<ReaperGuard> {
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

    /// At this point, REAPER cannot reliably give use yet the containing FX. As a
    /// consequence we also don't have a session yet, because creating an incomplete session
    /// pushes the problem of not knowing the containing FX into the application logic, which
    /// we for sure don't want. In the next main loop cycle, it should be possible to
    /// identify the containing FX.
    fn schedule_session_creation(&self) {
        let main_panel = self.main_panel.clone();
        let session_container = self.session.clone();
        let plugin_parameters = self.plugin_parameters.clone();
        let host = self.host;
        Reaper::get().do_later_in_main_thread_asap(move || {
            let session = Session::new(get_containing_fx(&host));
            let shared_session = Rc::new(debug_cell::RefCell::new(session));
            main_panel.notify_session_is_available(shared_session.clone());
            plugin_parameters.notify_session_is_available(shared_session.clone());
            session_container.fill(shared_session);
        });
    }
}

fn firewall<F: FnOnce() -> R, R>(f: F) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// Calling this in the `new()` method is too early. The containing FX can't generally be found
/// when we just open a REAPER project. We must wait for `init()` to be called.
fn get_containing_fx(host: &HostCallback) -> Fx {
    let reaper = Reaper::get();
    let aeffect = NonNull::new(host.raw_effect()).expect("must not be null");
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

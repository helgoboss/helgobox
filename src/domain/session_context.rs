use reaper_high::{Fx, Project, Reaper, Take, Track};
use reaper_medium::TypeSpecificPluginContext;
use std::ptr::NonNull;
use vst::plugin::HostCallback;

#[derive(Clone, Debug)]
pub struct SessionContext {
    containing_fx: Fx,
}

impl SessionContext {
    pub fn from_host(host: &HostCallback) -> SessionContext {
        SessionContext {
            containing_fx: get_containing_fx(host),
        }
    }

    pub fn containing_fx(&self) -> &Fx {
        &self.containing_fx
    }

    pub fn project(&self) -> Project {
        self.containing_fx
            .project()
            .unwrap_or(Reaper::get().current_project())
    }
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

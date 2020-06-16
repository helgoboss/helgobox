use reaper_high::{Fx, Project, Reaper, Take, Track};
use reaper_medium::{ReaperFunctionError, TypeSpecificPluginContext};
use std::ptr::NonNull;
use vst::plugin::HostCallback;

#[derive(Clone, Debug)]
pub struct SessionContext {
    containing_fx: Fx,
}

pub const WAITING_FOR_SESSION_PARAM_NAME: &'static str = "realearn/waiting-for-session";

impl SessionContext {
    pub fn from_host(host: &HostCallback) -> SessionContext {
        SessionContext {
            containing_fx: get_containing_fx(host),
        }
    }

    pub fn containing_fx(&self) -> &Fx {
        &self.containing_fx
    }

    pub fn track(&self) -> Option<&Track> {
        Some(self.containing_fx.track())
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
        // TODO Use REAPER 6.11 API addition instead, if available
        track
            .normal_fx_chain()
            .fxs()
            .find(is_realearn_waiting_for_session)
            .or_else(|| {
                track
                    .input_fx_chain()
                    .fxs()
                    .find(is_realearn_waiting_for_session)
            })
            .expect("couldn't find containing FX")
    } else if let Some(take) = unsafe { vst_context.request_containing_take(aeffect) } {
        let take = Take::new(take);
        // TODO-medium Support usage as take FX
        take.fx_chain().fx_by_index_untracked(0)
    } else {
        // TODO-medium Support usage as monitoring FX
        reaper.monitoring_fx_chain().fx_by_index_untracked(0)
    }
}

fn is_realearn_waiting_for_session(fx: &Fx) -> bool {
    let result = fx.get_named_config_param(WAITING_FOR_SESSION_PARAM_NAME, 1);
    match result {
        Ok(buffer) => *buffer.first().expect("impossible") == 1,
        Err(_) => false,
    }
}

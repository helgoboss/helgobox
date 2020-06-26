use reaper_high::{Fx, FxChain, Project, Reaper, Take, Track};
use reaper_medium::{MessageBoxType, ReaperFunctionError, TypeSpecificPluginContext};
use std::ptr::NonNull;
use vst::plugin::HostCallback;

#[derive(Clone, Debug)]
pub struct SessionContext {
    containing_fx: Fx,
}

pub const WAITING_FOR_SESSION_PARAM_NAME: &'static str = "realearn/waiting-for-session";

impl SessionContext {
    pub fn from_host(host: &HostCallback) -> Result<SessionContext, &'static str> {
        let context = SessionContext {
            containing_fx: get_containing_fx(host)?,
        };
        Ok(context)
    }

    pub fn containing_fx(&self) -> &Fx {
        &self.containing_fx
    }

    pub fn track(&self) -> Option<&Track> {
        self.containing_fx.track()
    }

    pub fn project(&self) -> Project {
        self.containing_fx
            .project()
            .unwrap_or(Reaper::get().current_project())
    }
}

/// Calling this in the `new()` method is too early. The containing FX can't generally be found
/// when we just open a REAPER project. We must wait for `init()` to be called.
fn get_containing_fx(host: &HostCallback) -> Result<Fx, &'static str> {
    let reaper = Reaper::get();
    let aeffect = NonNull::new(host.raw_effect()).expect("must not be null");
    let plugin_context = reaper.medium_reaper().plugin_context();
    let vst_context = match plugin_context.type_specific() {
        TypeSpecificPluginContext::Vst(ctx) => ctx,
        _ => unreachable!(),
    };
    let fx = if let Some(track) = unsafe { vst_context.request_containing_track(aeffect) } {
        let project = unsafe { vst_context.request_containing_project(aeffect) };
        let track = Track::new(track, Some(project));
        // We could use the following but it only works for REAPER v6.11+, so let's rely on our own
        // technique for now.
        // let location = unsafe { vst_context.request_containing_fx_location(aeffect) };
        find_realearn_fx_waiting_for_session(&track.normal_fx_chain())
            .or_else(|| find_realearn_fx_waiting_for_session(&track.input_fx_chain()))
            .ok_or("couldn't find containing FX on track FX chains")?
    } else if let Some(take) = unsafe { vst_context.request_containing_take(aeffect) } {
        return Err("ReaLearn as take FX is not supported yet");
    } else {
        find_realearn_fx_waiting_for_session(&reaper.monitoring_fx_chain())
            .ok_or("couldn't find containing FX on monitoring FX chain")?
    };
    Ok(fx)
}

fn is_realearn_waiting_for_session(fx: &Fx) -> bool {
    let result = fx.get_named_config_param(WAITING_FOR_SESSION_PARAM_NAME, 1);
    match result {
        Ok(buffer) => *buffer.first().expect("impossible") == 1,
        Err(_) => false,
    }
}

fn find_realearn_fx_waiting_for_session(fx_chain: &FxChain) -> Option<Fx> {
    // TODO-low Use REAPER 6.11 API addition instead, if available
    fx_chain.fxs().find(is_realearn_waiting_for_session)
}

use crate::domain::{ControlContext, PluginParams};
use anyhow::{bail, Context};
use derivative::Derivative;
use reaper_high::{Fx, FxChainContext, Project, Reaper, Track};
use reaper_low::{static_plugin_context, PluginContext};
use reaper_medium::{MainThreadScope, ParamId, TrackFxLocation, TypeSpecificPluginContext};
use std::ptr::NonNull;
use vst::host::Host;
use vst::plugin::HostCallback;

#[derive(Copy, Clone, Debug)]
pub struct ExtendedProcessorContext<'a> {
    pub context: &'a ProcessorContext,
    pub params: &'a PluginParams,
    pub control_context: ControlContext<'a>,
}

impl<'a> ExtendedProcessorContext<'a> {
    pub fn new(
        context: &'a ProcessorContext,
        params: &'a PluginParams,
        control_context: ControlContext<'a>,
    ) -> Self {
        Self {
            context,
            params,
            control_context,
        }
    }

    pub fn context(&self) -> &'a ProcessorContext {
        self.context
    }

    pub fn params(&self) -> &'a PluginParams {
        self.params
    }

    pub fn control_context(&self) -> ControlContext {
        self.control_context
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ProcessorContext {
    #[derivative(Debug = "ignore")]
    host: HostCallback,
    containing_fx: Fx,
    project: Option<Project>,
    bypass_param_index: u32,
}

pub const HELGOBOX_INSTANCE_ID_KEY: &str = "instance_id";

impl ProcessorContext {
    pub fn from_host(host: HostCallback) -> anyhow::Result<ProcessorContext> {
        let fx = get_containing_fx(&host)?;
        let project = fx.project();
        let bypass_param = fx
            .parameter_by_id(ParamId::Bypass)
            .context("bypass parameter not found")?;
        let context = ProcessorContext {
            host,
            containing_fx: fx,
            project,
            bypass_param_index: bypass_param.index(),
        };
        Ok(context)
    }

    pub fn containing_fx(&self) -> &Fx {
        &self.containing_fx
    }

    pub fn track(&self) -> Option<&Track> {
        self.containing_fx.track()
    }

    /// Returns the index of ReaLearn's "Bypass" parameter.
    pub fn bypass_param_index(&self) -> u32 {
        self.bypass_param_index
    }

    /// This falls back to the current project if on the monitoring FX chain.
    pub fn project_or_current_project(&self) -> Project {
        self.project
            .unwrap_or_else(|| Reaper::get().current_project())
    }

    pub fn project(&self) -> Option<Project> {
        self.project
    }

    pub fn is_on_monitoring_fx_chain(&self) -> bool {
        matches!(
            self.containing_fx.chain().context(),
            FxChainContext::Monitoring
        )
    }

    pub fn notify_dirty(&self) {
        self.host.automate(-1, 0.0);
    }
}

/// Calling this in the `new()` method is too early. The containing FX can't generally be found
/// when we just open a REAPER project. We must wait for `init()` to be called. No! Even longer.
/// We need to wait until the next main loop cycle.
fn get_containing_fx(host: &HostCallback) -> anyhow::Result<Fx> {
    let aeffect = NonNull::new(host.raw_effect()).context("aeffect must not be null")?;
    // We must not use the plug-in context from the global `Reaper` instance because this was
    // probably initialized by the extension entry point or another instance.
    let plugin_context = PluginContext::from_vst_plugin(host, static_plugin_context())
        .context("host callback not available")?;
    let plugin_context = reaper_medium::PluginContext::<'_, MainThreadScope>::new(&plugin_context);
    let vst_context = match plugin_context.type_specific() {
        TypeSpecificPluginContext::Vst(ctx) => ctx,
        _ => unreachable!(),
    };
    let fx_location = unsafe {
        // This would fail if we would call it too soon. That's why this function needs to be
        // called in the main loop cycle after loading the VST.
        vst_context
            .request_containing_fx_location(aeffect)
            .context("This version of ReaLearn needs REAPER >= v6.11.")?
    };
    let fx = if let Some(track) = unsafe { vst_context.request_containing_track(aeffect) } {
        let project = unsafe { vst_context.request_containing_project(aeffect) };
        let track = Track::new(track, Some(project));
        let (fx_chain, index) = match fx_location {
            TrackFxLocation::NormalFxChain(index) => (track.normal_fx_chain(), index),
            TrackFxLocation::InputFxChain(index) => (track.input_fx_chain(), index),
            TrackFxLocation::Unknown(_) => {
                bail!("Unknown ReaLearn FX location");
            }
        };
        fx_chain
            .fx_by_index(index)
            .context("couldn't find containing FX on track FX chains")?
    } else if let Some(_take) = unsafe { vst_context.request_containing_take(aeffect) } {
        bail!("Sorry, Helgobox doesn't support operation as item/take FX!");
    } else {
        let TrackFxLocation::InputFxChain(index) = fx_location else {
            bail!("Sorry, Helgobox doesn't support operation within an FX container!",);
        };
        Reaper::get().monitoring_fx_chain().fx_by_index(index)
            .context("Couldn't find containing FX on monitoring FX chain. It's okay if this occurs during plug-in scanning.")?
    };
    Ok(fx)
}

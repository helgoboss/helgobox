use crate::domain::{ActionInvocationType, ProcessorContext, ReaperTarget, TransportAction};
use derive_more::{Display, Error};
use reaper_high::{Action, Fx, FxChain, FxParameter, Guid, Project, Track, TrackSend};
use reaper_medium::{MasterTrackBehavior, TrackLocation};
use smallvec::alloc::fmt::Formatter;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum UnresolvedReaperTarget {
    Action {
        action: Action,
        invocation_type: ActionInvocationType,
    },
    FxParameter {
        fx_descriptor: FxDescriptor,
        fx_param_index: u32,
    },
    TrackVolume {
        track_descriptor: TrackDescriptor,
    },
    TrackSendVolume {
        track_descriptor: TrackDescriptor,
        send_index: u32,
    },
    TrackPan {
        track_descriptor: TrackDescriptor,
    },
    TrackArm {
        track_descriptor: TrackDescriptor,
    },
    TrackSelection {
        track_descriptor: TrackDescriptor,
        select_exclusively: bool,
    },
    TrackMute {
        track_descriptor: TrackDescriptor,
    },
    TrackSolo {
        track_descriptor: TrackDescriptor,
    },
    TrackSendPan {
        track_descriptor: TrackDescriptor,
        send_index: u32,
    },
    Tempo,
    Playrate,
    FxEnable {
        fx_descriptor: FxDescriptor,
    },
    FxPreset {
        fx_descriptor: FxDescriptor,
    },
    SelectedTrack,
    AllTrackFxEnable {
        track_descriptor: TrackDescriptor,
    },
    Transport {
        action: TransportAction,
    },
}

impl UnresolvedReaperTarget {
    pub fn resolve(&self, context: &ProcessorContext) -> Result<ReaperTarget, &'static str> {
        use UnresolvedReaperTarget::*;
        let resolved = match self {
            Action {
                action,
                invocation_type,
            } => ReaperTarget::Action {
                action: action.clone(),
                invocation_type: *invocation_type,
                project: context.project(),
            },
            FxParameter {
                fx_descriptor,
                fx_param_index,
            } => ReaperTarget::FxParameter {
                param: get_fx_param(context, fx_descriptor, *fx_param_index)?,
            },
            TrackVolume { track_descriptor } => ReaperTarget::TrackVolume {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackSendVolume {
                track_descriptor,
                send_index,
            } => ReaperTarget::TrackSendVolume {
                send: get_track_send(context, &track_descriptor.track, *send_index)?,
            },
            TrackPan { track_descriptor } => ReaperTarget::TrackPan {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackArm { track_descriptor } => ReaperTarget::TrackArm {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackSelection {
                track_descriptor,
                select_exclusively,
            } => ReaperTarget::TrackSelection {
                track: get_effective_track(context, &track_descriptor.track)?,
                select_exclusively: *select_exclusively,
            },
            TrackMute { track_descriptor } => ReaperTarget::TrackMute {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackSolo { track_descriptor } => ReaperTarget::TrackSolo {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackSendPan {
                track_descriptor,
                send_index,
            } => ReaperTarget::TrackSendVolume {
                send: get_track_send(context, &track_descriptor.track, *send_index)?,
            },
            Tempo => ReaperTarget::Tempo {
                project: context.project(),
            },
            Playrate => ReaperTarget::Playrate {
                project: context.project(),
            },
            FxEnable { fx_descriptor } => ReaperTarget::FxEnable {
                fx: get_fx(context, fx_descriptor)?,
            },
            FxPreset { fx_descriptor } => ReaperTarget::FxPreset {
                fx: get_fx(context, fx_descriptor)?,
            },
            SelectedTrack => ReaperTarget::SelectedTrack {
                project: context.project(),
            },
            AllTrackFxEnable { track_descriptor } => ReaperTarget::AllTrackFxEnable {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            Transport { action } => ReaperTarget::Transport {
                project: context.project(),
                action: *action,
            },
        };
        Ok(resolved)
    }

    /// Returns whether all conditions for this target to be active are met.
    ///
    /// Targets conditions are for example "track selected" or "FX focused".
    pub fn conditions_are_met(&self, target: &ReaperTarget) -> bool {
        let (track_descriptor, fx_descriptor) = self.descriptors();
        if let Some(desc) = track_descriptor {
            if desc.enable_only_if_track_selected {
                if let Some(track) = target.track() {
                    if !track.is_selected() {
                        return false;
                    }
                }
            }
        }
        if let Some(desc) = fx_descriptor {
            if desc.enable_only_if_fx_has_focus {
                if let Some(fx) = target.fx() {
                    if !fx.window_has_focus() {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn descriptors(&self) -> (Option<&TrackDescriptor>, Option<&FxDescriptor>) {
        use UnresolvedReaperTarget::*;
        match self {
            Action { .. } | Tempo | Playrate | SelectedTrack | Transport { .. } => (None, None),
            FxEnable { fx_descriptor }
            | FxPreset { fx_descriptor }
            | FxParameter { fx_descriptor, .. } => {
                (Some(&fx_descriptor.track_descriptor), Some(fx_descriptor))
            }
            TrackVolume { track_descriptor }
            | TrackSendVolume {
                track_descriptor, ..
            }
            | TrackPan { track_descriptor }
            | TrackArm { track_descriptor }
            | TrackSelection {
                track_descriptor, ..
            }
            | TrackMute { track_descriptor }
            | TrackSolo { track_descriptor }
            | TrackSendPan {
                track_descriptor, ..
            }
            | AllTrackFxEnable { track_descriptor } => (Some(track_descriptor), None),
        }
    }
}

pub fn get_effective_track(
    context: &ProcessorContext,
    virtual_track: &VirtualTrack,
) -> Result<Track, &'static str> {
    use VirtualTrack::*;
    let track = match virtual_track {
        This => context
            .containing_fx()
            .track()
            .cloned()
            // If this is monitoring FX, we want this to resolve to the master track since
            // in most functions, monitoring FX chain is the "input FX chain" of the master track.
            .unwrap_or_else(|| context.project().master_track()),
        Selected => context
            .project()
            .first_selected_track(MasterTrackBehavior::IncludeMasterTrack)
            .ok_or("no track selected")?,
        Master => context.project().master_track(),
        Particular(anchor) => anchor
            .resolve(context.project())
            .map_err(|_| "particular track couldn't be resolved")?,
    };
    Ok(track)
}

// Returns an error if that send (or track) doesn't exist.
pub fn get_track_send(
    context: &ProcessorContext,
    virtual_track: &VirtualTrack,
    send_index: u32,
) -> Result<TrackSend, &'static str> {
    let track = get_effective_track(context, virtual_track)?;
    let send = track.index_based_send_by_index(send_index);
    if !send.is_available() {
        return Err("send doesn't exist");
    }
    Ok(send)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackDescriptor {
    pub track: VirtualTrack,
    pub enable_only_if_track_selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxDescriptor {
    pub track_descriptor: TrackDescriptor,
    pub is_input_fx: bool,
    pub fx_index: u32,
    pub fx_guid: Option<Guid>,
    pub enable_only_if_fx_has_focus: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Display)]
pub enum VirtualTrack {
    /// Current track (the one which contains the ReaLearn instance).
    #[display(fmt = "<This>")]
    This,
    /// Currently selected track.
    #[display(fmt = "<Selected>")]
    Selected,
    /// Master track.
    #[display(fmt = "<Master>")]
    Master,
    /// A particular track.
    #[display(fmt = "<Particular>")]
    Particular(TrackAnchor),
}

impl VirtualTrack {
    pub fn with_context<'a>(
        &'a self,
        context: &'a ProcessorContext,
    ) -> VirtualTrackWithContext<'a> {
        VirtualTrackWithContext {
            virtual_track: self,
            context,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackAnchor {
    IdOrName(Guid, String),
    Id(Guid),
    Name(String),
    Index(u32),
}

impl fmt::Display for TrackAnchor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use TrackAnchor::*;
        match self {
            IdOrName(id, name) => write!(f, "{} or \"{}\"", id.to_string_without_braces(), name),
            Id(id) => write!(f, "{}", id.to_string_without_braces()),
            Name(name) => write!(f, "\"{}\"", name),
            Index(i) => write!(f, "{}", i + 1),
        }
    }
}

impl TrackAnchor {
    pub fn resolve(&self, project: Project) -> Result<Track, TrackResolveError> {
        use TrackAnchor::*;
        let track = match self {
            IdOrName(guid, name) => {
                let t = project.track_by_guid(guid);
                if t.is_available() {
                    t
                } else {
                    find_track_by_name(project, name).ok_or(TrackResolveError::TrackNotFound {
                        guid: Some(*guid),
                        name: Some(name.clone()),
                        index: None,
                    })?
                }
            }
            Id(guid) => {
                let t = project.track_by_guid(guid);
                if !t.is_available() {
                    return Err(TrackResolveError::TrackNotFound {
                        guid: Some(*guid),
                        name: None,
                        index: None,
                    });
                }
                t
            }
            Name(name) => {
                find_track_by_name(project, name).ok_or(TrackResolveError::TrackNotFound {
                    guid: None,
                    name: Some(name.clone()),
                    index: None,
                })?
            }
            Index(index) => {
                project
                    .track_by_index(*index)
                    .ok_or(TrackResolveError::TrackNotFound {
                        guid: None,
                        name: None,
                        index: Some(*index),
                    })?
            }
        };
        Ok(track)
    }
}

fn find_track_by_name(project: Project, name: &str) -> Option<Track> {
    project.tracks().find(|t| match t.name() {
        None => false,
        Some(n) => n.to_str() == name,
    })
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
pub enum TrackResolveError {
    #[display(fmt = "TrackNotFound")]
    TrackNotFound {
        guid: Option<Guid>,
        name: Option<String>,
        index: Option<u32>,
    },
}

pub struct VirtualTrackWithContext<'a> {
    virtual_track: &'a VirtualTrack,
    context: &'a ProcessorContext,
}

impl<'a> fmt::Display for VirtualTrackWithContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VirtualTrack::*;
        match self.virtual_track {
            This | Selected | Master => write!(f, "{}", self.virtual_track),
            Particular(anchor) => {
                // Here we don't want to log non-present tracks because this can be called pretty
                // often.
                if let Ok(t) = anchor.resolve(self.context.project()) {
                    write!(f, "{}", get_track_label(&t))
                } else {
                    write!(f, "<Not present>")
                }
            }
        }
    }
}

fn get_track_label(track: &Track) -> String {
    match track.location() {
        TrackLocation::MasterTrack => "<Master track>".into(),
        TrackLocation::NormalTrack(i) => {
            let position = i + 1;
            let name = track.name().expect("non-master track must have name");
            let name = name.to_str();
            if name.is_empty() {
                position.to_string()
            } else {
                format!("{}. {}", position, name)
            }
        }
    }
}

// Returns an error if that param (or FX) doesn't exist.
pub fn get_fx_param(
    context: &ProcessorContext,
    descriptor: &FxDescriptor,
    param_index: u32,
) -> Result<FxParameter, &'static str> {
    let fx = get_fx(context, descriptor)?;
    let param = fx.parameter_by_index(param_index);
    if !param.is_available() {
        return Err("parameter doesn't exist");
    }
    Ok(param)
}

// Returns an error if the FX doesn't exist.
pub fn get_fx(context: &ProcessorContext, descriptor: &FxDescriptor) -> Result<Fx, &'static str> {
    // Actually it's not that important whether we create an index-based or GUID-based FX.
    // The session listeners will recreate and resync the FX whenever something has
    // changed anyway. But for monitoring FX it could still be good (which we don't get notified
    // about unfortunately).
    if descriptor.track_descriptor.track == VirtualTrack::Selected {
        // When the target relates to the selected track, GUID-based FX doesn't make sense.
        get_index_based_fx(
            context,
            &descriptor.track_descriptor.track,
            descriptor.is_input_fx,
            descriptor.fx_index,
        )
    } else {
        let guid = descriptor.fx_guid.as_ref();
        match guid {
            None => get_index_based_fx(
                context,
                &descriptor.track_descriptor.track,
                descriptor.is_input_fx,
                descriptor.fx_index,
            ),
            Some(guid) => {
                // Track by GUID because target relates to a very particular FX
                get_guid_based_fx_by_guid_with_index_hint(
                    context,
                    &descriptor.track_descriptor.track,
                    descriptor.is_input_fx,
                    guid,
                    descriptor.fx_index,
                )
                // Fall back to index-based (otherwise this could have the unpleasant effect
                // that mapping panel FX menu doesn't find any FX anymore.
                .or_else(|_| {
                    get_index_based_fx(
                        context,
                        &descriptor.track_descriptor.track,
                        descriptor.is_input_fx,
                        descriptor.fx_index,
                    )
                })
            }
        }
    }
}

pub fn get_index_based_fx(
    context: &ProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    let fx = fx_chain.fx_by_index_untracked(fx_index);
    if !fx.is_available() {
        return Err("no FX at that index");
    }
    Ok(fx)
}

pub fn get_fx_chain(
    context: &ProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
) -> Result<FxChain, &'static str> {
    let track = get_effective_track(context, track)?;
    let result = if is_input_fx {
        track.input_fx_chain()
    } else {
        track.normal_fx_chain()
    };
    Ok(result)
}

fn get_guid_based_fx_by_guid_with_index_hint(
    context: &ProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    guid: &Guid,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    let fx = fx_chain.fx_by_guid_and_index(guid, fx_index);
    // is_available() also invalidates the index if necessary
    // TODO-low This is too implicit.
    if !fx.is_available() {
        return Err("no FX with that GUID");
    }
    Ok(fx)
}

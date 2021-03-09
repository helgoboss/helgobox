use crate::application::BookmarkAnchorType;
use crate::core::hash_util;
use crate::domain::{
    ActionInvocationType, DomainGlobal, ExtendedProcessorContext, ParameterArray, ReaperTarget,
    SoloBehavior, TouchedParameterType, TrackExclusivity, TransportAction, PLUGIN_PARAMETER_COUNT,
};
use derive_more::{Display, Error};
use fasteval::{Compiler, Evaler, Instruction, Slab};
use reaper_high::{
    Action, BookmarkType, FindBookmarkResult, Fx, FxChain, FxParameter, Guid, Project, Reaper,
    Track, TrackSend,
};
use reaper_medium::{BookmarkId, MasterTrackBehavior, TrackLocation};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::num::NonZeroU32;
use std::rc::Rc;

#[derive(Debug)]
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
    TrackWidth {
        track_descriptor: TrackDescriptor,
    },
    TrackArm {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
    },
    TrackSelection {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
    },
    TrackMute {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
    },
    TrackSolo {
        track_descriptor: TrackDescriptor,
        behavior: SoloBehavior,
        exclusivity: TrackExclusivity,
    },
    TrackSendPan {
        track_descriptor: TrackDescriptor,
        send_index: u32,
    },
    TrackSendMute {
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
        exclusivity: TrackExclusivity,
    },
    Transport {
        action: TransportAction,
    },
    LoadFxPreset {
        fx_descriptor: FxDescriptor,
        chunk: Rc<String>,
    },
    LastTouched,
    AutomationTouchState {
        track_descriptor: TrackDescriptor,
        parameter_type: TouchedParameterType,
        exclusivity: TrackExclusivity,
    },
    GoToBookmark {
        bookmark_type: BookmarkType,
        bookmark_anchor_type: BookmarkAnchorType,
        bookmark_ref: u32,
    },
}

impl UnresolvedReaperTarget {
    pub fn resolve(&self, context: ExtendedProcessorContext) -> Result<ReaperTarget, &'static str> {
        use UnresolvedReaperTarget::*;
        let resolved = match self {
            Action {
                action,
                invocation_type,
            } => ReaperTarget::Action {
                action: action.clone(),
                invocation_type: *invocation_type,
                project: context.context.project_or_current_project(),
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
            TrackWidth { track_descriptor } => ReaperTarget::TrackWidth {
                track: get_effective_track(context, &track_descriptor.track)?,
            },
            TrackArm {
                track_descriptor,
                exclusivity,
            } => ReaperTarget::TrackArm {
                track: get_effective_track(context, &track_descriptor.track)?,
                exclusivity: *exclusivity,
            },
            TrackSelection {
                track_descriptor,
                exclusivity,
            } => ReaperTarget::TrackSelection {
                track: get_effective_track(context, &track_descriptor.track)?,
                exclusivity: *exclusivity,
            },
            TrackMute {
                track_descriptor,
                exclusivity,
            } => ReaperTarget::TrackMute {
                track: get_effective_track(context, &track_descriptor.track)?,
                exclusivity: *exclusivity,
            },
            TrackSolo {
                track_descriptor,
                behavior,
                exclusivity,
            } => ReaperTarget::TrackSolo {
                track: get_effective_track(context, &track_descriptor.track)?,
                behavior: *behavior,
                exclusivity: *exclusivity,
            },
            TrackSendPan {
                track_descriptor,
                send_index,
            } => ReaperTarget::TrackSendPan {
                send: get_track_send(context, &track_descriptor.track, *send_index)?,
            },
            TrackSendMute {
                track_descriptor,
                send_index,
            } => ReaperTarget::TrackSendMute {
                send: get_track_send(context, &track_descriptor.track, *send_index)?,
            },
            Tempo => ReaperTarget::Tempo {
                project: context.context.project_or_current_project(),
            },
            Playrate => ReaperTarget::Playrate {
                project: context.context.project_or_current_project(),
            },
            FxEnable { fx_descriptor } => ReaperTarget::FxEnable {
                fx: get_fx(context, fx_descriptor)?,
            },
            FxPreset { fx_descriptor } => ReaperTarget::FxPreset {
                fx: get_fx(context, fx_descriptor)?,
            },
            SelectedTrack => ReaperTarget::SelectedTrack {
                project: context.context.project_or_current_project(),
            },
            AllTrackFxEnable {
                track_descriptor,
                exclusivity,
            } => ReaperTarget::AllTrackFxEnable {
                track: get_effective_track(context, &track_descriptor.track)?,
                exclusivity: *exclusivity,
            },
            Transport { action } => ReaperTarget::Transport {
                project: context.context.project_or_current_project(),
                action: *action,
            },
            LoadFxPreset {
                fx_descriptor,
                chunk,
            } => ReaperTarget::LoadFxSnapshot {
                fx: get_fx(context, fx_descriptor)?,
                chunk: chunk.clone(),
                chunk_hash: hash_util::calculate_non_crypto_hash(chunk),
            },
            LastTouched => DomainGlobal::get()
                .last_touched_target()
                .ok_or("no last touched target")?,
            AutomationTouchState {
                track_descriptor,
                parameter_type,
                exclusivity,
            } => ReaperTarget::AutomationTouchState {
                track: get_effective_track(context, &track_descriptor.track)?,
                parameter_type: *parameter_type,
                exclusivity: *exclusivity,
            },
            GoToBookmark {
                bookmark_type,
                bookmark_anchor_type,
                bookmark_ref,
            } => {
                let project = context.context.project_or_current_project();
                let res = find_bookmark(
                    project,
                    *bookmark_type,
                    *bookmark_anchor_type,
                    *bookmark_ref,
                )?;
                ReaperTarget::GoToBookmark {
                    project,
                    bookmark_type: *bookmark_type,
                    index: res.index,
                    position: NonZeroU32::new(res.index_within_type + 1).unwrap(),
                }
            }
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

    pub fn can_be_affected_by_parameters(&self) -> bool {
        let descriptors = self.descriptors();
        let track_is_dynamic = match descriptors.0 {
            None => false,
            Some(td) => matches!(&td.track, VirtualTrack::Dynamic(_)),
        };
        track_is_dynamic
    }

    fn descriptors(&self) -> (Option<&TrackDescriptor>, Option<&FxDescriptor>) {
        use UnresolvedReaperTarget::*;
        match self {
            Action { .. }
            | Tempo
            | Playrate
            | SelectedTrack
            | Transport { .. }
            | GoToBookmark { .. } => (None, None),
            FxEnable { fx_descriptor }
            | FxPreset { fx_descriptor }
            | FxParameter { fx_descriptor, .. }
            | LoadFxPreset { fx_descriptor, .. } => {
                (Some(&fx_descriptor.track_descriptor), Some(fx_descriptor))
            }
            TrackVolume { track_descriptor }
            | TrackSendVolume {
                track_descriptor, ..
            }
            | TrackPan { track_descriptor }
            | TrackWidth { track_descriptor }
            | TrackArm {
                track_descriptor, ..
            }
            | TrackSelection {
                track_descriptor, ..
            }
            | TrackMute {
                track_descriptor, ..
            }
            | TrackSolo {
                track_descriptor, ..
            }
            | TrackSendPan {
                track_descriptor, ..
            }
            | TrackSendMute {
                track_descriptor, ..
            }
            | AllTrackFxEnable {
                track_descriptor, ..
            }
            | AutomationTouchState {
                track_descriptor, ..
            } => (Some(track_descriptor), None),
            LastTouched => (None, None),
        }
    }
}

pub fn get_effective_track(
    context: ExtendedProcessorContext,
    virtual_track: &VirtualTrack,
) -> Result<Track, &'static str> {
    virtual_track
        .resolve(context)
        .map_err(|_| "track couldn't be resolved")
}

// Returns an error if that send (or track) doesn't exist.
pub fn get_track_send(
    context: ExtendedProcessorContext,
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

#[derive(Debug)]
pub struct TrackDescriptor {
    pub track: VirtualTrack,
    pub enable_only_if_track_selected: bool,
}

#[derive(Debug)]
pub struct FxDescriptor {
    pub track_descriptor: TrackDescriptor,
    pub fx: VirtualFx,
    pub enable_only_if_fx_has_focus: bool,
}

#[derive(Debug)]
pub enum VirtualTrack {
    /// Current track (the one which contains the ReaLearn instance).
    This,
    /// Currently selected track.
    Selected,
    /// Based on parameter values.
    Dynamic(ExpressionEvaluator),
    /// Master track.
    Master,
    /// Particular.
    ById(Guid),
    /// Particular.
    ByName(String),
    /// Particular.
    ByIndex(u32),
    /// This is the old default for targeting a particular track and it exists solely for backward
    /// compatibility.
    ByIdOrName(Guid, String),
}

#[derive(Debug)]
pub struct ExpressionEvaluator {
    slab: Slab,
    instruction: Instruction,
}

impl ExpressionEvaluator {
    pub fn compile(expression: &str) -> Result<ExpressionEvaluator, Box<dyn std::error::Error>> {
        let parser = fasteval::Parser::new();
        let mut slab = fasteval::Slab::new();
        let instruction = parser
            .parse(expression, &mut slab.ps)?
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);
        let evaluator = Self { slab, instruction };
        Ok(evaluator)
    }

    pub fn evaluate(&self, params: &ParameterArray) -> f64 {
        self.evaluate_internal(params).unwrap_or_default()
    }

    fn evaluate_internal(&self, params: &ParameterArray) -> Result<f64, fasteval::Error> {
        use fasteval::eval_compiled_ref;
        let mut cb = |name: &str, _args: Vec<f64>| -> Option<f64> {
            if !name.starts_with('p') {
                return None;
            }
            let value: u32 = name[1..].parse().ok()?;
            if !(1..=PLUGIN_PARAMETER_COUNT).contains(&value) {
                return None;
            }
            let index = (value - 1) as usize;
            let param_value = params[index];
            Some(param_value as f64)
        };
        let val = eval_compiled_ref!(&self.instruction, &self.slab, &mut cb);
        Ok(val)
    }
}

impl fmt::Display for VirtualTrack {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use VirtualTrack::*;
        match self {
            This => f.write_str("<This>"),
            Selected => f.write_str("<Selected>"),
            Master => f.write_str("<Master>"),
            Dynamic(_) => f.write_str("<Dynamic>"),
            ByIdOrName(id, name) => write!(f, "{} or \"{}\"", id.to_string_without_braces(), name),
            ById(id) => write!(f, "{}", id.to_string_without_braces()),
            ByName(name) => write!(f, "\"{}\"", name),
            ByIndex(i) => write!(f, "{}", i + 1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Display)]
pub enum VirtualFx {
    /// Focused or last focused FX.
    #[display(fmt = "<Focused>")]
    Focused,
    /// Particular FX.
    #[display(fmt = "<Particular>")]
    Particular { is_input_fx: bool, anchor: FxAnchor },
}

impl VirtualFx {
    pub fn refers_to_project(&self) -> bool {
        use VirtualFx::*;
        match self {
            Particular { anchor, .. } => {
                use FxAnchor::*;
                match anchor {
                    Id(_, _) | IdOrIndex(_, _) => true,
                    Name(_) | Index(_) => false,
                }
            }
            Focused => false,
        }
    }
}

impl VirtualTrack {
    pub fn resolve(&self, context: ExtendedProcessorContext) -> Result<Track, TrackResolveError> {
        use VirtualTrack::*;
        let project = context.context.project_or_current_project();
        let track = match self {
            This => context
                .context
                .containing_fx()
                .track()
                .cloned()
                // If this is monitoring FX, we want this to resolve to the master track since
                // in most functions, monitoring FX chain is the "input FX chain" of the master
                // track.
                .unwrap_or_else(|| project.master_track()),
            Selected => project
                .first_selected_track(MasterTrackBehavior::IncludeMasterTrack)
                .ok_or(TrackResolveError::NoTrackSelected)?,
            Dynamic(evaluator) => {
                let index = Self::evaluate_to_track_index(evaluator, context);
                project
                    .track_by_index(index)
                    .ok_or(TrackResolveError::TrackNotFound {
                        guid: None,
                        name: None,
                        index: Some(index),
                    })?
            }
            Master => project.master_track(),
            ByIdOrName(guid, name) => {
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
            ById(guid) => {
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
            ByName(name) => {
                find_track_by_name(project, name).ok_or(TrackResolveError::TrackNotFound {
                    guid: None,
                    name: Some(name.clone()),
                    index: None,
                })?
            }
            ByIndex(index) => {
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

    pub fn calculated_track_index(&self, context: ExtendedProcessorContext) -> Option<u32> {
        if let VirtualTrack::Dynamic(evaluator) = self {
            Some(Self::evaluate_to_track_index(evaluator, context))
        } else {
            None
        }
    }

    fn evaluate_to_track_index(
        evaluator: &ExpressionEvaluator,
        context: ExtendedProcessorContext,
    ) -> u32 {
        let result = evaluator.evaluate(context.params);
        result.max(0.0) as u32
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
    ) -> VirtualTrackWithContext<'a> {
        VirtualTrackWithContext {
            virtual_track: self,
            context,
        }
    }

    pub fn id(&self) -> Option<Guid> {
        use VirtualTrack::*;
        match self {
            ById(id) | ByIdOrName(id, _) => Some(*id),
            _ => None,
        }
    }

    pub fn index(&self) -> Option<u32> {
        use VirtualTrack::*;
        match self {
            ByIndex(i) => Some(*i),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&String> {
        use VirtualTrack::*;
        match self {
            ByName(name) | ByIdOrName(_, name) => Some(name),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxAnchor {
    /// This is the new default.
    ///
    /// The index is just used as performance hint, not as fallback.
    Id(Guid, Option<u32>),
    Name(String),
    Index(u32),
    /// This is the old default.
    ///
    /// The index comes into play as fallback whenever track is "<Selected>" or the GUID can't be
    /// determined (is `None`). I'm not sure how latter is possible but I keep it for backward
    /// compatibility.
    IdOrIndex(Option<Guid>, u32),
}

impl fmt::Display for FxAnchor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use FxAnchor::*;
        match self {
            Id(guid, _) => {
                write!(f, "{}", guid.to_string_without_braces())
            }
            Name(name) => write!(f, "\"{}\"", name),
            IdOrIndex(None, i) | Index(i) => write!(f, "{}", i + 1),
            IdOrIndex(Some(guid), i) => {
                write!(f, "{} ({})", guid.to_string_without_braces(), i + 1)
            }
        }
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
    NoTrackSelected,
}

impl FxAnchor {
    pub fn resolve(&self, fx_chain: &FxChain) -> Result<Fx, FxResolveError> {
        use FxAnchor::*;
        let fx = match self {
            Id(guid, index) => get_guid_based_fx_by_guid_on_chain_with_index_hint(
                fx_chain, guid, *index,
            )
            .map_err(|_| FxResolveError::FxNotFound {
                guid: Some(*guid),
                name: None,
                index: None,
            })?,
            Name(name) => {
                find_fx_by_name(fx_chain, name).ok_or_else(|| FxResolveError::FxNotFound {
                    guid: None,
                    name: Some(name.clone()),
                    index: None,
                })?
            }
            IdOrIndex(None, index) | Index(index) => get_index_based_fx_on_chain(fx_chain, *index)
                .map_err(|_| FxResolveError::FxNotFound {
                    guid: None,
                    name: None,
                    index: Some(*index),
                })?,
            IdOrIndex(Some(guid), index) => {
                // Track by GUID because target relates to a very particular FX
                get_guid_based_fx_by_guid_on_chain_with_index_hint(fx_chain, guid, Some(*index))
                    // Fall back to index-based
                    .or_else(|_| get_index_based_fx_on_chain(fx_chain, *index))
                    .map_err(|_| FxResolveError::FxNotFound {
                        guid: Some(*guid),
                        name: None,
                        index: Some(*index),
                    })?
            }
        };
        Ok(fx)
    }
}

fn find_fx_by_name(chain: &FxChain, name: &str) -> Option<Fx> {
    chain.fxs().find(|fx| fx.name().to_str() == name)
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
pub enum FxResolveError {
    #[display(fmt = "FxNotFound")]
    FxNotFound {
        guid: Option<Guid>,
        name: Option<String>,
        index: Option<u32>,
    },
}

pub struct VirtualTrackWithContext<'a> {
    virtual_track: &'a VirtualTrack,
    context: ExtendedProcessorContext<'a>,
}

impl<'a> fmt::Display for VirtualTrackWithContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VirtualTrack::*;
        match self.virtual_track {
            This | Selected | Master | Dynamic(_) => write!(f, "{}", self.virtual_track),
            _ => {
                if let Ok(t) = self.virtual_track.resolve(self.context) {
                    f.write_str(&get_track_label(&t))
                } else {
                    f.write_str(&get_non_present_virtual_track_label(&self.virtual_track))
                }
            }
        }
    }
}

pub fn get_non_present_virtual_track_label(track: &VirtualTrack) -> String {
    format!("<Not present> ({})", track)
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
    context: ExtendedProcessorContext,
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
pub fn get_fx(
    context: ExtendedProcessorContext,
    descriptor: &FxDescriptor,
) -> Result<Fx, &'static str> {
    match &descriptor.fx {
        VirtualFx::Particular {
            is_input_fx,
            anchor,
        } => {
            let actual_anchor = match anchor {
                FxAnchor::IdOrIndex(_, index) => {
                    // Actually it's not that important whether we create an index-based or
                    // GUID-based FX. The session listeners will recreate and
                    // resync the FX whenever something has changed anyway. But
                    // for monitoring FX it could still be good (which we don't get notified
                    // about unfortunately).
                    if matches!(descriptor.track_descriptor.track, VirtualTrack::Selected) {
                        FxAnchor::Index(*index)
                    } else {
                        anchor.clone()
                    }
                }
                _ => anchor.clone(),
            };
            let fx_chain = get_fx_chain(context, &descriptor.track_descriptor.track, *is_input_fx)?;
            actual_anchor
                .resolve(&fx_chain)
                .map_err(|_| "couldn't resolve particular FX")
        }
        VirtualFx::Focused => Reaper::get()
            .focused_fx()
            .ok_or("couldn't get (last) focused FX"),
    }
}

fn get_index_based_fx_on_chain(fx_chain: &FxChain, fx_index: u32) -> Result<Fx, &'static str> {
    let fx = fx_chain.fx_by_index_untracked(fx_index);
    if !fx.is_available() {
        return Err("no FX at that index");
    }
    Ok(fx)
}

pub fn get_fx_chain(
    context: ExtendedProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
) -> Result<FxChain, &'static str> {
    let track = get_effective_track(context, track)?;
    let result = if is_input_fx {
        if track.is_master_track() {
            // The combination "Master track + input FX chain" by convention represents the
            // monitoring FX chain in REAPER. It's a bit unfortunate that we have 2 representations
            // of the same thing: A special monitoring FX enum variant and this convention.
            // E.g. it leads to the result that both representations are not equal from a reaper-rs
            // perspective. We should enforce the enum variant whenever possible because the
            // convention is somehow flawed. E.g. what if we have 2 master tracks of different
            // projects? This should be done in reaper-high, there's already a to-do there.
            Reaper::get().monitoring_fx_chain()
        } else {
            track.input_fx_chain()
        }
    } else {
        track.normal_fx_chain()
    };
    Ok(result)
}

fn get_guid_based_fx_by_guid_on_chain_with_index_hint(
    fx_chain: &FxChain,
    guid: &Guid,
    fx_index: Option<u32>,
) -> Result<Fx, &'static str> {
    let fx = if let Some(i) = fx_index {
        fx_chain.fx_by_guid_and_index(guid, i)
    } else {
        fx_chain.fx_by_guid(guid)
    };
    // is_available() also invalidates the index if necessary
    // TODO-low This is too implicit.
    if !fx.is_available() {
        return Err("no FX with that GUID");
    }
    Ok(fx)
}

pub fn find_bookmark(
    project: Project,
    bookmark_type: BookmarkType,
    anchor_type: BookmarkAnchorType,
    bookmark_ref: u32,
) -> Result<FindBookmarkResult, &'static str> {
    if !project.is_available() {
        return Err("project not available");
    }
    match anchor_type {
        BookmarkAnchorType::Index => project
            .find_bookmark_by_type_and_index(bookmark_type, bookmark_ref)
            .ok_or("bookmark with that type and index not found"),
        BookmarkAnchorType::Id => project
            .find_bookmark_by_type_and_id(bookmark_type, BookmarkId::new(bookmark_ref))
            .ok_or("bookmark with that type and ID not found"),
    }
}

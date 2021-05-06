use crate::application::BookmarkAnchorType;
use crate::core::hash_util;
use crate::domain::{
    ActionInvocationType, ActionTarget, AllTrackFxEnableTarget, AutomationModeOverrideTarget,
    BackboneState, ExtendedProcessorContext, FxDisplayType, FxEnableTarget, FxNavigateTarget,
    FxOpenTarget, FxParameterTarget, FxPresetTarget, LoadFxSnapshotTarget, MappingCompartment,
    MidiSendTarget, OscDeviceId, ParameterSlice, PlayPosFeedbackResolution, PlayrateTarget,
    RealearnTarget, ReaperTarget, RouteMuteTarget, RoutePanTarget, RouteVolumeTarget, SeekOptions,
    SelectedTrackTarget, SendMidiDestination, SlotPlayOptions, SoloBehavior, TempoTarget,
    TouchedParameterType, TrackArmTarget, TrackAutomationModeTarget, TrackExclusivity,
    TrackMuteTarget, TrackPanTarget, TrackPeakTarget, TrackSelectionTarget, TrackShowTarget,
    TrackSoloTarget, TrackVolumeTarget, TrackWidthTarget, TransportAction, TransportTarget,
    COMPARTMENT_PARAMETER_COUNT,
};
use derive_more::{Display, Error};
use enum_iterator::IntoEnumIterator;
use fasteval::{Compiler, Evaler, Instruction, Slab};
use helgoboss_learn::{OscArgDescriptor, RawMidiPattern};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, BookmarkType, FindBookmarkResult, Fx, FxChain, FxParameter, Guid, Project, Reaper,
    SendPartnerType, Track, TrackRoute,
};
use reaper_medium::{
    AutomationMode, BookmarkId, GlobalAutomationModeOverride, MasterTrackBehavior, TrackArea,
    TrackLocation,
};
use serde::{Deserialize, Serialize};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::num::NonZeroU32;
use std::rc::Rc;
use wildmatch::WildMatch;

/// Maximum number of "allow multiple" resolves (e.g. affected <Selected> tracks).
const MAX_MULTIPLE: usize = 1000;

#[derive(Debug)]
pub enum UnresolvedReaperTarget {
    Action {
        action: Action,
        invocation_type: ActionInvocationType,
    },
    FxParameter {
        fx_parameter_descriptor: FxParameterDescriptor,
    },
    TrackVolume {
        track_descriptor: TrackDescriptor,
    },
    TrackPeak {
        track_descriptor: TrackDescriptor,
    },
    TrackSendVolume {
        descriptor: TrackRouteDescriptor,
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
        scroll_arrange_view: bool,
        scroll_mixer: bool,
    },
    TrackMute {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
    },
    TrackShow {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
        area: TrackArea,
    },
    TrackSolo {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
        behavior: SoloBehavior,
    },
    TrackAutomationMode {
        track_descriptor: TrackDescriptor,
        exclusivity: TrackExclusivity,
        mode: AutomationMode,
    },
    TrackSendPan {
        descriptor: TrackRouteDescriptor,
    },
    TrackSendMute {
        descriptor: TrackRouteDescriptor,
    },
    Tempo,
    Playrate,
    AutomationModeOverride {
        mode_override: Option<GlobalAutomationModeOverride>,
    },
    FxEnable {
        fx_descriptor: FxDescriptor,
    },
    FxOpen {
        fx_descriptor: FxDescriptor,
        display_type: FxDisplayType,
    },
    FxPreset {
        fx_descriptor: FxDescriptor,
    },
    SelectedTrack {
        scroll_arrange_view: bool,
        scroll_mixer: bool,
    },
    FxNavigate {
        track_descriptor: TrackDescriptor,
        is_input_fx: bool,
        display_type: FxDisplayType,
    },
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
        set_time_selection: bool,
        set_loop_points: bool,
    },
    Seek {
        options: SeekOptions,
    },
    SendMidi {
        pattern: RawMidiPattern,
        destination: SendMidiDestination,
    },
    SendOsc {
        address_pattern: String,
        arg_descriptor: Option<OscArgDescriptor>,
        device_id: Option<OscDeviceId>,
    },
    ClipTransport {
        track_descriptor: Option<TrackDescriptor>,
        slot_index: usize,
        action: TransportAction,
        play_options: SlotPlayOptions,
    },
    ClipSeek {
        slot_index: usize,
        feedback_resolution: PlayPosFeedbackResolution,
    },
    ClipVolume {
        slot_index: usize,
    },
}

impl UnresolvedReaperTarget {
    pub fn is_always_active(&self) -> bool {
        matches!(self, Self::LastTouched)
    }

    pub fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        use UnresolvedReaperTarget::*;
        let resolved_targets = match self {
            Action {
                action,
                invocation_type,
            } => vec![ReaperTarget::Action(ActionTarget {
                action: action.clone(),
                invocation_type: *invocation_type,
                project: context.context().project_or_current_project(),
            })],
            FxParameter {
                fx_parameter_descriptor,
            } => vec![ReaperTarget::FxParameter(FxParameterTarget {
                param: get_fx_param(context, fx_parameter_descriptor, compartment)?,
            })],
            TrackVolume { track_descriptor } => {
                get_effective_tracks(context, &track_descriptor.track, compartment)?
                    .into_iter()
                    .map(|track| ReaperTarget::TrackVolume(TrackVolumeTarget { track }))
                    .collect()
            }
            TrackPeak { track_descriptor } => {
                get_effective_tracks(context, &track_descriptor.track, compartment)?
                    .into_iter()
                    .map(|track| ReaperTarget::TrackPeak(TrackPeakTarget { track }))
                    .collect()
            }
            TrackSendVolume { descriptor } => {
                vec![ReaperTarget::TrackRouteVolume(RouteVolumeTarget {
                    route: get_track_route(context, descriptor, compartment)?,
                })]
            }
            TrackPan { track_descriptor } => {
                get_effective_tracks(context, &track_descriptor.track, compartment)?
                    .into_iter()
                    .map(|track| ReaperTarget::TrackPan(TrackPanTarget { track }))
                    .collect()
            }
            TrackWidth { track_descriptor } => {
                get_effective_tracks(context, &track_descriptor.track, compartment)?
                    .into_iter()
                    .map(|track| ReaperTarget::TrackWidth(TrackWidthTarget { track }))
                    .collect()
            }
            TrackArm {
                track_descriptor,
                exclusivity,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackArm(TrackArmTarget {
                        track,
                        exclusivity: *exclusivity,
                    })
                })
                .collect(),
            TrackSelection {
                track_descriptor,
                exclusivity,
                scroll_arrange_view,
                scroll_mixer,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackSelection(TrackSelectionTarget {
                        track,
                        exclusivity: *exclusivity,
                        scroll_arrange_view: *scroll_arrange_view,
                        scroll_mixer: *scroll_mixer,
                    })
                })
                .collect(),
            TrackMute {
                track_descriptor,
                exclusivity,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackMute(TrackMuteTarget {
                        track,
                        exclusivity: *exclusivity,
                    })
                })
                .collect(),
            TrackShow {
                track_descriptor,
                exclusivity,
                area,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackShow(TrackShowTarget {
                        track,
                        exclusivity: *exclusivity,
                        area: *area,
                    })
                })
                .collect(),
            TrackSolo {
                track_descriptor,
                exclusivity,
                behavior,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackSolo(TrackSoloTarget {
                        track,
                        exclusivity: *exclusivity,
                        behavior: *behavior,
                    })
                })
                .collect(),
            TrackAutomationMode {
                track_descriptor,
                exclusivity,
                mode,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackAutomationMode(TrackAutomationModeTarget {
                        track,
                        exclusivity: *exclusivity,
                        mode: *mode,
                    })
                })
                .collect(),
            TrackSendPan { descriptor } => vec![ReaperTarget::TrackRoutePan(RoutePanTarget {
                route: get_track_route(context, descriptor, compartment)?,
            })],
            TrackSendMute { descriptor } => vec![ReaperTarget::TrackRouteMute(RouteMuteTarget {
                route: get_track_route(context, descriptor, compartment)?,
            })],
            Tempo => vec![ReaperTarget::Tempo(TempoTarget {
                project: context.context().project_or_current_project(),
            })],
            Playrate => vec![ReaperTarget::Playrate(PlayrateTarget {
                project: context.context().project_or_current_project(),
            })],
            AutomationModeOverride { mode_override } => {
                vec![ReaperTarget::AutomationModeOverride(
                    AutomationModeOverrideTarget {
                        mode_override: *mode_override,
                    },
                )]
            }
            FxEnable { fx_descriptor } => vec![ReaperTarget::FxEnable(FxEnableTarget {
                fx: get_fx(context, fx_descriptor, compartment)?,
            })],
            FxOpen {
                fx_descriptor,
                display_type,
            } => vec![ReaperTarget::FxOpen(FxOpenTarget {
                fx: get_fx(context, fx_descriptor, compartment)?,
                display_type: *display_type,
            })],
            FxPreset { fx_descriptor } => vec![ReaperTarget::FxPreset(FxPresetTarget {
                fx: get_fx(context, fx_descriptor, compartment)?,
            })],
            SelectedTrack {
                scroll_arrange_view,
                scroll_mixer,
            } => vec![ReaperTarget::SelectedTrack(SelectedTrackTarget {
                project: context.context().project_or_current_project(),
                scroll_arrange_view: *scroll_arrange_view,
                scroll_mixer: *scroll_mixer,
            })],
            FxNavigate {
                track_descriptor,
                is_input_fx,
                display_type,
            } => vec![ReaperTarget::FxNavigate(FxNavigateTarget {
                fx_chain: get_fx_chain(
                    context,
                    &track_descriptor.track,
                    *is_input_fx,
                    compartment,
                )?,
                display_type: *display_type,
            })],
            AllTrackFxEnable {
                track_descriptor,
                exclusivity,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::AllTrackFxEnable(AllTrackFxEnableTarget {
                        track,
                        exclusivity: *exclusivity,
                    })
                })
                .collect(),
            Transport { action } => vec![ReaperTarget::Transport(TransportTarget {
                project: context.context().project_or_current_project(),
                action: *action,
            })],
            LoadFxPreset {
                fx_descriptor,
                chunk,
            } => vec![ReaperTarget::LoadFxSnapshot(LoadFxSnapshotTarget {
                fx: get_fx(context, fx_descriptor, compartment)?,
                chunk: chunk.clone(),
                chunk_hash: hash_util::calculate_non_crypto_hash(chunk),
            })],
            LastTouched => {
                let last_touched_target = BackboneState::get()
                    .last_touched_target()
                    .ok_or("no last touched target")?;
                if !last_touched_target.is_available() {
                    return Err("last touched target gone");
                }
                vec![last_touched_target]
            }
            AutomationTouchState {
                track_descriptor,
                parameter_type,
                exclusivity,
            } => get_effective_tracks(context, &track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| ReaperTarget::AutomationTouchState {
                    track,
                    parameter_type: *parameter_type,
                    exclusivity: *exclusivity,
                })
                .collect(),
            GoToBookmark {
                bookmark_type,
                bookmark_anchor_type,
                bookmark_ref,
                set_time_selection,
                set_loop_points,
            } => {
                let project = context.context().project_or_current_project();
                let res = find_bookmark(
                    project,
                    *bookmark_type,
                    *bookmark_anchor_type,
                    *bookmark_ref,
                )?;
                vec![ReaperTarget::GoToBookmark {
                    project,
                    bookmark_type: *bookmark_type,
                    index: res.index,
                    position: NonZeroU32::new(res.index_within_type + 1).unwrap(),
                    set_time_selection: *set_time_selection,
                    set_loop_points: *set_loop_points,
                }]
            }
            Seek { options } => {
                let project = context.context().project_or_current_project();
                vec![ReaperTarget::Seek {
                    project,
                    options: *options,
                }]
            }
            SendMidi {
                pattern,
                destination,
            } => vec![ReaperTarget::SendMidi(MidiSendTarget {
                pattern: pattern.clone(),
                destination: *destination,
            })],
            SendOsc {
                address_pattern,
                arg_descriptor,
                device_id,
            } => vec![ReaperTarget::SendOsc {
                address_pattern: address_pattern.clone(),
                arg_descriptor: *arg_descriptor,
                device_id: *device_id,
            }],
            ClipTransport {
                track_descriptor,
                slot_index,
                action,
                play_options,
            } => {
                if let Some(desc) = track_descriptor.as_ref() {
                    get_effective_tracks(context, &desc.track, compartment)?
                        .into_iter()
                        .map(|track| ReaperTarget::ClipTransport {
                            track: Some(track),
                            slot_index: *slot_index,
                            action: *action,
                            play_options: *play_options,
                        })
                        .collect()
                } else {
                    vec![ReaperTarget::ClipTransport {
                        track: None,
                        slot_index: *slot_index,
                        action: *action,
                        play_options: *play_options,
                    }]
                }
            }
            ClipSeek {
                slot_index,
                feedback_resolution,
            } => vec![ReaperTarget::ClipSeek {
                slot_index: *slot_index,
                feedback_resolution: *feedback_resolution,
            }],
            ClipVolume { slot_index } => vec![ReaperTarget::ClipVolume {
                slot_index: *slot_index,
            }],
        };
        Ok(resolved_targets)
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
        match descriptors.0 {
            None => false,
            Some(td) => matches!(&td.track, VirtualTrack::Dynamic(_)),
        }
    }

    fn descriptors(&self) -> (Option<&TrackDescriptor>, Option<&FxDescriptor>) {
        use UnresolvedReaperTarget::*;
        match self {
            Action { .. }
            | Tempo
            | Playrate
            | SelectedTrack { .. }
            | Transport { .. }
            | LastTouched
            | Seek { .. }
            | ClipSeek { .. }
            | ClipVolume { .. }
            | AutomationModeOverride { .. }
            | SendMidi { .. }
            | SendOsc { .. }
            | GoToBookmark { .. } => (None, None),
            FxOpen { fx_descriptor, .. }
            | FxEnable { fx_descriptor }
            | FxPreset { fx_descriptor }
            | LoadFxPreset { fx_descriptor, .. } => {
                (Some(&fx_descriptor.track_descriptor), Some(fx_descriptor))
            }
            FxParameter {
                fx_parameter_descriptor,
            } => (
                Some(&fx_parameter_descriptor.fx_descriptor.track_descriptor),
                Some(&fx_parameter_descriptor.fx_descriptor),
            ),
            TrackVolume { track_descriptor }
            | TrackPeak { track_descriptor }
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
            | TrackShow {
                track_descriptor, ..
            }
            | TrackAutomationMode {
                track_descriptor, ..
            }
            | TrackSolo {
                track_descriptor, ..
            }
            | FxNavigate {
                track_descriptor, ..
            }
            | AllTrackFxEnable {
                track_descriptor, ..
            }
            | AutomationTouchState {
                track_descriptor, ..
            } => (Some(track_descriptor), None),
            TrackSendVolume { descriptor }
            | TrackSendPan { descriptor }
            | TrackSendMute { descriptor } => (Some(&descriptor.track_descriptor), None),
            ClipTransport {
                track_descriptor, ..
            } => (track_descriptor.as_ref(), None),
        }
    }

    pub fn play_pos_feedback_resolution(&self) -> Option<PlayPosFeedbackResolution> {
        use UnresolvedReaperTarget::*;
        let res = match self {
            Action { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackSendVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | FxOpen { .. }
            | AutomationModeOverride { .. }
            | FxNavigate { .. }
            | TrackSolo { .. }
            | TrackSendPan { .. }
            | TrackSendMute { .. }
            | Tempo
            | Playrate
            | FxEnable { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. }
            | LoadFxPreset { .. }
            | LastTouched
            | SendMidi { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | ClipVolume { .. }
            | AutomationTouchState { .. } => return None,
            Transport { .. } | GoToBookmark { .. } | ClipSeek { .. } => {
                PlayPosFeedbackResolution::Beat
            }
            Seek { options, .. } => options.feedback_resolution,
            TrackPeak { .. } => PlayPosFeedbackResolution::High,
        };
        Some(res)
    }
}

pub fn get_effective_tracks(
    context: ExtendedProcessorContext,
    virtual_track: &VirtualTrack,
    compartment: MappingCompartment,
) -> Result<Vec<Track>, &'static str> {
    virtual_track
        .resolve(context, compartment)
        .map_err(|_| "track couldn't be resolved")
}

// Returns an error if that send (or track) doesn't exist.
pub fn get_track_route(
    context: ExtendedProcessorContext,
    descriptor: &TrackRouteDescriptor,
    compartment: MappingCompartment,
) -> Result<TrackRoute, &'static str> {
    let track = get_effective_tracks(context, &descriptor.track_descriptor.track, compartment)?
        // TODO-medium Support multiple tracks
        .into_iter()
        .next()
        .ok_or("no track resolved")?;
    descriptor
        .route
        .resolve(&track, context, compartment)
        .map_err(|_| "route doesn't exist")
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
pub struct FxParameterDescriptor {
    pub fx_descriptor: FxDescriptor,
    pub fx_parameter: VirtualFxParameter,
}

#[derive(Debug)]
pub struct TrackRouteDescriptor {
    pub track_descriptor: TrackDescriptor,
    pub route: VirtualTrackRoute,
}

#[derive(Debug)]
pub struct VirtualTrackRoute {
    pub r#type: TrackRouteType,
    pub selector: TrackRouteSelector,
}

#[derive(Debug)]
pub enum TrackRouteSelector {
    Dynamic(Box<ExpressionEvaluator>),
    ById(Guid),
    ByName(WildMatch),
    ByIndex(u32),
}

impl TrackRouteSelector {
    pub fn resolve(
        &self,
        track: &Track,
        route_type: TrackRouteType,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<TrackRoute, TrackRouteResolveError> {
        use TrackRouteSelector::*;
        let route = match self {
            Dynamic(evaluator) => {
                let i = Self::evaluate_to_route_index(evaluator, context, compartment);
                resolve_track_route_by_index(track, route_type, i)?
            }
            ById(guid) => {
                let related_track = track.project().track_by_guid(guid);
                let route = find_route_by_related_track(track, &related_track, route_type)?;
                route.ok_or_else(|| TrackRouteResolveError::TrackRouteNotFound {
                    guid: Some(*guid),
                    name: None,
                    index: None,
                })?
            }
            ByName(name) => find_route_by_name(track, name, route_type).ok_or_else(|| {
                TrackRouteResolveError::TrackRouteNotFound {
                    guid: None,
                    name: Some(name.clone()),
                    index: None,
                }
            })?,
            ByIndex(i) => resolve_track_route_by_index(track, route_type, *i)?,
        };
        Ok(route)
    }

    pub fn calculated_route_index(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Option<u32> {
        if let TrackRouteSelector::Dynamic(evaluator) = self {
            Some(Self::evaluate_to_route_index(
                evaluator,
                context,
                compartment,
            ))
        } else {
            None
        }
    }

    fn evaluate_to_route_index(
        evaluator: &ExpressionEvaluator,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> u32 {
        let sliced_params = compartment.slice_params(context.params());
        let result = evaluator.evaluate(sliced_params);
        result.round().max(0.0) as u32
    }

    pub fn id(&self) -> Option<Guid> {
        use TrackRouteSelector::*;
        match self {
            ById(id) => Some(*id),
            _ => None,
        }
    }

    pub fn index(&self) -> Option<u32> {
        use TrackRouteSelector::*;
        match self {
            ByIndex(i) => Some(*i),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        use TrackRouteSelector::*;
        match self {
            ByName(name) => Some(name.to_string()),
            _ => None,
        }
    }
}

impl fmt::Display for VirtualTrackRoute {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use TrackRouteSelector::*;
        match &self.selector {
            Dynamic(_) => f.write_str("<Dynamic>"),
            ById(id) => write!(f, "{}", id.to_string_without_braces()),
            ByName(name) => write!(f, "\"{}\"", name),
            ByIndex(i) => write!(f, "{}", i + 1),
        }
    }
}

impl VirtualTrackRoute {
    pub fn resolve(
        &self,
        track: &Track,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<TrackRoute, TrackRouteResolveError> {
        self.selector
            .resolve(track, self.r#type, context, compartment)
    }

    pub fn id(&self) -> Option<Guid> {
        self.selector.id()
    }

    pub fn index(&self) -> Option<u32> {
        self.selector.index()
    }

    pub fn name(&self) -> Option<String> {
        self.selector.name()
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TrackRouteType {
    #[serde(rename = "send")]
    #[display(fmt = "Send")]
    Send,
    #[serde(rename = "receive")]
    #[display(fmt = "Receive")]
    Receive,
    #[serde(rename = "output")]
    #[display(fmt = "Output")]
    HardwareOutput,
}

impl Default for TrackRouteType {
    fn default() -> Self {
        Self::Send
    }
}

#[derive(Debug)]
pub enum VirtualTrack {
    /// Current track (the one which contains the ReaLearn instance).
    This,
    /// Currently selected track.
    Selected { allow_multiple: bool },
    /// Position in project based on parameter values.
    Dynamic(Box<ExpressionEvaluator>),
    /// Master track.
    Master,
    /// Particular.
    ById(Guid),
    /// Particular.
    ByName(WildMatch),
    /// Particular.
    ByIndex(u32),
    /// This is the old default for targeting a particular track and it exists solely for backward
    /// compatibility.
    ByIdOrName(Guid, WildMatch),
}

#[derive(Debug)]
pub enum VirtualFxParameter {
    Dynamic(Box<ExpressionEvaluator>),
    ByName(WildMatch),
    ByIndex(u32),
}

impl VirtualFxParameter {
    pub fn resolve(
        &self,
        fx: &Fx,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<FxParameter, FxParameterResolveError> {
        use VirtualFxParameter::*;
        match self {
            Dynamic(evaluator) => {
                let i = Self::evaluate_to_fx_parameter_index(evaluator, context, compartment);
                resolve_parameter_by_index(fx, i)
            }
            ByName(name) => fx
                .parameters()
                .find(|p| name.matches(p.name().to_str()))
                .ok_or_else(|| FxParameterResolveError::FxParameterNotFound {
                    name: Some(name.clone()),
                    index: None,
                }),
            ByIndex(i) => resolve_parameter_by_index(fx, *i),
        }
    }

    pub fn calculated_fx_parameter_index(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Option<u32> {
        if let VirtualFxParameter::Dynamic(evaluator) = self {
            Some(Self::evaluate_to_fx_parameter_index(
                evaluator,
                context,
                compartment,
            ))
        } else {
            None
        }
    }

    fn evaluate_to_fx_parameter_index(
        evaluator: &ExpressionEvaluator,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> u32 {
        let sliced_params = compartment.slice_params(context.params());
        let result = evaluator.evaluate(sliced_params);
        result.round().max(0.0) as u32
    }

    pub fn index(&self) -> Option<u32> {
        use VirtualFxParameter::*;
        match self {
            ByIndex(i) => Some(*i),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        use VirtualFxParameter::*;
        match self {
            ByName(name) => Some(name.to_string()),
            _ => None,
        }
    }
}

impl fmt::Display for VirtualFxParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use VirtualFxParameter::*;
        match self {
            Dynamic(_) => f.write_str("<Dynamic>"),
            ByName(name) => write!(f, "\"{}\"", name),
            ByIndex(i) => write!(f, "{}", i + 1),
        }
    }
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

    pub fn evaluate(&self, params: &ParameterSlice) -> f64 {
        self.evaluate_internal(params, |_| None).unwrap_or_default()
    }

    pub fn evaluate_with_additional_vars(
        &self,
        params: &ParameterSlice,
        additional_vars: impl Fn(&str) -> Option<f64>,
    ) -> f64 {
        self.evaluate_internal(params, additional_vars)
            .unwrap_or_default()
    }

    fn evaluate_internal(
        &self,
        params: &ParameterSlice,
        additional_vars: impl Fn(&str) -> Option<f64>,
    ) -> Result<f64, fasteval::Error> {
        use fasteval::eval_compiled_ref;
        let mut cb = |name: &str, _args: Vec<f64>| -> Option<f64> {
            if let Some(value) = additional_vars(name) {
                return Some(value);
            }
            if !name.starts_with('p') {
                return None;
            }
            let value: u32 = name[1..].parse().ok()?;
            if !(1..=COMPARTMENT_PARAMETER_COUNT).contains(&value) {
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
            Selected { allow_multiple } => f.write_str(if *allow_multiple {
                "<All selected>"
            } else {
                "<Selected>"
            }),
            Master => f.write_str("<Master>"),
            Dynamic(_) => f.write_str("<Dynamic>"),
            ByIdOrName(id, name) => write!(f, "{} or \"{}\"", id.to_string_without_braces(), name),
            ById(id) => write!(f, "{}", id.to_string_without_braces()),
            ByName(name) => write!(f, "\"{}\"", name),
            ByIndex(i) => write!(f, "{}", i + 1),
        }
    }
}

#[derive(Debug)]
pub enum VirtualFx {
    /// This ReaLearn FX (nice for controlling conditional activation parameters).
    This,
    /// Focused or last focused FX.
    Focused,
    /// Particular FX.
    ChainFx {
        is_input_fx: bool,
        chain_fx: VirtualChainFx,
    },
}

impl VirtualFx {
    pub fn id(&self) -> Option<Guid> {
        match self {
            VirtualFx::This => None,
            VirtualFx::Focused => None,
            VirtualFx::ChainFx { chain_fx, .. } => chain_fx.id(),
        }
    }

    pub fn is_input_fx(&self) -> bool {
        match self {
            // In case of <This>, it doesn't matter.
            VirtualFx::This => false,
            VirtualFx::Focused => false,
            VirtualFx::ChainFx { is_input_fx, .. } => *is_input_fx,
        }
    }

    pub fn index(&self) -> Option<u32> {
        match self {
            VirtualFx::This => None,
            VirtualFx::Focused => None,
            VirtualFx::ChainFx { chain_fx, .. } => chain_fx.index(),
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            VirtualFx::This => None,
            VirtualFx::Focused => None,
            VirtualFx::ChainFx { chain_fx, .. } => chain_fx.name(),
        }
    }
}

impl VirtualTrack {
    pub fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<Track>, TrackResolveError> {
        use VirtualTrack::*;
        let project = context.context().project_or_current_project();
        let tracks = match self {
            This => {
                let single = context
                    .context()
                    .containing_fx()
                    .track()
                    .cloned()
                    // If this is monitoring FX, we want this to resolve to the master track since
                    // in most functions, monitoring FX chain is the "input FX chain" of the master
                    // track.
                    .unwrap_or_else(|| project.master_track());
                vec![single]
            }
            Selected { allow_multiple } => project
                .selected_tracks(MasterTrackBehavior::IncludeMasterTrack)
                .take(if *allow_multiple { MAX_MULTIPLE } else { 1 })
                .collect(),
            Dynamic(evaluator) => {
                let index = Self::evaluate_to_track_index(evaluator, context, compartment);
                let single = resolve_track_by_index(project, index)?;
                vec![single]
            }
            Master => vec![project.master_track()],
            ByIdOrName(guid, name) => {
                let t = project.track_by_guid(guid);
                let single = if t.is_available() {
                    t
                } else {
                    find_track_by_name(project, name).ok_or(TrackResolveError::TrackNotFound {
                        guid: Some(*guid),
                        name: Some(name.clone()),
                        index: None,
                    })?
                };
                vec![single]
            }
            ById(guid) => {
                let single = project.track_by_guid(guid);
                if !single.is_available() {
                    return Err(TrackResolveError::TrackNotFound {
                        guid: Some(*guid),
                        name: None,
                        index: None,
                    });
                }
                vec![single]
            }
            ByName(name) => {
                let single =
                    find_track_by_name(project, name).ok_or(TrackResolveError::TrackNotFound {
                        guid: None,
                        name: Some(name.clone()),
                        index: None,
                    })?;
                vec![single]
            }
            ByIndex(index) => {
                let single = resolve_track_by_index(project, *index)?;
                vec![single]
            }
        };
        Ok(tracks)
    }

    pub fn calculated_track_index(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Option<u32> {
        if let VirtualTrack::Dynamic(evaluator) = self {
            Some(Self::evaluate_to_track_index(
                evaluator,
                context,
                compartment,
            ))
        } else {
            None
        }
    }

    fn evaluate_to_track_index(
        evaluator: &ExpressionEvaluator,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> u32 {
        let sliced_params = compartment.slice_params(context.params());
        let result = evaluator.evaluate_with_additional_vars(sliced_params, |name| match name {
            "this_track_index" => {
                let index = context.context().track()?.index()?;
                Some(index as f64)
            }
            _ => None,
        });
        result.round().max(0.0) as u32
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
        compartment: MappingCompartment,
    ) -> VirtualTrackWithContext<'a> {
        VirtualTrackWithContext {
            virtual_track: self,
            context,
            compartment,
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

    pub fn name(&self) -> Option<String> {
        use VirtualTrack::*;
        match self {
            ByName(name) | ByIdOrName(_, name) => Some(name.to_string()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum VirtualChainFx {
    /// Position in FX chain based on parameter values.
    Dynamic(Box<ExpressionEvaluator>),
    /// This is the new default.
    ///
    /// The index is just used as performance hint, not as fallback.
    ById(Guid, Option<u32>),
    ByName(WildMatch),
    ByIndex(u32),
    /// This is the old default.
    ///
    /// The index comes into play as fallback whenever track is "<Selected>" or the GUID can't be
    /// determined (is `None`). I'm not sure how latter is possible but I keep it for backward
    /// compatibility.
    ByIdOrIndex(Option<Guid>, u32),
}

impl fmt::Display for VirtualChainFx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use VirtualChainFx::*;
        match self {
            Dynamic(_) => f.write_str("<Dynamic>"),
            ById(guid, _) => {
                write!(f, "{}", guid.to_string_without_braces())
            }
            ByName(name) => write!(f, "\"{}\"", name),
            ByIdOrIndex(None, i) | ByIndex(i) => write!(f, "{}", i + 1),
            ByIdOrIndex(Some(guid), i) => {
                write!(f, "{} ({})", guid.to_string_without_braces(), i + 1)
            }
        }
    }
}

fn find_track_by_name(project: Project, name: &WildMatch) -> Option<Track> {
    project.tracks().find(|t| match t.name() {
        None => false,
        Some(n) => name.matches(n.to_str()),
    })
}

#[derive(Clone, Debug, Display, Error)]
pub enum TrackResolveError {
    #[display(fmt = "TrackNotFound")]
    TrackNotFound {
        guid: Option<Guid>,
        name: Option<WildMatch>,
        index: Option<u32>,
    },
    NoTrackSelected,
}

#[derive(Clone, Debug, Display, Error)]
pub enum FxParameterResolveError {
    #[display(fmt = "FxParameterNotFound")]
    FxParameterNotFound {
        name: Option<WildMatch>,
        index: Option<u32>,
    },
}

#[derive(Clone, Debug, Display, Error)]
pub enum TrackRouteResolveError {
    #[display(fmt = "InvalidRoute")]
    InvalidRoute,
    #[display(fmt = "TrackRouteNotFound")]
    TrackRouteNotFound {
        guid: Option<Guid>,
        name: Option<WildMatch>,
        index: Option<u32>,
    },
}

impl VirtualChainFx {
    pub fn resolve(
        &self,
        fx_chain: &FxChain,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Fx, FxResolveError> {
        use VirtualChainFx::*;
        let fx = match self {
            Dynamic(evaluator) => {
                let index = Self::evaluate_to_fx_index(evaluator, context, compartment);
                get_index_based_fx_on_chain(fx_chain, index).map_err(|_| {
                    FxResolveError::FxNotFound {
                        guid: None,
                        name: None,
                        index: Some(index),
                    }
                })?
            }
            ById(guid, index) => get_guid_based_fx_by_guid_on_chain_with_index_hint(
                fx_chain, guid, *index,
            )
            .map_err(|_| FxResolveError::FxNotFound {
                guid: Some(*guid),
                name: None,
                index: None,
            })?,
            ByName(name) => {
                find_fx_by_name(fx_chain, name).ok_or_else(|| FxResolveError::FxNotFound {
                    guid: None,
                    name: Some(name.clone()),
                    index: None,
                })?
            }
            ByIndex(index) | ByIdOrIndex(None, index) => {
                get_index_based_fx_on_chain(fx_chain, *index).map_err(|_| {
                    FxResolveError::FxNotFound {
                        guid: None,
                        name: None,
                        index: Some(*index),
                    }
                })?
            }
            ByIdOrIndex(Some(guid), index) => {
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

    pub fn calculated_fx_index(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Option<u32> {
        if let VirtualChainFx::Dynamic(evaluator) = self {
            Some(Self::evaluate_to_fx_index(evaluator, context, compartment))
        } else {
            None
        }
    }

    fn evaluate_to_fx_index(
        evaluator: &ExpressionEvaluator,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> u32 {
        let sliced_params = compartment.slice_params(context.params());
        let result = evaluator.evaluate(sliced_params);
        result.round().max(0.0) as u32
    }

    pub fn id(&self) -> Option<Guid> {
        use VirtualChainFx::*;
        match self {
            ById(id, _) => Some(*id),
            ByIdOrIndex(id, _) => *id,
            _ => None,
        }
    }

    pub fn index(&self) -> Option<u32> {
        use VirtualChainFx::*;
        match self {
            ByIndex(i) | ByIdOrIndex(_, i) => Some(*i),
            ById(_, index_hint) => *index_hint,
            _ => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        use VirtualChainFx::*;
        match self {
            ByName(name) => Some(name.to_string()),
            _ => None,
        }
    }
}

fn find_fx_by_name(chain: &FxChain, name: &WildMatch) -> Option<Fx> {
    chain.fxs().find(|fx| name.matches(fx.name().to_str()))
}

#[derive(Clone, Debug, Display, Error)]
pub enum FxResolveError {
    #[display(fmt = "FxNotFound")]
    FxNotFound {
        guid: Option<Guid>,
        name: Option<WildMatch>,
        index: Option<u32>,
    },
}

pub struct VirtualTrackWithContext<'a> {
    virtual_track: &'a VirtualTrack,
    context: ExtendedProcessorContext<'a>,
    compartment: MappingCompartment,
}

impl<'a> fmt::Display for VirtualTrackWithContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VirtualTrack::*;
        match self.virtual_track {
            This | Selected { .. } | Master | Dynamic(_) => write!(f, "{}", self.virtual_track),
            _ => {
                if let Some(t) = self
                    .virtual_track
                    .resolve(self.context, self.compartment)
                    .ok()
                    .and_then(|tracks| tracks.into_iter().next())
                {
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

pub fn get_non_present_virtual_route_label(route: &VirtualTrackRoute) -> String {
    format!("<Not present> ({})", route)
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
    fx_parameter_descriptor: &FxParameterDescriptor,
    compartment: MappingCompartment,
) -> Result<FxParameter, &'static str> {
    let fx = get_fx(context, &fx_parameter_descriptor.fx_descriptor, compartment)?;
    fx_parameter_descriptor
        .fx_parameter
        .resolve(&fx, context, compartment)
        .map_err(|_| "parameter doesn't exist")
}

// Returns an error if the FX doesn't exist.
pub fn get_fx(
    context: ExtendedProcessorContext,
    descriptor: &FxDescriptor,
    compartment: MappingCompartment,
) -> Result<Fx, &'static str> {
    match &descriptor.fx {
        VirtualFx::This => {
            let fx = context.context().containing_fx();
            if fx.is_available() {
                Ok(fx.clone())
            } else {
                Err("this FX not available anymore")
            }
        }
        VirtualFx::Focused => Reaper::get()
            .focused_fx()
            .ok_or("couldn't get (last) focused FX"),
        VirtualFx::ChainFx {
            is_input_fx,
            chain_fx,
        } => {
            enum MaybeOwned<'a, T> {
                Owned(T),
                Borrowed(&'a T),
            }
            impl<'a, T> MaybeOwned<'a, T> {
                fn get(&self) -> &T {
                    match self {
                        MaybeOwned::Owned(o) => &o,
                        MaybeOwned::Borrowed(b) => b,
                    }
                }
            }
            let chain_fx = match chain_fx {
                VirtualChainFx::ByIdOrIndex(_, index) => {
                    // Actually it's not that important whether we create an index-based or
                    // GUID-based FX. The session listeners will recreate and
                    // resync the FX whenever something has changed anyway. But
                    // for monitoring FX it could still be good (which we don't get notified
                    // about unfortunately).
                    if matches!(
                        descriptor.track_descriptor.track,
                        VirtualTrack::Selected { .. }
                    ) {
                        MaybeOwned::Owned(VirtualChainFx::ByIndex(*index))
                    } else {
                        MaybeOwned::Borrowed(chain_fx)
                    }
                }
                _ => MaybeOwned::Borrowed(chain_fx),
            };
            let fx_chain = get_fx_chain(
                context,
                &descriptor.track_descriptor.track,
                *is_input_fx,
                compartment,
            )?;
            chain_fx
                .get()
                .resolve(&fx_chain, context, compartment)
                .map_err(|_| "couldn't resolve particular FX")
        }
    }
}

fn get_index_based_fx_on_chain(fx_chain: &FxChain, fx_index: u32) -> Result<Fx, &'static str> {
    let fx = fx_chain.fx_by_index_untracked(fx_index);
    if !fx.is_available() {
        return Err("no FX at that index");
    }
    Ok(fx)
}

fn resolve_parameter_by_index(fx: &Fx, index: u32) -> Result<FxParameter, FxParameterResolveError> {
    let param = fx.parameter_by_index(index);
    if !param.is_available() {
        return Err(FxParameterResolveError::FxParameterNotFound {
            name: None,
            index: Some(index),
        });
    }
    Ok(param)
}

fn resolve_track_by_index(project: Project, index: u32) -> Result<Track, TrackResolveError> {
    project
        .track_by_index(index)
        .ok_or(TrackResolveError::TrackNotFound {
            guid: None,
            name: None,
            index: Some(index),
        })
}

pub fn resolve_track_route_by_index(
    track: &Track,
    route_type: TrackRouteType,
    index: u32,
) -> Result<TrackRoute, TrackRouteResolveError> {
    let option = match route_type {
        TrackRouteType::Send => track.typed_send_by_index(SendPartnerType::Track, index),
        TrackRouteType::Receive => track.receive_by_index(index),
        TrackRouteType::HardwareOutput => {
            track.typed_send_by_index(SendPartnerType::HardwareOutput, index)
        }
    };
    if let Some(route) = option {
        Ok(route)
    } else {
        Err(TrackRouteResolveError::TrackRouteNotFound {
            guid: None,
            name: None,
            index: Some(index),
        })
    }
}

pub fn get_fx_chain(
    context: ExtendedProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    compartment: MappingCompartment,
) -> Result<FxChain, &'static str> {
    let track = get_effective_tracks(context, track, compartment)?
        // TODO-medium Support multiple tracks
        .into_iter()
        .next()
        .ok_or("no track resolved")?;
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

fn find_route_by_related_track(
    main_track: &Track,
    related_track: &Track,
    route_type: TrackRouteType,
) -> Result<Option<TrackRoute>, TrackRouteResolveError> {
    let option = match route_type {
        TrackRouteType::Send => main_track.find_send_by_destination_track(&related_track),
        TrackRouteType::Receive => main_track.find_receive_by_source_track(&related_track),
        TrackRouteType::HardwareOutput => {
            return Err(TrackRouteResolveError::InvalidRoute);
        }
    };
    Ok(option)
}

fn find_route_by_name(
    track: &Track,
    name: &WildMatch,
    route_type: TrackRouteType,
) -> Option<TrackRoute> {
    let matcher = |r: &TrackRoute| name.matches(r.name().to_str());
    match route_type {
        TrackRouteType::Send => track.typed_sends(SendPartnerType::Track).find(matcher),
        TrackRouteType::Receive => track.receives().find(matcher),
        TrackRouteType::HardwareOutput => track
            .typed_sends(SendPartnerType::HardwareOutput)
            .find(matcher),
    }
}

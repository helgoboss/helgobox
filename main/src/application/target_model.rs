use crate::base::default_util::is_default;
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    ControlType, Interval, OscArgDescriptor, OscTypeTag, Target, DEFAULT_OSC_ARG_VALUE_RANGE,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, BookmarkType, Fx, FxChain, FxParameter, Guid, Project, Track, TrackRoute,
    TrackRoutePartner,
};

use serde::{Deserialize, Serialize};

use crate::application::{
    Affected, Change, GetProcessingRelevance, ProcessingRelevance, VirtualControlElementType,
};
use crate::domain::{
    find_bookmark, get_fx_params, get_fxs, get_non_present_virtual_route_label,
    get_non_present_virtual_track_label, get_track_routes, ActionInvocationType, AnyOnParameter,
    Compartment, CompoundMappingTarget, Exclusivity, ExpressionEvaluator, ExtendedProcessorContext,
    FeedbackResolution, FxDescriptor, FxDisplayType, FxParameterDescriptor, GroupId, OscDeviceId,
    ProcessorContext, RealearnTarget, ReaperTarget, ReaperTargetType, SeekOptions,
    SendMidiDestination, SoloBehavior, Tag, TagScope, TouchedRouteParameterType,
    TouchedTrackParameterType, TrackDescriptor, TrackExclusivity, TrackRouteDescriptor,
    TrackRouteSelector, TrackRouteType, TransportAction, UnresolvedActionTarget,
    UnresolvedAllTrackFxEnableTarget, UnresolvedAnyOnTarget,
    UnresolvedAutomationModeOverrideTarget, UnresolvedClipColumnTarget,
    UnresolvedClipManagementTarget, UnresolvedClipMatrixTarget, UnresolvedClipRowTarget,
    UnresolvedClipSeekTarget, UnresolvedClipTransportTarget, UnresolvedClipVolumeTarget,
    UnresolvedCompoundMappingTarget, UnresolvedEnableInstancesTarget,
    UnresolvedEnableMappingsTarget, UnresolvedFxEnableTarget, UnresolvedFxNavigateTarget,
    UnresolvedFxOnlineTarget, UnresolvedFxOpenTarget, UnresolvedFxParameterTarget,
    UnresolvedFxParameterTouchStateTarget, UnresolvedFxPresetTarget, UnresolvedGoToBookmarkTarget,
    UnresolvedLastTouchedTarget, UnresolvedLoadFxSnapshotTarget,
    UnresolvedLoadMappingSnapshotTarget, UnresolvedMidiSendTarget,
    UnresolvedNavigateWithinGroupTarget, UnresolvedOscSendTarget, UnresolvedPlayrateTarget,
    UnresolvedReaperTarget, UnresolvedRouteAutomationModeTarget, UnresolvedRouteMonoTarget,
    UnresolvedRouteMuteTarget, UnresolvedRoutePanTarget, UnresolvedRoutePhaseTarget,
    UnresolvedRouteTouchStateTarget, UnresolvedRouteVolumeTarget, UnresolvedSeekTarget,
    UnresolvedSelectedTrackTarget, UnresolvedTempoTarget, UnresolvedTrackArmTarget,
    UnresolvedTrackAutomationModeTarget, UnresolvedTrackMonitoringModeTarget,
    UnresolvedTrackMuteTarget, UnresolvedTrackPanTarget, UnresolvedTrackPeakTarget,
    UnresolvedTrackPhaseTarget, UnresolvedTrackSelectionTarget, UnresolvedTrackShowTarget,
    UnresolvedTrackSoloTarget, UnresolvedTrackToolTarget, UnresolvedTrackTouchStateTarget,
    UnresolvedTrackVolumeTarget, UnresolvedTrackWidthTarget, UnresolvedTransportTarget,
    VirtualChainFx, VirtualClipColumn, VirtualClipRow, VirtualClipSlot, VirtualControlElement,
    VirtualControlElementId, VirtualFx, VirtualFxParameter, VirtualTarget, VirtualTrack,
    VirtualTrackRoute,
};
use serde_repr::*;
use std::borrow::Cow;
use std::error::Error;

use playtime_api::{ClipPlayStartTiming, ClipPlayStopTiming};
use playtime_clip_engine::main::ClipTransportOptions;
use realearn_api::persistence::{
    ClipColumnAction, ClipColumnDescriptor, ClipColumnTrackContext, ClipManagementAction,
    ClipMatrixAction, ClipRowAction, ClipRowDescriptor, ClipSlotDescriptor, ClipTransportAction,
    MonitoringMode,
};
use reaper_medium::{
    AutomationMode, BookmarkId, GlobalAutomationModeOverride, InputMonitoringMode, TrackArea,
    TrackLocation, TrackSendDirection,
};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use wildmatch::WildMatch;

#[allow(clippy::enum_variant_names)]
pub enum TargetCommand {
    SetCategory(TargetCategory),
    SetUnit(TargetUnit),
    SetControlElementType(VirtualControlElementType),
    SetControlElementId(VirtualControlElementId),
    SetTargetType(ReaperTargetType),
    SetAction(Option<Action>),
    SetActionInvocationType(ActionInvocationType),
    SetWithTrack(bool),
    SetTrackName(String),
    SetTrackIndex(u32),
    SetTrackExpression(String),
    SetEnableOnlyIfTrackSelected(bool),
    SetFxIsInputFx(bool),
    SetFxName(String),
    SetFxIndex(u32),
    SetFxExpression(String),
    SetEnableOnlyIfFxHasFocus(bool),
    SetParamType(VirtualFxParameterType),
    SetParamIndex(u32),
    SetParamName(String),
    SetParamExpression(String),
    SetRouteSelectorType(TrackRouteSelectorType),
    SetRouteType(TrackRouteType),
    SetRouteId(Option<Guid>),
    SetRouteIndex(u32),
    SetRouteName(String),
    SetRouteExpression(String),
    SetSoloBehavior(SoloBehavior),
    SetTrackExclusivity(TrackExclusivity),
    SetTransportAction(TransportAction),
    SetAnyOnParameter(AnyOnParameter),
    SetFxSnapshot(Option<FxSnapshot>),
    SetTouchedTrackParameterType(TouchedTrackParameterType),
    SetTouchedRouteParameterType(TouchedRouteParameterType),
    SetBookmarkRef(u32),
    SetBookmarkType(BookmarkType),
    SetBookmarkAnchorType(BookmarkAnchorType),
    SetUseTimeSelection(bool),
    SetUseLoopPoints(bool),
    SetUseRegions(bool),
    SetUseProject(bool),
    SetMoveView(bool),
    SetSeekPlay(bool),
    SetFeedbackResolution(FeedbackResolution),
    SetTrackArea(RealearnTrackArea),
    SetAutomationMode(RealearnAutomationMode),
    SetMonitoringMode(MonitoringMode),
    SetAutomationModeOverrideType(AutomationModeOverrideType),
    SetFxDisplayType(FxDisplayType),
    SetScrollArrangeView(bool),
    SetScrollMixer(bool),
    SetRawMidiPattern(String),
    SetSendMidiDestination(SendMidiDestination),
    SetOscAddressPattern(String),
    SetOscArgIndex(Option<u32>),
    SetOscArgTypeTag(OscTypeTag),
    SetOscArgValueRange(Interval<f64>),
    SetOscDevId(Option<OscDeviceId>),
    SetClipSlot(ClipSlotDescriptor),
    SetClipColumn(ClipColumnDescriptor),
    SetClipRow(ClipRowDescriptor),
    SetClipManagementAction(ClipManagementAction),
    SetClipTransportAction(ClipTransportAction),
    SetClipMatrixAction(ClipMatrixAction),
    SetClipColumnAction(ClipColumnAction),
    SetClipRowAction(ClipRowAction),
    SetClipPlayStartTiming(Option<ClipPlayStartTiming>),
    SetClipPlayStopTiming(Option<ClipPlayStopTiming>),
    SetRecordOnlyIfTrackArmed(bool),
    SetStopColumnIfSlotEmpty(bool),
    SetPollForFeedback(bool),
    SetTags(Vec<Tag>),
    SetExclusivity(Exclusivity),
    SetGroupId(GroupId),
    SetActiveMappingsOnly(bool),
}

#[derive(PartialEq)]
pub enum TargetProp {
    Category,
    Unit,
    ControlElementType,
    ControlElementId,
    TargetType,
    Action,
    ActionInvocationType,
    WithTrack,
    TrackType,
    TrackId,
    TrackName,
    TrackIndex,
    TrackExpression,
    EnableOnlyIfTrackSelected,
    FxType,
    FxIsInputFx,
    FxId,
    FxName,
    FxIndex,
    FxExpression,
    EnableOnlyIfFxHasFocus,
    ParamType,
    ParamIndex,
    ParamName,
    ParamExpression,
    RouteSelectorType,
    RouteType,
    RouteId,
    RouteIndex,
    RouteName,
    RouteExpression,
    SoloBehavior,
    TrackExclusivity,
    TransportAction,
    AnyOnParameter,
    FxSnapshot,
    TouchedTrackParameterType,
    TouchedRouteParameterType,
    BookmarkRef,
    BookmarkType,
    BookmarkAnchorType,
    UseTimeSelection,
    UseLoopPoints,
    UseRegions,
    UseProject,
    MoveView,
    SeekPlay,
    FeedbackResolution,
    TrackArea,
    AutomationMode,
    MonitoringMode,
    AutomationModeOverrideType,
    FxDisplayType,
    ScrollArrangeView,
    ScrollMixer,
    RawMidiPattern,
    SendMidiDestination,
    OscAddressPattern,
    OscArgIndex,
    OscArgTypeTag,
    OscArgValueRange,
    OscDevId,
    ClipSlot,
    ClipColumn,
    ClipRow,
    ClipManagementAction,
    ClipTransportAction,
    ClipMatrixAction,
    ClipColumnAction,
    ClipRowAction,
    ClipPlayStartTiming,
    ClipPlayStopTiming,
    RecordOnlyIfTrackArmed,
    StopColumnIfSlotEmpty,
    PollForFeedback,
    Tags,
    Exclusivity,
    GroupId,
    ActiveMappingsOnly,
}

impl GetProcessingRelevance for TargetProp {
    fn processing_relevance(&self) -> Option<ProcessingRelevance> {
        // At the moment, all target aspects are relevant for processing.
        Some(ProcessingRelevance::ProcessingRelevant)
    }
}

impl<'a> Change<'a> for TargetModel {
    type Command = TargetCommand;
    type Prop = TargetProp;

    fn change(&mut self, cmd: Self::Command) -> Option<Affected<TargetProp>> {
        use Affected::*;
        use TargetCommand as C;
        use TargetProp as P;
        let affected = match cmd {
            C::SetCategory(v) => {
                self.category = v;
                One(P::Category)
            }
            C::SetUnit(v) => {
                self.unit = v;
                One(P::Unit)
            }
            C::SetControlElementType(v) => {
                self.control_element_type = v;
                One(P::ControlElementType)
            }
            C::SetControlElementId(v) => {
                self.control_element_id = v;
                One(P::ControlElementId)
            }
            C::SetTargetType(v) => {
                self.r#type = v;
                One(P::TargetType)
            }
            C::SetAction(v) => {
                self.action = v;
                One(P::Action)
            }
            C::SetActionInvocationType(v) => {
                self.action_invocation_type = v;
                One(P::ActionInvocationType)
            }
            C::SetWithTrack(v) => {
                self.with_track = v;
                One(P::WithTrack)
            }
            C::SetTrackName(v) => {
                self.track_name = v;
                One(P::TrackName)
            }
            C::SetTrackIndex(v) => {
                self.track_index = v;
                One(P::TrackIndex)
            }
            C::SetTrackExpression(v) => {
                self.track_expression = v;
                One(P::TrackExpression)
            }
            C::SetEnableOnlyIfTrackSelected(v) => {
                self.enable_only_if_track_selected = v;
                One(P::EnableOnlyIfTrackSelected)
            }
            C::SetFxIsInputFx(v) => {
                self.fx_is_input_fx = v;
                One(P::FxIsInputFx)
            }
            C::SetFxName(v) => {
                self.fx_name = v;
                One(P::FxName)
            }
            C::SetFxIndex(v) => {
                self.fx_index = v;
                One(P::FxIndex)
            }
            C::SetFxExpression(v) => {
                self.fx_expression = v;
                One(P::FxExpression)
            }
            C::SetEnableOnlyIfFxHasFocus(v) => {
                self.enable_only_if_fx_has_focus = v;
                One(P::EnableOnlyIfFxHasFocus)
            }
            C::SetParamType(v) => {
                self.param_type = v;
                One(P::ParamType)
            }
            C::SetParamIndex(v) => {
                self.param_index = v;
                One(P::ParamIndex)
            }
            C::SetParamName(v) => {
                self.param_name = v;
                One(P::ParamName)
            }
            C::SetParamExpression(v) => {
                self.param_expression = v;
                One(P::ParamExpression)
            }
            C::SetRouteSelectorType(v) => {
                self.route_selector_type = v;
                One(P::RouteSelectorType)
            }
            C::SetRouteType(v) => {
                self.route_type = v;
                One(P::RouteType)
            }
            C::SetRouteId(v) => {
                self.route_id = v;
                One(P::RouteId)
            }
            C::SetRouteIndex(v) => {
                self.route_index = v;
                One(P::RouteIndex)
            }
            C::SetRouteName(v) => {
                self.route_name = v;
                One(P::RouteName)
            }
            C::SetRouteExpression(v) => {
                self.route_expression = v;
                One(P::RouteExpression)
            }
            C::SetSoloBehavior(v) => {
                self.solo_behavior = v;
                One(P::SoloBehavior)
            }
            C::SetTrackExclusivity(v) => {
                self.track_exclusivity = v;
                One(P::TrackExclusivity)
            }
            C::SetTransportAction(v) => {
                self.transport_action = v;
                One(P::TransportAction)
            }
            C::SetAnyOnParameter(v) => {
                self.any_on_parameter = v;
                One(P::AnyOnParameter)
            }
            C::SetFxSnapshot(v) => {
                self.fx_snapshot = v;
                One(P::FxSnapshot)
            }
            C::SetTouchedTrackParameterType(v) => {
                self.touched_track_parameter_type = v;
                One(P::TouchedTrackParameterType)
            }
            C::SetTouchedRouteParameterType(v) => {
                self.touched_route_parameter_type = v;
                One(P::TouchedRouteParameterType)
            }
            C::SetBookmarkRef(v) => {
                self.bookmark_ref = v;
                One(P::BookmarkRef)
            }
            C::SetBookmarkType(v) => {
                self.bookmark_type = v;
                One(P::BookmarkType)
            }
            C::SetBookmarkAnchorType(v) => {
                self.bookmark_anchor_type = v;
                One(P::BookmarkAnchorType)
            }
            C::SetUseTimeSelection(v) => {
                self.use_time_selection = v;
                One(P::UseTimeSelection)
            }
            C::SetUseLoopPoints(v) => {
                self.use_loop_points = v;
                One(P::UseLoopPoints)
            }
            C::SetUseRegions(v) => {
                self.use_regions = v;
                One(P::UseRegions)
            }
            C::SetUseProject(v) => {
                self.use_project = v;
                One(P::UseProject)
            }
            C::SetMoveView(v) => {
                self.move_view = v;
                One(P::MoveView)
            }
            C::SetSeekPlay(v) => {
                self.seek_play = v;
                One(P::SeekPlay)
            }
            C::SetFeedbackResolution(v) => {
                self.feedback_resolution = v;
                One(P::FeedbackResolution)
            }
            C::SetTrackArea(v) => {
                self.track_area = v;
                One(P::TrackArea)
            }
            C::SetAutomationMode(v) => {
                self.automation_mode = v;
                One(P::AutomationMode)
            }
            C::SetMonitoringMode(v) => {
                self.monitoring_mode = v;
                One(P::MonitoringMode)
            }
            C::SetAutomationModeOverrideType(v) => {
                self.automation_mode_override_type = v;
                One(P::AutomationModeOverrideType)
            }
            C::SetFxDisplayType(v) => {
                self.fx_display_type = v;
                One(P::FxDisplayType)
            }
            C::SetScrollArrangeView(v) => {
                self.scroll_arrange_view = v;
                One(P::ScrollArrangeView)
            }
            C::SetScrollMixer(v) => {
                self.scroll_mixer = v;
                One(P::ScrollMixer)
            }
            C::SetRawMidiPattern(v) => {
                self.raw_midi_pattern = v;
                One(P::RawMidiPattern)
            }
            C::SetSendMidiDestination(v) => {
                self.send_midi_destination = v;
                One(P::SendMidiDestination)
            }
            C::SetOscAddressPattern(v) => {
                self.osc_address_pattern = v;
                One(P::OscAddressPattern)
            }
            C::SetOscArgIndex(v) => {
                self.osc_arg_index = v;
                One(P::OscArgIndex)
            }
            C::SetOscArgTypeTag(v) => {
                self.osc_arg_type_tag = v;
                One(P::OscArgTypeTag)
            }
            C::SetOscArgValueRange(v) => {
                self.osc_arg_value_range = v;
                One(P::OscArgValueRange)
            }
            C::SetOscDevId(v) => {
                self.osc_dev_id = v;
                One(P::OscDevId)
            }
            C::SetPollForFeedback(v) => {
                self.poll_for_feedback = v;
                One(P::PollForFeedback)
            }
            C::SetTags(v) => {
                self.tags = v;
                One(P::Tags)
            }
            C::SetExclusivity(v) => {
                self.exclusivity = v;
                One(P::Exclusivity)
            }
            C::SetGroupId(v) => {
                self.group_id = v;
                One(P::GroupId)
            }
            C::SetActiveMappingsOnly(v) => {
                self.active_mappings_only = v;
                One(P::ActiveMappingsOnly)
            }
            C::SetClipSlot(s) => {
                self.clip_slot = s;
                One(P::ClipSlot)
            }
            C::SetClipColumn(c) => {
                self.clip_column = c;
                One(P::ClipColumn)
            }
            C::SetClipRow(r) => {
                self.clip_row = r;
                One(P::ClipRow)
            }
            C::SetClipManagementAction(v) => {
                self.clip_management_action = v;
                One(P::ClipManagementAction)
            }
            C::SetClipTransportAction(v) => {
                self.clip_transport_action = v;
                One(P::ClipTransportAction)
            }
            C::SetClipMatrixAction(v) => {
                self.clip_matrix_action = v;
                One(P::ClipMatrixAction)
            }
            C::SetClipColumnAction(v) => {
                self.clip_column_action = v;
                One(P::ClipColumnAction)
            }
            C::SetClipRowAction(v) => {
                self.clip_row_action = v;
                One(P::ClipRowAction)
            }
            C::SetClipPlayStartTiming(v) => {
                self.clip_play_start_timing = v;
                One(P::ClipPlayStartTiming)
            }
            C::SetClipPlayStopTiming(v) => {
                self.clip_play_stop_timing = v;
                One(P::ClipPlayStopTiming)
            }
            C::SetRecordOnlyIfTrackArmed(v) => {
                self.record_only_if_track_armed = v;
                One(P::RecordOnlyIfTrackArmed)
            }
            C::SetStopColumnIfSlotEmpty(v) => {
                self.stop_column_if_slot_empty = v;
                One(P::StopColumnIfSlotEmpty)
            }
        };
        Some(affected)
    }
}

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // # For all targets
    category: TargetCategory,
    unit: TargetUnit,
    // # For virtual targets
    control_element_type: VirtualControlElementType,
    control_element_id: VirtualControlElementId,
    // # For REAPER targets
    // TODO-low Rename this to reaper_target_type
    r#type: ReaperTargetType,
    // # For action targets only
    // TODO-low Maybe replace Action with just command ID and/or command name
    action: Option<Action>,
    action_invocation_type: ActionInvocationType,
    with_track: bool,
    // # For track targets
    track_type: VirtualTrackType,
    track_id: Option<Guid>,
    track_name: String,
    track_index: u32,
    track_expression: String,
    enable_only_if_track_selected: bool,
    clip_column_track_context: ClipColumnTrackContext,
    // # For track FX targets
    fx_type: VirtualFxType,
    fx_is_input_fx: bool,
    fx_id: Option<Guid>,
    fx_name: String,
    fx_index: u32,
    fx_expression: String,
    enable_only_if_fx_has_focus: bool,
    // # For track FX parameter targets
    param_type: VirtualFxParameterType,
    param_index: u32,
    param_name: String,
    param_expression: String,
    // # For track route targets
    route_selector_type: TrackRouteSelectorType,
    route_type: TrackRouteType,
    route_id: Option<Guid>,
    route_index: u32,
    route_name: String,
    route_expression: String,
    touched_route_parameter_type: TouchedRouteParameterType,
    // # For track solo targets
    solo_behavior: SoloBehavior,
    // # For toggleable track targets
    track_exclusivity: TrackExclusivity,
    // # For transport target
    transport_action: TransportAction,
    // # For any-on target
    any_on_parameter: AnyOnParameter,
    // # For "Load FX snapshot" target
    fx_snapshot: Option<FxSnapshot>,
    // # For "Automation touch state" target
    touched_track_parameter_type: TouchedTrackParameterType,
    // # For "Go to marker/region" target
    bookmark_ref: u32,
    bookmark_type: BookmarkType,
    bookmark_anchor_type: BookmarkAnchorType,
    // # For "Go to marker/region" target and "Seek" target
    use_time_selection: bool,
    use_loop_points: bool,
    // # For "Seek" target
    use_regions: bool,
    use_project: bool,
    move_view: bool,
    seek_play: bool,
    feedback_resolution: FeedbackResolution,
    // # For track show target
    track_area: RealearnTrackArea,
    // # For track and route automation mode target
    automation_mode: RealearnAutomationMode,
    // # For track monitoring mode target
    monitoring_mode: MonitoringMode,
    // # For automation mode override target
    automation_mode_override_type: AutomationModeOverrideType,
    // # For FX Open and FX Navigate target
    fx_display_type: FxDisplayType,
    // # For track selection related targets
    scroll_arrange_view: bool,
    scroll_mixer: bool,
    // # For Send MIDI target
    raw_midi_pattern: String,
    send_midi_destination: SendMidiDestination,
    // # For Send OSC target
    osc_address_pattern: String,
    osc_arg_index: Option<u32>,
    osc_arg_type_tag: OscTypeTag,
    osc_arg_value_range: Interval<f64>,
    osc_dev_id: Option<OscDeviceId>,
    // # For clip targets
    clip_slot: ClipSlotDescriptor,
    clip_column: ClipColumnDescriptor,
    clip_row: ClipRowDescriptor,
    clip_management_action: ClipManagementAction,
    clip_transport_action: ClipTransportAction,
    clip_matrix_action: ClipMatrixAction,
    clip_column_action: ClipColumnAction,
    clip_row_action: ClipRowAction,
    record_only_if_track_armed: bool,
    stop_column_if_slot_empty: bool,
    clip_play_start_timing: Option<ClipPlayStartTiming>,
    clip_play_stop_timing: Option<ClipPlayStopTiming>,
    // # For targets that might have to be polled in order to get automatic feedback in all cases.
    poll_for_feedback: bool,
    tags: Vec<Tag>,
    exclusivity: Exclusivity,
    group_id: GroupId,
    active_mappings_only: bool,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            category: TargetCategory::default(),
            unit: Default::default(),
            control_element_type: VirtualControlElementType::default(),
            control_element_id: Default::default(),
            r#type: ReaperTargetType::FxParameterValue,
            action: None,
            action_invocation_type: ActionInvocationType::default(),
            track_type: Default::default(),
            track_id: None,
            track_name: "".to_owned(),
            track_index: 0,
            track_expression: "".to_owned(),
            enable_only_if_track_selected: false,
            with_track: false,
            fx_type: Default::default(),
            fx_is_input_fx: false,
            fx_id: None,
            fx_name: "".to_owned(),
            fx_index: 0,
            fx_expression: "".to_owned(),
            enable_only_if_fx_has_focus: false,
            param_type: Default::default(),
            param_index: 0,
            param_name: "".to_owned(),
            param_expression: "".to_owned(),
            route_selector_type: Default::default(),
            route_type: Default::default(),
            route_id: None,
            route_index: 0,
            route_name: Default::default(),
            route_expression: Default::default(),
            touched_route_parameter_type: Default::default(),
            solo_behavior: Default::default(),
            track_exclusivity: Default::default(),
            transport_action: TransportAction::default(),
            any_on_parameter: AnyOnParameter::default(),
            fx_snapshot: None,
            touched_track_parameter_type: Default::default(),
            bookmark_ref: 0,
            bookmark_type: BookmarkType::Marker,
            bookmark_anchor_type: Default::default(),
            use_time_selection: false,
            use_loop_points: false,
            use_regions: false,
            use_project: true,
            move_view: true,
            seek_play: true,
            feedback_resolution: Default::default(),
            track_area: Default::default(),
            automation_mode: Default::default(),
            monitoring_mode: Default::default(),
            automation_mode_override_type: Default::default(),
            fx_display_type: Default::default(),
            scroll_arrange_view: false,
            scroll_mixer: false,
            raw_midi_pattern: Default::default(),
            send_midi_destination: Default::default(),
            osc_address_pattern: "".to_owned(),
            osc_arg_index: Some(0),
            osc_arg_type_tag: Default::default(),
            osc_arg_value_range: DEFAULT_OSC_ARG_VALUE_RANGE,
            osc_dev_id: None,
            poll_for_feedback: true,
            tags: Default::default(),
            exclusivity: Default::default(),
            group_id: Default::default(),
            active_mappings_only: false,
            clip_slot: Default::default(),
            clip_column: Default::default(),
            clip_row: Default::default(),
            clip_management_action: Default::default(),
            clip_transport_action: Default::default(),
            clip_column_action: Default::default(),
            clip_matrix_action: Default::default(),
            record_only_if_track_armed: false,
            stop_column_if_slot_empty: false,
            clip_play_start_timing: None,
            clip_column_track_context: Default::default(),
            clip_row_action: Default::default(),
            clip_play_stop_timing: None,
        }
    }
}

impl TargetModel {
    pub fn category(&self) -> TargetCategory {
        self.category
    }

    pub fn unit(&self) -> TargetUnit {
        self.unit
    }

    pub fn control_element_type(&self) -> VirtualControlElementType {
        self.control_element_type
    }

    pub fn control_element_id(&self) -> VirtualControlElementId {
        self.control_element_id
    }

    pub fn target_type(&self) -> ReaperTargetType {
        self.r#type
    }

    pub fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }

    pub fn action_invocation_type(&self) -> ActionInvocationType {
        self.action_invocation_type
    }

    pub fn with_track(&self) -> bool {
        self.with_track
    }

    pub fn track_type(&self) -> VirtualTrackType {
        self.track_type
    }

    pub fn track_name(&self) -> &str {
        &self.track_name
    }

    pub fn track_index(&self) -> u32 {
        self.track_index
    }

    pub fn track_expression(&self) -> &str {
        &self.track_expression
    }

    pub fn enable_only_if_track_selected(&self) -> bool {
        self.enable_only_if_track_selected
    }

    pub fn fx_type(&self) -> VirtualFxType {
        self.fx_type
    }

    pub fn fx_is_input_fx(&self) -> bool {
        self.fx_is_input_fx
    }

    pub fn fx_name(&self) -> &str {
        &self.fx_name
    }

    pub fn fx_index(&self) -> u32 {
        self.fx_index
    }

    pub fn fx_expression(&self) -> &str {
        &self.fx_expression
    }

    pub fn enable_only_if_fx_has_focus(&self) -> bool {
        self.enable_only_if_fx_has_focus
    }

    pub fn param_type(&self) -> VirtualFxParameterType {
        self.param_type
    }

    pub fn param_index(&self) -> u32 {
        self.param_index
    }

    pub fn param_name(&self) -> &str {
        &self.param_name
    }

    pub fn param_expression(&self) -> &str {
        &self.param_expression
    }

    pub fn route_selector_type(&self) -> TrackRouteSelectorType {
        self.route_selector_type
    }

    pub fn route_type(&self) -> TrackRouteType {
        self.route_type
    }

    pub fn route_index(&self) -> u32 {
        self.route_index
    }

    pub fn route_name(&self) -> &str {
        &self.route_name
    }

    pub fn route_expression(&self) -> &str {
        &self.route_expression
    }

    pub fn solo_behavior(&self) -> SoloBehavior {
        self.solo_behavior
    }

    pub fn track_exclusivity(&self) -> TrackExclusivity {
        self.track_exclusivity
    }

    pub fn transport_action(&self) -> TransportAction {
        self.transport_action
    }

    pub fn any_on_parameter(&self) -> AnyOnParameter {
        self.any_on_parameter
    }

    pub fn fx_snapshot(&self) -> Option<&FxSnapshot> {
        self.fx_snapshot.as_ref()
    }

    pub fn touched_track_parameter_type(&self) -> TouchedTrackParameterType {
        self.touched_track_parameter_type
    }

    pub fn touched_route_parameter_type(&self) -> TouchedRouteParameterType {
        self.touched_route_parameter_type
    }

    pub fn bookmark_ref(&self) -> u32 {
        self.bookmark_ref
    }

    pub fn bookmark_type(&self) -> BookmarkType {
        self.bookmark_type
    }

    pub fn bookmark_anchor_type(&self) -> BookmarkAnchorType {
        self.bookmark_anchor_type
    }

    pub fn use_time_selection(&self) -> bool {
        self.use_time_selection
    }

    pub fn use_loop_points(&self) -> bool {
        self.use_loop_points
    }

    pub fn use_regions(&self) -> bool {
        self.use_regions
    }

    pub fn use_project(&self) -> bool {
        self.use_project
    }

    pub fn move_view(&self) -> bool {
        self.move_view
    }

    pub fn seek_play(&self) -> bool {
        self.seek_play
    }

    pub fn feedback_resolution(&self) -> FeedbackResolution {
        self.feedback_resolution
    }

    pub fn track_area(&self) -> RealearnTrackArea {
        self.track_area
    }

    pub fn automation_mode(&self) -> RealearnAutomationMode {
        self.automation_mode
    }

    pub fn monitoring_mode(&self) -> MonitoringMode {
        self.monitoring_mode
    }

    pub fn automation_mode_override_type(&self) -> AutomationModeOverrideType {
        self.automation_mode_override_type
    }

    pub fn fx_display_type(&self) -> FxDisplayType {
        self.fx_display_type
    }

    pub fn scroll_arrange_view(&self) -> bool {
        self.scroll_arrange_view
    }

    pub fn scroll_mixer(&self) -> bool {
        self.scroll_mixer
    }

    pub fn raw_midi_pattern(&self) -> &str {
        &self.raw_midi_pattern
    }

    pub fn send_midi_destination(&self) -> SendMidiDestination {
        self.send_midi_destination
    }

    pub fn osc_address_pattern(&self) -> &str {
        &self.osc_address_pattern
    }

    pub fn osc_arg_index(&self) -> Option<u32> {
        self.osc_arg_index
    }

    pub fn osc_arg_type_tag(&self) -> OscTypeTag {
        self.osc_arg_type_tag
    }

    pub fn osc_arg_value_range(&self) -> Interval<f64> {
        self.osc_arg_value_range
    }

    pub fn osc_dev_id(&self) -> Option<OscDeviceId> {
        self.osc_dev_id
    }

    pub fn clip_management_action(&self) -> &ClipManagementAction {
        &self.clip_management_action
    }

    pub fn poll_for_feedback(&self) -> bool {
        self.poll_for_feedback
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn exclusivity(&self) -> Exclusivity {
        self.exclusivity
    }

    pub fn group_id(&self) -> GroupId {
        self.group_id
    }

    pub fn active_mappings_only(&self) -> bool {
        self.active_mappings_only
    }

    pub fn supports_control(&self) -> bool {
        use TargetCategory::*;
        match self.category {
            Reaper => self.r#type.supports_control(),
            Virtual => true,
        }
    }

    pub fn supports_feedback(&self) -> bool {
        use TargetCategory::*;
        match self.category {
            Reaper => self.r#type.supports_feedback(),
            Virtual => true,
        }
    }

    pub fn make_track_sticky(
        &mut self,
        compartment: Compartment,
        context: ExtendedProcessorContext,
    ) -> Result<Option<Affected<TargetProp>>, Box<dyn Error>> {
        if self.track_type.is_sticky() {
            return Ok(None);
        };
        let track = self
            .with_context(context, compartment)
            .first_effective_track()?;
        let virtual_track = virtualize_track(&track, context.context(), false);
        let _ = self.set_virtual_track(virtual_track, Some(context.context()));
        Ok(Some(Affected::Multiple))
    }

    pub fn make_fx_sticky(
        &mut self,
        compartment: Compartment,
        context: ExtendedProcessorContext,
    ) -> Result<Option<Affected<TargetProp>>, Box<dyn Error>> {
        if self.fx_type.is_sticky() {
            return Ok(None);
        };
        let fx = self.with_context(context, compartment).first_fx()?;
        let virtual_fx = virtualize_fx(&fx, context.context(), false);
        Ok(self.set_virtual_fx(virtual_fx, context, compartment))
    }

    pub fn make_route_sticky(
        &mut self,
        compartment: Compartment,
        context: ExtendedProcessorContext,
    ) -> Result<Option<Affected<TargetProp>>, Box<dyn Error>> {
        if self.route_selector_type.is_sticky() {
            return Ok(None);
        };
        let desc = self.route_descriptor()?;
        let route = desc.resolve_first(context, compartment)?;
        let virtual_route = virtualize_route(&route, context.context(), false);
        Ok(self.set_virtual_route(virtual_route))
    }

    pub fn take_fx_snapshot(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<FxSnapshot, &'static str> {
        let fx = self.with_context(context, compartment).first_fx()?;
        let fx_info = fx.info()?;
        let fx_snapshot = FxSnapshot {
            fx_type: if fx_info.sub_type_expression.is_empty() {
                fx_info.type_expression
            } else {
                fx_info.sub_type_expression
            },
            fx_name: fx_info.effect_name,
            preset_name: fx.preset_name().map(|n| n.into_string()),
            chunk: Rc::new(fx.tag_chunk()?.content().to_owned()),
        };
        Ok(fx_snapshot)
    }

    #[must_use]
    pub fn invalidate_fx_index(
        &mut self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Option<Affected<TargetProp>> {
        if !self.supports_fx() {
            return None;
        }
        if let Ok(actual_fx) = self.with_context(context, compartment).first_fx() {
            let new_virtual_fx = match self.virtual_fx() {
                Some(virtual_fx) => {
                    match virtual_fx {
                        VirtualFx::ChainFx {
                            is_input_fx,
                            chain_fx: anchor,
                        } => match anchor {
                            VirtualChainFx::ByIdOrIndex(guid, _) => Some(VirtualFx::ChainFx {
                                is_input_fx,
                                chain_fx: VirtualChainFx::ByIdOrIndex(guid, actual_fx.index()),
                            }),
                            _ => None,
                        },
                        // No update necessary
                        VirtualFx::Focused | VirtualFx::This => None,
                    }
                }
                // Shouldn't happen
                None => None,
            };
            if let Some(virtual_fx) = new_virtual_fx {
                self.set_virtual_fx(virtual_fx, context, compartment)
            } else {
                None
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn set_virtual_track(
        &mut self,
        track: VirtualTrack,
        context: Option<&ProcessorContext>,
    ) -> Option<Affected<TargetProp>> {
        self.set_track_from_prop_values(TrackPropValues::from_virtual_track(track), true, context)
    }

    /// Sets the track type and in certain cases also updates a few other target properties.
    #[must_use]
    pub fn set_track_type_from_ui(
        &mut self,
        track_type: VirtualTrackType,
        context: &ProcessorContext,
    ) -> Option<Affected<TargetProp>> {
        use VirtualTrackType::*;
        match track_type {
            This => {
                self.set_concrete_track(ConcreteTrackInstruction::This(Some(context)), true, false)
            }
            ById => self.set_concrete_track(
                ConcreteTrackInstruction::ById {
                    id: None,
                    context: Some(context),
                },
                true,
                false,
            ),
            _ => {
                self.track_type = track_type;
                Some(Affected::One(TargetProp::TrackType))
            }
        }
    }

    #[must_use]
    pub fn set_fx_type_from_ui(
        &mut self,
        fx_type: VirtualFxType,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Option<Affected<TargetProp>> {
        use VirtualFxType::*;
        match fx_type {
            This => self.set_concrete_fx(
                ConcreteFxInstruction::This(Some(context.context())),
                true,
                false,
            ),
            ById => self.set_concrete_fx(
                ConcreteFxInstruction::ById {
                    is_input_fx: None,
                    id: None,
                    track: self
                        .with_context(context, compartment)
                        .first_effective_track()
                        .ok(),
                },
                true,
                false,
            ),
            _ => {
                self.fx_type = fx_type;
                Some(Affected::One(TargetProp::FxType))
            }
        }
    }

    #[must_use]
    pub fn set_track_from_prop_values(
        &mut self,
        track: TrackPropValues,
        with_notification: bool,
        context: Option<&ProcessorContext>,
    ) -> Option<Affected<TargetProp>> {
        self.track_type = track.r#type;
        self.track_expression = track.expression;
        use VirtualTrackType::*;
        match track.r#type {
            This => self.set_concrete_track(
                ConcreteTrackInstruction::This(context),
                // Already notified above
                false,
                with_notification,
            ),
            ById => self.set_concrete_track(
                ConcreteTrackInstruction::ById {
                    id: track.id,
                    context,
                },
                // Already notified above
                false,
                with_notification,
            ),
            ByName | AllByName => {
                self.track_name = track.name;
                Some(Affected::One(TargetProp::TrackName))
            }
            ByIndex => {
                self.track_index = track.index;
                Some(Affected::One(TargetProp::TrackIndex))
            }
            ByIdOrName => {
                self.track_id = track.id;
                self.track_name = track.name;
                Some(Affected::Multiple)
            }
            FromClipColumn => {
                self.clip_column = track.clip_column;
                self.clip_column_track_context = track.clip_column_track_context;
                Some(Affected::Multiple)
            }
            Selected | AllSelected | Dynamic | Master => None,
        }
    }

    #[must_use]
    pub fn set_virtual_route(&mut self, route: VirtualTrackRoute) -> Option<Affected<TargetProp>> {
        self.set_route(TrackRoutePropValues::from_virtual_route(route))
    }

    #[must_use]
    pub fn set_route(&mut self, route: TrackRoutePropValues) -> Option<Affected<TargetProp>> {
        self.route_selector_type = route.selector_type;
        self.route_type = route.r#type;
        self.route_id = route.id;
        self.route_name = route.name;
        self.route_index = route.index;
        self.route_expression = route.expression;
        Some(Affected::Multiple)
    }

    #[must_use]
    pub fn set_virtual_fx(
        &mut self,
        fx: VirtualFx,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Option<Affected<TargetProp>> {
        self.set_fx_from_prop_values(
            FxPropValues::from_virtual_fx(fx),
            true,
            Some(context),
            compartment,
        )
    }

    #[must_use]
    pub fn set_fx_from_prop_values(
        &mut self,
        fx: FxPropValues,
        with_notification: bool,
        context: Option<ExtendedProcessorContext>,
        compartment: Compartment,
    ) -> Option<Affected<TargetProp>> {
        self.fx_type = fx.r#type;
        self.fx_expression = fx.expression;
        self.fx_is_input_fx = fx.is_input_fx;
        use VirtualFxType::*;
        match fx.r#type {
            This => {
                let _ = self.set_concrete_fx(
                    ConcreteFxInstruction::This(context.map(|c| c.context())),
                    // Already notified above
                    false,
                    with_notification,
                );
            }
            ById => {
                let _ = self.set_concrete_fx(
                    ConcreteFxInstruction::ById {
                        is_input_fx: Some(fx.is_input_fx),
                        id: fx.id,
                        track: context.and_then(|c| {
                            self.with_context(c, compartment)
                                .first_effective_track()
                                .ok()
                        }),
                    },
                    // Already notified above
                    false,
                    with_notification,
                );
            }
            ByName | AllByName => {
                self.fx_name = fx.name;
            }
            ByIndex => {
                self.fx_index = fx.index;
            }
            ByIdOrIndex => {
                self.fx_id = fx.id;
                self.fx_index = fx.index;
            }
            Dynamic | Focused => {}
        };
        Some(Affected::Multiple)
    }

    #[must_use]
    pub fn set_fx_parameter(
        &mut self,
        param: FxParameterPropValues,
    ) -> Option<Affected<TargetProp>> {
        self.param_type = param.r#type;
        self.param_name = param.name;
        self.param_index = param.index;
        self.param_expression = param.expression;
        Some(Affected::Multiple)
    }

    #[must_use]
    pub fn set_seek_options(&mut self, options: SeekOptions) -> Option<Affected<TargetProp>> {
        self.use_time_selection = options.use_time_selection;
        self.use_loop_points = options.use_loop_points;
        self.use_regions = options.use_regions;
        self.use_project = options.use_project;
        self.move_view = options.move_view;
        self.seek_play = options.seek_play;
        self.feedback_resolution = options.feedback_resolution;
        Some(Affected::Multiple)
    }

    /// Sets the track to one of the concrete types ById or This, also setting other important
    /// properties for UI convenience.
    #[must_use]
    pub fn set_concrete_track(
        &mut self,
        instruction: ConcreteTrackInstruction,
        notify_about_type_change: bool,
        notify_about_id_change: bool,
    ) -> Option<Affected<TargetProp>> {
        let resolved = instruction.resolve();
        self.track_type = resolved.virtual_track_type();
        if let Some(id) = resolved.id() {
            self.track_id = Some(id);
        }
        // We also set index and name so that we can easily switch between types.
        if let Some(i) = resolved.index() {
            self.track_index = i;
        }
        if let Some(name) = resolved.name() {
            self.track_name = name;
        }
        if !notify_about_type_change && !notify_about_id_change {
            None
        } else if !notify_about_type_change && notify_about_id_change {
            Some(Affected::One(TargetProp::TrackId))
        } else if notify_about_type_change && !notify_about_id_change {
            Some(Affected::One(TargetProp::TrackType))
        } else {
            Some(Affected::Multiple)
        }
    }

    /// Sets the FX to one of the concrete types (ById only for now), also setting other important
    /// properties for UI convenience.
    pub fn set_concrete_fx(
        &mut self,
        instruction: ConcreteFxInstruction,
        notify_about_type_and_input_fx_change: bool,
        notify_about_id_change: bool,
    ) -> Option<Affected<TargetProp>> {
        let resolved = instruction.resolve();
        self.fx_type = resolved.virtual_fx_type();
        if let Some(is_input_fx) = resolved.is_input_fx() {
            self.fx_is_input_fx = is_input_fx;
        }
        if let Some(id) = resolved.id() {
            self.fx_id = Some(id);
        }
        // We also set index and name so that we can easily switch between types.
        if let Some(i) = resolved.index() {
            self.fx_index = i;
        }
        if let Some(name) = resolved.name() {
            self.fx_name = name;
        }
        if notify_about_type_and_input_fx_change {
            Some(Affected::Multiple)
        } else if notify_about_id_change {
            Some(Affected::One(TargetProp::FxId))
        } else {
            None
        }
    }

    pub fn seek_options(&self) -> SeekOptions {
        SeekOptions {
            use_time_selection: self.use_time_selection,
            use_loop_points: self.use_loop_points,
            use_regions: self.use_regions,
            use_project: self.use_project,
            move_view: self.move_view,
            seek_play: self.seek_play,
            feedback_resolution: self.feedback_resolution,
        }
    }

    #[must_use]
    pub fn apply_from_target(
        &mut self,
        target: &ReaperTarget,
        extended_context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Option<Affected<TargetProp>> {
        let context = extended_context.context();
        use ReaperTarget::*;
        self.category = TargetCategory::Reaper;
        self.r#type = ReaperTargetType::from_target(target);
        if let Some(actual_fx) = target.fx() {
            let virtual_fx = virtualize_fx(actual_fx, context, true);
            let _ = self.set_virtual_fx(virtual_fx, extended_context, compartment);
            let track = if let Some(track) = actual_fx.track() {
                track.clone()
            } else {
                // Must be monitoring FX. In this case we want the master track (it's REAPER's
                // convention and ours).
                context.project_or_current_project().master_track()
            };
            let _ = self.set_virtual_track(virtualize_track(&track, context, true), Some(context));
        } else if let Some(track) = target.track() {
            let _ = self.set_virtual_track(virtualize_track(track, context, true), Some(context));
        }
        if let Some(send) = target.route() {
            let virtual_route = virtualize_route(send, context, true);
            let _ = self.set_virtual_route(virtual_route);
        }
        if let Some(track_exclusivity) = target.track_exclusivity() {
            self.track_exclusivity = track_exclusivity;
        }
        match target {
            Action(t) => {
                self.action = Some(t.action.clone());
                self.action_invocation_type = t.invocation_type;
            }
            FxParameter(t) => {
                self.param_type = VirtualFxParameterType::ById;
                self.param_index = t.param.index();
            }
            Transport(t) => {
                self.transport_action = t.action;
            }
            TrackSolo(t) => {
                self.solo_behavior = t.behavior;
            }
            GoToBookmark(t) => {
                self.bookmark_ref = t.index;
                self.bookmark_type = t.bookmark_type;
            }
            TrackAutomationMode(t) => {
                self.automation_mode = RealearnAutomationMode::from_reaper(t.mode);
            }
            TrackMonitoringMode(t) => {
                self.monitoring_mode = convert_monitoring_mode_to_realearn(t.mode);
            }
            RouteAutomationMode(t) => {
                self.automation_mode = RealearnAutomationMode::from_reaper(t.mode);
            }
            AutomationModeOverride(t) => match t.mode_override {
                None => {
                    self.automation_mode_override_type = AutomationModeOverrideType::None;
                }
                Some(GlobalAutomationModeOverride::Bypass) => {
                    self.automation_mode_override_type = AutomationModeOverrideType::Bypass;
                }
                Some(GlobalAutomationModeOverride::Mode(am)) => {
                    self.automation_mode_override_type = AutomationModeOverrideType::Override;
                    self.automation_mode = RealearnAutomationMode::from_reaper(am);
                }
            },
            _ => {}
        };
        Some(Affected::Multiple)
    }

    pub fn virtual_default(
        control_element_type: VirtualControlElementType,
        next_index: u32,
    ) -> Self {
        TargetModel {
            category: TargetCategory::Virtual,
            control_element_type,
            control_element_id: VirtualControlElementId::Indexed(next_index),
            ..Default::default()
        }
    }

    pub fn default_for_compartment(compartment: Compartment) -> Self {
        use Compartment::*;
        TargetModel {
            category: match compartment {
                Controller => TargetCategory::Virtual,
                Main => TargetCategory::Reaper,
            },
            ..Default::default()
        }
    }

    pub fn virtual_track(&self) -> Option<VirtualTrack> {
        use VirtualTrackType::*;
        let track = match self.track_type {
            This => VirtualTrack::This,
            Selected => VirtualTrack::Selected {
                allow_multiple: false,
            },
            AllSelected => VirtualTrack::Selected {
                allow_multiple: true,
            },
            Master => VirtualTrack::Master,
            ById => VirtualTrack::ById(self.track_id?),
            ByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(&self.track_name),
                allow_multiple: false,
            },
            AllByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(&self.track_name),
                allow_multiple: true,
            },
            ByIndex => VirtualTrack::ByIndex(self.track_index),
            ByIdOrName => {
                VirtualTrack::ByIdOrName(self.track_id?, WildMatch::new(&self.track_name))
            }
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(&self.track_expression).ok()?;
                VirtualTrack::Dynamic(Box::new(evaluator))
            }
            FromClipColumn => VirtualTrack::FromClipColumn {
                column: self.virtual_clip_column().ok()?,
                context: self.clip_column_track_context,
            },
        };
        Some(track)
    }

    pub fn track(&self) -> TrackPropValues {
        TrackPropValues {
            r#type: self.track_type,
            id: self.track_id,
            name: self.track_name.clone(),
            expression: self.track_expression.clone(),
            index: self.track_index,
            clip_column: self.clip_column.clone(),
            clip_column_track_context: self.clip_column_track_context,
        }
    }

    pub fn virtual_fx(&self) -> Option<VirtualFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type {
            Focused => VirtualFx::Focused,
            This => VirtualFx::This,
            _ => VirtualFx::ChainFx {
                is_input_fx: self.fx_is_input_fx,
                chain_fx: self.virtual_chain_fx()?,
            },
        };
        Some(fx)
    }

    pub fn track_route_selector(&self) -> Option<TrackRouteSelector> {
        use TrackRouteSelectorType::*;
        let selector = match self.route_selector_type {
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(&self.route_expression).ok()?;
                TrackRouteSelector::Dynamic(Box::new(evaluator))
            }
            ById => {
                if self.route_type == TrackRouteType::HardwareOutput {
                    // Hardware outputs don't offer stable IDs.
                    TrackRouteSelector::ByIndex(self.route_index)
                } else {
                    TrackRouteSelector::ById(self.route_id?)
                }
            }
            ByName => TrackRouteSelector::ByName(WildMatch::new(&self.route_name)),
            ByIndex => TrackRouteSelector::ByIndex(self.route_index),
        };
        Some(selector)
    }

    pub fn virtual_chain_fx(&self) -> Option<VirtualChainFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type {
            Focused | This => return None,
            ById => VirtualChainFx::ById(self.fx_id?, Some(self.fx_index)),
            ByName => VirtualChainFx::ByName {
                wild_match: WildMatch::new(&self.fx_name),
                allow_multiple: false,
            },
            AllByName => VirtualChainFx::ByName {
                wild_match: WildMatch::new(&self.fx_name),
                allow_multiple: true,
            },
            ByIndex => VirtualChainFx::ByIndex(self.fx_index),
            ByIdOrIndex => VirtualChainFx::ByIdOrIndex(self.fx_id, self.fx_index),
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(&self.fx_expression).ok()?;
                VirtualChainFx::Dynamic(Box::new(evaluator))
            }
        };
        Some(fx)
    }

    pub fn fx(&self) -> FxPropValues {
        FxPropValues {
            r#type: self.fx_type,
            is_input_fx: self.fx_is_input_fx,
            id: self.fx_id,
            name: self.fx_name.clone(),
            expression: self.fx_expression.clone(),
            index: self.fx_index,
        }
    }

    pub fn track_route(&self) -> TrackRoutePropValues {
        TrackRoutePropValues {
            selector_type: self.route_selector_type,
            r#type: self.route_type,
            id: self.route_id,
            name: self.route_name.clone(),
            expression: self.route_expression.clone(),
            index: self.route_index,
        }
    }

    pub fn fx_parameter(&self) -> FxParameterPropValues {
        FxParameterPropValues {
            r#type: self.param_type,
            name: self.param_name.clone(),
            expression: self.param_expression.clone(),
            index: self.param_index,
        }
    }

    pub fn track_descriptor(&self) -> Result<TrackDescriptor, &'static str> {
        let desc = TrackDescriptor {
            track: self.virtual_track().ok_or("virtual track not complete")?,
            enable_only_if_track_selected: self.enable_only_if_track_selected,
        };
        Ok(desc)
    }

    fn virtual_clip_slot(&self) -> Result<VirtualClipSlot, &'static str> {
        use ClipSlotDescriptor::*;
        let slot = match &self.clip_slot {
            Selected => VirtualClipSlot::Selected,
            ByIndex {
                column_index,
                row_index,
            } => VirtualClipSlot::ByIndex {
                column_index: *column_index,
                row_index: *row_index,
            },
            Dynamic {
                column_expression,
                row_expression,
            } => {
                let column_evaluator = ExpressionEvaluator::compile(column_expression)
                    .map_err(|_| "couldn't evaluate row")?;
                let row_evaluator = ExpressionEvaluator::compile(row_expression)
                    .map_err(|_| "couldn't evaluate row")?;
                VirtualClipSlot::Dynamic {
                    column_evaluator: Box::new(column_evaluator),
                    row_evaluator: Box::new(row_evaluator),
                }
            }
        };
        Ok(slot)
    }

    fn virtual_clip_column(&self) -> Result<VirtualClipColumn, &'static str> {
        use ClipColumnDescriptor::*;
        let column = match &self.clip_column {
            Selected => VirtualClipColumn::Selected,
            ByIndex { index } => VirtualClipColumn::ByIndex(*index),
            Dynamic {
                expression: index_expression,
            } => {
                let index_evaluator = ExpressionEvaluator::compile(index_expression)
                    .map_err(|_| "couldn't evaluate column index")?;
                VirtualClipColumn::Dynamic(Box::new(index_evaluator))
            }
        };
        Ok(column)
    }

    fn virtual_clip_row(&self) -> Result<VirtualClipRow, &'static str> {
        use ClipRowDescriptor::*;
        let row = match &self.clip_row {
            Selected => VirtualClipRow::Selected,
            ByIndex { index } => VirtualClipRow::ByIndex(*index),
            Dynamic {
                expression: index_expression,
            } => {
                let index_evaluator = ExpressionEvaluator::compile(index_expression)
                    .map_err(|_| "couldn't evaluate row index")?;
                VirtualClipRow::Dynamic(Box::new(index_evaluator))
            }
        };
        Ok(row)
    }

    pub fn fx_descriptor(&self) -> Result<FxDescriptor, &'static str> {
        let desc = FxDescriptor {
            track_descriptor: self.track_descriptor()?,
            enable_only_if_fx_has_focus: self.enable_only_if_fx_has_focus,
            fx: self.virtual_fx().ok_or("FX not set")?,
        };
        Ok(desc)
    }

    pub fn route_descriptor(&self) -> Result<TrackRouteDescriptor, &'static str> {
        let desc = TrackRouteDescriptor {
            track_descriptor: self.track_descriptor()?,
            route: self.virtual_track_route()?,
        };
        Ok(desc)
    }

    pub fn virtual_track_route(&self) -> Result<VirtualTrackRoute, &'static str> {
        let route = VirtualTrackRoute {
            r#type: self.route_type,
            selector: self.track_route_selector().ok_or("track route not set")?,
        };
        Ok(route)
    }

    pub fn virtual_fx_parameter(&self) -> Option<VirtualFxParameter> {
        use VirtualFxParameterType::*;
        let param = match self.param_type {
            ByName => VirtualFxParameter::ByName(WildMatch::new(&self.param_name)),
            ById => VirtualFxParameter::ById(self.param_index),
            ByIndex => VirtualFxParameter::ByIndex(self.param_index),
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(&self.param_expression).ok()?;
                VirtualFxParameter::Dynamic(Box::new(evaluator))
            }
        };
        Some(param)
    }

    fn fx_parameter_descriptor(&self) -> Result<FxParameterDescriptor, &'static str> {
        let desc = FxParameterDescriptor {
            fx_descriptor: self.fx_descriptor()?,
            fx_parameter: self.virtual_fx_parameter().ok_or("FX parameter not set")?,
        };
        Ok(desc)
    }

    pub fn create_target(
        &self,
        compartment: Compartment,
    ) -> Result<UnresolvedCompoundMappingTarget, &'static str> {
        use TargetCategory::*;
        match self.category {
            Reaper => {
                use ReaperTargetType::*;
                let target = match self.r#type {
                    Action => UnresolvedReaperTarget::Action(UnresolvedActionTarget {
                        action: self.resolved_action()?,
                        invocation_type: self.action_invocation_type,
                        track_descriptor: if self.with_track {
                            Some(self.track_descriptor()?)
                        } else {
                            None
                        },
                    }),
                    FxParameterValue => {
                        UnresolvedReaperTarget::FxParameter(UnresolvedFxParameterTarget {
                            fx_parameter_descriptor: self.fx_parameter_descriptor()?,
                            poll_for_feedback: self.poll_for_feedback,
                        })
                    }
                    FxParameterTouchState => UnresolvedReaperTarget::FxParameterTouchState(
                        UnresolvedFxParameterTouchStateTarget {
                            fx_parameter_descriptor: self.fx_parameter_descriptor()?,
                        },
                    ),
                    TrackVolume => {
                        UnresolvedReaperTarget::TrackVolume(UnresolvedTrackVolumeTarget {
                            track_descriptor: self.track_descriptor()?,
                        })
                    }
                    TrackTool => UnresolvedReaperTarget::TrackTool(UnresolvedTrackToolTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackPeak => UnresolvedReaperTarget::TrackPeak(UnresolvedTrackPeakTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    RouteVolume => {
                        UnresolvedReaperTarget::TrackSendVolume(UnresolvedRouteVolumeTarget {
                            descriptor: self.route_descriptor()?,
                        })
                    }
                    TrackPan => UnresolvedReaperTarget::TrackPan(UnresolvedTrackPanTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackWidth => UnresolvedReaperTarget::TrackWidth(UnresolvedTrackWidthTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackArm => UnresolvedReaperTarget::TrackArm(UnresolvedTrackArmTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                    }),
                    TrackSelection => {
                        UnresolvedReaperTarget::TrackSelection(UnresolvedTrackSelectionTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity,
                            scroll_arrange_view: self.scroll_arrange_view,
                            scroll_mixer: self.scroll_mixer,
                        })
                    }
                    TrackMute => UnresolvedReaperTarget::TrackMute(UnresolvedTrackMuteTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                    }),
                    TrackPhase => UnresolvedReaperTarget::TrackPhase(UnresolvedTrackPhaseTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    TrackShow => UnresolvedReaperTarget::TrackShow(UnresolvedTrackShowTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                        area: match self.track_area {
                            RealearnTrackArea::Tcp => TrackArea::Tcp,
                            RealearnTrackArea::Mcp => TrackArea::Mcp,
                        },
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    TrackAutomationMode => UnresolvedReaperTarget::TrackAutomationMode(
                        UnresolvedTrackAutomationModeTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity,
                            mode: self.automation_mode.to_reaper(),
                        },
                    ),
                    TrackMonitoringMode => UnresolvedReaperTarget::TrackMonitoringMode(
                        UnresolvedTrackMonitoringModeTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity,
                            mode: convert_monitoring_mode_to_reaper(self.monitoring_mode),
                        },
                    ),
                    TrackSolo => UnresolvedReaperTarget::TrackSolo(UnresolvedTrackSoloTarget {
                        track_descriptor: self.track_descriptor()?,
                        behavior: self.solo_behavior,
                        exclusivity: self.track_exclusivity,
                    }),
                    RoutePan => UnresolvedReaperTarget::RoutePan(UnresolvedRoutePanTarget {
                        descriptor: self.route_descriptor()?,
                    }),
                    RouteMute => UnresolvedReaperTarget::RouteMute(UnresolvedRouteMuteTarget {
                        descriptor: self.route_descriptor()?,
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    RoutePhase => UnresolvedReaperTarget::RoutePhase(UnresolvedRoutePhaseTarget {
                        descriptor: self.route_descriptor()?,
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    RouteMono => UnresolvedReaperTarget::RouteMono(UnresolvedRouteMonoTarget {
                        descriptor: self.route_descriptor()?,
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    RouteAutomationMode => UnresolvedReaperTarget::RouteAutomationMode(
                        UnresolvedRouteAutomationModeTarget {
                            descriptor: self.route_descriptor()?,
                            mode: self.automation_mode.to_reaper(),
                            poll_for_feedback: self.poll_for_feedback,
                        },
                    ),
                    RouteTouchState => {
                        UnresolvedReaperTarget::RouteTouchState(UnresolvedRouteTouchStateTarget {
                            descriptor: self.route_descriptor()?,
                            parameter_type: self.touched_route_parameter_type,
                        })
                    }
                    Tempo => UnresolvedReaperTarget::Tempo(UnresolvedTempoTarget),
                    Playrate => UnresolvedReaperTarget::Playrate(UnresolvedPlayrateTarget),
                    AutomationModeOverride => UnresolvedReaperTarget::AutomationModeOverride(
                        UnresolvedAutomationModeOverrideTarget {
                            mode_override: match self.automation_mode_override_type {
                                AutomationModeOverrideType::Bypass => {
                                    Some(GlobalAutomationModeOverride::Bypass)
                                }
                                AutomationModeOverrideType::Override => {
                                    Some(GlobalAutomationModeOverride::Mode(
                                        self.automation_mode.to_reaper(),
                                    ))
                                }
                                AutomationModeOverrideType::None => None,
                            },
                        },
                    ),
                    FxEnable => UnresolvedReaperTarget::FxEnable(UnresolvedFxEnableTarget {
                        fx_descriptor: self.fx_descriptor()?,
                    }),
                    FxOnline => UnresolvedReaperTarget::FxOnline(UnresolvedFxOnlineTarget {
                        fx_descriptor: self.fx_descriptor()?,
                    }),
                    FxOpen => UnresolvedReaperTarget::FxOpen(UnresolvedFxOpenTarget {
                        fx_descriptor: self.fx_descriptor()?,
                        display_type: self.fx_display_type,
                    }),
                    FxPreset => UnresolvedReaperTarget::FxPreset(UnresolvedFxPresetTarget {
                        fx_descriptor: self.fx_descriptor()?,
                    }),
                    SelectedTrack => {
                        UnresolvedReaperTarget::SelectedTrack(UnresolvedSelectedTrackTarget {
                            scroll_arrange_view: self.scroll_arrange_view,
                            scroll_mixer: self.scroll_mixer,
                        })
                    }
                    FxNavigate => UnresolvedReaperTarget::FxNavigate(UnresolvedFxNavigateTarget {
                        track_descriptor: self.track_descriptor()?,
                        is_input_fx: self.fx_is_input_fx,
                        display_type: self.fx_display_type,
                    }),
                    AllTrackFxEnable => {
                        UnresolvedReaperTarget::AllTrackFxEnable(UnresolvedAllTrackFxEnableTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity,
                            poll_for_feedback: self.poll_for_feedback,
                        })
                    }
                    Transport => UnresolvedReaperTarget::Transport(UnresolvedTransportTarget {
                        action: self.transport_action,
                    }),
                    LoadFxSnapshot => {
                        UnresolvedReaperTarget::LoadFxPreset(UnresolvedLoadFxSnapshotTarget {
                            fx_descriptor: self.fx_descriptor()?,
                            chunk: self
                                .fx_snapshot
                                .as_ref()
                                .ok_or("FX chunk not set")?
                                .chunk
                                .clone(),
                        })
                    }
                    LastTouched => UnresolvedReaperTarget::LastTouched(UnresolvedLastTouchedTarget),
                    TrackTouchState => {
                        UnresolvedReaperTarget::TrackTouchState(UnresolvedTrackTouchStateTarget {
                            track_descriptor: self.track_descriptor()?,
                            parameter_type: self.touched_track_parameter_type,
                            exclusivity: self.track_exclusivity,
                        })
                    }
                    GoToBookmark => {
                        UnresolvedReaperTarget::GoToBookmark(UnresolvedGoToBookmarkTarget {
                            bookmark_type: self.bookmark_type,
                            bookmark_anchor_type: self.bookmark_anchor_type,
                            bookmark_ref: self.bookmark_ref,
                            set_time_selection: self.use_time_selection,
                            set_loop_points: self.use_loop_points,
                        })
                    }
                    Seek => UnresolvedReaperTarget::Seek(UnresolvedSeekTarget {
                        options: self.seek_options(),
                    }),
                    SendMidi => UnresolvedReaperTarget::SendMidi(UnresolvedMidiSendTarget {
                        pattern: self.raw_midi_pattern.parse().unwrap_or_default(),
                        destination: self.send_midi_destination,
                    }),
                    SendOsc => UnresolvedReaperTarget::SendOsc(UnresolvedOscSendTarget {
                        address_pattern: self.osc_address_pattern.clone(),
                        arg_descriptor: self.osc_arg_descriptor(),
                        device_id: self.osc_dev_id,
                    }),
                    ClipTransport => {
                        UnresolvedReaperTarget::ClipTransport(UnresolvedClipTransportTarget {
                            slot: self.virtual_clip_slot()?,
                            action: self.clip_transport_action,
                            options: self.clip_transport_options(),
                        })
                    }
                    ClipColumn => UnresolvedReaperTarget::ClipColumn(UnresolvedClipColumnTarget {
                        column: self.virtual_clip_column()?,
                        action: self.clip_column_action,
                    }),
                    ClipRow => UnresolvedReaperTarget::ClipRow(UnresolvedClipRowTarget {
                        row: self.virtual_clip_row()?,
                        action: self.clip_row_action,
                    }),
                    ClipSeek => UnresolvedReaperTarget::ClipSeek(UnresolvedClipSeekTarget {
                        slot: self.virtual_clip_slot()?,
                        feedback_resolution: self.feedback_resolution,
                    }),
                    ClipVolume => UnresolvedReaperTarget::ClipVolume(UnresolvedClipVolumeTarget {
                        slot: self.virtual_clip_slot()?,
                    }),
                    ClipManagement => {
                        UnresolvedReaperTarget::ClipManagement(UnresolvedClipManagementTarget {
                            slot: self.virtual_clip_slot()?,
                            action: self.clip_management_action.clone(),
                        })
                    }
                    ClipMatrix => UnresolvedReaperTarget::ClipMatrix(UnresolvedClipMatrixTarget {
                        action: self.clip_matrix_action,
                    }),
                    LoadMappingSnapshot => UnresolvedReaperTarget::LoadMappingSnapshot(
                        UnresolvedLoadMappingSnapshotTarget {
                            scope: TagScope {
                                tags: self.tags.iter().cloned().collect(),
                            },
                            active_mappings_only: self.active_mappings_only,
                        },
                    ),
                    EnableMappings => {
                        UnresolvedReaperTarget::EnableMappings(UnresolvedEnableMappingsTarget {
                            compartment,
                            scope: TagScope {
                                tags: self.tags.iter().cloned().collect(),
                            },
                            exclusivity: self.exclusivity,
                        })
                    }
                    EnableInstances => {
                        UnresolvedReaperTarget::EnableInstances(UnresolvedEnableInstancesTarget {
                            scope: TagScope {
                                tags: self.tags.iter().cloned().collect(),
                            },
                            exclusivity: self.exclusivity,
                        })
                    }
                    NavigateWithinGroup => UnresolvedReaperTarget::NavigateWithinGroup(
                        UnresolvedNavigateWithinGroupTarget {
                            compartment,
                            group_id: self.group_id,
                            exclusivity: self.exclusivity.into(),
                        },
                    ),
                    AnyOn => UnresolvedReaperTarget::AnyOn(UnresolvedAnyOnTarget {
                        parameter: self.any_on_parameter,
                    }),
                };
                Ok(UnresolvedCompoundMappingTarget::Reaper(target))
            }
            Virtual => {
                let virtual_target = VirtualTarget::new(self.create_control_element());
                Ok(UnresolvedCompoundMappingTarget::Virtual(virtual_target))
            }
        }
    }

    pub fn clip_slot(&self) -> &ClipSlotDescriptor {
        &self.clip_slot
    }

    pub fn clip_column(&self) -> &ClipColumnDescriptor {
        &self.clip_column
    }

    pub fn clip_row(&self) -> &ClipRowDescriptor {
        &self.clip_row
    }

    pub fn clip_transport_action(&self) -> ClipTransportAction {
        self.clip_transport_action
    }

    pub fn clip_matrix_action(&self) -> ClipMatrixAction {
        self.clip_matrix_action
    }

    pub fn clip_column_action(&self) -> ClipColumnAction {
        self.clip_column_action
    }

    pub fn clip_row_action(&self) -> ClipRowAction {
        self.clip_row_action
    }

    pub fn record_only_if_track_armed(&self) -> bool {
        self.record_only_if_track_armed
    }

    pub fn stop_column_if_slot_empty(&self) -> bool {
        self.stop_column_if_slot_empty
    }

    pub fn clip_play_start_timing(&self) -> Option<ClipPlayStartTiming> {
        self.clip_play_start_timing
    }

    pub fn clip_play_stop_timing(&self) -> Option<ClipPlayStopTiming> {
        self.clip_play_stop_timing
    }

    pub fn clip_transport_options(&self) -> ClipTransportOptions {
        ClipTransportOptions {
            record_only_if_track_armed: self.record_only_if_track_armed,
            stop_column_if_slot_empty: self.stop_column_if_slot_empty,
            play_start_timing: self.clip_play_start_timing,
            play_stop_timing: self.clip_play_stop_timing,
        }
    }

    fn osc_arg_descriptor(&self) -> Option<OscArgDescriptor> {
        let arg_index = self.osc_arg_index?;
        Some(OscArgDescriptor::new(
            arg_index,
            self.osc_arg_type_tag,
            // Doesn't matter for sending
            false,
            self.osc_arg_value_range,
        ))
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
        compartment: Compartment,
    ) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
            compartment,
        }
    }

    pub fn supports_track(&self) -> bool {
        let target_type = self.r#type;
        if !target_type.supports_track() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    pub fn supports_track_must_be_selected(&self) -> bool {
        if !self.r#type.supports_track_must_be_selected() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    pub fn supports_osc_arg_value_range(&self) -> bool {
        self.category == TargetCategory::Reaper
            && self.osc_arg_index.is_some()
            && self.osc_arg_type_tag.supports_value_range()
    }

    fn supports_track_apart_from_type(&self) -> bool {
        match self.r#type {
            ReaperTargetType::ClipTransport => {
                use TransportAction::*;
                matches!(self.transport_action, PlayStop | PlayPause)
            }
            ReaperTargetType::Action => self.with_track,
            _ => true,
        }
    }

    pub fn supports_fx(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.supports_fx()
    }

    pub fn supports_route(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.supports_send()
    }

    pub fn supports_automation_mode(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        use ReaperTargetType::*;
        match self.r#type {
            TrackAutomationMode | RouteAutomationMode => true,
            AutomationModeOverride => {
                self.automation_mode_override_type == AutomationModeOverrideType::Override
            }
            _ => false,
        }
    }

    pub fn create_control_element(&self) -> VirtualControlElement {
        self.control_element_type
            .create_control_element(self.control_element_id)
    }

    fn is_reaper(&self) -> bool {
        self.category == TargetCategory::Reaper
    }

    pub fn is_virtual(&self) -> bool {
        self.category == TargetCategory::Virtual
    }

    fn command_id_label(&self) -> Cow<str> {
        match self.action.as_ref() {
            None => "-".into(),
            Some(action) => {
                if action.is_available() {
                    action.command_id().to_string().into()
                } else if let Some(command_name) = action.command_name() {
                    format!("<Not present> ({})", command_name.to_str()).into()
                } else {
                    "<Not present>".into()
                }
            }
        }
    }

    pub fn resolved_action(&self) -> Result<Action, &'static str> {
        let action = self.action.as_ref().ok_or("action not set")?;
        if !action.is_available() {
            return Err("action not available");
        }
        Ok(action.clone())
    }

    pub fn action_name_label(&self) -> Cow<str> {
        match self.resolved_action().ok() {
            None => "-".into(),
            Some(a) => a.name().into_string().into(),
        }
    }
}

pub struct TargetModelFormatVeryShort<'a>(pub &'a TargetModel);

impl<'a> Display for TargetModelFormatVeryShort<'a> {
    /// Produces a short single-line name which is for example used to derive the automatic name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.category {
            TargetCategory::Reaper => {
                use ReaperTargetType::*;
                let tt = self.0.r#type;
                match tt {
                    Action => match self.0.resolved_action().ok() {
                        None => write!(f, "Action {}", self.0.command_id_label()),
                        Some(a) => f.write_str(a.name().to_str()),
                    },
                    AutomationModeOverride => {
                        write!(f, "{}: ", tt.short_name())?;
                        use AutomationModeOverrideType::*;
                        let ovr_type = self.0.automation_mode_override_type;
                        match ovr_type {
                            None | Bypass => write!(f, "{}", ovr_type),
                            Override => write!(f, "{}", self.0.automation_mode),
                        }
                    }
                    Transport => {
                        write!(f, "{}", self.0.transport_action)
                    }
                    AnyOn => {
                        write!(f, "{}", self.0.any_on_parameter)
                    }
                    GoToBookmark => {
                        let type_label = match self.0.bookmark_type {
                            BookmarkType::Marker => "Marker",
                            BookmarkType::Region => "Region",
                        };
                        let bm_prefix = match self.0.bookmark_anchor_type {
                            BookmarkAnchorType::Id => "",
                            BookmarkAnchorType::Index => "#",
                        };
                        write!(
                            f,
                            "Go to {} {}{}",
                            type_label, bm_prefix, self.0.bookmark_ref
                        )
                    }
                    TrackAutomationMode => {
                        write!(f, "{}: {}", tt.short_name(), self.0.automation_mode)
                    }
                    TrackTouchState => {
                        write!(
                            f,
                            "{}: {}",
                            tt.short_name(),
                            self.0.touched_track_parameter_type
                        )
                    }
                    _ => f.write_str(tt.short_name()),
                }
            }
            TargetCategory::Virtual => match self.0.control_element_id {
                VirtualControlElementId::Indexed(i) => {
                    write!(f, "{} {}", self.0.control_element_type, i + 1)
                }
                VirtualControlElementId::Named(n) => {
                    write!(f, "{} ({})", n, self.0.control_element_type)
                }
            },
        }
    }
}

pub struct TargetModelFormatMultiLine<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: Compartment,
}

impl<'a> TargetModelFormatMultiLine<'a> {
    pub fn new(
        target: &'a TargetModel,
        context: ExtendedProcessorContext<'a>,
        compartment: Compartment,
    ) -> Self {
        TargetModelFormatMultiLine {
            target,
            context,
            compartment,
        }
    }

    fn track_label(&self) -> String {
        let virtual_track = self.target.virtual_track();
        let virtual_track = match virtual_track.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(t) => t,
        };
        use VirtualTrack::*;
        match virtual_track {
            ById(_) | ByIdOrName(_, _) => {
                if let Ok(t) = self.target_with_context().first_effective_track() {
                    get_track_label(&t)
                } else {
                    get_non_present_virtual_track_label(virtual_track)
                }
            }
            _ => virtual_track.to_string(),
        }
    }

    fn route_label(&self) -> Cow<str> {
        let virtual_route = self.target.virtual_track_route().ok();
        let virtual_route = match virtual_route.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(r) => r,
        };
        use TrackRouteSelector::*;
        match &virtual_route.selector {
            ById(_) => {
                if let Ok(r) = self.resolve_first_track_route() {
                    get_route_label(&r).into()
                } else {
                    get_non_present_virtual_route_label(virtual_route).into()
                }
            }
            _ => virtual_route.to_string().into(),
        }
    }

    fn fx_label(&self) -> Cow<str> {
        let virtual_fx = self.target.virtual_fx();
        let virtual_fx = match virtual_fx.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(f) => f,
        };
        match virtual_fx {
            VirtualFx::ChainFx { chain_fx, .. } => {
                use VirtualChainFx::*;
                match chain_fx {
                    ById(_, _) | ByIdOrIndex(_, _) => get_optional_fx_label(
                        chain_fx,
                        self.target_with_context().first_fx().ok().as_ref(),
                    )
                    .into(),
                    _ => virtual_fx.to_string().into(),
                }
            }
            _ => virtual_fx.to_string().into(),
        }
    }

    fn fx_param_label(&self) -> Cow<str> {
        let virtual_param = self.target.virtual_fx_parameter();
        let virtual_param = match virtual_param.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(p) => p,
        };
        use VirtualFxParameter::*;
        match virtual_param {
            ById(_) => {
                if let Ok(p) = self.resolve_first_fx_param() {
                    get_fx_param_label(Some(&p), p.index())
                } else {
                    format!("<Not present> ({})", virtual_param).into()
                }
            }
            _ => virtual_param.to_string().into(),
        }
    }

    fn bookmark_label(&self) -> String {
        // TODO-medium We should do this similar to the other target objects and introduce a
        //  virtual struct.
        let bookmark_type = self.target.bookmark_type;
        {
            let anchor_type = self.target.bookmark_anchor_type;
            let bookmark_ref = self.target.bookmark_ref;
            let res = find_bookmark(
                self.context.context().project_or_current_project(),
                bookmark_type,
                anchor_type,
                bookmark_ref,
            );
            if let Ok(res) = res {
                get_bookmark_label(
                    res.index_within_type,
                    res.basic_info.id,
                    &res.bookmark.name(),
                )
            } else {
                get_non_present_bookmark_label(anchor_type, bookmark_ref)
            }
        }
    }

    // Returns an error if that send (or track) doesn't exist.
    pub fn resolve_first_track_route(&self) -> Result<TrackRoute, &'static str> {
        let routes = get_track_routes(
            self.context,
            &self.target.route_descriptor()?,
            self.compartment,
        )?;
        routes.into_iter().next().ok_or("empty list of routes")
    }

    // Returns an error if that param (or FX) doesn't exist.
    fn resolve_first_fx_param(&self) -> Result<FxParameter, &'static str> {
        let params = get_fx_params(
            self.context,
            &self.target.fx_parameter_descriptor()?,
            self.compartment,
        )?;
        params.into_iter().next().ok_or("empty FX param list")
    }

    fn target_with_context(&self) -> TargetModelWithContext<'a> {
        self.target.with_context(self.context, self.compartment)
    }
}

impl<'a> Display for TargetModelFormatMultiLine<'a> {
    /// Produces a multi-line description of the target.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetCategory::*;
        match self.target.category {
            Reaper => {
                use ReaperTargetType::*;
                let tt = self.target.r#type;
                match tt {
                    ClipTransport | ClipSeek | ClipVolume => {
                        write!(f, "{}", tt)
                    }
                    Action => write!(
                        f,
                        "{}\n{}\n{}",
                        tt,
                        self.target.command_id_label(),
                        self.target.action_name_label()
                    ),
                    FxParameterValue => write!(
                        f,
                        "{}\nTrack {}\nFX {}\nParam {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                        self.fx_param_label()
                    ),
                    TrackTool | TrackVolume | TrackPeak | TrackPan | TrackWidth | TrackArm
                    | TrackSelection | TrackMute | TrackPhase | TrackSolo | TrackShow
                    | FxNavigate | AllTrackFxEnable => {
                        write!(f, "{}\nTrack {}", tt, self.track_label())
                    }
                    TrackAutomationMode => {
                        write!(
                            f,
                            "{}\nTrack {}\n{}",
                            tt,
                            self.track_label(),
                            self.target.automation_mode
                        )
                    }
                    RouteVolume | RoutePan | RouteMute | RoutePhase | RouteMono
                    | RouteAutomationMode => write!(
                        f,
                        "{}\nTrack {}\n{} {}",
                        tt,
                        self.track_label(),
                        self.target.route_type,
                        self.route_label()
                    ),
                    FxOpen | FxEnable | FxPreset => write!(
                        f,
                        "{}\nTrack {}\nFX {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                    ),
                    Transport => write!(f, "{}\n{}", tt, self.target.transport_action),
                    AnyOn => write!(f, "{}\n{}", tt, self.target.any_on_parameter),
                    AutomationModeOverride => {
                        write!(f, "{}\n{}", tt, self.target.automation_mode_override_type)
                    }
                    LoadFxSnapshot => write!(
                        f,
                        "{}\n{}",
                        tt,
                        self.target
                            .fx_snapshot
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ),
                    TrackTouchState => write!(
                        f,
                        "{}\nTrack {}\n{}",
                        tt,
                        self.track_label(),
                        self.target.touched_track_parameter_type
                    ),
                    GoToBookmark => {
                        write!(f, "{}\n{}", tt, self.bookmark_label())
                    }
                    _ => write!(f, "{}", tt),
                }
            }
            Virtual => write!(f, "Virtual\n{}", self.target.create_control_element()),
        }
    }
}

pub fn get_fx_param_label(fx_param: Option<&FxParameter>, index: u32) -> Cow<'static, str> {
    let position = index + 1;
    match fx_param {
        None => format!("{}. <Not present>", position).into(),
        Some(p) => {
            let name = p.name().into_inner();
            // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
            let name = name.to_string_lossy();
            if name.is_empty() {
                position.to_string().into()
            } else {
                format!("{}. {}", position, name).into()
            }
        }
    }
}

pub fn get_route_label(route: &TrackRoute) -> String {
    format!("{}. {}", route.index() + 1, route.name().to_str())
}

pub fn get_optional_fx_label(virtual_chain_fx: &VirtualChainFx, fx: Option<&Fx>) -> String {
    match virtual_chain_fx {
        VirtualChainFx::Dynamic(_) => virtual_chain_fx.to_string(),
        _ => match fx {
            None => format!("<Not present> ({})", virtual_chain_fx),
            Some(fx) => get_fx_label(fx.index(), fx),
        },
    }
}

pub fn get_fx_label(index: u32, fx: &Fx) -> String {
    format!(
        "{}. {}",
        index + 1,
        // When closing project, this is sometimes not available anymore although the FX is still
        // picked up when querying the list of FXs! Prevent a panic.
        if fx.is_available() {
            fx.name().into_string()
        } else {
            "".to_owned()
        }
    )
}

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: Compartment,
}

impl<'a> TargetModelWithContext<'a> {
    /// Creates a target based on this model's properties and the current REAPER state.
    ///
    /// This returns a target regardless of the activation conditions of the target. Example:
    /// If `enable_only_if_track_selected` is `true` and the track is _not_ selected when calling
    /// this function, the target will still be created!
    ///
    /// # Errors
    ///
    /// Returns an error if not enough information is provided by the model or if something (e.g.
    /// track/FX/parameter) is not available.
    pub fn resolve(&self) -> Result<Vec<CompoundMappingTarget>, &'static str> {
        let unresolved = self.target.create_target(self.compartment)?;
        unresolved.resolve(self.context, self.compartment)
    }

    pub fn resolve_first(&self) -> Result<CompoundMappingTarget, &'static str> {
        let targets = self.resolve()?;
        targets.into_iter().next().ok_or("resolved to empty list")
    }

    pub fn is_known_to_be_roundable(&self) -> bool {
        // TODO-low use cached
        self.resolve_first()
            .map(|t| {
                matches!(
                    t.control_type(self.context.control_context()),
                    ControlType::AbsoluteContinuousRoundable { .. }
                )
            })
            .unwrap_or(false)
    }
    // Returns an error if the FX doesn't exist.
    pub fn first_fx(&self) -> Result<Fx, &'static str> {
        get_fxs(
            self.context,
            &self.target.fx_descriptor()?,
            self.compartment,
        )?
        .into_iter()
        .next()
        .ok_or("resolves to empty FX list")
    }

    pub fn project(&self) -> Project {
        self.context.context().project_or_current_project()
    }

    pub fn first_fx_chain(&self) -> Result<FxChain, &'static str> {
        let track = self.first_effective_track()?;
        let chain = if self.target.fx_is_input_fx {
            track.input_fx_chain()
        } else {
            track.normal_fx_chain()
        };
        Ok(chain)
    }

    pub fn first_effective_track(&self) -> Result<Track, &'static str> {
        self.target
            .virtual_track()
            .ok_or("virtual track not complete")?
            .resolve(self.context, self.compartment)
            .map_err(|_| "particular track couldn't be resolved")?
            .into_iter()
            .next()
            .ok_or("resolved to empty track list")
    }
}

pub fn get_bookmark_label(index_within_type: u32, id: BookmarkId, name: &str) -> String {
    format!("{}. {} (ID {})", index_within_type + 1, name, id)
}

pub fn get_non_present_bookmark_label(
    anchor_type: BookmarkAnchorType,
    bookmark_ref: u32,
) -> String {
    match anchor_type {
        BookmarkAnchorType::Id => format!("<Not present> (ID {})", bookmark_ref),
        BookmarkAnchorType::Index => format!("{}. <Not present>", bookmark_ref),
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
pub enum TargetCategory {
    #[serde(rename = "reaper")]
    #[display(fmt = "Real")]
    Reaper,
    #[serde(rename = "virtual")]
    #[display(fmt = "Virtual")]
    Virtual,
}

impl TargetCategory {
    pub fn default_for(compartment: Compartment) -> Self {
        use TargetCategory::*;
        match compartment {
            Compartment::Controller => Virtual,
            Compartment::Main => Reaper,
        }
    }

    pub fn is_allowed_in(self, compartment: Compartment) -> bool {
        use TargetCategory::*;
        match compartment {
            Compartment::Controller => true,
            Compartment::Main => match self {
                Reaper => true,
                Virtual => false,
            },
        }
    }
}

impl Default for TargetCategory {
    fn default() -> Self {
        TargetCategory::Reaper
    }
}

fn virtualize_track(
    track: &Track,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualTrack {
    let own_track = context
        .track()
        .cloned()
        .unwrap_or_else(|| context.project_or_current_project().master_track());
    if own_track == *track {
        VirtualTrack::This
    } else if track.is_master_track() {
        VirtualTrack::Master
    } else if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
        // Doesn't make sense to refer to tracks via ID if we are on monitoring FX chain.
        VirtualTrack::ByIndex(track.index().expect("impossible"))
    } else {
        VirtualTrack::ById(*track.guid())
    }
}

fn virtualize_fx(
    fx: &Fx,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualFx {
    if context.containing_fx() == fx {
        VirtualFx::This
    } else {
        VirtualFx::ChainFx {
            is_input_fx: fx.is_input_fx(),
            chain_fx: if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
                // Doesn't make sense to refer to FX via UUID if we are on monitoring FX chain.
                VirtualChainFx::ByIndex(fx.index())
            } else if let Some(guid) = fx.guid() {
                VirtualChainFx::ById(guid, Some(fx.index()))
            } else {
                // This can happen if the incoming FX was created in an index-based way.
                // TODO-medium We really should use separate types in reaper-high!
                let guid = fx.chain().fx_by_index(fx.index()).and_then(|f| f.guid());
                if let Some(guid) = guid {
                    VirtualChainFx::ById(guid, Some(fx.index()))
                } else {
                    VirtualChainFx::ByIdOrIndex(None, fx.index())
                }
            },
        }
    }
}

fn virtualize_route(
    route: &TrackRoute,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualTrackRoute {
    let partner = route.partner();
    VirtualTrackRoute {
        r#type: match route.direction() {
            TrackSendDirection::Receive => TrackRouteType::Receive,
            TrackSendDirection::Send => {
                if matches!(partner, Some(TrackRoutePartner::HardwareOutput(_))) {
                    TrackRouteType::HardwareOutput
                } else {
                    TrackRouteType::Send
                }
            }
        },
        selector: if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
            // Doesn't make sense to refer to route via related-track UUID if we are on monitoring
            // FX chain.
            TrackRouteSelector::ByIndex(route.index())
        } else {
            match partner {
                None | Some(TrackRoutePartner::HardwareOutput(_)) => {
                    TrackRouteSelector::ByIndex(route.index())
                }
                Some(TrackRoutePartner::Track(t)) => TrackRouteSelector::ById(*t.guid()),
            }
        },
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, IntoEnumIterator, TryFromPrimitive, IntoPrimitive, Display,
)]
#[repr(usize)]
pub enum VirtualTrackType {
    #[display(fmt = "<This>")]
    This,
    #[display(fmt = "<Selected>")]
    Selected,
    #[display(fmt = "<All selected>")]
    AllSelected,
    #[display(fmt = "<Dynamic>")]
    Dynamic,
    #[display(fmt = "<Master>")]
    Master,
    #[display(fmt = "By ID")]
    ById,
    #[display(fmt = "By name")]
    ByName,
    #[display(fmt = "All by name")]
    AllByName,
    #[display(fmt = "By position")]
    ByIndex,
    #[display(fmt = "By ID or name")]
    ByIdOrName,
    #[display(fmt = "From clip column")]
    FromClipColumn,
}

impl Default for VirtualTrackType {
    fn default() -> Self {
        Self::This
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum BookmarkAnchorType {
    #[display(fmt = "By ID")]
    Id,
    #[display(fmt = "By position")]
    Index,
}

impl Default for BookmarkAnchorType {
    fn default() -> Self {
        Self::Id
    }
}

impl VirtualTrackType {
    pub fn from_virtual_track(virtual_track: &VirtualTrack) -> Self {
        use VirtualTrack::*;
        match virtual_track {
            This => Self::This,
            Selected { allow_multiple } => {
                if *allow_multiple {
                    Self::AllSelected
                } else {
                    Self::Selected
                }
            }
            Dynamic(_) => Self::Dynamic,
            Master => Self::Master,
            ByIdOrName(_, _) => Self::ByIdOrName,
            ById(_) => Self::ById,
            ByName { allow_multiple, .. } => {
                if *allow_multiple {
                    Self::AllByName
                } else {
                    Self::ByName
                }
            }
            ByIndex(_) => Self::ByIndex,
            FromClipColumn { .. } => Self::FromClipColumn,
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualTrackType::*;
        matches!(self, ByIdOrName | ById)
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualTrackType::*;
        matches!(self, ByIdOrName | ById | This | Master)
    }

    pub fn track_selected_condition_makes_sense(&self) -> bool {
        use VirtualTrackType::*;
        !matches!(self, Selected | AllSelected)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxType {
    #[display(fmt = "<This>")]
    #[serde(rename = "this")]
    This,
    #[display(fmt = "<Focused>")]
    #[serde(rename = "focused")]
    Focused,
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "All by name")]
    AllByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
    #[display(fmt = "By ID or pos")]
    #[serde(rename = "id-or-index")]
    ByIdOrIndex,
}

impl Default for VirtualFxType {
    fn default() -> Self {
        Self::ById
    }
}

impl VirtualFxType {
    pub fn from_virtual_fx(virtual_fx: &VirtualFx) -> Self {
        use VirtualFx::*;
        match virtual_fx {
            This => VirtualFxType::This,
            Focused => VirtualFxType::Focused,
            ChainFx { chain_fx, .. } => {
                use VirtualChainFx::*;
                match chain_fx {
                    Dynamic(_) => Self::Dynamic,
                    ById(_, _) => Self::ById,
                    ByName { allow_multiple, .. } => {
                        if *allow_multiple {
                            Self::AllByName
                        } else {
                            Self::ByName
                        }
                    }
                    ByIndex(_) => Self::ByIndex,
                    ByIdOrIndex(_, _) => Self::ByIdOrIndex,
                }
            }
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualFxType::*;
        matches!(self, ById | ByIdOrIndex)
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualFxType::*;
        matches!(self, ById | ByIdOrIndex | This)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxParameterType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By ID")]
    #[serde(rename = "index")]
    ById,
    #[display(fmt = "By position")]
    #[serde(rename = "index-manual")]
    ByIndex,
}

impl Default for VirtualFxParameterType {
    fn default() -> Self {
        Self::ById
    }
}

impl VirtualFxParameterType {
    pub fn from_virtual_fx_parameter(param: &VirtualFxParameter) -> Self {
        use VirtualFxParameter::*;
        match param {
            Dynamic(_) => Self::Dynamic,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
            ById(_) => Self::ById,
        }
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualFxParameterType::*;
        matches!(self, ById)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum TrackRouteSelectorType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
}

impl Default for TrackRouteSelectorType {
    fn default() -> Self {
        Self::ByIndex
    }
}

impl TrackRouteSelectorType {
    pub fn from_route_selector(selector: &TrackRouteSelector) -> Self {
        use TrackRouteSelector::*;
        match selector {
            Dynamic(_) => Self::Dynamic,
            ById(_) => Self::ById,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use TrackRouteSelectorType::*;
        matches!(self, ById)
    }

    pub fn is_sticky(&self) -> bool {
        use TrackRouteSelectorType::*;
        matches!(self, ById)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxSnapshot {
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_type: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub preset_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub chunk: Rc<String>,
}

impl Clone for FxSnapshot {
    fn clone(&self) -> Self {
        Self {
            fx_type: self.fx_type.clone(),
            fx_name: self.fx_name.clone(),
            preset_name: self.preset_name.clone(),
            // We want a totally detached duplicate.
            chunk: Rc::new((*self.chunk).clone()),
        }
    }
}

impl Display for FxSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let fmt_size = bytesize::ByteSize(self.chunk.len() as _);
        write!(
            f,
            "{} | {} | {}",
            self.preset_name.as_deref().unwrap_or("-"),
            fmt_size,
            self.fx_name,
        )
    }
}

#[derive(Default)]
pub struct TrackPropValues {
    pub r#type: VirtualTrackType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
    pub clip_column: ClipColumnDescriptor,
    pub clip_column_track_context: ClipColumnTrackContext,
}

impl TrackPropValues {
    pub fn from_virtual_track(track: VirtualTrack) -> Self {
        Self {
            r#type: VirtualTrackType::from_virtual_track(&track),
            id: track.id(),
            name: track.name().unwrap_or_default(),
            index: track.index().unwrap_or_default(),
            clip_column: match track.clip_column().unwrap_or(&Default::default()) {
                VirtualClipColumn::Selected => ClipColumnDescriptor::Selected,
                VirtualClipColumn::ByIndex(i) => ClipColumnDescriptor::ByIndex { index: *i },
                VirtualClipColumn::Dynamic(_) => ClipColumnDescriptor::Dynamic {
                    expression: Default::default(),
                },
            },
            expression: Default::default(),
            clip_column_track_context: track.clip_column_track_context().unwrap_or_default(),
        }
    }
}

#[derive(Default)]
pub struct TrackRoutePropValues {
    pub selector_type: TrackRouteSelectorType,
    pub r#type: TrackRouteType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl TrackRoutePropValues {
    pub fn from_virtual_route(route: VirtualTrackRoute) -> Self {
        Self {
            selector_type: TrackRouteSelectorType::from_route_selector(&route.selector),
            r#type: route.r#type,
            id: route.id(),
            name: route.name().unwrap_or_default(),
            index: route.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxPropValues {
    pub r#type: VirtualFxType,
    pub is_input_fx: bool,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl FxPropValues {
    pub fn from_virtual_fx(fx: VirtualFx) -> Self {
        Self {
            r#type: VirtualFxType::from_virtual_fx(&fx),
            is_input_fx: fx.is_input_fx(),
            id: fx.id(),
            name: fx.name().unwrap_or_default(),
            index: fx.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxParameterPropValues {
    pub r#type: VirtualFxParameterType,
    pub name: String,
    pub expression: String,
    pub index: u32,
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
pub enum RealearnTrackArea {
    #[serde(rename = "tcp")]
    #[display(fmt = "Track control panel")]
    Tcp,
    #[serde(rename = "mcp")]
    #[display(fmt = "Mixer control panel")]
    Mcp,
}

impl Default for RealearnTrackArea {
    fn default() -> Self {
        Self::Tcp
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RealearnAutomationMode {
    #[display(fmt = "Trim/Read")]
    TrimRead = 0,
    #[display(fmt = "Read")]
    Read = 1,
    #[display(fmt = "Touch")]
    Touch = 2,
    #[display(fmt = "Write")]
    Write = 3,
    #[display(fmt = "Latch")]
    Latch = 4,
    #[display(fmt = "Latch Preview")]
    LatchPreview = 5,
}

impl Default for RealearnAutomationMode {
    fn default() -> Self {
        Self::TrimRead
    }
}

impl RealearnAutomationMode {
    fn to_reaper(self) -> AutomationMode {
        use RealearnAutomationMode::*;
        match self {
            TrimRead => AutomationMode::TrimRead,
            Read => AutomationMode::Read,
            Touch => AutomationMode::Touch,
            Write => AutomationMode::Write,
            Latch => AutomationMode::Latch,
            LatchPreview => AutomationMode::LatchPreview,
        }
    }

    fn from_reaper(value: AutomationMode) -> Self {
        use AutomationMode::*;
        match value {
            TrimRead => Self::TrimRead,
            Read => Self::Read,
            Touch => Self::Touch,
            Write => Self::Write,
            Latch => Self::Latch,
            LatchPreview => Self::LatchPreview,
            Unknown(_) => Self::TrimRead,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum AutomationModeOverrideType {
    #[serde(rename = "none")]
    #[display(fmt = "None")]
    None,
    #[serde(rename = "bypass")]
    #[display(fmt = "Bypass all envelopes")]
    Bypass,
    #[serde(rename = "override")]
    #[display(fmt = "Override")]
    Override,
}

impl Default for AutomationModeOverrideType {
    fn default() -> Self {
        Self::Bypass
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetUnit {
    #[serde(rename = "native")]
    Native,
    #[serde(rename = "percent")]
    Percent,
}

impl Default for TargetUnit {
    fn default() -> Self {
        Self::Native
    }
}

#[derive(Debug)]
pub enum ConcreteTrackInstruction<'a> {
    /// If the context is not available, other track properties won't get set.
    This(Option<&'a ProcessorContext>),
    /// If the context is not available, other track properties won't get set.
    ById {
        id: Option<Guid>,
        context: Option<&'a ProcessorContext>,
    },
    ByIdWithTrack(Track),
}

impl<'a> ConcreteTrackInstruction<'a> {
    pub fn resolve(self) -> ResolvedConcreteTrackInstruction<'a> {
        use ConcreteTrackInstruction::*;
        ResolvedConcreteTrackInstruction {
            track: match &self {
                This(context) => context.and_then(|c| c.track().cloned()),
                ById {
                    id: Some(id),
                    context: Some(c),
                } => {
                    let t = c.project_or_current_project().track_by_guid(id);
                    if t.is_available() {
                        Some(t)
                    } else {
                        None
                    }
                }
                ByIdWithTrack(t) => Some(t.clone()),
                _ => None,
            },
            instruction: self,
        }
    }
}

pub struct ResolvedConcreteTrackInstruction<'a> {
    instruction: ConcreteTrackInstruction<'a>,
    track: Option<Track>,
}

impl<'a> ResolvedConcreteTrackInstruction<'a> {
    pub fn virtual_track_type(&self) -> VirtualTrackType {
        use ConcreteTrackInstruction::*;
        match &self.instruction {
            This(_) => VirtualTrackType::This,
            ById { .. } | ByIdWithTrack(_) => VirtualTrackType::ById,
        }
    }

    pub fn id(&self) -> Option<Guid> {
        use ConcreteTrackInstruction::*;
        match &self.instruction {
            ById { id, .. } => *id,
            _ => Some(*self.track.as_ref()?.guid()),
        }
    }

    pub fn name(&self) -> Option<String> {
        Some(self.track.as_ref()?.name()?.into_string())
    }

    pub fn index(&self) -> Option<u32> {
        self.track.as_ref()?.index()
    }
}

#[derive(Debug)]
pub enum ConcreteFxInstruction<'a> {
    /// If the context is not available, other FX properties won't get set.
    This(Option<&'a ProcessorContext>),
    /// If the context is not available, other FX properties won't get set.
    ById {
        is_input_fx: Option<bool>,
        id: Option<Guid>,
        track: Option<Track>,
    },
    ByIdWithFx(Fx),
}

impl<'a> ConcreteFxInstruction<'a> {
    pub fn resolve(self) -> ResolvedConcreteFxInstruction<'a> {
        use ConcreteFxInstruction::*;
        ResolvedConcreteFxInstruction {
            fx: match &self {
                This(context) => context.map(|c| c.containing_fx().clone()),
                ById {
                    is_input_fx: Some(is_input_fx),
                    id: Some(id),
                    track: Some(t),
                } => {
                    let chain = if *is_input_fx {
                        t.input_fx_chain()
                    } else {
                        t.normal_fx_chain()
                    };
                    let fx = chain.fx_by_guid(id);
                    if fx.is_available() {
                        Some(fx)
                    } else {
                        None
                    }
                }
                ByIdWithFx(fx) => Some(fx.clone()),
                _ => None,
            },
            instruction: self,
        }
    }
}

pub struct ResolvedConcreteFxInstruction<'a> {
    instruction: ConcreteFxInstruction<'a>,
    fx: Option<Fx>,
}

impl<'a> ResolvedConcreteFxInstruction<'a> {
    pub fn virtual_fx_type(&self) -> VirtualFxType {
        use ConcreteFxInstruction::*;
        match self.instruction {
            This(_) => VirtualFxType::This,
            ById { .. } | ByIdWithFx(_) => VirtualFxType::ById,
        }
    }

    pub fn is_input_fx(&self) -> Option<bool> {
        use ConcreteFxInstruction::*;
        match &self.instruction {
            ById { is_input_fx, .. } => *is_input_fx,
            _ => Some(self.fx.as_ref()?.is_input_fx()),
        }
    }

    pub fn id(&self) -> Option<Guid> {
        use ConcreteFxInstruction::*;
        match &self.instruction {
            ById { id, .. } => *id,
            _ => self.fx.as_ref()?.guid(),
        }
    }

    pub fn name(&self) -> Option<String> {
        Some(self.fx.as_ref()?.name().into_string())
    }

    pub fn index(&self) -> Option<u32> {
        Some(self.fx.as_ref()?.index())
    }
}

const TARGET_UNDEFINED_LABEL: &str = "<Undefined>";

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

fn convert_monitoring_mode_to_reaper(monitoring_mode: MonitoringMode) -> InputMonitoringMode {
    match monitoring_mode {
        MonitoringMode::Off => InputMonitoringMode::Off,
        MonitoringMode::Normal => InputMonitoringMode::Normal,
        MonitoringMode::TapeStyle => InputMonitoringMode::NotWhenPlaying,
    }
}

fn convert_monitoring_mode_to_realearn(monitoring_mode: InputMonitoringMode) -> MonitoringMode {
    match monitoring_mode {
        InputMonitoringMode::Off => MonitoringMode::Off,
        InputMonitoringMode::Normal => MonitoringMode::Normal,
        InputMonitoringMode::NotWhenPlaying => MonitoringMode::TapeStyle,
        InputMonitoringMode::Unknown(_) => MonitoringMode::Off,
    }
}

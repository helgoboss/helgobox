use base::default_util::is_default;
use derive_more::Display;
use helgoboss_learn::{
    AbsoluteValue, ControlType, Interval, OscArgDescriptor, OscTypeTag, Target,
    DEFAULT_OSC_ARG_VALUE_RANGE,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, BookmarkType, Fx, FxChain, FxParameter, Guid, Project, Track, TrackRoute,
    TrackRoutePartner,
};

use serde::{Deserialize, Serialize};

use crate::application::{
    build_action_from_smart_command_name, build_smart_command_name_from_action, Affected, Change,
    GetProcessingRelevance, ProcessingRelevance, UnitModel,
};
use crate::domain::{
    find_bookmark, get_fx_name, get_fx_params, get_non_present_virtual_route_label,
    get_non_present_virtual_track_label, get_track_routes, ActionInvocationType, AnyOnParameter,
    CompartmentKind, CompartmentParamIndex, CompoundMappingTarget, Exclusivity,
    ExpressionEvaluator, ExtendedProcessorContext, FeedbackResolution, FxDescriptor, FxDisplayType,
    FxParameterDescriptor, GroupId, MappingId, MappingKey, MappingRef, MappingSnapshotId,
    MouseActionType, OscDeviceId, PotFilterItemsTargetSettings, ProcessorContext,
    QualifiedMappingId, RealearnTarget, ReaperTarget, ReaperTargetType, SeekOptions,
    SendMidiDestinationType, SoloBehavior, Tag, TagScope, TouchedRouteParameterType,
    TouchedTrackParameterType, TrackDescriptor, TrackExclusivity, TrackGangBehavior,
    TrackRouteDescriptor, TrackRouteSelector, TrackRouteType, TransportAction,
    UnresolvedActionTarget, UnresolvedAllTrackFxEnableTarget, UnresolvedAnyOnTarget,
    UnresolvedAutomationModeOverrideTarget, UnresolvedBrowseFxsTarget, UnresolvedBrowseGroupTarget,
    UnresolvedBrowsePotFilterItemsTarget, UnresolvedBrowsePotPresetsTarget,
    UnresolvedBrowseTracksTarget, UnresolvedCompartmentParameterValueTarget,
    UnresolvedCompoundMappingTarget, UnresolvedDummyTarget, UnresolvedEnableInstancesTarget,
    UnresolvedEnableMappingsTarget, UnresolvedFxEnableTarget, UnresolvedFxOnlineTarget,
    UnresolvedFxOpenTarget, UnresolvedFxParameterTarget, UnresolvedFxParameterTouchStateTarget,
    UnresolvedFxPresetTarget, UnresolvedFxToolTarget, UnresolvedGoToBookmarkTarget,
    UnresolvedLastTouchedTarget, UnresolvedLoadFxSnapshotTarget,
    UnresolvedLoadMappingSnapshotTarget, UnresolvedLoadPotPresetTarget, UnresolvedMidiSendTarget,
    UnresolvedModifyMappingTarget, UnresolvedMouseTarget, UnresolvedOscSendTarget,
    UnresolvedPlayrateTarget, UnresolvedPreviewPotPresetTarget, UnresolvedReaperTarget,
    UnresolvedRouteAutomationModeTarget, UnresolvedRouteMonoTarget, UnresolvedRouteMuteTarget,
    UnresolvedRoutePanTarget, UnresolvedRoutePhaseTarget, UnresolvedRouteTouchStateTarget,
    UnresolvedRouteVolumeTarget, UnresolvedSeekTarget, UnresolvedStreamDeckBrightnessTarget,
    UnresolvedTakeMappingSnapshotTarget, UnresolvedTempoTarget, UnresolvedTrackArmTarget,
    UnresolvedTrackAutomationModeTarget, UnresolvedTrackMonitoringModeTarget,
    UnresolvedTrackMuteTarget, UnresolvedTrackPanTarget, UnresolvedTrackParentSendTarget,
    UnresolvedTrackPeakTarget, UnresolvedTrackPhaseTarget, UnresolvedTrackSelectionTarget,
    UnresolvedTrackShowTarget, UnresolvedTrackSoloTarget, UnresolvedTrackToolTarget,
    UnresolvedTrackTouchStateTarget, UnresolvedTrackVolumeTarget, UnresolvedTrackWidthTarget,
    UnresolvedTransportTarget, VirtualChainFx, VirtualControlElement, VirtualControlElementId,
    VirtualFx, VirtualFxParameter, VirtualMappingSnapshotIdForLoad,
    VirtualMappingSnapshotIdForTake, VirtualTarget, VirtualTrack, VirtualTrackRoute,
};

use crate::domain::{VirtualPlaytimeColumn, VirtualPlaytimeRow, VirtualPlaytimeSlot};
use serde_repr::*;
use std::borrow::Cow;
use std::error::Error;

use crate::domain::ui_util::format_tags_as_csv;
use base::hash_util::NonCryptoHashSet;
use helgobox_api::persistence::{
    ActionScope, Axis, BrowseTracksMode, ClipColumnTrackContext, FxChainDescriptor,
    FxDescriptorCommons, FxToolAction, InputDeviceMidiDestination, LearnTargetMappingModification,
    LearnableTargetKind, MappingModification, MappingSnapshotDescForLoad,
    MappingSnapshotDescForTake, MonitoringMode, MouseAction, MouseButton, PlaytimeColumnAction,
    PlaytimeColumnDescriptor, PlaytimeMatrixAction, PlaytimeRowAction, PlaytimeRowDescriptor,
    PlaytimeSlotDescriptor, PlaytimeSlotManagementAction, PlaytimeSlotTransportAction,
    PotFilterKind, SeekBehavior, SendMidiDestination, SetTargetToLastTouchedMappingModification,
    TargetTouchCause, TrackDescriptorCommons, TrackFxChain, TrackScope, TrackToolAction,
    VirtualControlElementCharacter,
};
use playtime_api::persistence::ColumnAddress;
use reaper_medium::{
    AutomationMode, BookmarkId, GlobalAutomationModeOverride, InputMonitoringMode,
    MidiInputDeviceId, SectionId, TrackArea, TrackLocation, TrackSendDirection,
};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use strum::{EnumIter, IntoEnumIterator};
use wildmatch::WildMatch;

#[allow(clippy::enum_variant_names)]
pub enum TargetCommand {
    SetCategory(TargetCategory),
    SetUnit(TargetUnit),
    SetControlElementCharacter(VirtualControlElementCharacter),
    SetControlElementId(VirtualControlElementId),
    SetLearnable(bool),
    SetTargetType(ReaperTargetType),
    SetActionScope(ActionScope),
    SetSmartCommandName(Option<String>),
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
    SetRetrigger(bool),
    SetRealTime(bool),
    SetRouteSelectorType(TrackRouteSelectorType),
    SetRouteType(TrackRouteType),
    SetRouteId(Option<Guid>),
    SetRouteIndex(u32),
    SetRouteName(String),
    SetRouteExpression(String),
    SetSeekBehavior(SeekBehavior),
    SetSoloBehavior(SoloBehavior),
    SetTrackExclusivity(TrackExclusivity),
    SetTrackToolAction(TrackToolAction),
    SetGangBehavior(TrackGangBehavior),
    SetBrowseTracksMode(BrowseTracksMode),
    SetFxToolAction(FxToolAction),
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
    SetSendMidiDestinationType(SendMidiDestinationType),
    SetMidiInputDevice(Option<MidiInputDeviceId>),
    SetOscAddressPattern(String),
    SetOscArgIndex(Option<u32>),
    SetOscArgTypeTag(OscTypeTag),
    SetOscArgValueRange(Interval<f64>),
    SetOscDevId(Option<OscDeviceId>),
    SetMouseActionType(MouseActionType),
    SetAxis(Axis),
    SetMouseButton(MouseButton),
    SetPlaytimeSlot(PlaytimeSlotDescriptor),
    SetPlaytimeColumn(PlaytimeColumnDescriptor),
    SetPlaytimeRow(PlaytimeRowDescriptor),
    SetPlaytimeSlotManagementAction(PlaytimeSlotManagementAction),
    SetPlaytimeSlotTransportAction(PlaytimeSlotTransportAction),
    SetPlaytimeMatrixAction(PlaytimeMatrixAction),
    SetPlaytimeColumnAction(PlaytimeColumnAction),
    SetPlaytimeRowAction(PlaytimeRowAction),
    SetStopColumnIfSlotEmpty(bool),
    SetPollForFeedback(bool),
    SetTags(Vec<Tag>),
    SetExclusivity(Exclusivity),
    SetGroupId(GroupId),
    SetActiveMappingsOnly(bool),
    SetMappingSnapshotTypeForLoad(MappingSnapshotTypeForLoad),
    SetMappingSnapshotTypeForTake(MappingSnapshotTypeForTake),
    SetMappingSnapshotId(Option<MappingSnapshotId>),
    SetMappingSnapshotDefaultValue(Option<AbsoluteValue>),
    SetPotFilterItemKind(PotFilterKind),
    SetMappingModificationKind(MappingModificationKind),
    SetMappingRef(MappingRefModel),
    SetLearnableTargetKinds(NonCryptoHashSet<LearnableTargetKind>),
    SetTouchCause(TargetTouchCause),
}

#[derive(Eq, PartialEq)]
pub enum TargetProp {
    Category,
    Unit,
    ControlElementType,
    ControlElementId,
    Learnable,
    TargetType,
    ActionScope,
    SmartCommandName,
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
    Retrigger,
    RealTime,
    RouteSelectorType,
    RouteType,
    RouteId,
    RouteIndex,
    RouteName,
    RouteExpression,
    SoloBehavior,
    SeekBehavior,
    TrackExclusivity,
    TrackToolAction,
    GangBehavior,
    BrowseTracksMode,
    FxToolAction,
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
    MidiInputDevice,
    OscAddressPattern,
    OscArgIndex,
    OscArgTypeTag,
    OscArgValueRange,
    OscDevId,
    MouseActionType,
    Axis,
    MouseButton,
    PlaytimeSlot,
    PlaytimeColumn,
    PlaytimeRow,
    PlaytimeSlotManagementAction,
    PlaytimeSlotTransportAction,
    PlaytimeMatrixAction,
    PlaytimeColumnAction,
    PlaytimeRowAction,
    StopColumnIfSlotEmpty,
    PollForFeedback,
    Tags,
    Exclusivity,
    GroupId,
    ActiveMappingsOnly,
    MappingSnapshotTypeForLoad,
    MappingSnapshotTypeForTake,
    MappingSnapshotId,
    MappingSnapshotDefaultValue,
    PotFilterItemKind,
    MappingModificationKind,
    MappingRef,
    IncludedTargets,
    TouchCause,
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
            C::SetControlElementCharacter(v) => {
                self.control_element_character = v;
                One(P::ControlElementType)
            }
            C::SetControlElementId(v) => {
                self.control_element_id = v;
                One(P::ControlElementId)
            }
            C::SetLearnable(v) => {
                self.learnable = v;
                One(P::Learnable)
            }
            C::SetTargetType(v) => {
                self.r#type = v;
                One(P::TargetType)
            }
            C::SetActionScope(v) => {
                self.action_scope = v;
                One(P::ActionScope)
            }
            C::SetSmartCommandName(v) => {
                self.smart_command_name = v;
                One(P::SmartCommandName)
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
            C::SetRetrigger(v) => {
                self.retrigger = v;
                One(P::Retrigger)
            }
            C::SetRealTime(v) => {
                self.real_time = v;
                One(P::RealTime)
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
            C::SetSeekBehavior(v) => {
                self.seek_behavior = v;
                One(P::SeekBehavior)
            }
            C::SetTrackExclusivity(v) => {
                self.track_exclusivity = v;
                One(P::TrackExclusivity)
            }
            C::SetTrackToolAction(v) => {
                self.track_tool_action = v;
                One(P::TrackToolAction)
            }
            C::SetGangBehavior(v) => {
                self.gang_behavior = v;
                One(P::GangBehavior)
            }
            C::SetBrowseTracksMode(v) => {
                self.browse_tracks_mode = v;
                One(P::BrowseTracksMode)
            }
            C::SetFxToolAction(v) => {
                self.fx_tool_action = v;
                One(P::FxToolAction)
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
            C::SetSendMidiDestinationType(v) => {
                self.send_midi_destination_type = v;
                One(P::SendMidiDestination)
            }
            C::SetMidiInputDevice(v) => {
                self.midi_input_device = v;
                One(P::MidiInputDevice)
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
            C::SetMouseActionType(v) => {
                self.mouse_action_type = v;
                One(P::MouseActionType)
            }
            C::SetAxis(v) => {
                self.axis = v;
                One(P::Axis)
            }
            C::SetMouseButton(v) => {
                self.mouse_button = v;
                One(P::MouseButton)
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
            C::SetMappingSnapshotTypeForLoad(v) => {
                self.mapping_snapshot_type_for_load = v;
                One(P::MappingSnapshotTypeForLoad)
            }
            C::SetMappingSnapshotTypeForTake(v) => {
                self.mapping_snapshot_type_for_take = v;
                One(P::MappingSnapshotTypeForTake)
            }
            C::SetMappingSnapshotId(v) => {
                self.mapping_snapshot_id = v;
                One(P::MappingSnapshotId)
            }
            C::SetMappingSnapshotDefaultValue(v) => {
                self.mapping_snapshot_default_value = v;
                One(P::MappingSnapshotDefaultValue)
            }
            C::SetPlaytimeSlot(s) => {
                self.playtime_slot = s;
                One(P::PlaytimeSlot)
            }
            C::SetPlaytimeColumn(c) => {
                self.playtime_column = c;
                One(P::PlaytimeColumn)
            }
            C::SetPlaytimeRow(r) => {
                self.playtime_row = r;
                One(P::PlaytimeRow)
            }
            C::SetPlaytimeSlotManagementAction(v) => {
                self.playtime_slot_management_action = v;
                One(P::PlaytimeSlotManagementAction)
            }
            C::SetPlaytimeSlotTransportAction(v) => {
                self.playtime_slot_transport_action = v;
                One(P::PlaytimeSlotTransportAction)
            }
            C::SetPlaytimeMatrixAction(v) => {
                self.playtime_matrix_action = v;
                One(P::PlaytimeMatrixAction)
            }
            C::SetPlaytimeColumnAction(v) => {
                self.playtime_column_action = v;
                One(P::PlaytimeColumnAction)
            }
            C::SetPlaytimeRowAction(v) => {
                self.playtime_row_action = v;
                One(P::PlaytimeRowAction)
            }
            C::SetStopColumnIfSlotEmpty(v) => {
                self.stop_column_if_slot_empty = v;
                One(P::StopColumnIfSlotEmpty)
            }
            C::SetPotFilterItemKind(v) => {
                self.pot_filter_item_kind = v;
                One(P::PotFilterItemKind)
            }
            C::SetMappingModificationKind(k) => {
                self.mapping_modification_kind = k;
                One(P::MappingModificationKind)
            }
            C::SetMappingRef(mapping_ref) => {
                self.mapping_ref = mapping_ref;
                One(P::MappingRef)
            }
            C::SetLearnableTargetKinds(kinds) => {
                self.included_targets = kinds;
                One(P::IncludedTargets)
            }
            C::SetTouchCause(touch_cause) => {
                self.touch_cause = touch_cause;
                One(P::TouchCause)
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
    control_element_character: VirtualControlElementCharacter,
    control_element_id: VirtualControlElementId,
    learnable: bool,
    // # For REAPER targets
    // TODO-low Rename this to reaper_target_type
    r#type: ReaperTargetType,
    // # For action targets only
    action_scope: ActionScope,
    smart_command_name: Option<String>,
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
    track_tool_action: TrackToolAction,
    gang_behavior: TrackGangBehavior,
    browse_tracks_mode: BrowseTracksMode,
    // # For track FX targets
    fx_type: VirtualFxType,
    fx_is_input_fx: bool,
    fx_id: Option<Guid>,
    fx_name: String,
    fx_index: u32,
    fx_expression: String,
    enable_only_if_fx_has_focus: bool,
    fx_tool_action: FxToolAction,
    // # For track FX or compartment parameter targets
    param_index: u32,
    // # For track FX parameter targets
    param_type: VirtualFxParameterType,
    param_name: String,
    param_expression: String,
    retrigger: bool,
    real_time: bool,
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
    // # For seek and goto bookmark targets
    seek_behavior: SeekBehavior,
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
    // # For FX Open and Browse FXs target
    fx_display_type: FxDisplayType,
    // # For track selection related targets
    scroll_arrange_view: bool,
    scroll_mixer: bool,
    // # For Send MIDI target
    raw_midi_pattern: String,
    send_midi_destination_type: SendMidiDestinationType,
    midi_input_device: Option<MidiInputDeviceId>,
    // # For Send OSC target
    osc_address_pattern: String,
    osc_arg_index: Option<u32>,
    osc_arg_type_tag: OscTypeTag,
    osc_arg_value_range: Interval<f64>,
    osc_dev_id: Option<OscDeviceId>,
    // # For mouse target
    mouse_action_type: MouseActionType,
    axis: Axis,
    mouse_button: MouseButton,
    // # For clip targets
    playtime_slot: PlaytimeSlotDescriptor,
    playtime_column: PlaytimeColumnDescriptor,
    playtime_row: PlaytimeRowDescriptor,
    playtime_slot_management_action: PlaytimeSlotManagementAction,
    playtime_slot_transport_action: PlaytimeSlotTransportAction,
    playtime_matrix_action: PlaytimeMatrixAction,
    playtime_column_action: PlaytimeColumnAction,
    playtime_row_action: PlaytimeRowAction,
    stop_column_if_slot_empty: bool,
    // # For targets that might have to be polled in order to get automatic feedback in all cases.
    poll_for_feedback: bool,
    tags: Vec<Tag>,
    mapping_snapshot_type_for_load: MappingSnapshotTypeForLoad,
    mapping_snapshot_type_for_take: MappingSnapshotTypeForTake,
    mapping_snapshot_id: Option<MappingSnapshotId>,
    mapping_snapshot_default_value: Option<AbsoluteValue>,
    exclusivity: Exclusivity,
    group_id: GroupId,
    active_mappings_only: bool,
    mapping_modification_kind: MappingModificationKind,
    mapping_ref: MappingRefModel,
    // # For Pot targets
    pot_filter_item_kind: PotFilterKind,
    // # For targets that deal with target learning/touching
    included_targets: NonCryptoHashSet<LearnableTargetKind>,
    touch_cause: TargetTouchCause,
}

#[derive(Clone, Debug)]
pub enum MappingRefModel {
    OwnMapping {
        mapping_id: Option<MappingId>,
    },
    ForeignMapping {
        session_id: String,
        mapping_key: Option<MappingKey>,
    },
}

impl MappingRefModel {
    pub fn session_id(&self) -> Option<&str> {
        if let Self::ForeignMapping { session_id, .. } = self {
            Some(session_id)
        } else {
            None
        }
    }
}

impl Default for MappingRefModel {
    fn default() -> Self {
        Self::OwnMapping { mapping_id: None }
    }
}

impl MappingRefModel {
    fn create_mapping_ref(&self) -> Result<MappingRef, &'static str> {
        let mapping_ref = match self {
            MappingRefModel::OwnMapping { mapping_id } => MappingRef::OwnMapping {
                mapping_id: (*mapping_id).ok_or("mapping not specified")?,
            },
            MappingRefModel::ForeignMapping {
                session_id,
                mapping_key,
            } => MappingRef::ForeignMapping {
                session_id: session_id.clone(),
                mapping_key: mapping_key.clone().ok_or("mapping_not_specified")?,
            },
        };
        Ok(mapping_ref)
    }
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            category: TargetCategory::default(),
            unit: Default::default(),
            control_element_character: VirtualControlElementCharacter::default(),
            control_element_id: Default::default(),
            learnable: true,
            r#type: ReaperTargetType::Dummy,
            action_scope: Default::default(),
            smart_command_name: None,
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
            retrigger: false,
            real_time: false,
            route_selector_type: Default::default(),
            route_type: Default::default(),
            route_id: None,
            route_index: 0,
            route_name: Default::default(),
            route_expression: Default::default(),
            touched_route_parameter_type: Default::default(),
            solo_behavior: Default::default(),
            seek_behavior: Default::default(),
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
            send_midi_destination_type: Default::default(),
            midi_input_device: None,
            osc_address_pattern: "".to_owned(),
            osc_arg_index: Some(0),
            osc_arg_type_tag: Default::default(),
            osc_arg_value_range: DEFAULT_OSC_ARG_VALUE_RANGE,
            osc_dev_id: None,
            mouse_action_type: Default::default(),
            axis: Default::default(),
            mouse_button: Default::default(),
            poll_for_feedback: true,
            tags: Default::default(),
            mapping_snapshot_type_for_load: MappingSnapshotTypeForLoad::Initial,
            mapping_snapshot_type_for_take: MappingSnapshotTypeForTake::LastLoaded,
            mapping_snapshot_id: None,
            mapping_snapshot_default_value: None,
            exclusivity: Default::default(),
            group_id: Default::default(),
            active_mappings_only: false,
            playtime_slot: Default::default(),
            playtime_column: Default::default(),
            playtime_row: Default::default(),
            playtime_slot_management_action: Default::default(),
            playtime_slot_transport_action: Default::default(),
            playtime_column_action: Default::default(),
            playtime_matrix_action: Default::default(),
            stop_column_if_slot_empty: false,
            clip_column_track_context: Default::default(),
            playtime_row_action: Default::default(),
            track_tool_action: Default::default(),
            fx_tool_action: Default::default(),
            gang_behavior: Default::default(),
            browse_tracks_mode: Default::default(),
            pot_filter_item_kind: Default::default(),
            mapping_modification_kind: Default::default(),
            mapping_ref: Default::default(),
            included_targets: LearnableTargetKind::iter().collect(),
            touch_cause: Default::default(),
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

    pub fn control_element_character(&self) -> VirtualControlElementCharacter {
        self.control_element_character
    }

    pub fn control_element_id(&self) -> VirtualControlElementId {
        self.control_element_id
    }

    pub fn learnable(&self) -> bool {
        self.learnable
    }

    pub fn target_type(&self) -> ReaperTargetType {
        self.r#type
    }

    pub fn action_scope(&self) -> ActionScope {
        self.action_scope
    }

    pub fn smart_command_name(&self) -> Option<&str> {
        self.smart_command_name.as_deref()
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

    pub fn fixed_gang_behavior(&self) -> TrackGangBehavior {
        self.gang_behavior.fixed(self.r#type.definition())
    }

    pub fn browse_tracks_mode(&self) -> BrowseTracksMode {
        self.browse_tracks_mode
    }

    pub fn param_type(&self) -> VirtualFxParameterType {
        self.param_type
    }

    pub fn param_index(&self) -> u32 {
        self.param_index
    }

    pub fn compartment_param_index(&self) -> CompartmentParamIndex {
        CompartmentParamIndex::try_from(self.param_index).unwrap_or_default()
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

    pub fn seek_behavior(&self) -> SeekBehavior {
        self.seek_behavior
    }

    pub fn track_exclusivity(&self) -> TrackExclusivity {
        self.track_exclusivity
    }

    pub fn track_tool_action(&self) -> TrackToolAction {
        self.track_tool_action
    }

    pub fn fx_tool_action(&self) -> FxToolAction {
        self.fx_tool_action
    }

    pub fn transport_action(&self) -> TransportAction {
        self.transport_action
    }

    pub fn mouse_action_type(&self) -> MouseActionType {
        self.mouse_action_type
    }

    pub fn axis(&self) -> Axis {
        self.axis
    }

    pub fn mouse_button(&self) -> MouseButton {
        self.mouse_button
    }

    pub fn any_on_parameter(&self) -> AnyOnParameter {
        self.any_on_parameter
    }

    pub fn fx_snapshot(&self) -> Option<&FxSnapshot> {
        self.fx_snapshot.as_ref()
    }

    pub fn mapping_snapshot_type_for_load(&self) -> MappingSnapshotTypeForLoad {
        self.mapping_snapshot_type_for_load
    }

    pub fn mapping_snapshot_type_for_take(&self) -> MappingSnapshotTypeForTake {
        self.mapping_snapshot_type_for_take
    }

    pub fn mapping_snapshot_id(&self) -> Option<&MappingSnapshotId> {
        self.mapping_snapshot_id.as_ref()
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

    pub fn send_midi_destination_type(&self) -> SendMidiDestinationType {
        self.send_midi_destination_type
    }

    pub fn midi_input_device(&self) -> Option<MidiInputDeviceId> {
        self.midi_input_device
    }

    pub fn osc_address_pattern(&self) -> &str {
        &self.osc_address_pattern
    }

    pub fn mapping_snapshot_default_value(&self) -> Option<AbsoluteValue> {
        self.mapping_snapshot_default_value
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

    pub fn playtime_slot_management_action(&self) -> PlaytimeSlotManagementAction {
        self.playtime_slot_management_action
    }

    pub fn poll_for_feedback(&self) -> bool {
        self.poll_for_feedback
    }

    pub fn retrigger(&self) -> bool {
        self.retrigger
    }

    pub fn real_time(&self) -> bool {
        self.real_time
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
        compartment: CompartmentKind,
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
        compartment: CompartmentKind,
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
        compartment: CompartmentKind,
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
        compartment: CompartmentKind,
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
            // chunk: Rc::new(fx.vst_chunk_encoded()?.into_string()),
        };
        Ok(fx_snapshot)
    }

    #[must_use]
    pub fn invalidate_fx_index(
        &mut self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
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
                        VirtualFx::Unit | VirtualFx::LastFocused | VirtualFx::This => None,
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
        compartment: CompartmentKind,
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
            This => {
                let _ = self.set_concrete_track(
                    ConcreteTrackInstruction::This(context),
                    // Already notified above
                    false,
                    with_notification,
                );
            }
            ById => {
                let _ = self.set_concrete_track(
                    ConcreteTrackInstruction::ById {
                        id: track.id,
                        context,
                    },
                    // Already notified above
                    false,
                    with_notification,
                );
            }
            ByName | AllByName => {
                self.track_name = track.name;
            }
            ByIndex | ByIndexTcp | ByIndexMcp => {
                self.track_index = track.index;
            }
            ByIdOrName => {
                self.track_id = track.id;
                self.track_name = track.name;
            }
            FromClipColumn => {
                self.playtime_column = track.clip_column;
                self.clip_column_track_context = track.clip_column_track_context;
            }
            Unit | Selected | AllSelected | Master | Dynamic | DynamicTcp | DynamicMcp => {}
        }
        Some(Affected::Multiple)
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
        compartment: CompartmentKind,
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
        compartment: CompartmentKind,
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
            Dynamic | Focused | Unit => {}
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
        compartment: CompartmentKind,
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
                context
                    .project_or_current_project()
                    .master_track()
                    .expect("no way")
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
        if let Some(slot_address) = target.clip_slot_address() {
            self.playtime_slot = PlaytimeSlotDescriptor::ByIndex(slot_address);
        }
        if let Some(column_address) = target.clip_column_address() {
            self.playtime_column = PlaytimeColumnDescriptor::ByIndex(column_address);
        }
        if let Some(row_address) = target.clip_row_address() {
            self.playtime_row = PlaytimeRowDescriptor::ByIndex(row_address);
        }

        match target {
            Action(t) => {
                let section_id = t.action.section().map(|s| s.id()).unwrap_or_default();
                self.action_scope = ActionScope::guess_from_section_id(section_id.get());
                self.smart_command_name = build_smart_command_name_from_action(&t.action);
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
            PlaytimeMatrixAction(t) => {
                self.playtime_matrix_action = t.action;
            }
            PlaytimeColumnAction(t) => {
                self.playtime_column_action = t.action;
            }
            PlaytimeRowAction(t) => {
                self.playtime_row_action = t.basics.action;
            }
            PlaytimeSlotTransportAction(t) => {
                self.playtime_slot_transport_action = t.basics.action;
            }
            _ => {}
        };
        Some(Affected::Multiple)
    }

    pub fn virtual_default(
        control_element_character: VirtualControlElementCharacter,
        next_index: u32,
    ) -> Self {
        TargetModel {
            category: TargetCategory::Virtual,
            control_element_character,
            control_element_id: VirtualControlElementId::Indexed(next_index),
            ..Default::default()
        }
    }

    pub fn default_for_compartment(compartment: CompartmentKind) -> Self {
        use CompartmentKind::*;
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
            Unit => VirtualTrack::Unit,
            ById => VirtualTrack::ById(self.track_id?),
            ByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(&self.track_name),
                allow_multiple: false,
            },
            AllByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(&self.track_name),
                allow_multiple: true,
            },
            ByIndex | ByIndexTcp | ByIndexMcp => VirtualTrack::ByIndex {
                index: self.track_index,
                scope: self.track_type.virtual_track_scope().unwrap_or_default(),
            },
            ByIdOrName => {
                VirtualTrack::ByIdOrName(self.track_id?, WildMatch::new(&self.track_name))
            }
            Dynamic | DynamicTcp | DynamicMcp => {
                let evaluator = ExpressionEvaluator::compile(&self.track_expression).ok()?;
                VirtualTrack::Dynamic {
                    evaluator: Box::new(evaluator),
                    scope: self.track_type.virtual_track_scope().unwrap_or_default(),
                }
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
            clip_column: self.playtime_column.clone(),
            clip_column_track_context: self.clip_column_track_context,
        }
    }

    pub fn virtual_fx(&self) -> Option<VirtualFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type {
            Focused => VirtualFx::LastFocused,
            This => VirtualFx::This,
            Unit => VirtualFx::Unit,
            _ => VirtualFx::ChainFx {
                is_input_fx: self.fx_is_input_fx,
                chain_fx: self.virtual_chain_fx()?,
            },
        };
        Some(fx)
    }

    pub fn virtual_mapping_snapshot_id_for_load(
        &self,
    ) -> Result<VirtualMappingSnapshotIdForLoad, &'static str> {
        match self.mapping_snapshot_type_for_load {
            MappingSnapshotTypeForLoad::Initial => Ok(VirtualMappingSnapshotIdForLoad::Initial),
            MappingSnapshotTypeForLoad::ById => {
                let id = self
                    .mapping_snapshot_id
                    .as_ref()
                    .ok_or("no mapping snapshot ID")?
                    .clone();
                Ok(VirtualMappingSnapshotIdForLoad::ById(id))
            }
        }
    }

    pub fn virtual_mapping_snapshot_id_for_take(
        &self,
    ) -> Result<VirtualMappingSnapshotIdForTake, &'static str> {
        match self.mapping_snapshot_type_for_take {
            MappingSnapshotTypeForTake::LastLoaded => {
                Ok(VirtualMappingSnapshotIdForTake::LastLoaded)
            }
            MappingSnapshotTypeForTake::ById => {
                let id = self
                    .mapping_snapshot_id
                    .as_ref()
                    .ok_or("no mapping snapshot ID")?
                    .clone();
                Ok(VirtualMappingSnapshotIdForTake::ById(id))
            }
        }
    }

    pub fn mapping_snapshot_desc_for_load(&self) -> MappingSnapshotDescForLoad {
        if self.target_type() == ReaperTargetType::TakeMappingSnapshot {
            Default::default()
        } else {
            match self.mapping_snapshot_type_for_load {
                MappingSnapshotTypeForLoad::Initial => MappingSnapshotDescForLoad::Initial,
                MappingSnapshotTypeForLoad::ById => MappingSnapshotDescForLoad::ById {
                    id: self
                        .mapping_snapshot_id
                        .as_ref()
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                },
            }
        }
    }

    pub fn mapping_snapshot_desc_for_take(&self) -> MappingSnapshotDescForTake {
        if self.target_type() == ReaperTargetType::LoadMappingSnapshot {
            Default::default()
        } else {
            match self.mapping_snapshot_type_for_take {
                MappingSnapshotTypeForTake::LastLoaded => MappingSnapshotDescForTake::LastLoaded,
                MappingSnapshotTypeForTake::ById => MappingSnapshotDescForTake::ById {
                    id: self
                        .mapping_snapshot_id
                        .as_ref()
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                },
            }
        }
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
            Focused | This | Unit => return None,
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

    pub fn api_track_descriptor(&self) -> helgobox_api::persistence::TrackDescriptor {
        use helgobox_api::persistence::TrackDescriptor;
        use VirtualTrackType::*;
        let commons = TrackDescriptorCommons {
            track_must_be_selected: Some(self.enable_only_if_track_selected),
        };
        match self.track_type {
            This => TrackDescriptor::This { commons },
            Selected => TrackDescriptor::Selected {
                allow_multiple: Some(false),
            },
            AllSelected => TrackDescriptor::Selected {
                allow_multiple: Some(true),
            },
            Master => TrackDescriptor::Master { commons },
            Unit => TrackDescriptor::Instance { commons },
            ById | ByIdOrName => TrackDescriptor::ById {
                commons,
                id: self
                    .track_id
                    .as_ref()
                    .map(|id| id.to_string_without_braces()),
            },
            ByName => TrackDescriptor::ByName {
                commons,
                name: self.track_name.clone(),
                allow_multiple: Some(false),
            },
            AllByName => TrackDescriptor::ByName {
                commons,
                name: self.track_name.clone(),
                allow_multiple: Some(true),
            },
            ByIndex | ByIndexTcp | ByIndexMcp => TrackDescriptor::ByIndex {
                commons,
                index: self.track_index,
                scope: self.track_type.virtual_track_scope(),
            },
            Dynamic | DynamicTcp | DynamicMcp => TrackDescriptor::Dynamic {
                commons,
                expression: self.track_expression.clone(),
                scope: self.track_type.virtual_track_scope(),
            },
            FromClipColumn => TrackDescriptor::FromClipColumn {
                commons,
                column: self.playtime_column.clone(),
                context: self.clip_column_track_context,
            },
        }
    }

    pub fn api_fx_descriptor(&self) -> helgobox_api::persistence::FxDescriptor {
        use helgobox_api::persistence::FxDescriptor;
        use VirtualFxType::*;
        let commons = FxDescriptorCommons {
            fx_must_have_focus: Some(self.enable_only_if_fx_has_focus),
        };
        let chain = FxChainDescriptor::Track {
            track: Some(self.api_track_descriptor()),
            chain: Some(if self.fx_is_input_fx {
                TrackFxChain::Input
            } else {
                TrackFxChain::Normal
            }),
        };
        match self.fx_type {
            This => FxDescriptor::This { commons },
            Focused => FxDescriptor::Focused,
            Dynamic => FxDescriptor::Dynamic {
                commons,
                chain,
                expression: self.track_expression.clone(),
            },
            Unit => FxDescriptor::Instance { commons },
            ById => FxDescriptor::ById {
                commons,
                chain,
                id: self.fx_id.as_ref().map(|id| id.to_string_without_braces()),
            },
            ByName => FxDescriptor::ByName {
                commons,
                chain,
                name: self.fx_name.clone(),
                allow_multiple: Some(false),
            },
            AllByName => FxDescriptor::ByName {
                commons,
                chain,
                name: self.fx_name.clone(),
                allow_multiple: Some(true),
            },
            ByIndex | ByIdOrIndex => FxDescriptor::ByIndex {
                commons,
                chain,
                index: self.fx_index,
            },
        }
    }

    fn virtual_clip_slot(&self) -> Result<VirtualPlaytimeSlot, &'static str> {
        use PlaytimeSlotDescriptor::*;
        let slot = match &self.playtime_slot {
            Active => VirtualPlaytimeSlot::Active,
            ByIndex(address) => VirtualPlaytimeSlot::ByIndex(*address),
            Dynamic {
                column_expression,
                row_expression,
            } => {
                let column_evaluator = ExpressionEvaluator::compile(column_expression)
                    .map_err(|_| "couldn't evaluate row")?;
                let row_evaluator = ExpressionEvaluator::compile(row_expression)
                    .map_err(|_| "couldn't evaluate row")?;
                VirtualPlaytimeSlot::Dynamic {
                    column_evaluator: Box::new(column_evaluator),
                    row_evaluator: Box::new(row_evaluator),
                }
            }
        };
        Ok(slot)
    }

    fn virtual_clip_column(&self) -> Result<VirtualPlaytimeColumn, &'static str> {
        VirtualPlaytimeColumn::from_descriptor(&self.playtime_column)
    }

    fn virtual_clip_row(&self) -> Result<VirtualPlaytimeRow, &'static str> {
        use PlaytimeRowDescriptor::*;
        let row = match &self.playtime_row {
            Active => VirtualPlaytimeRow::Active,
            ByIndex(address) => VirtualPlaytimeRow::ByIndex(address.index),
            Dynamic {
                expression: index_expression,
            } => {
                let index_evaluator = ExpressionEvaluator::compile(index_expression)
                    .map_err(|_| "couldn't evaluate row index")?;
                VirtualPlaytimeRow::Dynamic(Box::new(index_evaluator))
            }
        };
        Ok(row)
    }

    pub fn fx_descriptor(&self) -> Result<FxDescriptor, &'static str> {
        let desc = FxDescriptor {
            track_descriptor: if let Ok(desc) = self.track_descriptor() {
                desc
            } else if self.fx_type.requires_fx_chain() {
                return Err("couldn't resolve track but track required");
            } else {
                TrackDescriptor::default()
            },
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
        compartment: CompartmentKind,
    ) -> Result<UnresolvedCompoundMappingTarget, &'static str> {
        use TargetCategory::*;
        match self.category {
            Reaper => {
                use ReaperTargetType::*;
                let target = match self.r#type {
                    Mouse => UnresolvedReaperTarget::Mouse(UnresolvedMouseTarget {
                        action_type: self.mouse_action_type,
                        axis: self.axis,
                        button: self.mouse_button,
                    }),
                    CompartmentParameterValue => UnresolvedReaperTarget::CompartmentParameterValue(
                        UnresolvedCompartmentParameterValueTarget {
                            compartment,
                            index: self.compartment_param_index(),
                        },
                    ),
                    Action => UnresolvedReaperTarget::Action(UnresolvedActionTarget {
                        action: self.resolved_available_action()?,
                        scope: self.action_scope,
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
                            retrigger: self.retrigger,
                            real_time_even_if_not_rendering: self.real_time,
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
                            gang_behavior: self.fixed_gang_behavior(),
                        })
                    }
                    TrackTool => UnresolvedReaperTarget::TrackTool(UnresolvedTrackToolTarget {
                        track_descriptor: self.track_descriptor()?,
                        action: self.track_tool_action,
                        scope: self.tag_scope(),
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
                        gang_behavior: self.fixed_gang_behavior(),
                    }),
                    TrackWidth => UnresolvedReaperTarget::TrackWidth(UnresolvedTrackWidthTarget {
                        track_descriptor: self.track_descriptor()?,
                        gang_behavior: self.fixed_gang_behavior(),
                    }),
                    TrackArm => UnresolvedReaperTarget::TrackArm(UnresolvedTrackArmTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                        gang_behavior: self.fixed_gang_behavior(),
                    }),
                    TrackParentSend => {
                        UnresolvedReaperTarget::TrackParentSend(UnresolvedTrackParentSendTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity,
                        })
                    }
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
                        gang_behavior: self.fixed_gang_behavior(),
                    }),
                    TrackPhase => UnresolvedReaperTarget::TrackPhase(UnresolvedTrackPhaseTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                        gang_behavior: self.gang_behavior,
                        poll_for_feedback: self.poll_for_feedback,
                    }),
                    TrackShow => UnresolvedReaperTarget::TrackShow(UnresolvedTrackShowTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity,
                        area: match self.track_area {
                            RealearnTrackArea::Tcp => TrackArea::Tcp,
                            RealearnTrackArea::Mcp => TrackArea::Mcp,
                        },
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
                            gang_behavior: self.fixed_gang_behavior(),
                        },
                    ),
                    TrackSolo => UnresolvedReaperTarget::TrackSolo(UnresolvedTrackSoloTarget {
                        track_descriptor: self.track_descriptor()?,
                        behavior: self.solo_behavior,
                        exclusivity: self.track_exclusivity,
                        gang_behavior: self.fixed_gang_behavior(),
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
                    PlayRate => UnresolvedReaperTarget::Playrate(UnresolvedPlayrateTarget),
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
                    FxTool => UnresolvedReaperTarget::FxTool(UnresolvedFxToolTarget {
                        fx_descriptor: self.fx_descriptor()?,
                        action: self.fx_tool_action,
                        scope: self.tag_scope(),
                    }),
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
                    BrowseTracks => {
                        UnresolvedReaperTarget::SelectedTrack(UnresolvedBrowseTracksTarget {
                            scroll_arrange_view: self.scroll_arrange_view,
                            scroll_mixer: self.scroll_mixer,
                            mode: self.browse_tracks_mode,
                        })
                    }
                    BrowseFxs => UnresolvedReaperTarget::BrowseFxs(UnresolvedBrowseFxsTarget {
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
                    LastTouched => {
                        UnresolvedReaperTarget::LastTouched(UnresolvedLastTouchedTarget {
                            included_targets: self
                                .included_targets
                                .iter()
                                .copied()
                                .map(ReaperTargetType::from_learnable_target_kind)
                                .collect(),
                            touch_cause: self.touch_cause,
                        })
                    }
                    TrackTouchState => {
                        UnresolvedReaperTarget::TrackTouchState(UnresolvedTrackTouchStateTarget {
                            track_descriptor: self.track_descriptor()?,
                            parameter_type: self.touched_track_parameter_type,
                            exclusivity: self.track_exclusivity,
                            gang_behavior: self.fixed_gang_behavior(),
                        })
                    }
                    GoToBookmark => {
                        UnresolvedReaperTarget::GoToBookmark(UnresolvedGoToBookmarkTarget {
                            bookmark_type: self.bookmark_type,
                            bookmark_anchor_type: self.bookmark_anchor_type,
                            bookmark_ref: self.bookmark_ref,
                            set_time_selection: self.use_time_selection,
                            set_loop_points: self.use_loop_points,
                            seek_behavior: self.seek_behavior,
                        })
                    }
                    Seek => UnresolvedReaperTarget::Seek(UnresolvedSeekTarget {
                        options: self.seek_options(),
                        behavior: self.seek_behavior,
                    }),
                    SendMidi => UnresolvedReaperTarget::SendMidi(UnresolvedMidiSendTarget {
                        pattern: self.raw_midi_pattern.parse().unwrap_or_default(),
                        destination: match self.send_midi_destination_type {
                            SendMidiDestinationType::FxOutput => SendMidiDestination::FxOutput,
                            SendMidiDestinationType::FeedbackOutput => {
                                SendMidiDestination::FeedbackOutput
                            }
                            SendMidiDestinationType::InputDevice => {
                                SendMidiDestination::InputDevice(InputDeviceMidiDestination {
                                    device_id: self.midi_input_device.map(|d| d.get()),
                                })
                            }
                        },
                    }),
                    SendOsc => UnresolvedReaperTarget::SendOsc(UnresolvedOscSendTarget {
                        address_pattern: self.osc_address_pattern.clone(),
                        arg_descriptor: self.osc_arg_descriptor(),
                        device_id: self.osc_dev_id,
                    }),
                    PlaytimeSlotTransportAction => {
                        UnresolvedReaperTarget::PlaytimeSlotTransportAction(
                            crate::domain::UnresolvedPlaytimeSlotTransportTarget {
                                slot: self.virtual_clip_slot()?,
                                action: self.playtime_slot_transport_action,
                                options: self.clip_transport_options(),
                            },
                        )
                    }
                    PlaytimeColumnAction => UnresolvedReaperTarget::PlaytimeColumnAction(
                        crate::domain::UnresolvedPlaytimeColumnActionTarget {
                            column: self.virtual_clip_column()?,
                            action: self.playtime_column_action,
                        },
                    ),
                    PlaytimeRowAction => UnresolvedReaperTarget::PlaytimeRowAction(
                        crate::domain::UnresolvedPlaytimeRowActionTarget {
                            row: self.virtual_clip_row()?,
                            action: self.playtime_row_action,
                        },
                    ),
                    PlaytimeSlotSeek => UnresolvedReaperTarget::PlaytimeSlotSeek(
                        crate::domain::UnresolvedPlaytimeSlotSeekTarget {
                            slot: self.virtual_clip_slot()?,
                            feedback_resolution: self.feedback_resolution,
                        },
                    ),
                    PlaytimeSlotVolume => UnresolvedReaperTarget::PlaytimeSlotVolume(
                        crate::domain::UnresolvedPlaytimeSlotVolumeTarget {
                            slot: self.virtual_clip_slot()?,
                        },
                    ),
                    PlaytimeSlotManagementAction => {
                        UnresolvedReaperTarget::PlaytimeSlotManagementAction(
                            crate::domain::UnresolvedPlaytimeSlotManagementActionTarget {
                                slot: self.virtual_clip_slot()?,
                                action: self.playtime_slot_management_action,
                            },
                        )
                    }
                    PlaytimeMatrixAction => UnresolvedReaperTarget::PlaytimeMatrixAction(
                        crate::domain::UnresolvedPlaytimeMatrixActionTarget {
                            action: self.playtime_matrix_action,
                        },
                    ),
                    PlaytimeControlUnitScroll => UnresolvedReaperTarget::PlaytimeControlUnitScroll(
                        crate::domain::UnresolvedPlaytimeControlUnitScrollTarget {
                            axis: self.axis,
                        },
                    ),
                    PlaytimeBrowseCells => UnresolvedReaperTarget::PlaytimeBrowseCells(
                        crate::domain::UnresolvedPlaytimeBrowseCellsTarget { axis: self.axis },
                    ),
                    LoadMappingSnapshot => UnresolvedReaperTarget::LoadMappingSnapshot(
                        UnresolvedLoadMappingSnapshotTarget {
                            compartment,
                            scope: self.tag_scope(),
                            active_mappings_only: self.active_mappings_only,
                            snapshot_id: self.virtual_mapping_snapshot_id_for_load()?,
                            default_value: self.mapping_snapshot_default_value,
                        },
                    ),
                    TakeMappingSnapshot => UnresolvedReaperTarget::TakeMappingSnapshot(
                        UnresolvedTakeMappingSnapshotTarget {
                            compartment,
                            scope: self.tag_scope(),
                            active_mappings_only: self.active_mappings_only,
                            snapshot_id: self.virtual_mapping_snapshot_id_for_take()?,
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
                    ModifyMapping => {
                        UnresolvedReaperTarget::ModifyMapping(UnresolvedModifyMappingTarget {
                            compartment,
                            modification: match self.mapping_modification_kind {
                                MappingModificationKind::LearnTarget => {
                                    MappingModification::LearnTarget(
                                        LearnTargetMappingModification {
                                            included_targets: Some(
                                                self.included_targets.iter().cloned().collect(),
                                            ),
                                            touch_cause: Some(self.touch_cause),
                                        },
                                    )
                                }
                                MappingModificationKind::SetTargetToLastTouched => {
                                    MappingModification::SetTargetToLastTouched(
                                        SetTargetToLastTouchedMappingModification {
                                            included_targets: Some(
                                                self.included_targets.iter().cloned().collect(),
                                            ),
                                            touch_cause: Some(self.touch_cause),
                                        },
                                    )
                                }
                            },
                            mapping_ref: self.mapping_ref.create_mapping_ref()?,
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
                    BrowseGroup => {
                        UnresolvedReaperTarget::BrowseGroup(UnresolvedBrowseGroupTarget {
                            compartment,
                            group_id: self.group_id,
                            exclusivity: self.exclusivity.into(),
                        })
                    }
                    AnyOn => UnresolvedReaperTarget::AnyOn(UnresolvedAnyOnTarget {
                        parameter: self.any_on_parameter,
                    }),
                    Dummy => UnresolvedReaperTarget::Dummy(UnresolvedDummyTarget),
                    BrowsePotFilterItems => UnresolvedReaperTarget::BrowsePotFilterItems(
                        UnresolvedBrowsePotFilterItemsTarget {
                            settings: PotFilterItemsTargetSettings {
                                kind: self.pot_filter_item_kind,
                            },
                        },
                    ),
                    BrowsePotPresets => UnresolvedReaperTarget::BrowsePotPresets(
                        UnresolvedBrowsePotPresetsTarget {},
                    ),
                    PreviewPotPreset => UnresolvedReaperTarget::PreviewPotPreset(
                        UnresolvedPreviewPotPresetTarget {},
                    ),
                    LoadPotPreset => {
                        UnresolvedReaperTarget::LoadPotPreset(UnresolvedLoadPotPresetTarget {
                            fx_descriptor: self.fx_descriptor()?,
                        })
                    }
                    StreamDeckBrightness => UnresolvedReaperTarget::StreamDeckBrightness(
                        UnresolvedStreamDeckBrightnessTarget {},
                    ),
                };
                Ok(UnresolvedCompoundMappingTarget::Reaper(Box::new(target)))
            }
            Virtual => {
                let virtual_target = VirtualTarget {
                    control_element: self.create_control_element(),
                    learnable: self.learnable,
                };
                Ok(UnresolvedCompoundMappingTarget::Virtual(virtual_target))
            }
        }
    }

    pub fn playtime_slot(&self) -> &PlaytimeSlotDescriptor {
        &self.playtime_slot
    }

    pub fn playtime_column(&self) -> &PlaytimeColumnDescriptor {
        &self.playtime_column
    }

    pub fn playtime_row(&self) -> &PlaytimeRowDescriptor {
        &self.playtime_row
    }

    pub fn playtime_slot_transport_action(&self) -> PlaytimeSlotTransportAction {
        self.playtime_slot_transport_action
    }

    pub fn playtime_matrix_action(&self) -> PlaytimeMatrixAction {
        self.playtime_matrix_action
    }

    pub fn playtime_column_action(&self) -> PlaytimeColumnAction {
        self.playtime_column_action
    }

    pub fn simple_target(&self) -> Option<playtime_api::runtime::SimpleMappingTarget> {
        use helgobox_api::persistence;
        use playtime_api::runtime::SimpleMappingTarget;
        use ReaperTargetType as T;
        if self.category != TargetCategory::Reaper {
            return None;
        }
        let t = match self.r#type {
            T::PlaytimeSlotTransportAction
                if self.playtime_slot_transport_action()
                    == persistence::PlaytimeSlotTransportAction::Trigger =>
            {
                SimpleMappingTarget::TriggerSlot(self.playtime_slot.fixed_address()?)
            }
            T::PlaytimeColumnAction
                if self.playtime_column_action() == persistence::PlaytimeColumnAction::Stop =>
            {
                SimpleMappingTarget::TriggerColumn(self.playtime_column.fixed_address()?)
            }
            T::PlaytimeRowAction
                if self.playtime_row_action() == persistence::PlaytimeRowAction::PlayScene =>
            {
                SimpleMappingTarget::TriggerRow(self.playtime_row.fixed_address()?)
            }
            T::PlaytimeMatrixAction => match self.playtime_matrix_action {
                PlaytimeMatrixAction::Stop => SimpleMappingTarget::TriggerMatrix,
                PlaytimeMatrixAction::SmartRecord => SimpleMappingTarget::SmartRecord,
                PlaytimeMatrixAction::PlayIgnitedOrEnterSilenceMode => {
                    SimpleMappingTarget::EnterSilenceModeOrPlayIgnited
                }
                PlaytimeMatrixAction::SequencerRecordOnOffState => {
                    SimpleMappingTarget::SequencerRecordOnOffState
                }
                PlaytimeMatrixAction::SequencerPlayOnOffState => {
                    SimpleMappingTarget::SequencerPlayOnOffState
                }
                PlaytimeMatrixAction::TapTempo => SimpleMappingTarget::TapTempo,
                PlaytimeMatrixAction::Undo
                | PlaytimeMatrixAction::Redo
                | PlaytimeMatrixAction::BuildScene
                | PlaytimeMatrixAction::SetRecordLengthMode
                | PlaytimeMatrixAction::SetCustomRecordLengthInBars
                | PlaytimeMatrixAction::ClickOnOffState
                | PlaytimeMatrixAction::MidiAutoQuantizationOnOffState
                | PlaytimeMatrixAction::SilenceModeOnOffState
                | PlaytimeMatrixAction::Panic => return None,
            },
            _ => return None,
        };
        Some(t)
    }

    pub fn tag_scope(&self) -> TagScope {
        TagScope {
            tags: self.tags.iter().cloned().collect(),
        }
    }

    pub fn playtime_row_action(&self) -> PlaytimeRowAction {
        self.playtime_row_action
    }

    pub fn stop_column_if_slot_empty(&self) -> bool {
        self.stop_column_if_slot_empty
    }

    pub fn clip_transport_options(&self) -> crate::domain::ClipTransportOptions {
        crate::domain::ClipTransportOptions {
            stop_column_if_slot_empty: self.stop_column_if_slot_empty,
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

    pub fn mouse_action(&self) -> MouseAction {
        match self.mouse_action_type {
            MouseActionType::MoveTo => MouseAction::MoveTo {
                axis: Some(self.axis),
            },
            MouseActionType::MoveBy => MouseAction::MoveBy {
                axis: Some(self.axis),
            },
            MouseActionType::PressOrRelease => MouseAction::PressOrRelease {
                button: Some(self.mouse_button),
            },
            MouseActionType::Scroll => MouseAction::Scroll {
                axis: Some(self.axis),
            },
        }
    }

    pub fn pot_filter_item_kind(&self) -> PotFilterKind {
        self.pot_filter_item_kind
    }

    pub fn included_targets(&self) -> &NonCryptoHashSet<LearnableTargetKind> {
        &self.included_targets
    }

    pub fn touch_cause(&self) -> TargetTouchCause {
        self.touch_cause
    }

    pub fn mapping_modification_kind(&self) -> MappingModificationKind {
        self.mapping_modification_kind
    }

    pub fn mapping_ref(&self) -> &MappingRefModel {
        &self.mapping_ref
    }

    pub fn set_mouse_action_without_notification(&mut self, mouse_action: MouseAction) {
        match mouse_action {
            MouseAction::MoveTo { axis } => {
                self.mouse_action_type = MouseActionType::MoveTo;
                self.axis = axis.unwrap_or_default();
            }
            MouseAction::MoveBy { axis } => {
                self.mouse_action_type = MouseActionType::MoveBy;
                self.axis = axis.unwrap_or_default();
            }
            MouseAction::PressOrRelease { button } => {
                self.mouse_action_type = MouseActionType::PressOrRelease;
                self.mouse_button = button.unwrap_or_default();
            }
            MouseAction::Scroll { axis } => {
                self.mouse_action_type = MouseActionType::Scroll;
                self.axis = axis.unwrap_or_default();
            }
        }
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
        compartment: CompartmentKind,
    ) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
            compartment,
        }
    }

    pub fn supports_axis(&self) -> bool {
        if !self.r#type.definition().supports_axis() {
            return false;
        }
        if self.r#type == ReaperTargetType::Mouse {
            matches!(
                self.mouse_action_type,
                MouseActionType::MoveTo | MouseActionType::MoveBy | MouseActionType::Scroll
            )
        } else {
            true
        }
    }

    pub fn supports_mouse_button(&self) -> bool {
        if !self.r#type.definition().supports_mouse_button() {
            return false;
        }
        matches!(self.mouse_action_type, MouseActionType::PressOrRelease)
    }

    pub fn supports_gang_selected(&self) -> bool {
        self.r#type.definition().supports_gang_selected()
    }

    pub fn supports_gang_grouping(&self) -> bool {
        self.r#type.definition().supports_gang_grouping()
    }

    pub fn supports_track(&self) -> bool {
        if !self.r#type.supports_track() {
            return false;
        }
        self.requires_track_apart_from_type()
    }

    pub fn supports_fx_chain(&self) -> bool {
        let target_type = self.r#type;
        if !target_type.supports_fx_chain() {
            return false;
        }
        match self.r#type {
            t if t.supports_fx() => self.fx_type.requires_fx_chain(),
            _ => true,
        }
    }

    pub fn supports_track_must_be_selected(&self) -> bool {
        if !self.r#type.supports_track_must_be_selected() {
            return false;
        }
        self.uses_track_apart_from_type()
    }

    pub fn supports_osc_arg_value_range(&self) -> bool {
        self.category == TargetCategory::Reaper
            && self.osc_arg_index.is_some()
            && self.osc_arg_type_tag.supports_value_range()
    }

    /// "Requires" means that it requires the user to provide a track in the GUI.
    fn requires_track_apart_from_type(&self) -> bool {
        if !self.uses_track_apart_from_type() {
            return false;
        }
        if self.r#type.supports_fx() && !self.fx_type.requires_fx_chain() {
            return false;
        }
        true
    }

    /// "Uses" means that it works on a track (even if the user doesn't need to provide it).
    ///
    /// It makes sense then to present the "Track must be selected" checkbox then.
    fn uses_track_apart_from_type(&self) -> bool {
        match self.r#type {
            ReaperTargetType::PlaytimeSlotTransportAction => {
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

    pub fn supports_mapping_snapshot_id(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        use ReaperTargetType::*;
        match self.r#type {
            LoadMappingSnapshot => {
                self.mapping_snapshot_type_for_load == MappingSnapshotTypeForLoad::ById
            }
            TakeMappingSnapshot => {
                self.mapping_snapshot_type_for_take == MappingSnapshotTypeForTake::ById
            }
            _ => false,
        }
    }

    pub fn create_control_element(&self) -> VirtualControlElement {
        VirtualControlElement::new(self.control_element_id, self.control_element_character)
    }

    fn is_reaper(&self) -> bool {
        self.category == TargetCategory::Reaper
    }

    pub fn is_virtual(&self) -> bool {
        self.category == TargetCategory::Virtual
    }

    fn command_id_label(&self) -> Cow<str> {
        match self.resolve_action() {
            None => "-".into(),
            Some(action) => {
                if action.is_available() {
                    action
                        .command_id()
                        .expect("should be available")
                        .to_string()
                        .into()
                } else if let Some(command_name) = action.command_name() {
                    format!("<Not present> ({})", command_name.to_str()).into()
                } else {
                    "<Not present>".into()
                }
            }
        }
    }

    pub fn resolve_action(&self) -> Option<Action> {
        let command_name = self.smart_command_name.as_deref()?;
        build_action_from_smart_command_name(
            SectionId::new(self.action_scope.section_id()),
            command_name,
        )
    }

    pub fn resolved_available_action(&self) -> Result<Action, &'static str> {
        let action = self.resolve_action().ok_or("action not set")?;
        if !action.is_available() {
            return Err("action not available");
        }
        Ok(action.clone())
    }

    pub fn action_name_label(&self) -> Cow<str> {
        match self.resolved_available_action().ok() {
            None => "-".into(),
            Some(a) => a.name().expect("should be available").into_string().into(),
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
                    Action => match self.0.resolved_available_action().ok() {
                        None => write!(f, "Action {}", self.0.command_id_label()),
                        Some(a) => f.write_str(a.name().expect("should be available").to_str()),
                    },
                    AutomationModeOverride => {
                        write!(f, "{}: ", tt.short_name())?;
                        use AutomationModeOverrideType::*;
                        let ovr_type = self.0.automation_mode_override_type;
                        match ovr_type {
                            None | Bypass => write!(f, "{ovr_type}"),
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
                    write!(f, "{} {}", self.0.control_element_character, i + 1)
                }
                VirtualControlElementId::Named(n) => {
                    write!(f, "{} ({})", n, self.0.control_element_character)
                }
            },
        }
    }
}

pub struct TargetModelFormatMultiLine<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    session: &'a UnitModel,
    compartment: CompartmentKind,
}

impl<'a> TargetModelFormatMultiLine<'a> {
    pub fn new(
        target: &'a TargetModel,
        session: &'a UnitModel,
        compartment: CompartmentKind,
    ) -> Self {
        TargetModelFormatMultiLine {
            target,
            context: session.extended_context(),
            session,
            compartment,
        }
    }

    fn track_label(&self) -> String {
        let virtual_track = self.target.virtual_track();
        let virtual_track = match virtual_track.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(t) => t,
        };
        if self.target.supports_track() {
            get_virtual_track_label(virtual_track, self.compartment, self.context)
        } else {
            TARGET_OBJECT_IRRELEVANT_LABEL.to_string()
        }
    }

    fn mapping_snapshot_for_load_label(&self) -> String {
        match self.target.mapping_snapshot_type_for_load {
            MappingSnapshotTypeForLoad::Initial => MappingSnapshotTypeForLoad::Initial.to_string(),
            MappingSnapshotTypeForLoad::ById => self.mapping_snapshot_id_label(),
        }
    }

    fn mapping_snapshot_for_take_label(&self) -> String {
        match self.target.mapping_snapshot_type_for_take {
            MappingSnapshotTypeForTake::LastLoaded => {
                MappingSnapshotTypeForTake::LastLoaded.to_string()
            }
            MappingSnapshotTypeForTake::ById => self.mapping_snapshot_id_label(),
        }
    }

    fn mapping_snapshot_id_label(&self) -> String {
        match &self.target.mapping_snapshot_id {
            None => "-".into(),
            Some(id) => id.to_string(),
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
        let fx_descriptor = match self.target.fx_descriptor() {
            Ok(d) => d,
            Err(_) => return TARGET_UNDEFINED_LABEL.into(),
        };
        get_virtual_fx_label(&fx_descriptor, self.compartment, self.context).into()
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
                    format!("<Not present> ({virtual_param})").into()
                }
            }
            _ => virtual_param.to_string().into(),
        }
    }

    fn bookmark_label(&self) -> String {
        // TODO-medium We should do this similar to the other target objects and introduce a
        //  virtual struct.
        let bookmark_type = self.target.bookmark_type;
        let anchor_type = self.target.bookmark_anchor_type;
        let bookmark_ref = self.target.bookmark_ref;
        match anchor_type {
            BookmarkAnchorType::Id => {
                let res = find_bookmark(
                    self.context.context().project_or_current_project(),
                    bookmark_type,
                    anchor_type,
                    bookmark_ref,
                );
                if let Ok(res) = res {
                    get_bookmark_label_by_id(bookmark_type, res.basic_info.id, &res.bookmark.name())
                } else {
                    get_non_present_bookmark_label(anchor_type, bookmark_ref)
                }
            }
            BookmarkAnchorType::Index => {
                get_bookmark_label_by_position(bookmark_type, bookmark_ref)
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
}

const UNIT_NOT_FOUND_LABEL: &str = "<Unit not found>";
const MAPPING_NOT_FOUND_LABEL: &str = "<Mapping not found>";

const NONE_LABEL: &str = "<None>";

const MAPPING_LABEL: &str = "Mapping: ";

impl<'a> Display for TargetModelFormatMultiLine<'a> {
    /// Produces a multi-line description of the target.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetCategory::*;
        match self.target.category {
            Reaper => {
                use ReaperTargetType::*;
                let tt = self.target.r#type;
                match tt {
                    PlaytimeSlotTransportAction => {
                        let slot = &self.target.playtime_slot;
                        let action = &self.target.playtime_slot_transport_action;
                        write!(f, "{tt}\n{slot}\n{action}")
                    }
                    PlaytimeSlotSeek => {
                        let slot = &self.target.playtime_slot;
                        write!(f, "{tt}\n{slot}")
                    }
                    PlaytimeSlotVolume => {
                        let slot = &self.target.playtime_slot;
                        write!(f, "{tt}\n{slot}")
                    }
                    PlaytimeColumnAction => {
                        let column = &self.target.playtime_column;
                        let action = &self.target.playtime_column_action;
                        write!(f, "{tt}\n{column}\n{action}")
                    }
                    PlaytimeRowAction => {
                        let row = &self.target.playtime_row;
                        let action = &self.target.playtime_row_action;
                        write!(f, "{tt}\n{row}\n{action}")
                    }
                    PlaytimeMatrixAction => {
                        let action = &self.target.playtime_matrix_action;
                        write!(f, "{tt}\n{action}")
                    }
                    PlaytimeControlUnitScroll | PlaytimeBrowseCells => {
                        let axis = &self.target.axis;
                        write!(f, "{tt}\n{axis}")
                    }
                    PlaytimeSlotManagementAction => {
                        let slot = &self.target.playtime_slot;
                        let action = &self.target.playtime_slot_management_action;
                        write!(f, "{tt}\n{slot}\n{action}")
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
                    | BrowseFxs | AllTrackFxEnable | TrackParentSend => {
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
                    FxOpen | FxEnable | FxPreset | FxTool => write!(
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
                    LoadMappingSnapshot => {
                        write!(
                            f,
                            "{}\n\
                            Snapshot: {}\n\
                            Tags: {}",
                            tt,
                            self.mapping_snapshot_for_load_label(),
                            format_tags_as_csv(self.target.tags())
                        )
                    }
                    TakeMappingSnapshot => {
                        write!(
                            f,
                            "{}\n\
                            Snapshot: {}\n\
                            Tags: {}",
                            tt,
                            self.mapping_snapshot_for_take_label(),
                            format_tags_as_csv(self.target.tags())
                        )
                    }
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
                    Mouse => {
                        write!(f, "{}\n{}", tt, self.target.mouse_action_type)?;
                        if self.target.supports_axis() {
                            write!(f, "\n{}", self.target.axis)?;
                        }
                        if self.target.supports_mouse_button() {
                            write!(f, "\n{}", self.target.mouse_button)?;
                        }
                        Ok(())
                    }
                    ModifyMapping => {
                        write!(f, "{}\n{}\n", tt, self.target.mapping_modification_kind)?;
                        match &self.target.mapping_ref {
                            MappingRefModel::OwnMapping { mapping_id } => {
                                MAPPING_LABEL.fmt(f)?;
                                if let Some(id) = mapping_id {
                                    let qualified_id =
                                        QualifiedMappingId::new(self.compartment, *id);
                                    if let Some(m) =
                                        self.session.find_mapping_by_qualified_id(qualified_id)
                                    {
                                        m.borrow().effective_name().fmt(f)?;
                                    } else {
                                        MAPPING_NOT_FOUND_LABEL.fmt(f)?;
                                    }
                                } else {
                                    NONE_LABEL.fmt(f)?;
                                }
                            }
                            MappingRefModel::ForeignMapping {
                                session_id,
                                mapping_key,
                            } => {
                                write!(f, "Unit: {session_id}\n{MAPPING_LABEL}")?;
                                if let Some(mapping_key) = mapping_key {
                                    let session = self
                                        .context
                                        .control_context()
                                        .unit_container
                                        .find_session_by_id(session_id);
                                    if let Some(session) = session {
                                        let session = session.borrow();
                                        if let Some(m) = session
                                            .find_mapping_by_key(self.compartment, mapping_key)
                                        {
                                            m.borrow().effective_name().fmt(f)?;
                                        } else {
                                            MAPPING_NOT_FOUND_LABEL.fmt(f)?;
                                        }
                                    } else {
                                        UNIT_NOT_FOUND_LABEL.fmt(f)?;
                                    }
                                } else {
                                    NONE_LABEL.fmt(f)?;
                                }
                            }
                        }
                        Ok(())
                    }
                    _ => write!(f, "{tt}"),
                }
            }
            Virtual => write!(f, "Virtual\n{}", self.target.create_control_element()),
        }
    }
}

pub fn get_fx_param_label(fx_param: Option<&FxParameter>, index: u32) -> Cow<'static, str> {
    let position = index + 1;
    match fx_param.and_then(|p| p.name().ok()) {
        None => format!("{position}. <Not present>").into(),
        Some(name) => {
            let name = name.into_inner();
            // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
            let name = name.to_string_lossy();
            if name.is_empty() {
                position.to_string().into()
            } else {
                format!("{position}. {name}").into()
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
            None => format!("<Not present> ({virtual_chain_fx})"),
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
            get_fx_name(fx)
        } else {
            "".to_owned()
        }
    )
}

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: CompartmentKind,
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
        first_effective_fx(
            &self.target.fx_descriptor()?,
            self.compartment,
            self.context,
        )
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
        let virtual_track = self
            .target
            .virtual_track()
            .ok_or("virtual track not complete")?;
        first_effective_track(&virtual_track, self.compartment, self.context)
    }
}

pub fn first_effective_track(
    virtual_track: &VirtualTrack,
    compartment: CompartmentKind,
    context: ExtendedProcessorContext,
) -> Result<Track, &'static str> {
    virtual_track
        .resolve(context, compartment)
        .map_err(|_| "particular track couldn't be resolved")?
        .into_iter()
        .next()
        .ok_or("resolved to empty track list")
}

pub fn first_effective_fx(
    fx_descriptor: &FxDescriptor,
    compartment: CompartmentKind,
    context: ExtendedProcessorContext,
) -> Result<Fx, &'static str> {
    fx_descriptor
        .resolve(context, compartment)?
        .into_iter()
        .next()
        .ok_or("resolves to empty FX list")
}

pub fn get_bookmark_label_by_id(bookmark_type: BookmarkType, id: BookmarkId, name: &str) -> String {
    format!(
        "{} {}. {}",
        FormattableBookmarkType(bookmark_type),
        id,
        name
    )
}

pub fn get_bookmark_label_by_position(
    bookmark_type: BookmarkType,
    index_within_type: u32,
) -> String {
    format!(
        "{} #{}",
        FormattableBookmarkType(bookmark_type),
        index_within_type + 1
    )
}

struct FormattableBookmarkType(BookmarkType);

impl Display for FormattableBookmarkType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            BookmarkType::Marker => f.write_str("Marker"),
            BookmarkType::Region => f.write_str("Region"),
        }
    }
}

pub fn get_non_present_bookmark_label(
    anchor_type: BookmarkAnchorType,
    bookmark_ref: u32,
) -> String {
    match anchor_type {
        BookmarkAnchorType::Id => format!("<Not present> (ID {bookmark_ref})"),
        BookmarkAnchorType::Index => format!("{bookmark_ref}. <Not present>"),
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    EnumIter,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetCategory {
    #[default]
    #[serde(rename = "reaper")]
    #[display(fmt = "Real")]
    Reaper,
    #[serde(rename = "virtual")]
    #[display(fmt = "Virtual")]
    Virtual,
}

impl TargetCategory {
    pub fn default_for(compartment: CompartmentKind) -> Self {
        use TargetCategory::*;
        match compartment {
            CompartmentKind::Controller => Virtual,
            CompartmentKind::Main => Reaper,
        }
    }

    pub fn is_allowed_in(self, compartment: CompartmentKind) -> bool {
        use TargetCategory::*;
        match compartment {
            CompartmentKind::Controller => true,
            CompartmentKind::Main => match self {
                Reaper => true,
                Virtual => false,
            },
        }
    }
}

fn virtualize_track(
    track: &Track,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualTrack {
    if !track.is_available() {
        // Fixes https://github.com/helgoboss/helgobox/issues/1126
        return VirtualTrack::ById(*track.guid());
    }
    let own_track = context.track().cloned().unwrap_or_else(|| {
        context
            .project_or_current_project()
            .master_track()
            .expect("no way")
    });
    if own_track == *track {
        VirtualTrack::This
    } else if track.is_master_track() {
        VirtualTrack::Master
    } else if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
        // It doesn't make sense to refer to tracks via ID if we are on the monitoring FX chain.
        VirtualTrack::ByIndex {
            index: track.index().expect("impossible"),
            scope: TrackScope::AllTracks,
        }
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
            } else if let Ok(guid) = fx.get_or_query_guid() {
                VirtualChainFx::ById(guid, Some(fx.index()))
            } else {
                VirtualChainFx::ByIdOrIndex(None, fx.index())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, TryFromPrimitive, IntoPrimitive, Display)]
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
    #[display(fmt = "<Dynamic (TCP)>")]
    DynamicTcp,
    #[display(fmt = "<Dynamic (MCP)>")]
    DynamicMcp,
    #[display(fmt = "<Master>")]
    Master,
    #[display(fmt = "<Unit>")]
    Unit,
    #[display(fmt = "Particular")]
    ById,
    #[display(fmt = "Named")]
    ByName,
    #[display(fmt = "All named")]
    AllByName,
    #[display(fmt = "At position")]
    ByIndex,
    #[display(fmt = "At TCP position")]
    ByIndexTcp,
    #[display(fmt = "At MCP position")]
    ByIndexMcp,
    #[display(fmt = "By ID or name (legacy)")]
    ByIdOrName,
    #[display(fmt = "From Playtime column")]
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
    EnumIter,
    TryFromPrimitive,
    IntoPrimitive,
    Serialize,
    Deserialize,
    Display,
)]
#[repr(usize)]
pub enum MappingSnapshotTypeForLoad {
    #[display(fmt = "<Initial>")]
    #[serde(rename = "initial")]
    Initial,
    #[display(fmt = "By ID")]
    ById,
}

impl Default for MappingSnapshotTypeForLoad {
    fn default() -> Self {
        Self::Initial
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    EnumIter,
    TryFromPrimitive,
    IntoPrimitive,
    Serialize,
    Deserialize,
    Display,
)]
#[repr(usize)]
pub enum MappingSnapshotTypeForTake {
    #[display(fmt = "<Last loaded>")]
    #[serde(rename = "last-loaded")]
    LastLoaded,
    #[display(fmt = "By ID")]
    ById,
}

impl Default for MappingSnapshotTypeForTake {
    fn default() -> Self {
        Self::LastLoaded
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    EnumIter,
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
    #[display(fmt = "At position")]
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
            Dynamic { scope, .. } => match scope {
                TrackScope::AllTracks => Self::Dynamic,
                TrackScope::TracksVisibleInTcp => Self::DynamicTcp,
                TrackScope::TracksVisibleInMcp => Self::DynamicMcp,
            },
            Master => Self::Master,
            Unit => Self::Unit,
            ByIdOrName(_, _) => Self::ByIdOrName,
            ById(_) => Self::ById,
            ByName { allow_multiple, .. } => {
                if *allow_multiple {
                    Self::AllByName
                } else {
                    Self::ByName
                }
            }
            ByIndex { scope, .. } => match scope {
                TrackScope::AllTracks => Self::ByIndex,
                TrackScope::TracksVisibleInTcp => Self::ByIndexTcp,
                TrackScope::TracksVisibleInMcp => Self::ByIndexMcp,
            },
            FromClipColumn { .. } => Self::FromClipColumn,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic | Self::DynamicTcp | Self::DynamicMcp)
    }

    pub fn is_by_index(&self) -> bool {
        matches!(self, Self::ByIndex | Self::ByIndexTcp | Self::ByIndexMcp)
    }

    pub fn virtual_track_scope(&self) -> Option<TrackScope> {
        use VirtualTrackType::*;
        match self {
            ByIndex | Dynamic => Some(TrackScope::AllTracks),
            ByIndexTcp | DynamicTcp => Some(TrackScope::TracksVisibleInTcp),
            ByIndexMcp | DynamicMcp => Some(TrackScope::TracksVisibleInMcp),
            _ => None,
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
    EnumIter,
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
    #[display(fmt = "<Unit>")]
    #[serde(rename = "instance")]
    Unit,
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "Particular")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "Named")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "All named")]
    AllByName,
    #[display(fmt = "At position")]
    #[serde(rename = "index")]
    ByIndex,
    #[display(fmt = "By ID or pos (legacy)")]
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
            LastFocused => VirtualFxType::Focused,
            Unit => VirtualFxType::Unit,
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

    pub fn requires_fx_chain(&self) -> bool {
        use VirtualFxType::*;
        match self {
            This => false,
            Focused => false,
            Dynamic => true,
            Unit => false,
            ById => true,
            ByName => true,
            AllByName => true,
            ByIndex => true,
            ByIdOrIndex => true,
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
    EnumIter,
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
    #[display(fmt = "Named")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "Particular")]
    #[serde(rename = "index")]
    ById,
    #[display(fmt = "At position")]
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
    EnumIter,
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
    #[display(fmt = "Particular")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "Named")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "At position")]
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
    pub clip_column: PlaytimeColumnDescriptor,
    pub clip_column_track_context: ClipColumnTrackContext,
}

impl TrackPropValues {
    pub fn from_virtual_track(track: VirtualTrack) -> Self {
        Self {
            r#type: VirtualTrackType::from_virtual_track(&track),
            id: track.id(),
            name: track.name().unwrap_or_default(),
            index: track.index().unwrap_or_default(),
            clip_column: {
                match track.clip_column().unwrap_or(&Default::default()) {
                    VirtualPlaytimeColumn::Active => PlaytimeColumnDescriptor::Active,
                    VirtualPlaytimeColumn::ByIndex(i) => {
                        PlaytimeColumnDescriptor::ByIndex(ColumnAddress::new(*i))
                    }
                    VirtualPlaytimeColumn::Dynamic(_) => PlaytimeColumnDescriptor::Dynamic {
                        expression: Default::default(),
                    },
                }
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
    EnumIter,
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
    EnumIter,
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
    EnumIter,
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
    EnumIter,
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
                } => c
                    .project_or_current_project()
                    .track_by_guid(id)
                    .ok()
                    .and_then(|t| if t.is_available() { Some(t) } else { None }),
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
            _ => self.fx.as_ref()?.get_or_query_guid().ok(),
        }
    }

    pub fn name(&self) -> Option<String> {
        Some(get_fx_name(self.fx.as_ref()?))
    }

    pub fn index(&self) -> Option<u32> {
        Some(self.fx.as_ref()?.index())
    }
}

const TARGET_UNDEFINED_LABEL: &str = "<Undefined>";
const TARGET_OBJECT_IRRELEVANT_LABEL: &str = "<Irrelevant>";

pub fn get_virtual_track_label(
    virtual_track: &VirtualTrack,
    compartment: CompartmentKind,
    context: ExtendedProcessorContext,
) -> String {
    use VirtualTrack::*;
    match virtual_track {
        ById(_) | ByIdOrName(_, _) => {
            if let Ok(t) = first_effective_track(virtual_track, compartment, context) {
                get_track_label(&t)
            } else {
                get_non_present_virtual_track_label(virtual_track)
            }
        }
        _ => virtual_track.to_string(),
    }
}

pub fn get_virtual_fx_label(
    fx_descriptor: &FxDescriptor,
    compartment: CompartmentKind,
    context: ExtendedProcessorContext,
) -> String {
    match &fx_descriptor.fx {
        VirtualFx::ChainFx { chain_fx, .. } => {
            use VirtualChainFx::*;
            match chain_fx {
                ById(_, _) | ByIdOrIndex(_, _) => {
                    let optional_fx = first_effective_fx(fx_descriptor, compartment, context);
                    get_optional_fx_label(chain_fx, optional_fx.ok().as_ref())
                }
                _ => fx_descriptor.fx.to_string(),
            }
        }
        _ => fx_descriptor.fx.to_string(),
    }
}

pub fn get_track_label(track: &Track) -> String {
    match track.location() {
        TrackLocation::MasterTrack => MASTER_TRACK_LABEL.into(),
        TrackLocation::NormalTrack(i) => {
            let position = i + 1;
            let name = track.name().expect("non-master track must have name");
            let name = name.to_str();
            if name.is_empty() {
                format!("{position}. <no name>")
            } else {
                format!("{position}. {name}")
            }
        }
    }
}

pub const MASTER_TRACK_LABEL: &str = "<Master track>";

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

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    Default,
    Serialize,
    Deserialize,
    derive_more::Display,
    EnumIter,
    num_enum::TryFromPrimitive,
    num_enum::IntoPrimitive,
)]
#[repr(usize)]
pub enum MappingModificationKind {
    #[display(fmt = "Learn target")]
    #[default]
    LearnTarget,
    #[display(fmt = "Set target to last touched")]
    SetTargetToLastTouched,
}

impl MappingModificationKind {
    pub fn from_modification(modification: &MappingModification) -> Self {
        match modification {
            MappingModification::LearnTarget(_) => Self::LearnTarget,
            MappingModification::SetTargetToLastTouched(_) => Self::SetTargetToLastTouched,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, derive_more::Display, EnumIter)]
pub enum MakeFxNonStickyMode {
    #[display(fmt = "<Focused>")]
    Focused,
    #[display(fmt = "<Unit>")]
    Unit,
    #[display(fmt = "Named")]
    Named,
    #[display(fmt = "All named")]
    AllNamed,
    #[display(fmt = "At position")]
    AtPosition,
}

#[derive(Copy, Clone, Eq, PartialEq, derive_more::Display, EnumIter)]
pub enum MakeTrackNonStickyMode {
    #[display(fmt = "<Selected>")]
    Selected,
    #[display(fmt = "<All selected>")]
    AllSelected,
    #[display(fmt = "<Unit>")]
    Unit,
    #[display(fmt = "Named")]
    Named,
    #[display(fmt = "All named")]
    AllNamed,
    #[display(fmt = "At position")]
    AtPosition,
    #[display(fmt = "At TCP position")]
    AtTcpPosition,
    #[display(fmt = "At MCP position")]
    AtMcpPosition,
}

impl MakeFxNonStickyMode {
    pub fn build_virtual_fx(&self, fx: Option<&Fx>) -> Option<VirtualFx> {
        let virtual_fx = match self {
            MakeFxNonStickyMode::Focused => VirtualFx::LastFocused,
            MakeFxNonStickyMode::Unit => VirtualFx::Unit,
            MakeFxNonStickyMode::Named | MakeFxNonStickyMode::AllNamed => {
                let fx = fx?;
                VirtualFx::ChainFx {
                    is_input_fx: fx.is_input_fx(),
                    chain_fx: VirtualChainFx::ByName {
                        wild_match: WildMatch::new(fx.name().to_str()),
                        allow_multiple: *self == MakeFxNonStickyMode::AllNamed,
                    },
                }
            }
            MakeFxNonStickyMode::AtPosition => {
                let fx = fx?;
                VirtualFx::ChainFx {
                    is_input_fx: fx.is_input_fx(),
                    chain_fx: VirtualChainFx::ByIndex(fx.index()),
                }
            }
        };
        Some(virtual_fx)
    }
}

impl MakeTrackNonStickyMode {
    pub fn build_virtual_track(&self, track: Option<&Track>) -> Option<VirtualTrack> {
        let virtual_track = match self {
            MakeTrackNonStickyMode::Selected => VirtualTrack::Selected {
                allow_multiple: false,
            },
            MakeTrackNonStickyMode::AllSelected => VirtualTrack::Selected {
                allow_multiple: true,
            },
            MakeTrackNonStickyMode::Unit => VirtualTrack::Unit,
            MakeTrackNonStickyMode::Named | MakeTrackNonStickyMode::AllNamed => {
                if let Some(name) = track?.name() {
                    VirtualTrack::ByName {
                        wild_match: WildMatch::new(name.to_str()),
                        allow_multiple: *self == MakeTrackNonStickyMode::AllNamed,
                    }
                } else {
                    VirtualTrack::Master
                }
            }
            MakeTrackNonStickyMode::AtPosition
            | MakeTrackNonStickyMode::AtTcpPosition
            | MakeTrackNonStickyMode::AtMcpPosition => {
                if let Some(index) = track?.index() {
                    VirtualTrack::ByIndex {
                        index,
                        scope: match *self {
                            MakeTrackNonStickyMode::AtPosition => TrackScope::AllTracks,
                            MakeTrackNonStickyMode::AtTcpPosition => TrackScope::TracksVisibleInTcp,
                            MakeTrackNonStickyMode::AtMcpPosition => TrackScope::TracksVisibleInMcp,
                            _ => unreachable!(),
                        },
                    }
                } else {
                    VirtualTrack::Master
                }
            }
        };
        Some(virtual_track)
    }
}

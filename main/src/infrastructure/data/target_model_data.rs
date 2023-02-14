use super::f32_as_u32;
use super::none_if_minus_one;
use reaper_high::{BookmarkType, Fx, Guid, Reaper};

use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, Change, FxParameterPropValues, FxPropValues,
    FxSnapshot, MappingSnapshotTypeForLoad, MappingSnapshotTypeForTake, RealearnAutomationMode,
    RealearnTrackArea, TargetCategory, TargetCommand, TargetModel, TargetUnit, TrackPropValues,
    TrackRoutePropValues, TrackRouteSelectorType, VirtualControlElementType,
    VirtualFxParameterType, VirtualFxType, VirtualTrackType,
};
use crate::base::default_util::{
    bool_true, deserialize_null_default, is_bool_true, is_default, is_none_or_some_default,
};
use crate::base::notification;
use crate::domain::{
    get_fx_chains, ActionInvocationType, AnyOnParameter, Compartment, Exclusivity,
    ExtendedProcessorContext, FxDisplayType, GroupKey, MappingKey, OscDeviceId, ReaperTargetType,
    SeekOptions, SendMidiDestination, SoloBehavior, Tag, TouchedRouteParameterType,
    TouchedTrackParameterType, TrackExclusivity, TrackGangBehavior, TrackRouteType,
    TransportAction, VirtualTrack,
};
use crate::infrastructure::data::common::OscValueRange;
use crate::infrastructure::data::{
    DataToModelConversionContext, MigrationDescriptor, ModelToDataConversionContext,
    VirtualControlElementIdData,
};
use crate::infrastructure::plugin::App;
use helgoboss_learn::{AbsoluteValue, Fraction, OscTypeTag, UnitValue};
use playtime_api::persistence::{ClipPlayStartTiming, ClipPlayStopTiming};
use realearn_api::persistence::{
    BrowseTracksMode, ClipColumnAction, ClipColumnDescriptor, ClipColumnTrackContext,
    ClipManagementAction, ClipMatrixAction, ClipRowAction, ClipRowDescriptor, ClipSlotDescriptor,
    ClipTransportAction, FxToolAction, LearnableMappingFeature, MappingSnapshotDescForLoad,
    MappingSnapshotDescForTake, MonitoringMode, MouseAction, PotFilterItemKind, SeekBehavior,
    TargetValue, TrackScope, TrackToolAction,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetModelData {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub category: TargetCategory,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub unit: TargetUnit,
    // reaper_type would be a better name but we need backwards compatibility
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub r#type: ReaperTargetType,
    // Action target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub command_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub invocation_type: ActionInvocationType,
    // Until ReaLearn 1.0.0-beta6
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing
    )]
    pub invoke_relative: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub with_track: bool,
    // Track target
    #[serde(flatten)]
    pub track_data: TrackData,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub enable_only_if_track_is_selected: bool,
    // FX target
    #[serde(flatten)]
    pub fx_data: FxData,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub enable_only_if_fx_has_focus: bool,
    /// Introduced with ReaLearn v2.14.0-pre.5.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub use_selection_ganging: Option<bool>,
    /// Introduced with ReaLearn v2.14.0-pre.5.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub use_track_grouping: Option<bool>,
    // Track route target
    #[serde(flatten)]
    pub track_route_data: TrackRouteData,
    // FX parameter target
    #[serde(flatten)]
    pub fx_parameter_data: FxParameterData,
    // Track selection target (replaced with `track_exclusivity` since v2.4.0)
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub select_exclusively: Option<bool>,
    // Track solo target (since v2.4.0, also changed default from "ignore routing" to "in place")
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub solo_behavior: Option<SoloBehavior>,
    // Seek and goto bookmark target, available from v2.14.0-pre.2
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub seek_behavior: Option<SeekBehavior>,
    // Toggleable track targets (since v2.4.0)
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub track_exclusivity: TrackExclusivity,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub track_tool_action: TrackToolAction,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub fx_tool_action: FxToolAction,
    // Transport target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub transport_action: TransportAction,
    // Any-on target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub any_on_parameter: AnyOnParameter,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub control_element_type: VirtualControlElementType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub control_element_index: VirtualControlElementIdData,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub fx_snapshot: Option<FxSnapshot>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub touched_parameter_type: TouchedTrackParameterType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub touched_route_parameter_type: TouchedRouteParameterType,
    // Bookmark target
    #[serde(flatten)]
    pub bookmark_data: BookmarkData,
    // Seek target
    #[serde(flatten)]
    pub seek_options: SeekOptions,
    // Track show target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub track_area: RealearnTrackArea,
    // Track automation mode target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub track_automation_mode: RealearnAutomationMode,
    // Track monitoring mode target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub track_monitoring_mode: MonitoringMode,
    // Automation mode override target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub automation_mode_override_type: AutomationModeOverrideType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    #[serde(alias = "cycleThroughTracksMode")]
    pub browse_tracks_mode: BrowseTracksMode,
    // FX Open and Browse FXs target
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub fx_display_type: FxDisplayType,
    // Track selection related targets
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub scroll_arrange_view: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub scroll_mixer: bool,
    // Send MIDI
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub send_midi_destination: SendMidiDestination,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub raw_midi_pattern: String,
    // Send OSC
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_address_pattern: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_index: Option<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_type: OscTypeTag,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_value_range: OscValueRange,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_dev_id: Option<OscDeviceId>,
    // Mouse
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mouse_action: MouseAction,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub poll_for_feedback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub retrigger: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub tags: Vec<Tag>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mapping_snapshot: MappingSnapshotDescForLoad,
    /// Introduced with ReaLearn v2.14.0-pre.1.
    /// Before that, it was always "By ID" and encoded as part of "mapping_snapshot".
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub take_mapping_snapshot: Option<MappingSnapshotDescForTake>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mapping_snapshot_default_value: Option<TargetValue>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub exclusivity: Exclusivity,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub group_id: GroupKey,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub active_mappings_only: bool,
    /// Replaced with `clip_slot` since v2.12.0-pre.5
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub slot_index: usize,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_management_action: ClipManagementAction,
    /// Not supported anymore since v2.12.0-pre.5
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub next_bar: bool,
    /// Not supported anymore since v2.12.0-pre.5
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub buffered: bool,
    /// New since ReaLearn v2.12.0-pre.5
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_slot: Option<ClipSlotDescriptor>,
    /// Clip matrix column.
    ///
    /// For track targets, this contains the clip column from which we want to "borrow" the track.
    ///
    /// For clip column targets, this contains the clip column to which we want to refer.
    ///
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_column: ClipColumnDescriptor,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_row: ClipRowDescriptor,
    /// New since ReaLearn v2.13.0-pre.4.
    ///
    /// Migrated from `transport_action` if not given.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_transport_action: Option<ClipTransportAction>,
    /// New since ReaLearn v2.13.0-pre.4.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_column_action: ClipColumnAction,
    /// New since ReaLearn v2.13.0-pre.4.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_row_action: ClipRowAction,
    /// New since ReaLearn v2.13.0-pre.4.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_matrix_action: ClipMatrixAction,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub record_only_if_track_armed: bool,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub stop_column_if_slot_empty: bool,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_play_start_timing: Option<ClipPlayStartTiming>,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_play_stop_timing: Option<ClipPlayStopTiming>,
    /// New since ReaLearn v2.13.0-pre.4
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub pot_filter_item_kind: PotFilterItemKind,
    /// New since ReaLearn v2.15.0-pre.1
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub learnable_feature: LearnableMappingFeature,
    /// New since ReaLearn v2.15.0-pre.1
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mapping_key: Option<MappingKey>,
}

impl TargetModelData {
    pub fn from_model(
        model: &TargetModel,
        conversion_context: &impl ModelToDataConversionContext,
    ) -> Self {
        let (track_data, track_selector_clip_column) = serialize_track(model.track());
        Self {
            category: model.category(),
            unit: model.unit(),
            r#type: model.target_type(),
            command_name: model.action().and_then(|a| match a.command_name() {
                // Built-in actions don't have a command name but a persistent command ID.
                // Use command ID as string.
                None => a.command_id().ok().map(|id| id.to_string()),
                // ReaScripts and custom actions have a command name as persistent identifier.
                Some(name) => Some(name.into_string()),
            }),
            invocation_type: model.action_invocation_type(),
            // Not serialized anymore because deprecated
            invoke_relative: None,
            track_data,
            enable_only_if_track_is_selected: model.enable_only_if_track_selected(),
            with_track: model.with_track(),
            fx_data: serialize_fx(model.fx()),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus(),
            use_selection_ganging: Some(model.fixed_gang_behavior().use_selection_ganging()),
            use_track_grouping: Some(model.fixed_gang_behavior().use_track_grouping()),
            track_route_data: serialize_track_route(model.track_route()),
            fx_parameter_data: serialize_fx_parameter(model.fx_parameter()),
            select_exclusively: None,
            solo_behavior: Some(model.solo_behavior()),
            seek_behavior: Some(model.seek_behavior()),
            track_exclusivity: model.track_exclusivity(),
            track_tool_action: model.track_tool_action(),
            fx_tool_action: model.fx_tool_action(),
            transport_action: model.transport_action(),
            any_on_parameter: model.any_on_parameter(),
            control_element_type: model.control_element_type(),
            control_element_index: VirtualControlElementIdData::from_model(
                model.control_element_id(),
            ),
            fx_snapshot: model.fx_snapshot().cloned(),
            touched_parameter_type: model.touched_track_parameter_type(),
            touched_route_parameter_type: model.touched_route_parameter_type(),
            bookmark_data: BookmarkData {
                anchor: model.bookmark_anchor_type(),
                r#ref: model.bookmark_ref(),
                is_region: model.bookmark_type() == BookmarkType::Region,
            },
            seek_options: model.seek_options(),
            track_area: model.track_area(),
            track_automation_mode: model.automation_mode(),
            track_monitoring_mode: model.monitoring_mode(),
            automation_mode_override_type: model.automation_mode_override_type(),
            browse_tracks_mode: model.browse_tracks_mode(),
            fx_display_type: model.fx_display_type(),
            scroll_arrange_view: model.scroll_arrange_view(),
            scroll_mixer: model.scroll_mixer(),
            send_midi_destination: model.send_midi_destination(),
            raw_midi_pattern: model.raw_midi_pattern().to_owned(),
            osc_address_pattern: model.osc_address_pattern().to_owned(),
            osc_arg_index: model.osc_arg_index(),
            osc_arg_type: model.osc_arg_type_tag(),
            osc_arg_value_range: OscValueRange::from_interval(model.osc_arg_value_range()),
            osc_dev_id: model.osc_dev_id(),
            slot_index: 0,
            clip_management_action: model.clip_management_action().clone(),
            next_bar: false,
            buffered: false,
            poll_for_feedback: model.poll_for_feedback(),
            retrigger: model.retrigger(),
            tags: model.tags().to_vec(),
            mapping_snapshot: model.mapping_snapshot_desc_for_load(),
            take_mapping_snapshot: Some(model.mapping_snapshot_desc_for_take()),
            mapping_snapshot_default_value: model
                .mapping_snapshot_default_value()
                .map(convert_target_value_to_api),
            exclusivity: model.exclusivity(),
            group_id: conversion_context
                .group_key_by_id(model.group_id())
                .unwrap_or_default(),
            active_mappings_only: model.active_mappings_only(),
            clip_slot: if model.target_type().supports_clip_slot() {
                Some(model.clip_slot().clone())
            } else {
                None
            },
            clip_column: track_selector_clip_column.unwrap_or_else(|| model.clip_column().clone()),
            clip_row: model.clip_row().clone(),
            clip_transport_action: if model.target_type() == ReaperTargetType::ClipTransport {
                Some(model.clip_transport_action())
            } else {
                None
            },
            clip_column_action: model.clip_column_action(),
            clip_row_action: model.clip_row_action(),
            clip_matrix_action: model.clip_matrix_action(),
            record_only_if_track_armed: model.record_only_if_track_armed(),
            stop_column_if_slot_empty: model.stop_column_if_slot_empty(),
            clip_play_start_timing: model.clip_play_start_timing(),
            clip_play_stop_timing: model.clip_play_stop_timing(),
            mouse_action: model.mouse_action(),
            pot_filter_item_kind: model.pot_filter_item_kind(),
            learnable_feature: model.learnable_feature(),
            session_id: model
                .instance_id()
                .and_then(|id| conversion_context.session_id_by_instance_id(id)),
            mapping_key: model
                .mapping_id()
                .and_then(|id| conversion_context.mapping_key_by_id(id)),
        }
    }

    pub fn apply_to_model(
        &self,
        model: &mut TargetModel,
        compartment: Compartment,
        context: ExtendedProcessorContext,
        conversion_context: &impl DataToModelConversionContext,
    ) -> Result<(), &'static str> {
        self.apply_to_model_flexible(
            model,
            Some(context),
            Some(App::version()),
            compartment,
            conversion_context,
            &MigrationDescriptor::default(),
        )
    }

    /// The context - if available - will be used to resolve some track/FX properties for UI
    /// convenience. The context is necessary if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn apply_to_model_flexible(
        &self,
        model: &mut TargetModel,
        context: Option<ExtendedProcessorContext>,
        preset_version: Option<&Version>,
        compartment: Compartment,
        conversion_context: &impl DataToModelConversionContext,
        migration_descriptor: &MigrationDescriptor,
    ) -> Result<(), &'static str> {
        use TargetCommand as C;
        let final_category = if self.category.is_allowed_in(compartment) {
            self.category
        } else {
            TargetCategory::default_for(compartment)
        };
        model.change(C::SetCategory(final_category));
        model.change(C::SetUnit(self.unit));
        model.change(C::SetTargetType(self.r#type));
        if self.category == TargetCategory::Reaper && self.r#type == ReaperTargetType::Action {
            let reaper = Reaper::get();
            let action = match self.command_name.as_ref() {
                None => None,
                Some(command_name) => match command_name.parse::<u32>() {
                    // Could parse this as command ID integer. This is a built-in action.
                    Ok(command_id_int) => match command_id_int.try_into() {
                        Ok(command_id) => {
                            Some(reaper.main_section().action_by_command_id(command_id))
                        }
                        Err(_) => {
                            notification::warn(format!("Invalid command ID {}", command_id_int));
                            None
                        }
                    },
                    // Couldn't parse this as integer. This is a ReaScript or custom action.
                    Err(_) => Some(reaper.action_by_command_name(command_name.as_str())),
                },
            };
            model.change(C::SetAction(action));
        }
        let invocation_type = if let Some(invoke_relative) = self.invoke_relative {
            // Very old ReaLearn version
            if invoke_relative {
                ActionInvocationType::Relative
            } else {
                ActionInvocationType::Absolute14Bit
            }
        } else if migration_descriptor.action_invocation_swap_761 {
            match self.invocation_type {
                ActionInvocationType::Absolute14Bit => ActionInvocationType::Absolute7Bit,
                ActionInvocationType::Absolute7Bit => ActionInvocationType::Absolute14Bit,
                x => x,
            }
        } else {
            self.invocation_type
        };
        model.change(C::SetActionInvocationType(invocation_type));
        let track_prop_values = deserialize_track(&self.track_data, &self.clip_column);
        let _ = model.set_track_from_prop_values(
            track_prop_values,
            false,
            context.map(|c| c.context()),
        );
        model.change(C::SetEnableOnlyIfTrackSelected(
            self.enable_only_if_track_is_selected,
        ));
        let gang_behavior = match (self.use_selection_ganging, self.use_track_grouping) {
            (Some(use_selection_ganging), Some(use_track_grouping)) => {
                TrackGangBehavior::from_bools(
                    self.r#type.definition(),
                    use_selection_ganging,
                    use_track_grouping,
                )
            }
            _ => {
                // Older versions had a target-specific behavior.
                use ReaperTargetType::*;
                match self.r#type {
                    TrackArm | TrackMute | TrackSolo => TrackGangBehavior::GroupingOnly,
                    TrackPan | TrackVolume | TrackWidth => TrackGangBehavior::Off,
                    TrackMonitoringMode => TrackGangBehavior::Off,
                    _ => Default::default(),
                }
            }
        };
        model.change(C::SetGangBehavior(gang_behavior));
        model.change(C::SetBrowseTracksMode(self.browse_tracks_mode));
        model.change(C::SetWithTrack(self.with_track));
        let virtual_track = model.virtual_track().unwrap_or(VirtualTrack::This);
        let fx_prop_values = deserialize_fx(
            &self.fx_data,
            context.map(|c| (c, compartment, &virtual_track)),
            migration_descriptor,
        );
        let _ = model.set_fx_from_prop_values(fx_prop_values, false, context, compartment);
        model.change(C::SetEnableOnlyIfFxHasFocus(
            self.enable_only_if_fx_has_focus,
        ));
        let route_prop_values = deserialize_track_route(&self.track_route_data);
        let _ = model.set_route(route_prop_values);
        let fx_param_prop_values = deserialize_fx_parameter(&self.fx_parameter_data);
        let _ = model.set_fx_parameter(fx_param_prop_values);
        let track_exclusivity = if let Some(select_exclusively) = self.select_exclusively {
            // Should only be set in versions < 2.4.0.
            if select_exclusively {
                TrackExclusivity::ExclusiveWithinProject
            } else {
                TrackExclusivity::NonExclusive
            }
        } else {
            self.track_exclusivity
        };
        model.change(C::SetTrackExclusivity(track_exclusivity));
        let solo_behavior = self.solo_behavior.unwrap_or_else(|| {
            let is_old_preset = preset_version
                .map(|v| v < &Version::new(2, 4, 0))
                .unwrap_or(true);
            if is_old_preset {
                SoloBehavior::IgnoreRouting
            } else {
                SoloBehavior::InPlace
            }
        });
        model.change(C::SetSoloBehavior(solo_behavior));
        let seek_behavior = self.seek_behavior.unwrap_or_else(|| {
            // Older version didn't have an explicit seek behavior. Determine old behavior.
            match self.r#type {
                ReaperTargetType::Seek => SeekBehavior::ReaperPreference,
                ReaperTargetType::GoToBookmark => {
                    if self.bookmark_data.is_region {
                        // When targeting a region, we always used smooth region seeking
                        SeekBehavior::Smooth
                    } else {
                        // Otherwise we followed the REAPER preference
                        SeekBehavior::ReaperPreference
                    }
                }
                // Shouldn't matter for other targets
                _ => Default::default(),
            }
        });
        model.change(C::SetSeekBehavior(seek_behavior));
        model.change(C::SetTransportAction(self.transport_action));
        model.change(C::SetAnyOnParameter(self.any_on_parameter));
        model.change(C::SetControlElementType(self.control_element_type));
        model.change(C::SetControlElementId(
            self.control_element_index.to_model(),
        ));
        model.change(C::SetFxSnapshot(self.fx_snapshot.clone()));
        model.change(C::SetTouchedTrackParameterType(self.touched_parameter_type));
        model.change(C::SetTouchedRouteParameterType(
            self.touched_route_parameter_type,
        ));
        let bookmark_type = if self.bookmark_data.is_region {
            BookmarkType::Region
        } else {
            BookmarkType::Marker
        };
        model.change(C::SetBookmarkType(bookmark_type));
        model.change(C::SetBookmarkAnchorType(self.bookmark_data.anchor));
        model.change(C::SetBookmarkRef(self.bookmark_data.r#ref));
        let _ = model.set_seek_options(self.seek_options);
        model.change(C::SetTrackArea(self.track_area));
        model.change(C::SetAutomationMode(self.track_automation_mode));
        model.change(C::SetMonitoringMode(self.track_monitoring_mode));
        model.change(C::SetAutomationModeOverrideType(
            self.automation_mode_override_type,
        ));
        model.change(C::SetFxDisplayType(self.fx_display_type));
        model.change(C::SetScrollArrangeView(self.scroll_arrange_view));
        let scroll_mixer = if self.category == TargetCategory::Reaper
            && self.r#type == ReaperTargetType::TrackSelection
        {
            let is_old_preset = preset_version
                .map(|v| v < &Version::new(2, 8, 0))
                .unwrap_or(true);
            if is_old_preset {
                true
            } else {
                self.scroll_mixer
            }
        } else {
            self.scroll_mixer
        };
        model.change(C::SetScrollMixer(scroll_mixer));
        model.change(C::SetSendMidiDestination(self.send_midi_destination));
        model.change(C::SetRawMidiPattern(self.raw_midi_pattern.clone()));
        model.change(C::SetOscAddressPattern(self.osc_address_pattern.clone()));
        model.change(C::SetOscArgIndex(self.osc_arg_index));
        model.change(C::SetOscArgTypeTag(self.osc_arg_type));
        model.change(C::SetOscArgValueRange(
            self.osc_arg_value_range.to_interval(),
        ));
        model.change(C::SetOscDevId(self.osc_dev_id));
        model.change(C::SetPollForFeedback(self.poll_for_feedback));
        model.change(C::SetRetrigger(self.retrigger));
        model.change(C::SetTags(self.tags.clone()));
        model.change(C::SetExclusivity(self.exclusivity));
        let group_id = conversion_context
            .group_id_by_key(&self.group_id)
            .unwrap_or_default();
        model.change(C::SetGroupId(group_id));
        model.change(C::SetActiveMappingsOnly(self.active_mappings_only));
        let slot_descriptor = self
            .clip_slot
            .clone()
            .unwrap_or(ClipSlotDescriptor::ByIndex {
                column_index: self.slot_index,
                row_index: 0,
            });
        model.change(C::SetClipSlot(slot_descriptor));
        model.change(C::SetClipColumn(self.clip_column.clone()));
        model.change(C::SetClipRow(self.clip_row.clone()));
        model.change(C::SetClipManagementAction(
            self.clip_management_action.clone(),
        ));
        let clip_transport_action = self.clip_transport_action.unwrap_or_else(|| {
            use ClipTransportAction as T;
            use TransportAction::*;
            match self.transport_action {
                PlayStop => T::PlayStop,
                PlayPause => T::PlayPause,
                Stop => T::Stop,
                Pause => T::Pause,
                RecordStop => T::RecordStop,
                Repeat => T::Looped,
            }
        });
        model.change(C::SetClipTransportAction(clip_transport_action));
        model.change(C::SetClipColumnAction(self.clip_column_action));
        model.change(C::SetClipRowAction(self.clip_row_action));
        model.change(C::SetClipMatrixAction(self.clip_matrix_action));
        model.change(C::SetClipPlayStartTiming(self.clip_play_start_timing));
        model.change(C::SetClipPlayStopTiming(self.clip_play_stop_timing));
        model.change(C::SetRecordOnlyIfTrackArmed(
            self.record_only_if_track_armed,
        ));
        model.change(C::SetStopColumnIfSlotEmpty(self.stop_column_if_slot_empty));
        model.change(C::SetTrackToolAction(self.track_tool_action));
        model.change(C::SetFxToolAction(self.fx_tool_action));
        // "Load mapping snapshot" stuff
        let mapping_snapshot_id_for_load = {
            let (mapping_snapshot_type, mapping_snapshot_id) = match &self.mapping_snapshot {
                MappingSnapshotDescForLoad::Initial => (MappingSnapshotTypeForLoad::Initial, None),
                MappingSnapshotDescForLoad::ById { id } => {
                    (MappingSnapshotTypeForLoad::ById, id.parse().ok())
                }
            };
            model.change(C::SetMappingSnapshotTypeForLoad(mapping_snapshot_type));
            let mapping_snapshot_default_value = match self.mapping_snapshot_default_value.as_ref()
            {
                None => None,
                Some(v) => Some(convert_target_value_to_model(v)?),
            };
            model.change(C::SetMappingSnapshotDefaultValue(
                mapping_snapshot_default_value,
            ));
            mapping_snapshot_id
        };
        // "Take mapping snapshot" stuff
        let mapping_snapshot_id_for_take = {
            let (mapping_snapshot_type, mapping_snapshot_id) = match &self.take_mapping_snapshot {
                None => {
                    // Was written with ReaLearn < 2.14.0-pre.1 (take info from mapping_snapshot).
                    (
                        MappingSnapshotTypeForTake::ById,
                        self.mapping_snapshot.id().and_then(|id| id.parse().ok()),
                    )
                }
                Some(desc) => match desc {
                    MappingSnapshotDescForTake::LastLoaded => {
                        (MappingSnapshotTypeForTake::LastLoaded, None)
                    }
                    MappingSnapshotDescForTake::ById { id } => {
                        (MappingSnapshotTypeForTake::ById, id.parse().ok())
                    }
                },
            };
            model.change(C::SetMappingSnapshotTypeForTake(mapping_snapshot_type));
            mapping_snapshot_id
        };
        model.change(C::SetMappingSnapshotId(
            mapping_snapshot_id_for_load.or(mapping_snapshot_id_for_take),
        ));
        model.set_mouse_action_without_notification(self.mouse_action);
        model.change(C::SetPotFilterItemKind(self.pot_filter_item_kind));
        model.change(C::SetLearnableFeature(self.learnable_feature));
        let instance_id = self
            .session_id
            .as_ref()
            .and_then(|session_id| conversion_context.instance_id_by_session_id(session_id));
        model.change(C::SetInstanceId(instance_id));
        let mapping_id = self
            .mapping_key
            .as_ref()
            .and_then(|key| conversion_context.mapping_id_by_key(key));
        model.change(C::SetMappingId(mapping_id));
        Ok(())
    }
}

/// This function is so annoying because of backward compatibility. Once made the bad decision
/// to not introduce an explicit track type.
pub fn serialize_track(track: TrackPropValues) -> (TrackData, Option<ClipColumnDescriptor>) {
    use VirtualTrackType::*;
    match track.r#type {
        This => (TrackData::default(), None),
        Selected => (
            TrackData {
                guid: Some("selected".to_string()),
                ..Default::default()
            },
            None,
        ),
        AllSelected => (
            TrackData {
                guid: Some("selected*".to_string()),
                ..Default::default()
            },
            None,
        ),
        Master => (
            TrackData {
                guid: Some("master".to_string()),
                ..Default::default()
            },
            None,
        ),
        Instance => (
            TrackData {
                guid: Some("instance".to_string()),
                ..Default::default()
            },
            None,
        ),
        ByIdOrName => (
            TrackData {
                guid: track.id.map(|id| id.to_string_without_braces()),
                name: Some(track.name),
                ..Default::default()
            },
            None,
        ),
        ById => (
            TrackData {
                guid: track.id.map(|id| id.to_string_without_braces()),
                ..Default::default()
            },
            None,
        ),
        ByName => (
            TrackData {
                name: Some(track.name),
                ..Default::default()
            },
            None,
        ),
        AllByName => (
            TrackData {
                guid: Some("name*".to_string()),
                name: Some(track.name),
                ..Default::default()
            },
            None,
        ),
        ByIndex => (
            TrackData {
                index: Some(track.index),
                ..Default::default()
            },
            None,
        ),
        ByIndexTcp => (
            TrackData {
                guid: Some("index_tcp".to_string()),
                index: Some(track.index),
                ..Default::default()
            },
            None,
        ),
        ByIndexMcp => (
            TrackData {
                guid: Some("index_mcp".to_string()),
                index: Some(track.index),
                ..Default::default()
            },
            None,
        ),
        Dynamic => (
            TrackData {
                expression: Some(track.expression),
                ..Default::default()
            },
            None,
        ),
        DynamicTcp => (
            TrackData {
                guid: Some("dynamic_tcp".to_string()),
                expression: Some(track.expression),
                ..Default::default()
            },
            None,
        ),
        DynamicMcp => (
            TrackData {
                guid: Some("dynamic_mcp".to_string()),
                expression: Some(track.expression),
                ..Default::default()
            },
            None,
        ),
        FromClipColumn => (
            TrackData {
                guid: Some("from-clip-column".to_string()),
                expression: Some(track.expression),
                clip_column_track_context: track.clip_column_track_context,
                ..Default::default()
            },
            Some(track.clip_column),
        ),
    }
}

pub fn serialize_fx(fx: FxPropValues) -> FxData {
    use VirtualFxType::*;
    match fx.r#type {
        This => FxData {
            anchor: Some(VirtualFxType::This),
            guid: None,
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Focused => FxData {
            anchor: Some(VirtualFxType::Focused),
            guid: None,
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Instance => FxData {
            anchor: Some(VirtualFxType::Instance),
            guid: None,
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Dynamic => FxData {
            anchor: Some(VirtualFxType::Dynamic),
            guid: None,
            index: None,
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: Some(fx.expression),
        },
        ById => FxData {
            anchor: Some(VirtualFxType::ById),
            index: Some(fx.index),
            guid: fx.id.map(|id| id.to_string_without_braces()),
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByName => FxData {
            anchor: Some(VirtualFxType::ByName),
            index: None,
            guid: None,
            name: Some(fx.name),
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        AllByName => FxData {
            anchor: Some(VirtualFxType::AllByName),
            index: None,
            guid: None,
            name: Some(fx.name),
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByIndex => FxData {
            anchor: Some(VirtualFxType::ByIndex),
            index: Some(fx.index),
            guid: None,
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByIdOrIndex => FxData {
            anchor: Some(VirtualFxType::ByIdOrIndex),
            index: Some(fx.index),
            guid: fx.id.map(|id| id.to_string_without_braces()),
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
    }
}

pub fn serialize_fx_parameter(param: FxParameterPropValues) -> FxParameterData {
    use VirtualFxParameterType::*;
    match param.r#type {
        Dynamic => FxParameterData {
            r#type: Some(param.r#type),
            index: 0,
            name: None,
            expression: Some(param.expression),
        },
        ByName => FxParameterData {
            r#type: Some(param.r#type),
            index: 0,
            name: Some(param.name),
            expression: None,
        },
        ById => FxParameterData {
            // Before 2.8.0 we didn't have a type and this was the default ... let's leave it
            // at that.
            r#type: None,
            index: param.index,
            name: None,
            expression: None,
        },
        ByIndex => FxParameterData {
            // Before 2.8.0 we didn't have a type and this was the default ... let's leave it
            // at that.
            r#type: Some(param.r#type),
            index: param.index,
            name: None,
            expression: None,
        },
    }
}

pub fn serialize_track_route(route: TrackRoutePropValues) -> TrackRouteData {
    use TrackRouteSelectorType::*;
    match route.selector_type {
        Dynamic => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: None,
            name: None,
            expression: Some(route.expression),
        },
        ById => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: route.id.map(|id| id.to_string_without_braces()),
            name: None,
            expression: None,
        },
        ByName => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: None,
            name: Some(route.name),
            expression: None,
        },
        ByIndex => TrackRouteData {
            // Before 2.8.0 we didn't have a selector type and this was the default ... let's leave
            // it at that.
            selector_type: None,
            r#type: route.r#type,
            index: Some(route.index),
            guid: None,
            name: None,
            expression: None,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxParameterData {
    #[serde(
        rename = "paramType",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    r#type: Option<VirtualFxParameterType>,
    #[serde(
        rename = "paramIndex",
        default,
        deserialize_with = "f32_as_u32",
        skip_serializing_if = "is_default"
    )]
    index: u32,
    #[serde(rename = "paramName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(
        rename = "paramExpression",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRouteData {
    #[serde(
        rename = "routeSelectorType",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub selector_type: Option<TrackRouteSelectorType>,
    #[serde(
        rename = "routeType",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub r#type: TrackRouteType,
    /// The only reason this is an option is that in ReaLearn < 1.11.0 we allowed the send
    /// index to be undefined (-1). However, going with a default of 0 is also okay so
    /// `None` and `Some(0)` means essentially the same thing to us now.
    #[serde(
        rename = "sendIndex",
        default,
        deserialize_with = "none_if_minus_one",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub index: Option<u32>,
    #[serde(
        rename = "routeGuid",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub guid: Option<String>,
    #[serde(
        rename = "routeName",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub name: Option<String>,
    #[serde(
        rename = "routeExpression",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxData {
    /// Since 1.12.0-pre8. This is an option because we changed the default and wanted an easy
    /// way to detect when an old preset is loaded.
    // TODO-low If we would have a look at the version number at deserialization time, we could
    //  make it work without the option. Then we could also go without redundant "fxAnchor": "id" in
    //  current JSON. However, we introduced version numbers in 1.12.0-pre18 so this could
    //  negatively effect some prerelease testers. Another way to get rid of the redundant
    //  "fxAnchor" property would be to set this to none if the target type doesn't support FX.
    #[serde(
        rename = "fxAnchor",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub anchor: Option<VirtualFxType>,
    /// The only reason this is an option is that in ReaLearn < 1.11.0 we allowed the FX
    /// index to be undefined (-1). However, going with a default of 0 is also okay so
    /// `None` and `Some(0)` means essentially the same thing to us now.
    #[serde(
        rename = "fxIndex",
        default,
        deserialize_with = "none_if_minus_one",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub index: Option<u32>,
    /// Since 1.12.0-pre1
    #[serde(
        rename = "fxGUID",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub guid: Option<String>,
    /// Since 1.12.0-pre8
    #[serde(
        rename = "fxName",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub name: Option<String>,
    // TODO-medium This is actually a property of the track FX chain, not the FX
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub is_input_fx: bool,
    #[serde(
        rename = "fxExpression",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackData {
    // None means "This" track
    #[serde(
        rename = "trackGUID",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub guid: Option<String>,
    #[serde(
        rename = "trackName",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub name: Option<String>,
    #[serde(
        rename = "trackIndex",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub index: Option<u32>,
    #[serde(
        rename = "trackExpression",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub expression: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_column_track_context: ClipColumnTrackContext,
}

/// This function is so annoying because of backward compatibility. Once made the bad decision
/// to not introduce an explicit track type.
pub fn deserialize_track(
    track_data: &TrackData,
    clip_column: &ClipColumnDescriptor,
) -> TrackPropValues {
    match track_data {
        TrackData {
            guid: None,
            name: None,
            index: None,
            expression: None,
            ..
        } => TrackPropValues::from_virtual_track(VirtualTrack::This),
        TrackData { guid: Some(g), .. } if g == "master" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Master)
        }
        TrackData { guid: Some(g), .. } if g == "instance" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Instance)
        }
        TrackData { guid: Some(g), .. } if g == "selected" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Selected {
                allow_multiple: false,
            })
        }
        TrackData { guid: Some(g), .. } if g == "selected*" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Selected {
                allow_multiple: true,
            })
        }
        TrackData {
            guid: Some(g),
            index: Some(i),
            ..
        } if g == "index_tcp" => TrackPropValues::from_virtual_track(VirtualTrack::ByIndex {
            index: *i,
            scope: TrackScope::TracksVisibleInTcp,
        }),
        TrackData {
            guid: Some(g),
            index: Some(i),
            ..
        } if g == "index_mcp" => TrackPropValues::from_virtual_track(VirtualTrack::ByIndex {
            index: *i,
            scope: TrackScope::TracksVisibleInMcp,
        }),
        TrackData {
            guid: Some(g),
            expression: Some(e),
            ..
        } if g == "dynamic_tcp" => TrackPropValues {
            r#type: VirtualTrackType::DynamicTcp,
            expression: e.clone(),
            ..Default::default()
        },
        TrackData {
            guid: Some(g),
            expression: Some(e),
            ..
        } if g == "dynamic_mcp" => TrackPropValues {
            r#type: VirtualTrackType::DynamicMcp,
            expression: e.clone(),
            ..Default::default()
        },
        TrackData {
            guid: Some(g),
            clip_column_track_context,
            ..
        } if g == "from-clip-column" => TrackPropValues {
            r#type: VirtualTrackType::FromClipColumn,
            clip_column: clip_column.clone(),
            clip_column_track_context: *clip_column_track_context,
            ..Default::default()
        },
        TrackData {
            guid: Some(g),
            name: Some(n),
            ..
        } if g == "name*" => TrackPropValues {
            r#type: VirtualTrackType::AllByName,
            name: n.clone(),
            ..Default::default()
        },
        TrackData {
            guid: Some(g),
            name,
            ..
        } => {
            let id = Guid::from_string_without_braces(g).ok();
            match name {
                None => TrackPropValues {
                    r#type: VirtualTrackType::ById,
                    id,
                    ..Default::default()
                },
                Some(n) => TrackPropValues {
                    r#type: VirtualTrackType::ByIdOrName,
                    id,
                    name: n.clone(),
                    ..Default::default()
                },
            }
        }
        TrackData {
            guid: None,
            name: Some(n),
            ..
        } => TrackPropValues {
            r#type: VirtualTrackType::ByName,
            name: n.clone(),
            ..Default::default()
        },
        TrackData {
            guid: None,
            name: None,
            index: Some(i),
            ..
        } => TrackPropValues {
            r#type: VirtualTrackType::ByIndex,
            index: *i,
            ..Default::default()
        },
        TrackData {
            guid: None,
            name: None,
            index: None,
            expression: Some(e),
            ..
        } => TrackPropValues {
            r#type: VirtualTrackType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
    }
}

/// The context and so on is only necessary if you want to load < 1.12.0 presets.
pub fn deserialize_fx(
    fx_data: &FxData,
    ctx: Option<(ExtendedProcessorContext, Compartment, &VirtualTrack)>,
    migration_descriptor: &MigrationDescriptor,
) -> FxPropValues {
    match fx_data {
        // Special case: <Focused> for ReaLearn < 2.8.0-pre4.
        FxData { guid: Some(g), .. } if g == "focused" => FxPropValues {
            r#type: VirtualFxType::Instance,
            ..Default::default()
        },
        // Before ReaLearn 1.12.0 only the index was saved, even if it was (implicitly) always
        // IdOrIndex anchor. The GUID was looked up at runtime whenever loading the project. Do it!
        FxData {
            anchor: None,
            guid: None,
            expression: None,
            index: Some(i),
            is_input_fx,
            ..
        } => {
            let (context, compartment, virtual_track) =
                ctx.expect("trying to load < 1.12.0 FX target without processor context");
            let fx = get_first_guid_based_fx_at_index(
                context,
                virtual_track,
                *is_input_fx,
                *i,
                compartment,
            )
            .ok();
            FxPropValues {
                r#type: VirtualFxType::ByIdOrIndex,
                is_input_fx: *is_input_fx,
                id: fx.and_then(|f| f.guid()),
                index: *i,
                ..Default::default()
            }
        }
        // In ReaLearn 1.12.0-pre1 we started also saving the GUID, even for IdOrIndex anchor. We
        // still want to support that, even if no anchor is given.
        FxData {
            anchor: None,
            guid: Some(guid_string),
            name: None,
            expression: None,
            index: Some(index),
            is_input_fx,
        } => {
            let id = Guid::from_string_without_braces(guid_string).ok();
            FxPropValues {
                r#type: VirtualFxType::ByIdOrIndex,
                is_input_fx: *is_input_fx,
                id,
                index: *index,
                ..Default::default()
            }
        }
        // Since ReaLearn 1.12.0-pre8 we support Index anchor. We can't distinguish from < 1.12.0
        // data without explicitly given anchor.
        FxData {
            anchor: Some(VirtualFxType::ByIndex),
            guid: None,
            expression: None,
            index: Some(i),
            is_input_fx,
            ..
        } => FxPropValues {
            r#type: VirtualFxType::ByIndex,
            is_input_fx: *is_input_fx,
            index: *i,
            ..Default::default()
        },
        // From ReaLearn 1.12.0 to 2.8.0-pre2. We try to guess the anchor (what a mess).
        FxData {
            anchor: None,
            guid: Some(guid_string),
            name: _,
            expression: _,
            index,
            is_input_fx,
        } => {
            let id = Guid::from_string_without_braces(guid_string).ok();
            FxPropValues {
                r#type: VirtualFxType::ById,
                is_input_fx: *is_input_fx,
                id,
                index: index.unwrap_or_default(),
                ..Default::default()
            }
        }
        FxData {
            anchor: None,
            index: _,
            guid: _,
            name: Some(name),
            is_input_fx,
            expression: None,
        } => FxPropValues {
            r#type: VirtualFxType::ByName,
            is_input_fx: *is_input_fx,
            name: name.clone(),
            ..Default::default()
        },
        FxData {
            // Here we don't necessarily need the name anchor because there's no ambiguity.
            anchor: None,
            index: _,
            guid: _,
            name: _,
            is_input_fx: _,
            expression: Some(e),
        } => FxPropValues {
            r#type: VirtualFxType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
        // >= 2.8.0-pre3. Take everything we can get but watch the anchor.
        FxData {
            anchor: Some(fx_type),
            index,
            guid,
            name,
            is_input_fx,
            expression,
        } => FxPropValues {
            r#type: if *fx_type == VirtualFxType::Focused
                && migration_descriptor.fx_selector_transformation_188
            {
                VirtualFxType::Instance
            } else {
                *fx_type
            },
            is_input_fx: *is_input_fx,
            id: guid
                .as_ref()
                .and_then(|g| Guid::from_string_without_braces(g).ok()),
            name: name.clone().unwrap_or_default(),
            expression: expression.clone().unwrap_or_default(),
            index: index.unwrap_or_default(),
        },
        FxData {
            anchor: None,
            index: None,
            guid: None,
            name: None,
            expression: None,
            is_input_fx: _,
        } => FxPropValues::default(),
    }
}

pub fn deserialize_fx_parameter(param_data: &FxParameterData) -> FxParameterPropValues {
    match param_data {
        // This is the case for versions < 2.8.0.
        FxParameterData {
            // Important (because index is always given we need this as distinction).
            r#type: None,
            index: i,
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ById,
            index: *i,
            ..Default::default()
        },
        FxParameterData {
            name: Some(name), ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ByName,
            name: name.clone(),
            ..Default::default()
        },
        FxParameterData {
            expression: Some(e),
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
        FxParameterData {
            r#type: Some(VirtualFxParameterType::ByIndex),
            index: i,
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ByIndex,
            index: *i,
            ..Default::default()
        },
        _ => FxParameterPropValues::default(),
    }
}

pub fn deserialize_track_route(data: &TrackRouteData) -> TrackRoutePropValues {
    match data {
        // This is the case for versions < 2.8.0.
        TrackRouteData {
            // Important (because index is always given we need this as distinction).
            selector_type: None,
            r#type: TrackRouteType::Send,
            index: Some(i),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByIndex,
            r#type: TrackRouteType::Send,
            index: *i,
            ..Default::default()
        },
        // These are the new ones.
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ById),
            r#type: t,
            guid: Some(g),
            ..
        } => {
            let id = Guid::from_string_without_braces(g).ok();
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::ById,
                r#type: *t,
                id,
                ..Default::default()
            }
        }
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ByIndex) | None,
            r#type: t,
            index: i,
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByIndex,
            r#type: *t,
            index: i.unwrap_or(0),
            ..Default::default()
        },
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ByName),
            r#type: t,
            name: Some(name),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByName,
            r#type: *t,
            name: name.clone(),
            ..Default::default()
        },
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::Dynamic),
            r#type: t,
            expression: Some(e),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::Dynamic,
            r#type: *t,
            expression: e.clone(),
            ..Default::default()
        },
        _ => TrackRoutePropValues::default(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkData {
    #[serde(
        rename = "bookmarkAnchor",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub anchor: BookmarkAnchorType,
    #[serde(
        rename = "bookmarkRef",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub r#ref: u32,
    #[serde(
        rename = "bookmarkIsRegion",
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub is_region: bool,
}

pub fn get_first_guid_based_fx_at_index(
    context: ExtendedProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
    compartment: Compartment,
) -> Result<Fx, &'static str> {
    let fx_chains = get_fx_chains(context, track, is_input_fx, compartment)?;
    let fx_chain = fx_chains.first().ok_or("empty list of FX chains")?;
    fx_chain.fx_by_index(fx_index).ok_or("no FX at that index")
}

pub fn convert_target_value_to_api(value: AbsoluteValue) -> TargetValue {
    match value {
        AbsoluteValue::Continuous(v) => TargetValue::Unit { value: v.get() },
        AbsoluteValue::Discrete(v) => TargetValue::Discrete { value: v.actual() },
    }
}

pub fn convert_target_value_to_model(value: &TargetValue) -> Result<AbsoluteValue, &'static str> {
    match value {
        TargetValue::Unit { value } => Ok(AbsoluteValue::Continuous(UnitValue::try_from(*value)?)),
        TargetValue::Discrete { value } => Ok(AbsoluteValue::Discrete(Fraction::new_max(*value))),
    }
}

use super::f32_as_u32;
use super::none_if_minus_one;
use reaper_high::{BookmarkType, Guid, Reaper};

use crate::application::{
    BookmarkAnchorType, FxParameterPropValues, FxPropValues, FxSnapshot, ReaperTargetType,
    TargetCategory, TargetModel, TrackPropValues, VirtualControlElementType,
    VirtualFxParameterType, VirtualFxType, VirtualTrackType,
};
use crate::core::default_util::{is_default, is_none_or_some_default};
use crate::core::notification;
use crate::domain::{
    ActionInvocationType, ExtendedProcessorContext, SoloBehavior, TouchedParameterType,
    TrackExclusivity, TransportAction, VirtualTrack,
};
use derive_more::{Display, Error};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    category: TargetCategory,
    // reaper_type would be a better name but we need backwards compatibility
    #[serde(default, skip_serializing_if = "is_default")]
    r#type: ReaperTargetType,
    // Action target
    #[serde(default, skip_serializing_if = "is_default")]
    command_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    invocation_type: ActionInvocationType,
    // Until ReaLearn 1.0.0-beta6
    #[serde(default, skip_serializing)]
    invoke_relative: Option<bool>,
    // Track target
    #[serde(flatten)]
    track_data: TrackData,
    #[serde(default, skip_serializing_if = "is_default")]
    enable_only_if_track_is_selected: bool,
    // FX target
    #[serde(flatten)]
    fx_data: FxData,
    #[serde(default, skip_serializing_if = "is_default")]
    enable_only_if_fx_has_focus: bool,
    // Track send target
    #[serde(
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_default"
    )]
    send_index: Option<u32>,
    // FX parameter target
    #[serde(flatten)]
    fx_parameter_data: FxParameterData,
    // Track selection target (replaced with `track_exclusivity` since v2.4.0)
    #[serde(default, skip_serializing_if = "is_default")]
    select_exclusively: Option<bool>,
    // Track solo target (since v2.4.0, also changed default from "ignore routing" to "in place")
    #[serde(default, skip_serializing_if = "is_none_or_some_default")]
    solo_behavior: Option<SoloBehavior>,
    // Toggleable track targets (since v2.4.0)
    #[serde(default, skip_serializing_if = "is_default")]
    track_exclusivity: TrackExclusivity,
    // Transport target
    #[serde(default, skip_serializing_if = "is_default")]
    transport_action: TransportAction,
    #[serde(default, skip_serializing_if = "is_default")]
    control_element_type: VirtualControlElementType,
    #[serde(default, skip_serializing_if = "is_default")]
    control_element_index: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    fx_snapshot: Option<FxSnapshot>,
    #[serde(default, skip_serializing_if = "is_default")]
    touched_parameter_type: TouchedParameterType,
    // Bookmark target
    #[serde(flatten)]
    bookmark_data: BookmarkData,
}

impl TargetModelData {
    pub fn from_model(model: &TargetModel) -> Self {
        Self {
            category: model.category.get(),
            r#type: model.r#type.get(),
            command_name: model
                .action
                .get_ref()
                .as_ref()
                .map(|a| match a.command_name() {
                    // Built-in actions don't have a command name but a persistent command ID.
                    // Use command ID as string.
                    None => a.command_id().to_string(),
                    // ReaScripts and custom actions have a command name as persistent identifier.
                    Some(name) => name.into_string(),
                }),
            invocation_type: model.action_invocation_type.get(),
            // Not serialized anymore because deprecated
            invoke_relative: None,
            track_data: serialize_track(model.track()),
            enable_only_if_track_is_selected: model.enable_only_if_track_selected.get(),
            fx_data: serialize_fx(model.fx()),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus.get(),
            send_index: model.send_index.get(),
            fx_parameter_data: serialize_fx_parameter(model.fx_parameter()),
            select_exclusively: None,
            solo_behavior: Some(model.solo_behavior.get()),
            track_exclusivity: model.track_exclusivity.get(),
            transport_action: model.transport_action.get(),
            control_element_type: model.control_element_type.get(),
            control_element_index: model.control_element_index.get(),
            fx_snapshot: model.fx_snapshot.get_ref().clone(),
            touched_parameter_type: model.touched_parameter_type.get(),
            bookmark_data: BookmarkData {
                anchor: model.bookmark_anchor_type.get(),
                r#ref: model.bookmark_ref.get(),
                is_region: model.bookmark_type.get() == BookmarkType::Region,
            },
        }
    }

    /// The context is necessary only if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn apply_to_model(
        &self,
        model: &mut TargetModel,
        context: Option<ExtendedProcessorContext>,
        preset_version: Option<&Version>,
    ) {
        model.category.set_without_notification(self.category);
        model.r#type.set_without_notification(self.r#type);
        let reaper = Reaper::get();
        let action = match self.command_name.as_ref() {
            None => None,
            Some(command_name) => match command_name.parse::<u32>() {
                // Could parse this as command ID integer. This is a built-in action.
                Ok(command_id_int) => match command_id_int.try_into() {
                    Ok(command_id) => Some(reaper.main_section().action_by_command_id(command_id)),
                    Err(_) => {
                        notification::warn(&format!("Invalid command ID {}", command_id_int));
                        None
                    }
                },
                // Couldn't parse this as integer. This is a ReaScript or custom action.
                Err(_) => Some(reaper.action_by_command_name(command_name.as_str())),
            },
        };
        model.action.set_without_notification(action);
        let invocation_type = if let Some(invoke_relative) = self.invoke_relative {
            // Very old ReaLearn version
            if invoke_relative {
                ActionInvocationType::Relative
            } else {
                ActionInvocationType::Absolute
            }
        } else {
            self.invocation_type
        };
        model
            .action_invocation_type
            .set_without_notification(invocation_type);
        let track_prop_values = deserialize_track(&self.track_data);
        model.set_track_without_notification(track_prop_values);
        model
            .enable_only_if_track_selected
            .set_without_notification(self.enable_only_if_track_is_selected);
        let fx_prop_values = deserialize_fx(&self.fx_data);
        model.set_fx_without_notification(fx_prop_values);
        model
            .enable_only_if_fx_has_focus
            .set_without_notification(self.enable_only_if_fx_has_focus);
        model.send_index.set_without_notification(self.send_index);
        let fx_param_prop_values = deserialize_fx_parameter(&self.fx_parameter_data);
        model.set_fx_parameter_without_notification(fx_param_prop_values);
        let track_exclusivity = if let Some(select_exclusively) = self.select_exclusively {
            // Should only be set in versions < 2.4.0.
            if select_exclusively {
                TrackExclusivity::ExclusiveAll
            } else {
                TrackExclusivity::NonExclusive
            }
        } else {
            self.track_exclusivity
        };
        model
            .track_exclusivity
            .set_without_notification(track_exclusivity);
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
        model.solo_behavior.set_without_notification(solo_behavior);
        model
            .transport_action
            .set_without_notification(self.transport_action);
        model
            .control_element_type
            .set_without_notification(self.control_element_type);
        model
            .control_element_index
            .set_without_notification(self.control_element_index);
        model
            .fx_snapshot
            .set_without_notification(self.fx_snapshot.clone());
        model
            .touched_parameter_type
            .set_without_notification(self.touched_parameter_type);
        let bookmark_type = if self.bookmark_data.is_region {
            BookmarkType::Region
        } else {
            BookmarkType::Marker
        };
        model.bookmark_type.set_without_notification(bookmark_type);
        model
            .bookmark_anchor_type
            .set_without_notification(self.bookmark_data.anchor);
        model
            .bookmark_ref
            .set_without_notification(self.bookmark_data.r#ref);
    }
}

fn serialize_track(track: TrackPropValues) -> TrackData {
    use VirtualTrackType::*;
    match track.r#type {
        This => TrackData {
            guid: None,
            name: None,
            index: None,
            expression: None,
        },
        Selected => TrackData {
            guid: Some("selected".to_string()),
            name: None,
            index: None,
            expression: None,
        },
        Master => TrackData {
            guid: Some("master".to_string()),
            name: None,
            index: None,
            expression: None,
        },
        ByIdOrName => TrackData {
            guid: track.id.map(|id| id.to_string_without_braces()),
            name: Some(track.name),
            index: None,
            expression: None,
        },
        ById => TrackData {
            guid: track.id.map(|id| id.to_string_without_braces()),
            name: None,
            index: None,
            expression: None,
        },
        ByName => TrackData {
            guid: None,
            name: Some(track.name),
            index: None,
            expression: None,
        },
        ByIndex => TrackData {
            guid: None,
            name: None,
            index: Some(track.index),
            expression: None,
        },
        Dynamic => TrackData {
            guid: None,
            name: None,
            index: None,
            expression: Some(track.expression),
        },
    }
}

fn serialize_fx(fx: FxPropValues) -> FxData {
    use VirtualFxType::*;
    match fx.r#type {
        Focused => FxData {
            anchor: None,
            guid: Some("focused".to_string()),
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Dynamic => FxData {
            anchor: None,
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

fn serialize_fx_parameter(param: FxParameterPropValues) -> FxParameterData {
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
        ByIndex => FxParameterData {
            // Before 2.8.0 we didn't have a type and this was the default ... let's leave it
            // at that.
            r#type: None,
            index: param.index,
            name: None,
            expression: None,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxParameterData {
    #[serde(rename = "paramType", default, skip_serializing_if = "is_default")]
    r#type: Option<VirtualFxParameterType>,
    #[serde(
        rename = "paramIndex",
        deserialize_with = "f32_as_u32",
        default,
        skip_serializing_if = "is_default"
    )]
    index: u32,
    #[serde(rename = "paramName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(
        rename = "paramExpression",
        default,
        skip_serializing_if = "is_default"
    )]
    expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxData {
    /// Since 1.12.0-pre8
    #[serde(rename = "fxAnchor", default, skip_serializing_if = "is_default")]
    anchor: Option<VirtualFxType>,
    #[serde(
        rename = "fxIndex",
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_default"
    )]
    index: Option<u32>,
    /// Since 1.12.0-pre1
    #[serde(rename = "fxGUID", default, skip_serializing_if = "is_default")]
    guid: Option<String>,
    /// Since 1.12.0-pre8
    #[serde(rename = "fxName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    is_input_fx: bool,
    #[serde(rename = "fxExpression", default, skip_serializing_if = "is_default")]
    expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackData {
    // None means "This" track
    #[serde(rename = "trackGUID", default, skip_serializing_if = "is_default")]
    guid: Option<String>,
    #[serde(rename = "trackName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(rename = "trackIndex", default, skip_serializing_if = "is_default")]
    index: Option<u32>,
    #[serde(
        rename = "trackExpression",
        default,
        skip_serializing_if = "is_default"
    )]
    expression: Option<String>,
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
pub enum DeserializationError {
    InvalidCombination,
}

fn deserialize_track(track_data: &TrackData) -> TrackPropValues {
    match track_data {
        TrackData {
            guid: None,
            name: None,
            index: None,
            expression: None,
        } => TrackPropValues::from_virtual_track(VirtualTrack::This),
        TrackData { guid: Some(g), .. } if g == "master" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Master)
        }
        TrackData { guid: Some(g), .. } if g == "selected" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Selected)
        }
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
        } => TrackPropValues {
            r#type: VirtualTrackType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
    }
}

fn deserialize_fx(fx_data: &FxData) -> FxPropValues {
    match fx_data {
        FxData { guid: Some(g), .. } if g == "focused" => FxPropValues {
            r#type: VirtualFxType::Focused,
            ..Default::default()
        },
        FxData {
            index: None,
            name: None,
            guid: None,
            expression: None,
            ..
        } => FxPropValues::default(),
        // Since ReaLearn 1.12.0
        FxData {
            anchor: Some(VirtualFxType::ById),
            guid: Some(guid_string),
            expression: None,
            index,
            is_input_fx,
            ..
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
        // In ReaLearn 1.12.0-pre1 we started also saving the GUID, even for IdOrIndex anchor. We
        // still want to support that, even if no anchor is given.
        FxData {
            anchor: _,
            guid: Some(guid_string),
            expression: None,
            index: Some(index),
            is_input_fx,
            ..
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
        // Before ReaLearn 1.12.0 only the index was saved, even for IdOrIndex anchor. The GUID was
        // looked up at runtime whenever loading the project.
        FxData {
            anchor: None,
            guid: None,
            expression: None,
            index: Some(i),
            is_input_fx,
            ..
        } => {
            FxPropValues {
                r#type: VirtualFxType::ByIdOrIndex,
                is_input_fx: *is_input_fx,
                // TODO-high Before the ID was looked up ... make sure that other logic does this.
                index: *i,
                ..Default::default()
            }
        }
        // Since ReaLearn 1.12.0-pre8 we support Index anchor. We can't distinguish from pre-1.12.0
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
        // Since 1.12.0
        FxData {
            // Here we don't necessarily need the name anchor because there's no ambiguity.
            anchor: _,
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
            anchor: _,
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
        _ => FxPropValues::default(),
    }
}

fn deserialize_fx_parameter(param_data: &FxParameterData) -> FxParameterPropValues {
    match param_data {
        // This is the case for versions < 2.8.0.
        FxParameterData {
            // Important (because index is always given we need this as distinction).
            r#type: None,
            index: i,
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ByIndex,
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
        _ => FxParameterPropValues::default(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BookmarkData {
    #[serde(rename = "bookmarkAnchor", default, skip_serializing_if = "is_default")]
    anchor: BookmarkAnchorType,
    #[serde(rename = "bookmarkRef", default, skip_serializing_if = "is_default")]
    r#ref: u32,
    #[serde(
        rename = "bookmarkIsRegion",
        default,
        skip_serializing_if = "is_default"
    )]
    is_region: bool,
}

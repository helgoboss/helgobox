use super::f32_as_u32;
use super::none_if_minus_one;
use reaper_high::{BookmarkType, Guid, Reaper};

use crate::application::{
    get_guid_based_fx_at_index, BookmarkAnchorType, FxAnchorType, FxSnapshot, ReaperTargetType,
    TargetCategory, TargetModel, VirtualControlElementType,
};
use crate::core::default_util::{is_default, is_none_or_some_default};
use crate::core::notification;
use crate::domain::{
    ActionInvocationType, FxAnchor, ProcessorContext, SoloBehavior, TouchedParameterType,
    TrackAnchor, TrackExclusivity, TransportAction, VirtualFx, VirtualTrack,
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
    #[serde(
        deserialize_with = "f32_as_u32",
        default,
        skip_serializing_if = "is_default"
    )]
    param_index: u32,
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
            track_data: serialize_track(model.track.get_ref()),
            enable_only_if_track_is_selected: model.enable_only_if_track_selected.get(),
            fx_data: serialize_fx(model.fx.get_ref().as_ref()),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus.get(),
            send_index: model.send_index.get(),
            param_index: model.param_index.get(),
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
        context: Option<&ProcessorContext>,
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
        let virtual_track = match deserialize_track(&self.track_data) {
            Ok(t) => t,
            Err(e) => {
                handle_deserialization_error(e);
                VirtualTrack::This
            }
        };
        model.track.set_without_notification(virtual_track.clone());
        model
            .enable_only_if_track_selected
            .set_without_notification(self.enable_only_if_track_is_selected);
        let virtual_fx = match deserialize_fx(&self.fx_data, context, &virtual_track) {
            Ok(f) => f,
            Err(e) => {
                handle_deserialization_error(e);
                None
            }
        };
        model.fx.set_without_notification(virtual_fx);
        model
            .enable_only_if_fx_has_focus
            .set_without_notification(self.enable_only_if_fx_has_focus);
        model.send_index.set_without_notification(self.send_index);
        model.param_index.set_without_notification(self.param_index);
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

fn handle_deserialization_error(e: DeserializationError) {
    use DeserializationError::*;
    match e {
        InvalidGuid(guid) => notification::warn(&format!("Invalid GUID {}", guid)),
        InvalidCombination => notification::warn("Invalid combination of attributes"),
    }
}

fn serialize_track(virtual_track: &VirtualTrack) -> TrackData {
    use VirtualTrack::*;
    match virtual_track {
        This => TrackData {
            guid: None,
            name: None,
            index: None,
        },
        Selected => TrackData {
            guid: Some("selected".to_string()),
            name: None,
            index: None,
        },
        Master => TrackData {
            guid: Some("master".to_string()),
            name: None,
            index: None,
        },
        Particular(anchor) => match anchor {
            TrackAnchor::IdOrName(guid, name) => TrackData {
                guid: Some(guid.to_string_without_braces()),
                name: Some(name.clone()),
                index: None,
            },
            TrackAnchor::Id(guid) => TrackData {
                guid: Some(guid.to_string_without_braces()),
                name: None,
                index: None,
            },
            TrackAnchor::Name(name) => TrackData {
                guid: None,
                name: Some(name.clone()),
                index: None,
            },
            TrackAnchor::Index(index) => TrackData {
                guid: None,
                name: None,
                index: Some(*index),
            },
        },
    }
}

fn serialize_fx(virtual_fx: Option<&VirtualFx>) -> FxData {
    let virtual_fx = match virtual_fx {
        None => {
            return FxData {
                anchor: None,
                guid: None,
                index: None,
                name: None,
                is_input_fx: false,
            };
        }
        Some(f) => f,
    };
    use VirtualFx::*;
    match virtual_fx {
        Focused => FxData {
            anchor: None,
            guid: Some("focused".to_string()),
            index: None,
            name: None,
            is_input_fx: false,
        },
        Particular {
            is_input_fx,
            anchor,
        } => match anchor {
            FxAnchor::Id(guid, index_hint) => FxData {
                anchor: Some(FxAnchorType::Id),
                index: *index_hint,
                guid: Some(Guid::to_string_without_braces(guid)),
                name: None,
                is_input_fx: *is_input_fx,
            },
            FxAnchor::Name(name) => FxData {
                anchor: Some(FxAnchorType::Name),
                index: None,
                guid: None,
                name: Some(name.clone()),
                is_input_fx: *is_input_fx,
            },
            FxAnchor::Index(index) => FxData {
                anchor: Some(FxAnchorType::Index),
                index: Some(*index),
                guid: None,
                name: None,
                is_input_fx: *is_input_fx,
            },
            FxAnchor::IdOrIndex(guid, index) => FxData {
                anchor: Some(FxAnchorType::IdOrIndex),
                index: Some(*index),
                guid: guid.as_ref().map(Guid::to_string_without_braces),
                name: None,
                is_input_fx: *is_input_fx,
            },
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxData {
    /// Since 1.12.0-pre8
    #[serde(rename = "fxAnchor", default, skip_serializing_if = "is_default")]
    anchor: Option<FxAnchorType>,
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
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
pub enum DeserializationError {
    InvalidGuid(#[error(not(source))] String),
    InvalidCombination,
}

fn deserialize_track(track_data: &TrackData) -> Result<VirtualTrack, DeserializationError> {
    let virtual_track = match track_data {
        TrackData {
            guid: None,
            name: None,
            index: None,
        } => VirtualTrack::This,
        TrackData { guid: Some(g), .. } if g == "master" => VirtualTrack::Master,
        TrackData { guid: Some(g), .. } if g == "selected" => VirtualTrack::Selected,
        TrackData {
            guid: Some(g),
            name,
            ..
        } => {
            let guid = Guid::from_string_without_braces(g)
                .map_err(|_| DeserializationError::InvalidGuid(g.to_string()))?;
            let anchor = match name {
                None => TrackAnchor::Id(guid),
                Some(n) => TrackAnchor::IdOrName(guid, n.clone()),
            };
            VirtualTrack::Particular(anchor)
        }
        TrackData {
            guid: None,
            name: Some(n),
            ..
        } => VirtualTrack::Particular(TrackAnchor::Name(n.clone())),
        TrackData {
            guid: None,
            name: None,
            index: Some(i),
        } => VirtualTrack::Particular(TrackAnchor::Index(*i)),
    };
    Ok(virtual_track)
}

fn deserialize_fx(
    fx_data: &FxData,
    context: Option<&ProcessorContext>,
    virtual_track: &VirtualTrack,
) -> Result<Option<VirtualFx>, DeserializationError> {
    let virtual_fx = match fx_data {
        FxData { guid: Some(g), .. } if g == "focused" => Some(VirtualFx::Focused),
        FxData {
            index: None,
            name: None,
            guid: None,
            ..
        } => None,
        // Since ReaLearn 1.12.0
        FxData {
            anchor: Some(FxAnchorType::Id),
            guid: Some(guid_string),
            index,
            is_input_fx,
            ..
        } => {
            let guid = Guid::from_string_without_braces(guid_string)
                .map_err(|_| DeserializationError::InvalidGuid(guid_string.clone()))?;
            Some(VirtualFx::Particular {
                is_input_fx: *is_input_fx,
                anchor: FxAnchor::Id(guid, *index),
            })
        }
        // In ReaLearn 1.12.0-pre1 we started also saving the GUID, even for IdOrIndex anchor. We
        // still want to support that, even if no anchor is given.
        FxData {
            anchor: _,
            guid: Some(guid_string),
            index: Some(index),
            is_input_fx,
            ..
        } => {
            let guid = Guid::from_string_without_braces(guid_string)
                .map_err(|_| DeserializationError::InvalidGuid(guid_string.clone()))?;
            Some(VirtualFx::Particular {
                is_input_fx: *is_input_fx,
                anchor: FxAnchor::IdOrIndex(Some(guid), *index),
            })
        }
        // Before ReaLearn 1.12.0 only the index was saved, even for IdOrIndex anchor. The GUID was
        // looked up at runtime whenever loading the project.
        FxData {
            anchor: None,
            guid: None,
            index: Some(i),
            is_input_fx,
            ..
        } => {
            match get_guid_based_fx_at_index(
                context.expect("trying to load pre-1.12.0 FX target without processor context"),
                virtual_track,
                *is_input_fx,
                *i,
            ) {
                Ok(fx) => Some(VirtualFx::Particular {
                    is_input_fx: *is_input_fx,
                    anchor: FxAnchor::IdOrIndex(fx.guid(), *i),
                }),
                Err(e) => {
                    // TODO-low We should rather return an error.
                    notification::warn(e);
                    None
                }
            }
        }
        // Since ReaLearn 1.12.0-pre8 we support Index anchor. We can't distinguish from pre-1.12.0
        // data without explicitly given anchor.
        FxData {
            anchor: Some(FxAnchorType::Index),
            guid: None,
            index: Some(i),
            is_input_fx,
            ..
        } => Some(VirtualFx::Particular {
            is_input_fx: *is_input_fx,
            anchor: FxAnchor::Index(*i),
        }),
        // Since 1.12.0
        FxData {
            // Here we don't necessarily need the name anchor because there's no ambiguity.
            anchor: _,
            index: _,
            guid: _,
            name: Some(name),
            is_input_fx,
        } => Some(VirtualFx::Particular {
            is_input_fx: *is_input_fx,
            anchor: FxAnchor::Name(name.clone()),
        }),
        _ => return Err(DeserializationError::InvalidCombination),
    };
    Ok(virtual_fx)
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

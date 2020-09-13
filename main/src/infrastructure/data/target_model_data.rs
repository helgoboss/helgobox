use super::f32_as_u32;
use super::none_if_minus_one;
use reaper_high::{Guid, Project, Reaper, Track};

use crate::application::{
    get_guid_based_fx_at_index, ReaperTargetType, SessionContext, TargetCategory, TargetModel,
    VirtualControlElementType, VirtualTrack,
};
use crate::core::toast;
use crate::domain::{ActionInvocationType, TransportAction};
use derive_more::{Display, Error};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TargetModelData {
    pub category: TargetCategory,
    // reaper_type would be a better name but we need backwards compatibility
    r#type: ReaperTargetType,
    // Action target
    command_name: Option<String>,
    invocation_type: ActionInvocationType,
    // Until ReaLearn 1.0.0-beta6
    #[serde(skip_serializing)]
    invoke_relative: Option<bool>,
    // Track target
    // None means "This" track
    #[serde(rename = "trackGUID")]
    track_guid: Option<String>,
    track_name: Option<String>,
    enable_only_if_track_is_selected: bool,
    // FX target
    #[serde(deserialize_with = "none_if_minus_one")]
    fx_index: Option<u32>,
    is_input_fx: bool,
    enable_only_if_fx_has_focus: bool,
    // Track send target
    #[serde(deserialize_with = "none_if_minus_one")]
    send_index: Option<u32>,
    // FX parameter target
    #[serde(deserialize_with = "f32_as_u32")]
    param_index: u32,
    // Track selection target
    select_exclusively: bool,
    // Transport target
    transport_action: TransportAction,
    pub control_element_type: VirtualControlElementType,
    pub control_element_index: u32,
}

impl Default for TargetModelData {
    fn default() -> Self {
        Self {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxParameter,
            command_name: None,
            invocation_type: ActionInvocationType::Trigger,
            invoke_relative: None,
            track_guid: None,
            track_name: None,
            enable_only_if_track_is_selected: false,
            fx_index: None,
            is_input_fx: false,
            enable_only_if_fx_has_focus: false,
            send_index: None,
            param_index: 0,
            select_exclusively: false,
            transport_action: TransportAction::PlayStop,
            control_element_type: VirtualControlElementType::Continuous,
            control_element_index: 0,
        }
    }
}

impl TargetModelData {
    pub fn from_model(model: &TargetModel, _context: &SessionContext) -> Self {
        let (track_guid, track_name) = serialize_track(model.track.get_ref());
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
            track_guid,
            track_name,
            enable_only_if_track_is_selected: model.enable_only_if_track_selected.get(),
            fx_index: model.fx_index.get(),
            is_input_fx: model.is_input_fx.get(),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus.get(),
            send_index: model.send_index.get(),
            param_index: model.param_index.get(),
            select_exclusively: model.select_exclusively.get(),
            transport_action: model.transport_action.get(),
            control_element_type: model.control_element_type.get(),
            control_element_index: model.control_element_index.get(),
        }
    }

    pub fn apply_to_model(&self, model: &mut TargetModel, context: &SessionContext) {
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
                        toast::warn(&format!("Invalid command ID {}", command_id_int));
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
        let virtual_track =
            match deserialize_track(&self.track_guid, &self.track_name, context.project()) {
                Ok(t) => t,
                Err(e) => {
                    use TrackDeserializationError::*;
                    match e {
                        InvalidGuid(guid) => toast::warn(&format!(
                            "Invalid track GUID {}, falling back to <This>",
                            guid
                        )),
                        TrackNotFound { guid, name } => toast::warn(&format!(
                            "Track not found by GUID {} and name {}, falling back to <This>",
                            guid.to_string_with_braces(),
                            name.map(|n| format!("\"{}\"", n))
                                .unwrap_or_else(|| "-".to_string())
                        )),
                    }
                    VirtualTrack::This
                }
            };
        model.track.set_without_notification(virtual_track.clone());
        model
            .enable_only_if_track_selected
            .set_without_notification(self.enable_only_if_track_is_selected);
        // At loading time, we can reliably identify an FX using its index because the FX can't
        // be moved around while the project is not loaded.
        model.fx_index.set_without_notification(self.fx_index);
        // Therefore we just query the GUID from the FX at the given index.
        let fx_guid = self.fx_index.and_then(|fx_index| {
            match get_guid_based_fx_at_index(context, &virtual_track, self.is_input_fx, fx_index) {
                Ok(fx) => fx.guid(),
                Err(e) => {
                    toast::warn(e);
                    None
                }
            }
        });
        model.fx_guid.set(fx_guid);
        model.is_input_fx.set_without_notification(self.is_input_fx);
        model
            .enable_only_if_fx_has_focus
            .set_without_notification(self.enable_only_if_fx_has_focus);
        model.send_index.set_without_notification(self.send_index);
        model.param_index.set_without_notification(self.param_index);
        model
            .select_exclusively
            .set_without_notification(self.select_exclusively);
        model
            .transport_action
            .set_without_notification(self.transport_action);
        model
            .control_element_type
            .set_without_notification(self.control_element_type);
        model
            .control_element_index
            .set_without_notification(self.control_element_index);
    }
}

fn serialize_track(virtual_track: &VirtualTrack) -> (Option<String>, Option<String>) {
    use VirtualTrack::*;
    match virtual_track {
        This => (None, None),
        Selected => (Some("selected".to_string()), None),
        Master => (Some("master".to_string()), None),
        Particular(track) => {
            let guid = track.guid().to_string_without_braces();
            let name = track.name().expect("track must have name").into_string();
            (Some(guid), Some(name))
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
pub enum TrackDeserializationError {
    InvalidGuid(#[error(not(source))] String),
    #[display(fmt = "TrackNotFound")]
    TrackNotFound {
        guid: Guid,
        name: Option<String>,
    },
}

fn deserialize_track(
    id: &Option<String>,
    name: &Option<String>,
    project: Project,
) -> Result<VirtualTrack, TrackDeserializationError> {
    let virtual_track = match id.as_ref().map(String::as_str) {
        None => VirtualTrack::This,
        Some("master") => VirtualTrack::Master,
        Some("selected") => VirtualTrack::Selected,
        Some(s) => {
            let guid = Guid::from_string_without_braces(s)
                .map_err(|_| TrackDeserializationError::InvalidGuid(s.to_string()))?;
            let track = project.track_by_guid(&guid);
            let track = if track.is_available() {
                track
            } else {
                let name = name
                    .as_ref()
                    .ok_or(TrackDeserializationError::TrackNotFound { guid, name: None })?;
                find_track_by_name(project, name.as_str()).ok_or(
                    TrackDeserializationError::TrackNotFound {
                        guid,
                        name: Some(name.clone()),
                    },
                )?
            };
            VirtualTrack::Particular(track)
        }
    };
    Ok(virtual_track)
}

fn find_track_by_name(project: Project, name: &str) -> Option<Track> {
    project.tracks().find(|t| match t.name() {
        None => false,
        Some(n) => n.to_str() == name,
    })
}

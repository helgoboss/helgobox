use super::f32_as_u32;
use super::none_if_minus_one;
use crate::domain::{
    get_guid_based_fx_at_index, ActionInvocationType, SessionContext, TargetModel, TargetType,
    VirtualTrack,
};
use reaper_high::{Action, Guid, Project, Reaper, Track};
use reaper_medium::{CommandId, ReaperString};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TargetModelData {
    r#type: TargetType,
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
}

impl Default for TargetModelData {
    fn default() -> Self {
        Self {
            r#type: TargetType::FxParameter,
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
        }
    }
}

impl TargetModelData {
    pub fn from_model(model: &TargetModel, context: &SessionContext) -> Self {
        let (track_guid, track_name) = serialize_track(model.track.get_ref());
        Self {
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
        }
    }

    pub fn apply_to_model(
        &self,
        model: &mut TargetModel,
        context: &SessionContext,
    ) -> Result<(), &'static str> {
        model.r#type.set_without_notification(self.r#type);
        let reaper = Reaper::get();
        let action = match self.command_name.as_ref() {
            None => None,
            Some(command_name) => match command_name.parse::<u32>() {
                // Could parse this as command ID integer. This is a built-in action.
                Ok(command_id_int) => {
                    let command_id = command_id_int
                        .try_into()
                        .map_err(|_| "invalid command ID")?;
                    Some(reaper.main_section().action_by_command_id(command_id))
                }
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
            deserialize_track(&self.track_guid, &self.track_name, context.project())?;
        model.track.set_without_notification(virtual_track.clone());
        model
            .enable_only_if_track_selected
            .set_without_notification(self.enable_only_if_track_is_selected);
        // At loading time, we can reliably identify an FX using its index because the FX can't
        // be moved around while the project is not loaded.
        model.fx_index.set_without_notification(self.fx_index);
        // Therefore we just query the GUID from the FX at the given index.
        let fx_guid = self.fx_index.and_then(|fx_index| {
            let fx =
                get_guid_based_fx_at_index(context, &virtual_track, self.is_input_fx, fx_index)
                    .ok()?;
            fx.guid()
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
        Ok(())
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

fn deserialize_track(
    id: &Option<String>,
    name: &Option<String>,
    project: Project,
) -> Result<VirtualTrack, &'static str> {
    let virtual_track = match id.as_ref().map(String::as_str) {
        None => VirtualTrack::This,
        Some("master") => VirtualTrack::Master,
        Some("selected") => VirtualTrack::Selected,
        Some(s) => {
            let guid = Guid::from_string_without_braces(s)?;
            let track = project.track_by_guid(&guid);
            let track = if track.is_available() {
                track
            } else {
                let name = name
                    .as_ref()
                    .ok_or("track not found by ID and no name provided")?;
                find_track_by_name(project, name.as_str())
                    .ok_or("track not found, not even by name")?
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

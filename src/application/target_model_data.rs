use super::f32_as_u32;
use super::none_if_minus_one;
use crate::domain::{ActionInvocationType, TargetModel, TargetType, VirtualTrack};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TargetModelData {
    r#type: TargetType,
    // Action target
    command_name: Option<String>,
    invocation_type: ActionInvocationType,
    // Until ReaLearn 1.0.0-beta6
    #[serde(skip_serializing)]
    invoke_relative: bool,
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
            invoke_relative: false,
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
    pub fn from_model(model: &TargetModel) -> Self {
        Self {
            r#type: model.r#type.get(),
            // TODO
            command_name: None,
            invocation_type: model.action_invocation_type.get(),
            // Not serialized anymore because deprecated
            invoke_relative: false,
            // TODO
            track_guid: None,
            // TODO
            track_name: None,
            enable_only_if_track_is_selected: model.enable_only_if_track_selected.get(),
            fx_index: model.fx_index.get(),
            is_input_fx: model.is_input_fx.get(),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus.get(),
            send_index: model.send_index.get(),
            param_index: model.param_index.get(),
            select_exclusively: model.select_exclusively.get(),
        }
    }

    pub fn apply_to_model(&self, model: &mut TargetModel) -> Result<(), &'static str> {
        model.r#type.set(self.r#type);
        // TODO
        model.command_id.set(None);
        // TODO invoke_relative
        model.action_invocation_type.set(self.invocation_type);
        // TODO
        model.track.set(VirtualTrack::This);
        model
            .enable_only_if_track_selected
            .set(self.enable_only_if_track_is_selected);
        model.fx_index.set(self.fx_index);
        model.is_input_fx.set(self.is_input_fx);
        model
            .enable_only_if_fx_has_focus
            .set(self.enable_only_if_fx_has_focus);
        model.send_index.set(self.send_index);
        model.param_index.set(self.param_index);
        model.select_exclusively.set(self.select_exclusively);
        Ok(())
    }
}

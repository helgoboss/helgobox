use crate::application::{
    ActivationType, GroupId, GroupModel, ModifierConditionModel, ProgramConditionModel,
};
use crate::core::default_util::{bool_true, is_bool_true, is_default};
use crate::infrastructure::data::{
    ActivationData, EnabledData, ModeModelData, SourceModelData, TargetModelData,
};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupModelData {
    // Because main group UUID is the default, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    id: GroupId,
    // Because main group name is empty, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    name: String,
    #[serde(flatten)]
    enabled_data: EnabledData,
    #[serde(flatten)]
    activation_data: ActivationData,
}

impl GroupModelData {
    pub fn from_model(model: &GroupModel) -> GroupModelData {
        GroupModelData {
            id: model.id(),
            name: model.name.get_ref().clone(),
            enabled_data: EnabledData {
                control_is_enabled: model.control_is_enabled.get(),
                feedback_is_enabled: model.feedback_is_enabled.get(),
            },
            activation_data: ActivationData {
                activation_type: model.activation_type.get(),
                modifier_condition_1: model.modifier_condition_1.get(),
                modifier_condition_2: model.modifier_condition_2.get(),
                program_condition: model.program_condition.get(),
                eel_condition: model.eel_condition.get_ref().clone(),
            },
        }
    }

    pub fn to_model(&self) -> GroupModel {
        let mut model = GroupModel::new_from_data(self.id);
        self.apply_to_model(&mut model);
        model
    }

    fn apply_to_model(&self, model: &mut GroupModel) {
        model.name.set_without_notification(self.name.clone());
        model
            .control_is_enabled
            .set_without_notification(self.enabled_data.control_is_enabled);
        model
            .feedback_is_enabled
            .set_without_notification(self.enabled_data.feedback_is_enabled);
        model
            .activation_type
            .set_without_notification(self.activation_data.activation_type);
        model
            .modifier_condition_1
            .set_without_notification(self.activation_data.modifier_condition_1);
        model
            .modifier_condition_2
            .set_without_notification(self.activation_data.modifier_condition_2);
        model
            .program_condition
            .set_without_notification(self.activation_data.program_condition);
        model
            .eel_condition
            .set_without_notification(self.activation_data.eel_condition.clone());
    }
}

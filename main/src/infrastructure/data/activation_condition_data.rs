use crate::application::{
    ActivationConditionModel, ActivationType, ModifierConditionModel, ProgramConditionModel,
};
use crate::core::default_util::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationConditionData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub activation_type: ActivationType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_1: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_2: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub program_condition: ProgramConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub eel_condition: String,
}

impl ActivationConditionData {
    pub fn from_model(model: &ActivationConditionModel) -> ActivationConditionData {
        ActivationConditionData {
            activation_type: model.activation_type.get(),
            modifier_condition_1: model.modifier_condition_1.get(),
            modifier_condition_2: model.modifier_condition_2.get(),
            program_condition: model.program_condition.get(),
            eel_condition: model.eel_condition.get_ref().clone(),
        }
    }

    pub fn apply_to_model(&self, model: &mut ActivationConditionModel) {
        model
            .activation_type
            .set_without_notification(self.activation_type);
        model
            .modifier_condition_1
            .set_without_notification(self.modifier_condition_1);
        model
            .modifier_condition_2
            .set_without_notification(self.modifier_condition_2);
        model
            .program_condition
            .set_without_notification(self.program_condition);
        model
            .eel_condition
            .set_without_notification(self.eel_condition.clone());
    }
}

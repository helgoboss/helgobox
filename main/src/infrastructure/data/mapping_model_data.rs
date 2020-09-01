use crate::application::{
    ActivationType, MappingModel, ModifierConditionModel, ProgramConditionModel, SessionContext,
};
use crate::infrastructure::data::{ModeModelData, SourceModelData, TargetModelData};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MappingModelData {
    name: String,
    source: SourceModelData,
    mode: ModeModelData,
    target: TargetModelData,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
    prevent_echo_feedback: bool,
    send_feedback_after_control: bool,
    activation_type: ActivationType,
    modifier_condition_1: ModifierConditionModel,
    modifier_condition_2: ModifierConditionModel,
    program_condition: ProgramConditionModel,
    eel_condition: String,
}

impl Default for MappingModelData {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            source: Default::default(),
            mode: Default::default(),
            target: Default::default(),
            control_is_enabled: true,
            feedback_is_enabled: true,
            prevent_echo_feedback: false,
            send_feedback_after_control: false,
            activation_type: ActivationType::Always,
            modifier_condition_1: Default::default(),
            modifier_condition_2: Default::default(),
            program_condition: Default::default(),
            eel_condition: "".to_string(),
        }
    }
}

impl MappingModelData {
    pub fn from_model(model: &MappingModel, context: &SessionContext) -> MappingModelData {
        MappingModelData {
            name: model.name.get_ref().clone(),
            source: SourceModelData::from_model(&model.source_model),
            mode: ModeModelData::from_model(&model.mode_model),
            target: TargetModelData::from_model(&model.target_model, context),
            control_is_enabled: model.control_is_enabled.get(),
            feedback_is_enabled: model.feedback_is_enabled.get(),
            prevent_echo_feedback: model.prevent_echo_feedback.get(),
            send_feedback_after_control: model.send_feedback_after_control.get(),
            activation_type: model.activation_type.get(),
            modifier_condition_1: model.modifier_condition_1.get(),
            modifier_condition_2: model.modifier_condition_2.get(),
            program_condition: model.program_condition.get(),
            eel_condition: model.eel_condition.get_ref().clone(),
        }
    }

    pub fn to_model(&self, context: &SessionContext) -> MappingModel {
        let mut model = MappingModel::default();
        self.apply_to_model(&mut model, context);
        model
    }

    fn apply_to_model(&self, model: &mut MappingModel, context: &SessionContext) {
        model.name.set_without_notification(self.name.clone());
        self.source.apply_to_model(model.source_model.borrow_mut());
        self.mode.apply_to_model(model.mode_model.borrow_mut());
        self.target
            .apply_to_model(model.target_model.borrow_mut(), context);
        model
            .control_is_enabled
            .set_without_notification(self.control_is_enabled);
        model
            .feedback_is_enabled
            .set_without_notification(self.feedback_is_enabled);
        model
            .prevent_echo_feedback
            .set_without_notification(self.prevent_echo_feedback);
        model
            .send_feedback_after_control
            .set_without_notification(self.send_feedback_after_control);
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

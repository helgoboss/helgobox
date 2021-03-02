use crate::application::{GroupId, MappingModel};
use crate::core::default_util::is_default;
use crate::domain::{MappingCompartment, MappingId, ProcessorContext};
use crate::infrastructure::data::{
    ActivationConditionData, EnabledData, MigrationDescriptor, ModeModelData, SourceModelData,
    TargetModelData,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingModelData {
    // Saved since ReaLearn 1.12.0
    #[serde(default, skip_serializing_if = "is_default")]
    id: Option<MappingId>,
    #[serde(default, skip_serializing_if = "is_default")]
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    group_id: GroupId,
    source: SourceModelData,
    mode: ModeModelData,
    target: TargetModelData,
    #[serde(flatten)]
    enabled_data: EnabledData,
    #[serde(flatten)]
    activation_condition_data: ActivationConditionData,
    #[serde(default, skip_serializing_if = "is_default")]
    prevent_echo_feedback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    send_feedback_after_control: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    advanced: Option<serde_yaml::mapping::Mapping>,
}

impl MappingModelData {
    pub fn from_model(model: &MappingModel) -> MappingModelData {
        MappingModelData {
            id: Some(model.id()),
            name: model.name.get_ref().clone(),
            group_id: model.group_id.get(),
            source: SourceModelData::from_model(&model.source_model),
            mode: ModeModelData::from_model(&model.mode_model),
            target: TargetModelData::from_model(&model.target_model),
            enabled_data: EnabledData {
                control_is_enabled: model.control_is_enabled.get(),
                feedback_is_enabled: model.feedback_is_enabled.get(),
            },
            prevent_echo_feedback: model.prevent_echo_feedback.get(),
            send_feedback_after_control: model.send_feedback_after_control.get(),
            activation_condition_data: ActivationConditionData::from_model(
                &model.activation_condition_model,
            ),
            advanced: model.advanced_settings().cloned(),
        }
    }

    /// The context is necessary only if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn to_model(
        &self,
        compartment: MappingCompartment,
        context: Option<&ProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
    ) -> MappingModel {
        // Preliminary group ID
        let mut model = MappingModel::new(compartment, GroupId::default());
        self.apply_to_model(&mut model, context, migration_descriptor, preset_version);
        model
    }

    /// The context is necessary only if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    fn apply_to_model(
        &self,
        model: &mut MappingModel,
        context: Option<&ProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
    ) {
        if let Some(id) = self.id {
            model.set_id_without_notification(id);
        }
        model.name.set_without_notification(self.name.clone());
        model.group_id.set_without_notification(self.group_id);
        self.activation_condition_data
            .apply_to_model(model.activation_condition_model.borrow_mut());
        self.source.apply_to_model(model.source_model.borrow_mut());
        self.mode.apply_to_model(
            model.mode_model.borrow_mut(),
            migration_descriptor,
            &self.name,
        );
        self.target
            .apply_to_model(model.target_model.borrow_mut(), context, preset_version);
        model
            .control_is_enabled
            .set_without_notification(self.enabled_data.control_is_enabled);
        model
            .feedback_is_enabled
            .set_without_notification(self.enabled_data.feedback_is_enabled);
        model
            .prevent_echo_feedback
            .set_without_notification(self.prevent_echo_feedback);
        model
            .send_feedback_after_control
            .set_without_notification(self.send_feedback_after_control);
        model.set_advanced_settings_without_notification(self.advanced.clone());
    }
}

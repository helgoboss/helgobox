use crate::application::{GroupId, MappingModel};
use crate::core::default_util::is_default;
use crate::domain::{ExtendedProcessorContext, MappingCompartment, MappingId};
use crate::infrastructure::data::{
    ActivationConditionData, EnabledData, MigrationDescriptor, ModeModelData, SourceModelData,
    TargetModelData,
};
use crate::infrastructure::plugin::App;
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
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub group_id: GroupId,
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

    pub fn to_model(&self, compartment: MappingCompartment) -> MappingModel {
        self.to_model_flexible(
            compartment,
            None,
            &MigrationDescriptor::default(),
            Some(App::version()),
        )
    }

    /// The context is necessary only if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn to_model_flexible(
        &self,
        compartment: MappingCompartment,
        context: Option<ExtendedProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
    ) -> MappingModel {
        // Preliminary group ID
        let mut model = MappingModel::new(compartment, GroupId::default());
        self.apply_to_model_internal(
            &mut model,
            context,
            migration_descriptor,
            preset_version,
            false,
            true,
        );
        model
    }

    /// This is for realtime mapping modification (with notification, no ID changes), e.g. for copy
    /// & paste within one ReaLearn version.
    pub fn apply_to_model(&self, model: &mut MappingModel) {
        self.apply_to_model_internal(
            model,
            None,
            &MigrationDescriptor::default(),
            Some(App::version()),
            true,
            false,
        );
    }

    /// The context is necessary only if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    fn apply_to_model_internal(
        &self,
        model: &mut MappingModel,
        context: Option<ExtendedProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
        with_notification: bool,
        overwrite_id: bool,
    ) {
        if overwrite_id {
            if let Some(id) = self.id {
                model.set_id_without_notification(id);
            }
        }
        model
            .name
            .set_with_optional_notification(self.name.clone(), with_notification);
        model
            .group_id
            .set_with_optional_notification(self.group_id, with_notification);
        self.activation_condition_data.apply_to_model(
            model.activation_condition_model.borrow_mut(),
            with_notification,
        );
        let compartment = model.compartment();
        self.source.apply_to_model_flexible(
            model.source_model.borrow_mut(),
            with_notification,
            compartment,
        );
        self.mode.apply_to_model_flexible(
            model.mode_model.borrow_mut(),
            migration_descriptor,
            &self.name,
            with_notification,
        );
        self.target.apply_to_model_flexible(
            model.target_model.borrow_mut(),
            context,
            preset_version,
            with_notification,
            compartment,
        );
        model.control_is_enabled.set_with_optional_notification(
            self.enabled_data.control_is_enabled,
            with_notification,
        );
        model.feedback_is_enabled.set_with_optional_notification(
            self.enabled_data.feedback_is_enabled,
            with_notification,
        );
        model
            .prevent_echo_feedback
            .set_with_optional_notification(self.prevent_echo_feedback, with_notification);
        model
            .send_feedback_after_control
            .set_with_optional_notification(self.send_feedback_after_control, with_notification);
        let _ = model.set_advanced_settings(self.advanced.clone(), with_notification);
    }
}

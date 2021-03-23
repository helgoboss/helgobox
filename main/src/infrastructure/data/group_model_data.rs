use crate::application::{GroupId, GroupModel};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{ActivationConditionData, EnabledData};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupModelData {
    // Because default group UUID is the default, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    id: GroupId,
    // Because default group name is empty, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    name: String,
    #[serde(flatten)]
    enabled_data: EnabledData,
    #[serde(flatten)]
    activation_condition_data: ActivationConditionData,
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
            activation_condition_data: ActivationConditionData::from_model(
                &model.activation_condition_model,
            ),
        }
    }

    pub fn to_model(&self, compartment: MappingCompartment) -> GroupModel {
        let mut model = GroupModel::new_from_data(compartment, self.id);
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
        self.activation_condition_data
            .apply_to_model(model.activation_condition_model.borrow_mut(), false);
    }
}

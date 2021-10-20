use crate::application::GroupModel;
use crate::base::default_util::is_default;
use crate::domain::{GroupId, MappingCompartment, Tag};
use crate::infrastructure::data::{ActivationConditionData, EnabledData};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupModelData {
    // Because default group UUID is the default, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    pub id: GroupId,
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<String>,
    // Because default group name is empty, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(flatten)]
    pub enabled_data: EnabledData,
    #[serde(flatten)]
    pub activation_condition_data: ActivationConditionData,
}

impl GroupModelData {
    pub fn key_matches(&self, key: &str) -> bool {
        if let Some(k) = self.key.as_ref() {
            k == key
        } else {
            false
        }
    }

    pub fn from_model(model: &GroupModel) -> GroupModelData {
        GroupModelData {
            id: model.id(),
            key: model.key().cloned(),
            name: model.name.get_ref().clone(),
            tags: model.tags.get_ref().clone(),
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
        let mut model = GroupModel::new_from_data(compartment, self.id, self.key.clone());
        self.apply_to_model(&mut model);
        model
    }

    fn apply_to_model(&self, model: &mut GroupModel) {
        model.name.set_without_notification(self.name.clone());
        model.tags.set_without_notification(self.tags.clone());
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

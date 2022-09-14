use crate::application::{Change, GroupCommand, GroupModel};
use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::{Compartment, GroupId, GroupKey, Tag};
use crate::infrastructure::data::{
    ActivationConditionData, DataToModelConversionContext, EnabledData,
    ModelToDataConversionContext,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupModelData {
    /// Doesn't have to be a UUID since 2.11.0-pre.13 and corresponds to the model *key* instead!
    /// Because default group UUID is the default, it won't be serialized.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    // Saved only in some ReaLearn 2.11.0-pre-releases. Later we persist this in "id" field again.
    // So this is just for being compatible with those few pre-releases!
    #[serde(alias = "key")]
    pub id: GroupKey,
    // Because default group name is empty, it won't be serialized.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub tags: Vec<Tag>,
    #[serde(flatten)]
    pub enabled_data: EnabledData,
    #[serde(flatten)]
    pub activation_condition_data: ActivationConditionData,
}

impl GroupModelData {
    pub fn from_model(
        model: &GroupModel,
        conversion_context: &impl ModelToDataConversionContext,
    ) -> GroupModelData {
        GroupModelData {
            id: model.key().clone(),
            name: model.name().to_owned(),
            tags: model.tags().to_owned(),
            enabled_data: EnabledData {
                control_is_enabled: model.control_is_enabled(),
                feedback_is_enabled: model.feedback_is_enabled(),
            },
            activation_condition_data: ActivationConditionData::from_model(
                model.activation_condition_model(),
                conversion_context,
            ),
        }
    }

    pub fn to_model(
        &self,
        compartment: Compartment,
        is_default_group: bool,
        conversion_context: &impl DataToModelConversionContext,
    ) -> GroupModel {
        let mut model = GroupModel::new_from_data(
            compartment,
            if is_default_group {
                GroupId::default()
            } else {
                conversion_context
                    .group_id_by_key(&self.id)
                    .unwrap_or_else(GroupId::random)
            },
            if is_default_group {
                GroupKey::default()
            } else {
                self.id.clone()
            },
        );
        self.apply_to_model(&mut model, conversion_context);
        model
    }

    fn apply_to_model(
        &self,
        model: &mut GroupModel,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        model.change(GroupCommand::SetName(self.name.clone()));
        model.change(GroupCommand::SetTags(self.tags.clone()));
        model.change(GroupCommand::SetControlIsEnabled(
            self.enabled_data.control_is_enabled,
        ));
        model.change(GroupCommand::SetFeedbackIsEnabled(
            self.enabled_data.feedback_is_enabled,
        ));
        self.activation_condition_data
            .apply_to_model(&mut model.activation_condition_model, conversion_context);
    }
}

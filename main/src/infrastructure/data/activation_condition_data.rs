use crate::application::{
    ActivationConditionCommand, ActivationConditionModel, ActivationType, BankConditionModel,
    Change, ModifierConditionModel,
};
use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::MappingKey;
use crate::infrastructure::data::{DataToModelConversionContext, ModelToDataConversionContext};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationConditionData {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub activation_type: ActivationType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub modifier_condition_1: ModifierConditionModel,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub modifier_condition_2: ModifierConditionModel,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub program_condition: BankConditionModel,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub eel_condition: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mapping_key: Option<MappingKey>,
}

impl ActivationConditionData {
    pub fn from_model(
        model: &ActivationConditionModel,
        conversion_context: &impl ModelToDataConversionContext,
    ) -> ActivationConditionData {
        ActivationConditionData {
            activation_type: model.activation_type(),
            modifier_condition_1: model.modifier_condition_1(),
            modifier_condition_2: model.modifier_condition_2(),
            program_condition: model.bank_condition(),
            eel_condition: model.script().to_owned(),
            mapping_key: model
                .mapping_id()
                .and_then(|id| conversion_context.mapping_key_by_id(id)),
        }
    }

    pub fn apply_to_model(
        &self,
        model: &mut ActivationConditionModel,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        use ActivationConditionCommand as V;
        model.change(V::SetActivationType(self.activation_type));
        model.change(V::SetModifierCondition1(self.modifier_condition_1));
        model.change(V::SetModifierCondition2(self.modifier_condition_2));
        model.change(V::SetBankCondition(self.program_condition));
        model.change(V::SetScript(self.eel_condition.clone()));
        let mapping_id = self
            .mapping_key
            .as_ref()
            .and_then(|key| conversion_context.mapping_id_by_key(key));
        model.change(V::SetMappingId(mapping_id));
    }
}

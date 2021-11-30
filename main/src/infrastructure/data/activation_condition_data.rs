use crate::application::{
    ActivationConditionModel, ActivationConditionPropVal, ActivationType, BankConditionModel,
    ModifierConditionModel, Session,
};
use crate::base::default_util::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationConditionData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub activation_type: ActivationType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_1: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_2: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub program_condition: BankConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub eel_condition: String,
}

impl ActivationConditionData {
    pub fn from_model(model: &ActivationConditionModel) -> ActivationConditionData {
        ActivationConditionData {
            activation_type: model.activation_type(),
            modifier_condition_1: model.modifier_condition_1(),
            modifier_condition_2: model.modifier_condition_2(),
            program_condition: model.bank_condition(),
            eel_condition: model.eel_condition().to_owned(),
        }
    }

    pub fn apply_to_model(
        &self,
        session: &mut Session,
        mut set: impl FnMut(&mut Session, ActivationConditionPropVal),
    ) {
        use ActivationConditionPropVal as V;
        set(session, V::ActivationType(self.activation_type));
        set(session, V::ModifierCondition1(self.modifier_condition_1));
        set(session, V::ModifierCondition2(self.modifier_condition_2));
        set(session, V::BankCondition(self.program_condition));
        set(session, V::EelCondition(self.eel_condition.clone()));
    }
}

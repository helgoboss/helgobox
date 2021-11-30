use crate::application::{ActivationType, BankConditionModel, ModifierConditionModel};
use crate::base::Prop;
use crate::domain::{ActivationCondition, EelCondition};
use rxrust::prelude::*;

pub enum ActivationConditionPropVal {
    ActivationType(ActivationType),
    ModifierCondition1(ModifierConditionModel),
    ModifierCondition2(ModifierConditionModel),
    BankCondition(BankConditionModel),
    EelCondition(String),
}

impl ActivationConditionPropVal {
    pub fn prop(&self) -> ActivationConditionProp {
        use ActivationConditionProp as P;
        use ActivationConditionPropVal as V;
        match self {
            V::ActivationType(_) => P::ActivationType,
            V::ModifierCondition1(_) => P::ModifierCondition1,
            V::ModifierCondition2(_) => P::ModifierCondition2,
            V::BankCondition(_) => P::BankCondition,
            V::EelCondition(_) => P::EelCondition,
        }
    }
}

#[derive(Copy, Clone)]
pub enum ActivationConditionProp {
    ActivationType,
    ModifierCondition1,
    ModifierCondition2,
    BankCondition,
    EelCondition,
}

impl ActivationConditionProp {
    /// Returns true if this is a property that has an effect on control/feedback processing.
    pub fn is_processing_relevant(self) -> bool {
        use ActivationConditionProp as P;
        matches!(
            self,
            P::ActivationType
                | P::ModifierCondition1
                | P::ModifierCondition2
                | P::EelCondition
                | P::BankCondition
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct ActivationConditionModel {
    activation_type: ActivationType,
    modifier_condition_1: ModifierConditionModel,
    modifier_condition_2: ModifierConditionModel,
    bank_condition: BankConditionModel,
    eel_condition: String,
}

impl ActivationConditionModel {
    pub fn set(&mut self, val: ActivationConditionPropVal) -> Result<(), String> {
        use ActivationConditionPropVal as V;
        match val {
            V::ActivationType(v) => self.activation_type = v,
            V::ModifierCondition1(v) => self.modifier_condition_1 = v,
            V::ModifierCondition2(v) => self.modifier_condition_2 = v,
            V::BankCondition(v) => self.bank_condition = v,
            V::EelCondition(v) => self.eel_condition = v,
        };
        Ok(())
    }

    pub fn activation_type(&self) -> ActivationType {
        self.activation_type
    }

    pub fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.modifier_condition_1
    }

    pub fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.modifier_condition_2
    }

    pub fn bank_condition(&self) -> BankConditionModel {
        self.bank_condition
    }

    pub fn eel_condition(&self) -> &str {
        &self.eel_condition
    }

    /// Returns true if this is a property that has an effect on control/feedback processing.
    pub fn is_processing_relevant_prop(&self, prop: ActivationConditionProp) -> bool {
        use ActivationConditionProp as P;
        matches!(
            prop,
            P::ActivationType
                | P::ModifierCondition1
                | P::ModifierCondition2
                | P::EelCondition
                | P::BankCondition
        )
    }

    pub fn create_activation_condition(&self) -> ActivationCondition {
        use ActivationType::*;
        match self.activation_type() {
            Always => ActivationCondition::Always,
            Modifiers => {
                let conditions = self
                    .modifier_conditions()
                    .filter_map(|m| m.create_modifier_condition())
                    .collect();
                ActivationCondition::Modifiers(conditions)
            }
            Bank => ActivationCondition::Program {
                param_index: self.bank_condition().param_index(),
                program_index: self.bank_condition().bank_index(),
            },
            Eel => match EelCondition::compile(self.eel_condition()) {
                Ok(c) => ActivationCondition::Eel(Box::new(c)),
                Err(_) => ActivationCondition::Always,
            },
        }
    }

    fn modifier_conditions(&self) -> impl Iterator<Item = ModifierConditionModel> {
        use std::iter::once;
        once(self.modifier_condition_1()).chain(once(self.modifier_condition_2()))
    }
}

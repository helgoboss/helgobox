use crate::application::{
    ActivationType, Affected, BankConditionModel, Change, GetProcessingRelevance,
    ModifierConditionModel, ProcessingRelevance,
};
use crate::base::Prop;
use crate::domain::{ActivationCondition, EelCondition};
use rxrust::prelude::*;

pub enum ActivationConditionCommand {
    SetActivationType(ActivationType),
    SetModifierCondition1(ModifierConditionModel),
    SetModifierCondition2(ModifierConditionModel),
    SetBankCondition(BankConditionModel),
    SetEelCondition(String),
}

pub enum ActivationConditionProp {
    ActivationType,
    ModifierCondition1,
    ModifierCondition2,
    BankCondition,
    EelCondition,
}

impl GetProcessingRelevance for ActivationConditionProp {
    fn processing_relevance(&self) -> Option<ProcessingRelevance> {
        Some(ProcessingRelevance::ProcessingRelevant)
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

impl<'a> Change<'a> for ActivationConditionModel {
    type Command = ActivationConditionCommand;
    type Prop = ActivationConditionProp;

    fn change(
        &mut self,
        cmd: ActivationConditionCommand,
    ) -> Result<Affected<ActivationConditionProp>, String> {
        use ActivationConditionCommand as C;
        use ActivationConditionProp as P;
        use Affected::*;
        let affected = match cmd {
            C::SetActivationType(v) => {
                self.activation_type = v;
                One(P::ActivationType)
            }
            C::SetModifierCondition1(v) => {
                self.modifier_condition_1 = v;
                One(P::ModifierCondition1)
            }
            C::SetModifierCondition2(v) => {
                self.modifier_condition_2 = v;
                One(P::ModifierCondition2)
            }
            C::SetBankCondition(v) => {
                self.bank_condition = v;
                One(P::BankCondition)
            }
            C::SetEelCondition(v) => {
                self.eel_condition = v;
                One(P::EelCondition)
            }
        };
        Ok(affected)
    }
}

impl ActivationConditionModel {
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

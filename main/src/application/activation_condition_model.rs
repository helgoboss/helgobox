use rx_util::UnitEvent;

use crate::application::{ActivationType, BankConditionModel, ModifierConditionModel};
use crate::core::Prop;
use crate::domain::{ActivationCondition, EelCondition};

#[derive(Clone, Debug, Default)]
pub struct ActivationConditionModel {
    pub activation_type: Prop<ActivationType>,
    pub modifier_condition_1: Prop<ModifierConditionModel>,
    pub modifier_condition_2: Prop<ModifierConditionModel>,
    pub bank_condition: Prop<BankConditionModel>,
    pub eel_condition: Prop<String>,
}

impl ActivationConditionModel {
    /// Fires whenever a property has changed that has an effect on control/feedback processing.
    pub fn changed_processing_relevant(&self) -> impl UnitEvent {
        self.activation_type
            .changed()
            .merge(self.modifier_condition_1.changed())
            .merge(self.modifier_condition_2.changed())
            .merge(self.eel_condition.changed())
            .merge(self.bank_condition.changed())
    }

    pub fn create_activation_condition(&self) -> ActivationCondition {
        use ActivationType::*;
        match self.activation_type.get() {
            Always => ActivationCondition::Always,
            Modifiers => {
                let conditions = self
                    .modifier_conditions()
                    .filter_map(|m| m.create_modifier_condition())
                    .collect();
                ActivationCondition::Modifiers(conditions)
            }
            Bank => ActivationCondition::Program {
                param_index: self.bank_condition.get().param_index(),
                program_index: self.bank_condition.get().bank_index(),
            },
            Eel => match EelCondition::compile(self.eel_condition.get_ref()) {
                Ok(c) => ActivationCondition::Eel(Box::new(c)),
                Err(_) => ActivationCondition::Always,
            },
        }
    }

    fn modifier_conditions(&self) -> impl Iterator<Item = &ModifierConditionModel> {
        use std::iter::once;
        once(self.modifier_condition_1.get_ref()).chain(once(self.modifier_condition_2.get_ref()))
    }
}

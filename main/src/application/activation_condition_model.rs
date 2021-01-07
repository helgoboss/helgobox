use helgoboss_learn::{
    AbsoluteMode, ControlType, Interval, SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use rx_util::UnitEvent;

use crate::application::{
    convert_factor_to_unit_value, ActivationType, GroupId, ModeModel, ModifierConditionModel,
    ProgramConditionModel, SourceModel, TargetCategory, TargetModel, TargetModelWithContext,
};
use crate::core::{prop, Prop};
use crate::domain::{
    ActivationCondition, CompoundMappingTarget, EelCondition, ExtendedSourceCharacter, MainMapping,
    MappingCompartment, MappingId, ProcessorContext, ProcessorMappingOptions, RealearnTarget,
    ReaperTarget, TargetCharacter,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct ActivationConditionModel {
    pub activation_type: Prop<ActivationType>,
    pub modifier_condition_1: Prop<ModifierConditionModel>,
    pub modifier_condition_2: Prop<ModifierConditionModel>,
    pub program_condition: Prop<ProgramConditionModel>,
    pub eel_condition: Prop<String>,
}

impl Clone for ActivationConditionModel {
    fn clone(&self) -> Self {
        Self {
            activation_type: self.activation_type.clone(),
            modifier_condition_1: self.modifier_condition_1.clone(),
            modifier_condition_2: self.modifier_condition_2.clone(),
            program_condition: self.program_condition.clone(),
            eel_condition: self.eel_condition.clone(),
        }
    }
}

impl ActivationConditionModel {
    /// Fires whenever a property has changed that has an effect on control/feedback processing.
    pub fn changed_processing_relevant(&self) -> impl UnitEvent {
        self.activation_type
            .changed()
            .merge(self.modifier_condition_1.changed())
            .merge(self.modifier_condition_2.changed())
            .merge(self.eel_condition.changed())
            .merge(self.program_condition.changed())
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
            Program => ActivationCondition::Program {
                param_index: self.program_condition.get().param_index(),
                program_index: self.program_condition.get().program_index(),
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

use crate::core::{prop, Prop};
use helgoboss_learn::{Interval, SourceCharacter, SymmetricUnitValue, Target, UnitValue};

use crate::application::{
    convert_factor_to_unit_value, ActivationType, ModeModel, ModeType, ModifierConditionModel,
    ProgramConditionModel, SessionContext, SourceModel, TargetModel, TargetModelWithContext,
};
use crate::domain::{
    ActivationCondition, CompoundMappingSource, CompoundMappingTarget, EelCondition, Mapping,
    MappingCompartment, MappingId, ProcessorMappingOptions, ReaperTarget, TargetCharacter,
};
use rx_util::UnitEvent;

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Debug)]
pub struct MappingModel {
    id: MappingId,
    compartment: MappingCompartment,
    pub name: Prop<String>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub prevent_echo_feedback: Prop<bool>,
    pub send_feedback_after_control: Prop<bool>,
    pub activation_type: Prop<ActivationType>,
    pub modifier_condition_1: Prop<ModifierConditionModel>,
    pub modifier_condition_2: Prop<ModifierConditionModel>,
    pub program_condition: Prop<ProgramConditionModel>,
    pub eel_condition: Prop<String>,
    pub source_model: SourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
}

impl Clone for MappingModel {
    fn clone(&self) -> Self {
        Self {
            id: MappingId::random(),
            compartment: self.compartment,
            name: self.name.clone(),
            control_is_enabled: self.control_is_enabled.clone(),
            feedback_is_enabled: self.feedback_is_enabled.clone(),
            prevent_echo_feedback: self.prevent_echo_feedback.clone(),
            send_feedback_after_control: self.send_feedback_after_control.clone(),
            activation_type: self.activation_type.clone(),
            modifier_condition_1: self.modifier_condition_1.clone(),
            modifier_condition_2: self.modifier_condition_2.clone(),
            program_condition: self.program_condition.clone(),
            eel_condition: self.eel_condition.clone(),
            source_model: self.source_model.clone(),
            mode_model: self.mode_model.clone(),
            target_model: self.target_model.clone(),
        }
    }
}

// We design mapping models as entity (in the DDD sense), so we compare them by ID, not by value.
// Because we store everything in memory instead of working with a database, the memory
// address serves us as ID. That means we just compare pointers.
//
// In all functions which don't need access to the mapping's internal state (comparisons, hashing
// etc.) we use `*const MappingModel` as parameter type because this saves the consumer from
// having to borrow the mapping (when kept in a RefCell). Whenever we can we should compare pointers
// directly, in order to prevent borrowing just to make the following comparison (the RefCell
// comparison internally calls `borrow()`!).
impl PartialEq for MappingModel {
    fn eq(&self, other: &Self) -> bool {
        self as *const _ == other as *const _
    }
}

impl MappingModel {
    pub fn new(compartment: MappingCompartment) -> Self {
        Self {
            id: MappingId::random(),
            compartment,
            name: Default::default(),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            prevent_echo_feedback: prop(false),
            send_feedback_after_control: prop(false),
            activation_type: prop(ActivationType::Always),
            modifier_condition_1: Default::default(),
            modifier_condition_2: Default::default(),
            program_condition: Default::default(),
            eel_condition: Default::default(),
            source_model: Default::default(),
            mode_model: Default::default(),
            target_model: Default::default(),
        }
    }

    pub fn compartment(&self) -> MappingCompartment {
        self.compartment
    }

    pub fn with_context<'a>(&'a self, context: &'a SessionContext) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            context,
        }
    }

    pub fn adjust_mode_if_necessary(&mut self, context: &SessionContext) {
        let with_context = self.with_context(context);
        if with_context.mode_makes_sense().contains(&false) {
            if let Ok(preferred_mode_type) = with_context.preferred_mode_type() {
                self.mode_model.r#type.set(preferred_mode_type);
                self.set_preferred_mode_values(context);
            }
        }
    }

    pub fn reset_mode(&mut self, context: &SessionContext) {
        self.mode_model.reset_within_type();
        self.set_preferred_mode_values(context);
    }

    // Changes mode settings if there are some preferred ones for a certain source or target.
    pub fn set_preferred_mode_values(&mut self, context: &SessionContext) {
        self.mode_model
            .step_interval
            .set(self.with_context(context).preferred_step_interval())
    }

    /// Fires whenever a property has changed that doesn't have an effect on control/feedback
    /// processing.
    pub fn changed_non_processing_relevant(&self) -> impl UnitEvent {
        self.name.changed()
    }

    /// Fires whenever a property has changed that has an effect on control/feedback processing.
    pub fn changed_processing_relevant(&self) -> impl UnitEvent {
        self.source_model
            .changed()
            .merge(self.mode_model.changed())
            .merge(self.target_model.changed())
            .merge(self.control_is_enabled.changed())
            .merge(self.feedback_is_enabled.changed())
            .merge(self.prevent_echo_feedback.changed())
            .merge(self.send_feedback_after_control.changed())
            .merge(self.activation_type.changed())
            .merge(self.modifier_condition_1.changed())
            .merge(self.modifier_condition_2.changed())
            .merge(self.eel_condition.changed())
            .merge(self.program_condition.changed())
    }

    fn modifier_conditions(&self) -> impl Iterator<Item = &ModifierConditionModel> {
        use std::iter::once;
        once(self.modifier_condition_1.get_ref()).chain(once(self.modifier_condition_2.get_ref()))
    }
}

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    context: &'a SessionContext,
}

impl<'a> MappingModelWithContext<'a> {
    /// Creates an intermediate mapping for splintering into very dedicated mapping types that are
    /// then going to be distributed to real-time and main processor.
    pub fn create_processor_mapping(&self, params: &[f32]) -> Mapping {
        let id = self.mapping.id;
        let source = self.mapping.source_model.create_source();
        let mode = self.mapping.mode_model.create_mode();
        let target = self.target_with_context().create_target().ok();
        let activation_condition = self.create_activation_condition(params);
        let mapping_is_initially_active = activation_condition.is_fulfilled(params);
        let target_is_initially_active = match &target {
            None => false,
            Some(t) => {
                use CompoundMappingTarget::*;
                match t {
                    Reaper(t) => self.mapping.target_model.conditions_are_met(t),
                    Virtual(_) => true,
                }
            }
        };
        let options = ProcessorMappingOptions {
            mapping_is_active: mapping_is_initially_active,
            target_is_active: target_is_initially_active,
            control_is_enabled: self.mapping.control_is_enabled.get(),
            feedback_is_enabled: self.mapping.feedback_is_enabled.get(),
            prevent_echo_feedback: self.mapping.prevent_echo_feedback.get(),
            send_feedback_after_control: self.mapping.send_feedback_after_control.get(),
        };
        Mapping::new(id, source, mode, target, activation_condition, options)
    }

    fn create_activation_condition(&self, params: &[f32]) -> ActivationCondition {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Always => ActivationCondition::Always,
            Modifiers => {
                let conditions = self
                    .mapping
                    .modifier_conditions()
                    .filter_map(|m| m.create_modifier_condition())
                    .collect();
                ActivationCondition::Modifiers(conditions)
            }
            Program => ActivationCondition::Program {
                param_index: self.mapping.program_condition.get().param_index(),
                program_index: self.mapping.program_condition.get().program_index(),
            },
            Eel => match EelCondition::compile(self.mapping.eel_condition.get_ref(), params) {
                Ok(c) => ActivationCondition::Eel(Box::new(c)),
                Err(_) => ActivationCondition::Always,
            },
        }
    }

    pub fn mode_makes_sense(&self) -> Result<bool, &'static str> {
        use ModeType::*;
        use SourceCharacter::*;
        let mode_type = self.mapping.mode_model.r#type.get();
        let result = match self.mapping.source_model.character() {
            Range => mode_type == Absolute,
            Button => {
                let target = self.target_with_context().create_target()?;
                match mode_type {
                    Absolute | Toggle => !target.control_type().is_relative(),
                    Relative => {
                        if target.control_type().is_relative() {
                            true
                        } else {
                            match target.character() {
                                TargetCharacter::Discrete
                                | TargetCharacter::Continuous
                                | TargetCharacter::VirtualContinuous => true,
                                TargetCharacter::Trigger
                                | TargetCharacter::Switch
                                | TargetCharacter::VirtualButton => false,
                            }
                        }
                    }
                }
            }
            Encoder1 | Encoder2 | Encoder3 => mode_type == Relative,
        };
        Ok(result)
    }

    pub fn has_target(&self, target: &ReaperTarget) -> bool {
        match self.target_with_context().create_target() {
            Ok(CompoundMappingTarget::Reaper(t)) => t == *target,
            _ => false,
        }
    }

    pub fn preferred_mode_type(&self) -> Result<ModeType, &'static str> {
        use ModeType::*;
        use SourceCharacter::*;
        let result = match self.mapping.source_model.character() {
            Range => Absolute,
            Button => {
                let target = self.target_with_context().create_target()?;
                if target.control_type().is_relative() {
                    Relative
                } else {
                    match target.character() {
                        TargetCharacter::Trigger
                        | TargetCharacter::Continuous
                        | TargetCharacter::VirtualContinuous => Absolute,
                        TargetCharacter::Switch | TargetCharacter::VirtualButton => Toggle,
                        TargetCharacter::Discrete => Relative,
                    }
                }
            }
            Encoder1 | Encoder2 | Encoder3 => Relative,
        };
        Ok(result)
    }

    pub fn uses_step_counts(&self) -> bool {
        let target = self.target_with_context();
        target.is_known_to_be_relative() || target.is_known_to_be_discrete()
    }

    fn preferred_step_interval(&self) -> Interval<SymmetricUnitValue> {
        if self.uses_step_counts() {
            let one_step = convert_factor_to_unit_value(1).expect("impossible");
            Interval::new(one_step, one_step)
        } else {
            match self.target_step_size() {
                Some(step_size) => {
                    Interval::new(step_size.to_symmetric(), step_size.to_symmetric())
                }
                None => ModeModel::default_step_size_interval(),
            }
        }
    }

    fn target_step_size(&self) -> Option<UnitValue> {
        let target = self.target_with_context().create_target().ok()?;
        target.control_type().step_size()
    }

    fn target_with_context(&self) -> TargetModelWithContext<'_> {
        self.mapping.target_model.with_context(self.context)
    }
}

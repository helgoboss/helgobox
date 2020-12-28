use helgoboss_learn::{
    AbsoluteMode, ControlType, Interval, SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use rx_util::UnitEvent;

use crate::application::{
    convert_factor_to_unit_value, ActivationType, ModeModel, ModifierConditionModel,
    ProgramConditionModel, SourceModel, TargetCategory, TargetModel, TargetModelWithContext,
};
use crate::core::{prop, Prop};
use crate::domain::{
    ActivationCondition, CompoundMappingTarget, EelCondition, ExtendedSourceCharacter, MainMapping,
    MappingCompartment, MappingId, ProcessorContext, ProcessorMappingOptions, RealearnTarget,
    ReaperTarget, TargetCharacter,
};

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
            id: self.id,
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
        std::ptr::eq(self as _, other as _)
    }
}

fn get_default_target_category_for_compartment(compartment: MappingCompartment) -> TargetCategory {
    use MappingCompartment::*;
    match compartment {
        ControllerMappings => TargetCategory::Virtual,
        MainMappings => TargetCategory::Reaper,
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
            target_model: TargetModel {
                category: prop(get_default_target_category_for_compartment(compartment)),
                ..Default::default()
            },
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn duplicate(&self) -> MappingModel {
        MappingModel {
            id: MappingId::random(),
            ..self.clone()
        }
    }

    // TODO-low Setting an ID is code smell. We should rather provide a second factory function.
    //  Because we never map data on an existing Mapping anyway, we always create a new one.
    pub fn set_id_without_notification(&mut self, id: MappingId) {
        self.id = id;
    }

    pub fn compartment(&self) -> MappingCompartment {
        self.compartment
    }

    pub fn with_context<'a>(
        &'a self,
        context: &'a ProcessorContext,
    ) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            context,
        }
    }

    pub fn adjust_mode_if_necessary(&mut self, context: &ProcessorContext) {
        let with_context = self.with_context(context);
        if with_context.mode_makes_sense().contains(&false) {
            if let Ok(preferred_mode_type) = with_context.preferred_mode_type() {
                self.mode_model.r#type.set(preferred_mode_type);
                self.set_preferred_mode_values(context);
            }
        }
    }

    pub fn reset_mode(&mut self, context: &ProcessorContext) {
        self.mode_model.reset_within_type();
        self.set_preferred_mode_values(context);
    }

    // Changes mode settings if there are some preferred ones for a certain source or target.
    pub fn set_preferred_mode_values(&mut self, context: &ProcessorContext) {
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
    /// Creates an intermediate mapping for splintering into very dedicated mapping types that are
    /// then going to be distributed to real-time and main processor.
    pub fn create_main_mapping(&self) -> MainMapping {
        let id = self.id;
        let source = self.source_model.create_source();
        let mode = self.mode_model.create_mode();
        let unresolved_target = self.target_model.create_target().ok();
        let activation_condition = self.create_activation_condition();
        let options = ProcessorMappingOptions {
            // TODO-medium Encapsulate, don't set here
            mapping_is_active: false,
            // TODO-medium Encapsulate, don't set here
            target_is_active: false,
            control_is_enabled: self.control_is_enabled.get(),
            feedback_is_enabled: self.feedback_is_enabled.get(),
            prevent_echo_feedback: self.prevent_echo_feedback.get(),
            send_feedback_after_control: self.send_feedback_after_control.get(),
        };
        MainMapping::new(
            id,
            source,
            mode,
            unresolved_target,
            activation_condition,
            options,
        )
    }

    fn create_activation_condition(&self) -> ActivationCondition {
        if self.compartment == MappingCompartment::ControllerMappings {
            // Controller mappings are always active, no matter what weird stuff is in the model.
            return ActivationCondition::Always;
        }
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

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    context: &'a ProcessorContext,
}

impl<'a> MappingModelWithContext<'a> {
    pub fn mode_makes_sense(&self) -> Result<bool, &'static str> {
        use ExtendedSourceCharacter::*;
        use SourceCharacter::*;
        let mode_type = self.mapping.mode_model.r#type.get();
        let result = match self.mapping.source_model.character() {
            Normal(Range) => mode_type == AbsoluteMode::Normal,
            Normal(Button) => {
                let target = self.target_with_context().create_target()?;
                match mode_type {
                    AbsoluteMode::Normal | AbsoluteMode::ToggleButtons => {
                        !target.control_type().is_relative()
                    }
                    AbsoluteMode::IncrementalButtons => {
                        if target.control_type().is_relative() {
                            true
                        } else {
                            match target.character() {
                                TargetCharacter::Discrete
                                | TargetCharacter::Continuous
                                | TargetCharacter::VirtualMulti => true,
                                TargetCharacter::Trigger
                                | TargetCharacter::Switch
                                | TargetCharacter::VirtualButton => false,
                            }
                        }
                    }
                }
            }
            Normal(Encoder1) | Normal(Encoder2) | Normal(Encoder3) => true,
            VirtualContinuous => true,
        };
        Ok(result)
    }

    pub fn has_target(&self, target: &ReaperTarget) -> bool {
        match self.target_with_context().create_target() {
            Ok(CompoundMappingTarget::Reaper(t)) => t == *target,
            _ => false,
        }
    }

    pub fn preferred_mode_type(&self) -> Result<AbsoluteMode, &'static str> {
        use ExtendedSourceCharacter::*;
        use SourceCharacter::*;
        let result = match self.mapping.source_model.character() {
            Normal(Range) | VirtualContinuous => AbsoluteMode::Normal,
            Normal(Button) => {
                let target = self.target_with_context().create_target()?;
                if target.control_type().is_relative() {
                    AbsoluteMode::IncrementalButtons
                } else {
                    match target.character() {
                        TargetCharacter::Trigger
                        | TargetCharacter::Continuous
                        | TargetCharacter::VirtualMulti => AbsoluteMode::Normal,
                        TargetCharacter::Switch | TargetCharacter::VirtualButton => {
                            AbsoluteMode::ToggleButtons
                        }
                        TargetCharacter::Discrete => AbsoluteMode::IncrementalButtons,
                    }
                }
            }
            Normal(Encoder1) | Normal(Encoder2) | Normal(Encoder3) => AbsoluteMode::Normal,
        };
        Ok(result)
    }

    pub fn uses_step_counts(&self) -> bool {
        let target = match self.target_with_context().create_target().ok() {
            None => return false,
            Some(t) => t,
        };
        match target.control_type() {
            ControlType::AbsoluteTrigger => false,
            ControlType::AbsoluteSwitch => false,
            ControlType::AbsoluteContinuous => false,
            ControlType::AbsoluteContinuousRoundable { .. } => false,
            ControlType::AbsoluteDiscrete { .. } => true,
            ControlType::Relative => true,
            ControlType::VirtualMulti => true,
            ControlType::VirtualButton => false,
        }
    }

    fn preferred_step_interval(&self) -> Interval<SoftSymmetricUnitValue> {
        if self.uses_step_counts() {
            let one_step = convert_factor_to_unit_value(1);
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

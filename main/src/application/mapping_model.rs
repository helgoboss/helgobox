use helgoboss_learn::{
    AbsoluteMode, ControlType, DetailedSourceCharacter, Interval, ModeApplicabilityCheckInput,
    ModeParameter, SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use rx_util::UnitEvent;

use crate::application::{
    convert_factor_to_unit_value, ActivationConditionModel, GroupId, MappingExtensionModel,
    ModeModel, SourceModel, TargetCategory, TargetModel, TargetModelWithContext,
};
use crate::core::{prop, Prop};
use crate::domain::{
    ActivationCondition, CompoundMappingTarget, ExtendedProcessorContext, ExtendedSourceCharacter,
    MainMapping, MappingCompartment, MappingId, ProcessorMappingOptions, QualifiedMappingId,
    RealearnTarget, ReaperTarget, TargetCharacter,
};

use std::cell::RefCell;
use std::convert::TryInto;
use std::rc::Rc;

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug)]
pub struct MappingModel {
    id: MappingId,
    compartment: MappingCompartment,
    pub name: Prop<String>,
    pub group_id: Prop<GroupId>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub prevent_echo_feedback: Prop<bool>,
    pub send_feedback_after_control: Prop<bool>,
    pub activation_condition_model: ActivationConditionModel,
    pub source_model: SourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
    advanced_settings: Prop<Option<serde_yaml::mapping::Mapping>>,
    extension_model: MappingExtensionModel,
}

pub type SharedMapping = Rc<RefCell<MappingModel>>;

pub fn share_mapping(mapping: MappingModel) -> SharedMapping {
    Rc::new(RefCell::new(mapping))
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
    pub fn new(compartment: MappingCompartment, initial_group_id: GroupId) -> Self {
        Self {
            id: MappingId::random(),
            compartment,
            name: Default::default(),
            group_id: prop(initial_group_id),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            prevent_echo_feedback: prop(false),
            send_feedback_after_control: prop(false),
            activation_condition_model: Default::default(),
            source_model: Default::default(),
            mode_model: Default::default(),
            target_model: TargetModel {
                category: prop(get_default_target_category_for_compartment(compartment)),
                ..Default::default()
            },
            advanced_settings: prop(None),
            extension_model: Default::default(),
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn qualified_id(&self) -> QualifiedMappingId {
        QualifiedMappingId::new(self.compartment, self.id)
    }

    pub fn effective_name(&self) -> String {
        if self.name.get_ref().is_empty() {
            self.target_model.to_string()
        } else {
            self.name.get_ref().clone()
        }
    }

    pub fn clear_name(&mut self) {
        self.name.set(Default::default());
    }

    pub fn advanced_settings(&self) -> Option<&serde_yaml::Mapping> {
        self.advanced_settings.get_ref().as_ref()
    }

    pub fn set_advanced_settings(
        &mut self,
        value: Option<serde_yaml::Mapping>,
        with_notification: bool,
    ) -> Result<(), String> {
        self.advanced_settings
            .set_with_optional_notification(value, with_notification);
        self.update_extension_model_from_advanced_settings()?;
        Ok(())
    }

    fn update_extension_model_from_advanced_settings(&mut self) -> Result<(), String> {
        // Immediately update extension model
        let extension_model = if let Some(yaml_mapping) = self.advanced_settings.get_ref() {
            serde_yaml::from_value(serde_yaml::Value::Mapping(yaml_mapping.clone()))
                .map_err(|e| e.to_string())?
        } else {
            Default::default()
        };
        self.extension_model = extension_model;
        Ok(())
    }

    pub fn advanced_settings_changed(&self) -> impl UnitEvent {
        self.advanced_settings.changed()
    }

    pub fn duplicate(&self) -> MappingModel {
        MappingModel {
            id: MappingId::random(),
            ..self.clone()
        }
    }

    pub fn set_id_without_notification(&mut self, id: MappingId) {
        self.id = id;
    }

    pub fn compartment(&self) -> MappingCompartment {
        self.compartment
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
    ) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            context,
        }
    }

    pub fn adjust_mode_if_necessary(&mut self, context: ExtendedProcessorContext) {
        let with_context = self.with_context(context);
        if with_context.mode_makes_sense().contains(&false) {
            if let Ok(preferred_mode_type) = with_context.preferred_mode_type() {
                self.mode_model.r#type.set(preferred_mode_type);
                self.set_preferred_mode_values(context);
            }
        }
    }

    pub fn reset_mode(&mut self, context: ExtendedProcessorContext) {
        self.mode_model.reset_within_type();
        self.set_preferred_mode_values(context);
    }

    // Changes mode settings if there are some preferred ones for a certain source or target.
    pub fn set_preferred_mode_values(&mut self, context: ExtendedProcessorContext) {
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
            .merge(
                self.activation_condition_model
                    .changed_processing_relevant(),
            )
            .merge(self.advanced_settings.changed())
    }

    pub fn base_mode_applicability_check_input(&self) -> ModeApplicabilityCheckInput {
        ModeApplicabilityCheckInput {
            target_is_virtual: self.target_model.is_virtual(),
            is_feedback: false,
            make_absolute: self.mode_model.make_absolute.get(),
            // Any is okay, will be overwritten.
            source_character: DetailedSourceCharacter::RangeControl,
            absolute_mode: self.mode_model.r#type.get(),
            // Any is okay, will be overwritten.
            mode_parameter: ModeParameter::TargetMinMax,
        }
    }

    /// Creates an intermediate mapping for splintering into very dedicated mapping types that are
    /// then going to be distributed to real-time and main processor.
    pub fn create_main_mapping(
        &self,
        group_data: GroupData,
        _logger: &slog::Logger,
    ) -> MainMapping {
        let id = self.id;
        let source = self.source_model.create_source();
        let possible_source_characters = self.source_model.possible_detailed_characters();
        let mode = self.mode_model.create_mode(
            self.base_mode_applicability_check_input(),
            &possible_source_characters,
        );
        let unresolved_target = self.target_model.create_target().ok();
        let activation_condition = self
            .activation_condition_model
            .create_activation_condition();
        let options = ProcessorMappingOptions {
            // TODO-medium Encapsulate, don't set here
            target_is_active: false,
            control_is_enabled: group_data.control_is_enabled && self.control_is_enabled.get(),
            feedback_is_enabled: group_data.feedback_is_enabled && self.feedback_is_enabled.get(),
            prevent_echo_feedback: self.prevent_echo_feedback.get(),
            send_feedback_after_control: self.send_feedback_after_control.get(),
        };
        MainMapping::new(
            self.compartment,
            id,
            source,
            mode,
            unresolved_target,
            group_data.activation_condition,
            activation_condition,
            options,
            self.extension_model.clone().try_into().unwrap_or_default(),
        )
    }
}

pub struct GroupData {
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub activation_condition: ActivationCondition,
}

impl Default for GroupData {
    fn default() -> Self {
        Self {
            control_is_enabled: true,
            feedback_is_enabled: true,
            activation_condition: ActivationCondition::Always,
        }
    }
}

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    context: ExtendedProcessorContext<'a>,
}

impl<'a> MappingModelWithContext<'a> {
    pub fn mode_makes_sense(&self) -> Result<bool, &'static str> {
        use ExtendedSourceCharacter::*;
        use SourceCharacter::*;
        let mode_type = self.mapping.mode_model.r#type.get();
        let result = match self.mapping.source_model.character() {
            Normal(RangeElement) => mode_type == AbsoluteMode::Normal,
            Normal(MomentaryButton) | Normal(ToggleButton) => {
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
            Normal(RangeElement) | VirtualContinuous => AbsoluteMode::Normal,
            Normal(MomentaryButton) | Normal(ToggleButton) => {
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
        if self.mapping.mode_model.make_absolute.get() {
            // If we convert increments to absolute values, we want step sizes of course.
            return false;
        }
        let target = match self.target_with_context().create_target().ok() {
            None => return false,
            Some(t) => t,
        };
        match target.control_type() {
            ControlType::AbsoluteContinuousRetriggerable => false,
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
        self.mapping
            .target_model
            .with_context(self.context, self.mapping.compartment)
    }
}

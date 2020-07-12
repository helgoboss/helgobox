use crate::core::{prop, Prop};
use crate::domain::{
    convert_factor_to_unit_value, MainProcessorMapping, MappingId, MidiSourceModel, ModeModel,
    ModeType, ProcessorMapping, RealTimeProcessorMapping, ReaperTarget, SessionContext,
    SharedMapping, TargetCharacter, TargetModel, TargetModelWithContext,
};
use helgoboss_learn::{
    Interval, MidiSource, SourceCharacter, SymmetricUnitValue, Target, UnitValue,
};
use reaper_high::Fx;
use rx_util::{BoxedUnitEvent, UnitEvent};
use rxrust::prelude::ops::box_it::{BoxObservable, LocalBoxOp};
use rxrust::prelude::*;
use uuid::Uuid;

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Debug)]
pub struct MappingModel {
    id: MappingId,
    pub name: Prop<String>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub source_model: MidiSourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
}

impl Clone for MappingModel {
    fn clone(&self) -> Self {
        Self {
            id: MappingId::random(),
            name: self.name.clone(),
            control_is_enabled: self.control_is_enabled.clone(),
            feedback_is_enabled: self.feedback_is_enabled.clone(),
            source_model: self.source_model.clone(),
            mode_model: self.mode_model.clone(),
            target_model: self.target_model.clone(),
        }
    }
}

impl Default for MappingModel {
    fn default() -> Self {
        Self {
            id: MappingId::random(),
            name: Default::default(),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            source_model: Default::default(),
            mode_model: Default::default(),
            target_model: Default::default(),
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
    pub fn id(&self) -> &MappingId {
        &self.id
    }

    pub fn with_context<'a>(&'a self, context: &'a SessionContext) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            context,
        }
    }

    pub fn has_source(&self, source: &MidiSource) -> bool {
        self.source_model.create_source() == *source
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
    }
}

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    context: &'a SessionContext,
}

impl<'a> MappingModelWithContext<'a> {
    /// Creates a mapping for usage in real-time and main processors.
    ///
    /// `control_is_enabled` and `feedback_is_enabled` won't just reflect the manual setting
    /// but also the target condition (e.g. "track selected" or "FX focused").
    pub fn create_processor_mapping(&self) -> ProcessorMapping {
        let target = self.target_with_context().create_target().ok();
        let target_conditions_are_met = match &target {
            None => false,
            Some(t) => self.mapping.target_model.conditions_are_met(t),
        };
        ProcessorMapping::new(
            self.mapping.id,
            self.mapping.source_model.create_source(),
            self.mapping.mode_model.create_mode(),
            target,
            self.mapping.control_is_enabled.get() && target_conditions_are_met,
            self.mapping.feedback_is_enabled.get() && target_conditions_are_met,
        )
    }

    pub fn mode_makes_sense(&self) -> Result<bool, &'static str> {
        use ModeType::*;
        use SourceCharacter::*;
        let mode_type = self.mapping.mode_model.r#type.get();
        let result = match self.mapping.source_model.character() {
            Range => mode_type == Absolute,
            Switch => {
                let target = self.target_with_context().create_target()?;
                match mode_type {
                    Absolute | Toggle => !target.control_type().is_relative(),
                    Relative => {
                        if target.control_type().is_relative() {
                            true
                        } else {
                            match target.character() {
                                TargetCharacter::Discrete | TargetCharacter::Continuous => true,
                                TargetCharacter::Trigger | TargetCharacter::Switch => false,
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
            Ok(t) => t == *target,
            Err(_) => false,
        }
    }

    pub fn preferred_mode_type(&self) -> Result<ModeType, &'static str> {
        use ModeType::*;
        use SourceCharacter::*;
        let result = match self.mapping.source_model.character() {
            Range => Absolute,
            Switch => {
                let target = self.target_with_context().create_target()?;
                if target.control_type().is_relative() {
                    Relative
                } else {
                    match target.character() {
                        TargetCharacter::Trigger | TargetCharacter::Continuous => Absolute,
                        TargetCharacter::Switch => Toggle,
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

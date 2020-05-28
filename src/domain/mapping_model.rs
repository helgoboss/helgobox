use crate::domain::{
    MidiSourceModel, ModeModel, ReaperTarget, TargetCharacter, TargetModel, TargetModelWithContext,
};
use helgoboss_learn::{Interval, Target, UnitValue};
use reaper_high::Fx;
use rx_util::{LocalProp, LocalStaticProp};

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug, Default)]
pub struct MappingModel {
    pub name: LocalStaticProp<String>,
    pub control_is_enabled: LocalStaticProp<bool>,
    pub feedback_is_enabled: LocalStaticProp<bool>,
    pub source_model: MidiSourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
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
    pub fn with_context<'a>(&'a self, containing_fx: &'a Fx) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            containing_fx,
        }
    }

    pub fn reset_mode(&mut self, containing_fx: &Fx) {
        self.mode_model.reset_within_type();
        self.set_preferred_mode_values(containing_fx);
    }

    // Changes mode settings if there are some preferred ones for a certain source or target.
    pub fn set_preferred_mode_values(&mut self, containing_fx: &Fx) {
        self.mode_model.step_size_interval.set(
            self.with_context(containing_fx)
                .preferred_step_size_interval(),
        )
    }
}

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    containing_fx: &'a Fx,
}

impl<'a> MappingModelWithContext<'a> {
    pub fn target_should_be_hit_with_increments(&self) -> bool {
        let target = self.target_with_context();
        target.is_known_to_want_increments()
            || (self.mapping.source_model.emits_increments() && target.is_known_to_be_discrete())
    }

    fn preferred_step_size_interval(&self) -> Interval<UnitValue> {
        match self.target_step_size() {
            Some(step_size) => Interval::new(step_size, step_size),
            None => ModeModel::default_step_size_interval(),
        }
    }

    fn target_step_size(&self) -> Option<UnitValue> {
        let target = self.target_with_context().create_target().ok()?;
        target.step_size()
    }

    fn target_with_context(&self) -> TargetModelWithContext<'_> {
        self.mapping.target_model.with_context(self.containing_fx)
    }
}

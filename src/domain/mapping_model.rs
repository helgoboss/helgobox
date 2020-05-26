use crate::domain::{MidiSourceModel, ModeModel, TargetCharacter, TargetModel};
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
    pub fn target_should_be_hit_with_increments(&self, containing_fx: &Fx) -> bool {
        self.target_model.is_known_to_want_increments(containing_fx)
            || (self.source_model.emits_increments()
                && self.target_model.is_known_to_be_discrete(containing_fx))
    }
}

use crate::domain::{
    CompoundMappingSource, FeedbackOutput, RealSource, RealearnTargetContext, ReaperTarget,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

make_available_globally_in_main_thread_on_demand!(DomainGlobal);

/// This is the domain-layer "backbone" which can hold state that's shared among all ReaLearn
/// instances.
pub struct DomainGlobal {
    target_context: RefCell<RealearnTargetContext>,
    last_touched_target: RefCell<Option<ReaperTarget>>,
    /// Source usage
    ///
    /// Key chain: Feedback output and source
    ///
    /// Values: IDs of ReaLearn instances that use that source (a vector because there will
    /// usually be a handful only).
    source_usage: HashMap<FeedbackOutput, HashMap<CompoundMappingSource, Vec<String>>>,
}

impl DomainGlobal {
    pub fn new(target_context: RealearnTargetContext) -> Self {
        Self {
            target_context: RefCell::new(target_context),
            last_touched_target: RefCell::new(None),
            source_usage: Default::default(),
        }
    }

    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &DomainGlobal::get().target_context
    }

    pub fn last_touched_target(&self) -> Option<ReaperTarget> {
        self.last_touched_target.borrow().clone()
    }

    // TODO-high We need to fill the maps!
    pub fn source_receives_feedback_from_other_instance(
        &self,
        this_instance_id: &str,
        feedback_output: FeedbackOutput,
        real_source: &CompoundMappingSource,
    ) -> bool {
        if let Some(instance_ids) = self.find_instance_ids(feedback_output, real_source) {
            instance_ids.iter().any(|id| id != this_instance_id)
        } else {
            false
        }
    }

    pub(super) fn set_last_touched_target(&self, target: ReaperTarget) {
        *self.last_touched_target.borrow_mut() = Some(target);
    }

    fn find_instance_ids(
        &self,
        feedback_output: FeedbackOutput,
        real_source: &CompoundMappingSource,
    ) -> Option<&Vec<String>> {
        let by_source = self.source_usage.get(&feedback_output)?;
        by_source.get(real_source)
    }
}

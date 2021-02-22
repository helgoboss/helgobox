use crate::domain::{RealearnTargetContext, ReaperTarget};
use std::cell::RefCell;

make_available_globally_in_main_thread!(DomainGlobal);

#[derive(Default)]
pub struct DomainGlobal {
    target_context: RefCell<RealearnTargetContext>,
    last_touched_target: RefCell<Option<ReaperTarget>>,
}

impl DomainGlobal {
    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &DomainGlobal::get().target_context
    }

    pub fn last_touched_target(&self) -> Option<ReaperTarget> {
        self.last_touched_target.borrow().clone()
    }

    pub(super) fn set_last_touched_target(&self, target: ReaperTarget) {
        *self.last_touched_target.borrow_mut() = Some(target);
    }
}

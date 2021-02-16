use crate::domain::RealearnTargetContext;
use std::cell::RefCell;

make_available_globally_in_main_thread!(DomainGlobal);

#[derive(Default)]
pub struct DomainGlobal {
    target_context: RefCell<RealearnTargetContext>,
}

impl DomainGlobal {
    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &DomainGlobal::get().target_context
    }
}

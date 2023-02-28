use crate::domain::realearn_target::RealearnTarget;
use crate::domain::{
    BackboneState, Compartment, ExtendedProcessorContext, ReaperTarget, ReaperTargetType,
    UnresolvedReaperTargetDef,
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct UnresolvedLastTouchedTarget {
    pub included_targets: HashSet<ReaperTargetType>,
}

impl UnresolvedReaperTargetDef for UnresolvedLastTouchedTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let last_touched_target = BackboneState::get()
            .find_last_touched_target(&self.included_targets)
            .ok_or("no last touched target")?;
        if !last_touched_target.is_available(context.control_context()) {
            return Err("last touched target gone");
        }
        Ok(vec![last_touched_target])
    }
}

use crate::domain::realearn_target::RealearnTarget;
use crate::domain::{
    BackboneState, Compartment, ExtendedProcessorContext, LastTouchedTargetFilter, ReaperTarget,
    ReaperTargetType, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use realearn_api::persistence::TargetTouchCause;
use std::collections::HashSet;

#[derive(Debug)]
pub struct UnresolvedLastTouchedTarget {
    pub included_targets: HashSet<ReaperTargetType>,
    pub touch_cause: TargetTouchCause,
}

impl UnresolvedReaperTargetDef for UnresolvedLastTouchedTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let filter = LastTouchedTargetFilter {
            included_target_types: &self.included_targets,
            touch_cause: self.touch_cause,
        };
        let last_touched_target = BackboneState::get()
            .find_last_touched_target(filter)
            .ok_or("no last touched target")?;
        if !last_touched_target.is_available(context.control_context()) {
            return Err("last touched target gone");
        }
        Ok(vec![last_touched_target])
    }
}

pub const LAST_TOUCHED_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Global: Last touched",
    short_name: "Last touched",
    supports_included_targets: true,
    ..DEFAULT_TARGET
};

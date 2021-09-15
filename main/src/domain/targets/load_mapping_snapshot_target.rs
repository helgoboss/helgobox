use crate::domain::{
    ControlContext, GroupId, HitInstruction, HitInstructionContext, HitInstructionReturnValue,
    MappingControlContext, MappingControlResult, MappingScope, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Clone, Debug, PartialEq)]
pub struct LoadMappingSnapshotTarget {
    pub scope: MappingScope,
    pub active_mappings_only: bool,
}

impl RealearnTarget for LoadMappingSnapshotTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            return Ok(None);
        }
        struct LoadMappingSnapshotInstruction {
            scope: MappingScope,
            required_group_id: GroupId,
            active_mappings_only: bool,
        }
        impl HitInstruction for LoadMappingSnapshotInstruction {
            fn execute(&self, context: HitInstructionContext) -> Vec<MappingControlResult> {
                let mut control_results = vec![];
                for m in context.mappings.values_mut() {
                    if !m.control_is_enabled() {
                        continue;
                    }
                    if !self.scope.matches(m, self.required_group_id) {
                        continue;
                    }
                    if self.active_mappings_only && !m.is_active() {
                        continue;
                    }
                    if let Some(r) = m.hit_target_with_initial_value_snapshot_if_any(
                        context.control_context,
                        context.logger,
                        context.processor_context,
                    ) {
                        control_results.push(r);
                    }
                }
                control_results
            }
        }
        let instruction = LoadMappingSnapshotInstruction {
            // So far this clone is okay because loading a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            required_group_id: context.mapping_data.group_id,
            active_mappings_only: self.active_mappings_only,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn can_report_current_value(&self) -> bool {
        false
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }
}

impl<'a> Target<'a> for LoadMappingSnapshotTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

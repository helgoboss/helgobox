use crate::domain::{
    GroupId, HitInstruction, HitInstructionContext, HitInstructionReturnValue,
    MappingControlContext, MappingScope, RealearnTarget, Tag, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Clone, Debug, PartialEq)]
pub struct LoadMappingSnapshotTarget {
    pub scope: MappingScope,
    pub tags: Vec<Tag>,
}

impl RealearnTarget for LoadMappingSnapshotTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
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
            tags: Vec<Tag>,
            group_id: GroupId,
        }
        impl HitInstruction for LoadMappingSnapshotInstruction {
            fn execute(&self, context: HitInstructionContext) {
                for m in context.mappings.values_mut() {
                    if !m.control_is_enabled() {
                        continue;
                    }
                    if self.scope.active_mappings_only() && !m.is_active() {
                        continue;
                    }
                    if self.scope == MappingScope::AllInGroup && m.group_id() != self.group_id {
                        continue;
                    }
                    if !self.tags.is_empty() && !m.has_any_tag(&self.tags) {
                        continue;
                    }
                    m.hit_target_with_initial_value_snapshot(context.control_context)
                }
            }
        }
        let instruction = LoadMappingSnapshotInstruction {
            scope: self.scope,
            // So far this clone is okay because loading a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            tags: self.tags.clone(),
            group_id: context.mapping_data.group_id,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn can_report_current_value(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl<'a> Target<'a> for LoadMappingSnapshotTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

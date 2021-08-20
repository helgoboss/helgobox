use crate::domain::{
    ControlContext, HitInstruction, HitInstructionContext, HitInstructionReturnValue,
    RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Clone, Debug, PartialEq)]
pub struct LoadMappingSnapshotTarget {}

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
        _: ControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            return Ok(None);
        }
        struct LoadMappingSnapshotInstruction;
        impl HitInstruction for LoadMappingSnapshotInstruction {
            fn execute(&self, context: HitInstructionContext) {
                for m in context.mappings.values_mut() {
                    m.hit_target_with_initial_value_snapshot(context.control_context)
                }
            }
        }
        Ok(Some(Box::new(LoadMappingSnapshotInstruction)))
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

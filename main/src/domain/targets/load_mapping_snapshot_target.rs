use crate::domain::{
    ControlContext, HitInstruction, HitInstructionContext, HitInstructionReturnValue,
    MappingControlContext, MappingControlResult, RealearnTarget, ReaperTargetType, TagScope,
    TargetCharacter, TargetTypeDef, DEFAULT_TARGET_TYPE_DEF,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Clone, Debug, PartialEq)]
pub struct LoadMappingSnapshotTarget {
    pub scope: TagScope,
    pub active_mappings_only: bool,
}

impl RealearnTarget for LoadMappingSnapshotTarget {
    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::LoadMappingSnapshot)
    }

    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            return Ok(None);
        }
        struct LoadMappingSnapshotInstruction {
            scope: TagScope,
            active_mappings_only: bool,
        }
        impl HitInstruction for LoadMappingSnapshotInstruction {
            fn execute(
                self: Box<Self>,
                context: HitInstructionContext,
            ) -> Vec<MappingControlResult> {
                let mut control_results = vec![];
                for m in context.mappings.values_mut() {
                    if !m.control_is_enabled() {
                        // If "Control disabled", it doesn't make much sense because then it means
                        // we don't have a chance to modify the target via this mapping via
                        // ReaLearn anyway.
                        continue;
                    }
                    if self.scope.has_tags() && !m.has_any_tag(&self.scope.tags) {
                        continue;
                    }
                    if self.active_mappings_only && !m.is_effectively_on() {
                        continue;
                    }
                    if let Some(inital_value) = m.initial_target_value_snapshot() {
                        context
                            .domain_event_handler
                            .notify_mapping_matched(m.compartment(), m.id());
                        let res = m.control_from_target_directly(
                            context.control_context,
                            context.logger,
                            context.processor_context,
                            inital_value,
                        );
                        control_results.push(res);
                    }
                }
                control_results
            }
        }
        let instruction = LoadMappingSnapshotInstruction {
            // So far this clone is okay because loading a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
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

pub const LOAD_MAPPING_SNAPSHOT_TARGET_TYPE_DEF: TargetTypeDef = TargetTypeDef {
    short_name: "Load mapping snapshot",
    supports_tags: true,
    ..DEFAULT_TARGET_TYPE_DEF
};

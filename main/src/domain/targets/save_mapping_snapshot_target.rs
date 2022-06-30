use crate::domain::{
    Compartment, ControlContext, ExtendedProcessorContext, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, MappingControlContext, MappingControlResult, MappingSnapshot,
    MappingSnapshotId, RealearnTarget, ReaperTarget, ReaperTargetType, TagScope, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Debug)]
pub struct UnresolvedSaveMappingSnapshotTarget {
    /// Mappings which are not in the tag scope don't make it into the snapshot.
    pub scope: TagScope,
    /// Defines whether mappings that are inactive due to conditional activation should make it
    /// into the snapshot or not.
    ///
    /// Mappings which are explicitly disabled for control are ignored anyway because they won't be
    /// loaded with the "Load mapping snapshot" target anyway.
    pub active_mappings_only: bool,
    pub snapshot_id: MappingSnapshotId,
}

impl UnresolvedReaperTargetDef for UnresolvedSaveMappingSnapshotTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::SaveMappingSnapshot(
            SaveMappingSnapshotTarget {
                scope: self.scope.clone(),
                active_mappings_only: self.active_mappings_only,
                snapshot_id: self.snapshot_id.clone(),
            },
        )])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveMappingSnapshotTarget {
    pub scope: TagScope,
    pub active_mappings_only: bool,
    pub snapshot_id: MappingSnapshotId,
}

impl RealearnTarget for SaveMappingSnapshotTarget {
    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::SaveMappingSnapshot)
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
        if !value.is_on() {
            return Ok(None);
        }
        struct SaveMappingSnapshotInstruction {
            scope: TagScope,
            active_mappings_only: bool,
            snapshot_id: MappingSnapshotId,
        }
        impl HitInstruction for SaveMappingSnapshotInstruction {
            fn execute(
                self: Box<Self>,
                context: HitInstructionContext,
            ) -> Vec<MappingControlResult> {
                let target_values = context
                    .mappings
                    .values_mut()
                    .filter_map(|m| {
                        if !m.control_is_enabled() {
                            return None;
                        }
                        if self.scope.has_tags() && !m.has_any_tag(&self.scope.tags) {
                            return None;
                        }
                        if self.active_mappings_only && !m.is_effectively_active() {
                            return None;
                        }
                        let target_value =
                            m.current_aggregated_target_value(context.control_context)?;
                        Some((m.id(), target_value))
                    })
                    .collect();
                let snapshot = MappingSnapshot::new(target_values);
                let mut instance_state = context.control_context.instance_state.borrow_mut();
                let snapshot_container = instance_state.mapping_snapshot_container_mut();
                snapshot_container.update_snapshot(self.snapshot_id.clone(), snapshot);
                vec![]
            }
        }
        let instruction = SaveMappingSnapshotInstruction {
            // So far this clone is okay because saveing a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            active_mappings_only: self.active_mappings_only,
            snapshot_id: self.snapshot_id.clone(),
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

impl<'a> Target<'a> for SaveMappingSnapshotTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const SAVE_MAPPING_SNAPSHOT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Save mapping snapshot",
    short_name: "Save mapping snapshot",
    supports_tags: true,
    ..DEFAULT_TARGET
};

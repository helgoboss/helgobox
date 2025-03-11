use crate::domain::{
    CompartmentKind, ControlContext, ExtendedProcessorContext, HitInstruction,
    HitInstructionContext, HitInstructionResponse, HitResponse, MappingControlContext,
    MappingSnapshot, MappingSnapshotId, RealearnTarget, ReaperTarget, ReaperTargetType, TagScope,
    TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use helgobox_api::persistence::MappingSnapshotDescForTake;

#[derive(Debug)]
pub struct UnresolvedTakeMappingSnapshotTarget {
    pub compartment: CompartmentKind,
    /// Mappings which are not in the tag scope don't make it into the snapshot.
    pub scope: TagScope,
    /// Defines whether mappings that are inactive due to conditional activation should make it
    /// into the snapshot or not.
    ///
    /// Mappings which are explicitly disabled for control are ignored anyway because they won't be
    /// loaded with the "Load mapping snapshot" target anyway.
    pub active_mappings_only: bool,
    pub snapshot_id: VirtualMappingSnapshotIdForTake,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum VirtualMappingSnapshotIdForTake {
    LastLoaded,
    ById(MappingSnapshotId),
}

impl VirtualMappingSnapshotIdForTake {
    pub fn id(&self) -> Option<&MappingSnapshotId> {
        match self {
            VirtualMappingSnapshotIdForTake::LastLoaded => None,
            VirtualMappingSnapshotIdForTake::ById(id) => Some(id),
        }
    }
}

impl TryFrom<MappingSnapshotDescForTake> for VirtualMappingSnapshotIdForTake {
    type Error = &'static str;

    fn try_from(value: MappingSnapshotDescForTake) -> Result<Self, Self::Error> {
        let res = match value {
            MappingSnapshotDescForTake::LastLoaded => Self::LastLoaded,
            MappingSnapshotDescForTake::ById { id } => Self::ById(id.parse()?),
        };
        Ok(res)
    }
}

impl From<VirtualMappingSnapshotIdForTake> for MappingSnapshotDescForTake {
    fn from(value: VirtualMappingSnapshotIdForTake) -> Self {
        match value {
            VirtualMappingSnapshotIdForTake::LastLoaded => Self::LastLoaded,
            VirtualMappingSnapshotIdForTake::ById(s) => Self::ById { id: s.to_string() },
        }
    }
}

impl UnresolvedReaperTargetDef for UnresolvedTakeMappingSnapshotTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::TakeMappingSnapshot(
            TakeMappingSnapshotTarget {
                compartment: self.compartment,
                scope: self.scope.clone(),
                active_mappings_only: self.active_mappings_only,
                snapshot_id: self.snapshot_id.clone(),
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TakeMappingSnapshotTarget {
    pub compartment: CompartmentKind,
    pub scope: TagScope,
    pub active_mappings_only: bool,
    pub snapshot_id: VirtualMappingSnapshotIdForTake,
}

impl RealearnTarget for TakeMappingSnapshotTarget {
    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TakeMappingSnapshot)
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
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        let instruction = TakeMappingSnapshotInstruction {
            compartment: self.compartment,
            // So far this clone is okay because saveing a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            active_mappings_only: self.active_mappings_only,
            snapshot_id: self.snapshot_id.clone(),
        };
        Ok(HitResponse::hit_instruction(Box::new(instruction)))
    }

    fn can_report_current_value(&self) -> bool {
        false
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }
}

impl<'a> Target<'a> for TakeMappingSnapshotTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const SAVE_MAPPING_SNAPSHOT_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::ReaLearn,
    name: "Take mapping snapshot",
    short_name: "Take mapping snapshot",
    supports_tags: true,
    ..DEFAULT_TARGET
};

struct TakeMappingSnapshotInstruction {
    compartment: CompartmentKind,
    scope: TagScope,
    active_mappings_only: bool,
    snapshot_id: VirtualMappingSnapshotIdForTake,
}

impl HitInstruction for TakeMappingSnapshotInstruction {
    fn execute(self: Box<Self>, context: HitInstructionContext) -> HitInstructionResponse {
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
                if self.active_mappings_only && !m.is_active() {
                    return None;
                }
                let target_value = m.current_aggregated_target_value(context.control_context)?;
                Some((m.id(), target_value))
            })
            .collect();
        let snapshot = MappingSnapshot::new(target_values);
        let mut instance_state = context.control_context.unit.borrow_mut();
        let snapshot_container = instance_state.mapping_snapshot_container_mut(self.compartment);
        let resolved_snapshot_id = match self.snapshot_id {
            VirtualMappingSnapshotIdForTake::LastLoaded => {
                match snapshot_container.last_loaded_snapshot_id(&self.scope) {
                    None => return HitInstructionResponse::Ignored,
                    Some(id) => id,
                }
            }
            VirtualMappingSnapshotIdForTake::ById(id) => id,
        };
        snapshot_container.update_snapshot(resolved_snapshot_id, snapshot);
        HitInstructionResponse::CausedEffect(vec![])
    }
}

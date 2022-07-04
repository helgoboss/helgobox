use crate::domain::{
    Compartment, ControlContext, ExtendedProcessorContext, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, MainMapping, MappingControlContext, MappingControlResult,
    MappingSnapshotId, RealearnTarget, ReaperTarget, ReaperTargetType, TagScope, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use realearn_api::persistence::MappingSnapshotDesc;

#[derive(Debug)]
pub struct UnresolvedLoadMappingSnapshotTarget {
    /// Mappings which are in the snapshot but not in the tag scope will be ignored.
    pub scope: TagScope,
    /// If `false`, mappings which are contained in the snapshot but are now inactive
    /// due to conditional activation will be ignored.
    ///
    /// Mappings for which control is disabled are ignored anyway because it would be misleading
    /// to load snapshots for them ... a no is a no. That's consequent and can't lead to
    /// surprises. It's the same for group interaction. If one wants to use load snapshots for some
    /// mappings but don't control them, one can always use the "None" source.
    //
    // TODO-high There's one issue though: At the moment, mappings which are completely disabled
    //  (upper-left checkbox in the row) are also *always* ignored ... same as in group
    //  interaction! But we wanted to consider "not active due to conditional activation" as
    //  equivalent to "not active due to completely disabled". Maybe this goal is nonsense. After
    //  all, one would expect the mapping to be totally disabled when unticking that checkbox,
    //  "more disabled" than just unchecking "Control". For group interaction, it's
    //  consistent ATM because it *always* ignores mappings which are inactive due to conditional
    //  activation. So it ignores completely disabled, control-disabled and
    //  inactive-due-to-conditional-activation mappings.
    //  ... So the question is, which consistency is more important:
    //  a) top-left-checkbox = conditional-activation
    //  b) top-left-checkbox = control-checkbox
    pub active_mappings_only: bool,
    pub snapshot: VirtualMappingSnapshot,
    pub default_value: Option<AbsoluteValue>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum VirtualMappingSnapshot {
    Initial,
    ById(MappingSnapshotId),
}

impl VirtualMappingSnapshot {
    pub fn id(&self) -> Option<&MappingSnapshotId> {
        match self {
            VirtualMappingSnapshot::Initial => None,
            VirtualMappingSnapshot::ById(id) => Some(id),
        }
    }
}

impl TryFrom<MappingSnapshotDesc> for VirtualMappingSnapshot {
    type Error = &'static str;

    fn try_from(value: MappingSnapshotDesc) -> Result<Self, Self::Error> {
        let res = match value {
            MappingSnapshotDesc::Initial => Self::Initial,
            MappingSnapshotDesc::ById { id } => Self::ById(id.parse()?),
        };
        Ok(res)
    }
}

impl From<VirtualMappingSnapshot> for MappingSnapshotDesc {
    fn from(value: VirtualMappingSnapshot) -> Self {
        match value {
            VirtualMappingSnapshot::Initial => Self::Initial,
            VirtualMappingSnapshot::ById(s) => Self::ById { id: s.to_string() },
        }
    }
}

impl UnresolvedReaperTargetDef for UnresolvedLoadMappingSnapshotTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::LoadMappingSnapshot(
            LoadMappingSnapshotTarget {
                scope: self.scope.clone(),
                active_mappings_only: self.active_mappings_only,
                snapshot: self.snapshot.clone(),
                default_value: self.default_value,
            },
        )])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadMappingSnapshotTarget {
    pub scope: TagScope,
    pub active_mappings_only: bool,
    pub snapshot: VirtualMappingSnapshot,
    pub default_value: Option<AbsoluteValue>,
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
        let instruction = LoadMappingSnapshotInstruction {
            // So far this clone is okay because loading a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            active_mappings_only: self.active_mappings_only,
            snapshot: self.snapshot.clone(),
            default_value: self.default_value,
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

pub const LOAD_MAPPING_SNAPSHOT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Load mapping snapshot",
    short_name: "Load mapping snapshot",
    supports_tags: true,
    ..DEFAULT_TARGET
};

struct LoadMappingSnapshotInstruction {
    scope: TagScope,
    active_mappings_only: bool,
    snapshot: VirtualMappingSnapshot,
    default_value: Option<AbsoluteValue>,
}

impl LoadMappingSnapshotInstruction {
    fn load_snapshot(
        &self,
        context: HitInstructionContext,
        get_snapshot_value: impl Fn(&MainMapping) -> Option<AbsoluteValue>,
    ) -> Vec<MappingControlResult> {
        context
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
                let snapshot_value = get_snapshot_value(m).or(self.default_value)?;
                context
                    .domain_event_handler
                    .notify_mapping_matched(m.compartment(), m.id());
                let res = m.control_from_target_directly(
                    context.control_context,
                    context.logger,
                    context.processor_context,
                    snapshot_value,
                    context.basic_settings.target_control_logger(
                        context.processor_context.control_context.instance_state,
                        "mapping snapshot loading",
                        m.qualified_id(),
                    ),
                );
                if res.successful {
                    m.update_last_non_performance_target_value(snapshot_value);
                }
                Some(res)
            })
            .collect()
    }
}

impl HitInstruction for LoadMappingSnapshotInstruction {
    fn execute(self: Box<Self>, context: HitInstructionContext) -> Vec<MappingControlResult> {
        match &self.snapshot {
            VirtualMappingSnapshot::Initial => {
                self.load_snapshot(context, |m| m.initial_target_value())
            }
            VirtualMappingSnapshot::ById(id) => {
                let instance_state = context.control_context.instance_state.borrow();
                let snapshot_container = instance_state.mapping_snapshot_container();
                let snapshot = snapshot_container.find_snapshot_by_id(id);
                self.load_snapshot(context, |m| {
                    snapshot.and_then(|s| s.find_target_value_by_mapping_id(m.id()))
                })
            }
        }
    }
}

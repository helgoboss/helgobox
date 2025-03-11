use crate::domain::{
    CompartmentKind, CompoundChangeEvent, ControlContext, ControlLogContext,
    ExtendedProcessorContext, HitInstruction, HitInstructionContext, HitInstructionResponse,
    HitResponse, MainMapping, MappingControlContext, MappingControlResult, MappingSnapshotId,
    RealearnTarget, ReaperTarget, ReaperTargetType, TagScope, TargetCharacter, TargetSection,
    TargetTypeDef, Unit, UnitEvent, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use helgobox_api::persistence::MappingSnapshotDescForLoad;

#[derive(Debug)]
pub struct UnresolvedLoadMappingSnapshotTarget {
    pub compartment: CompartmentKind,
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
    // TODO-medium There's one issue though: At the moment, mappings which are completely disabled
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
    pub snapshot_id: VirtualMappingSnapshotIdForLoad,
    pub default_value: Option<AbsoluteValue>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum VirtualMappingSnapshotIdForLoad {
    Initial,
    ById(MappingSnapshotId),
}

impl VirtualMappingSnapshotIdForLoad {
    pub fn id(&self) -> Option<&MappingSnapshotId> {
        match self {
            VirtualMappingSnapshotIdForLoad::Initial => None,
            VirtualMappingSnapshotIdForLoad::ById(id) => Some(id),
        }
    }
}

impl TryFrom<MappingSnapshotDescForLoad> for VirtualMappingSnapshotIdForLoad {
    type Error = &'static str;

    fn try_from(value: MappingSnapshotDescForLoad) -> Result<Self, Self::Error> {
        let res = match value {
            MappingSnapshotDescForLoad::Initial => Self::Initial,
            MappingSnapshotDescForLoad::ById { id } => Self::ById(id.parse()?),
        };
        Ok(res)
    }
}

impl From<VirtualMappingSnapshotIdForLoad> for MappingSnapshotDescForLoad {
    fn from(value: VirtualMappingSnapshotIdForLoad) -> Self {
        match value {
            VirtualMappingSnapshotIdForLoad::Initial => Self::Initial,
            VirtualMappingSnapshotIdForLoad::ById(s) => Self::ById { id: s.to_string() },
        }
    }
}

impl UnresolvedReaperTargetDef for UnresolvedLoadMappingSnapshotTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::LoadMappingSnapshot(
            LoadMappingSnapshotTarget {
                compartment: self.compartment,
                scope: self.scope.clone(),
                active_mappings_only: self.active_mappings_only,
                snapshot_id: self.snapshot_id.clone(),
                default_value: self.default_value,
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadMappingSnapshotTarget {
    pub compartment: CompartmentKind,
    pub scope: TagScope,
    pub active_mappings_only: bool,
    pub snapshot_id: VirtualMappingSnapshotIdForLoad,
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
    ) -> Result<HitResponse, &'static str> {
        if value.to_unit_value()?.is_zero() {
            return Ok(HitResponse::ignored());
        }
        let instruction = LoadMappingSnapshotInstruction {
            // So far this clone is okay because loading a snapshot is not something that happens
            // every few milliseconds. No need to use a ref to this target.
            compartment: self.compartment,
            scope: self.scope.clone(),
            active_mappings_only: self.active_mappings_only,
            snapshot: self.snapshot_id.clone(),
            default_value: self.default_value,
        };
        Ok(HitResponse::hit_instruction(Box::new(instruction)))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Unit(UnitEvent::MappingSnapshotActivated {
                compartment,
                tag_scope,
                ..
            }) if *compartment == self.compartment && self.scope.overlaps_with(tag_scope) => {
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for LoadMappingSnapshotTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let instance_state = context.unit.borrow();
        let is_active = instance_state
            .mapping_snapshot_container(self.compartment)
            .snapshot_is_active(&self.scope, &self.snapshot_id);
        Some(AbsoluteValue::from_bool(is_active))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const LOAD_MAPPING_SNAPSHOT_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::ReaLearn,
    name: "Load mapping snapshot",
    short_name: "Load mapping snapshot",
    supports_tags: true,
    ..DEFAULT_TARGET
};

struct LoadMappingSnapshotInstruction {
    compartment: CompartmentKind,
    scope: TagScope,
    active_mappings_only: bool,
    snapshot: VirtualMappingSnapshotIdForLoad,
    default_value: Option<AbsoluteValue>,
}

impl LoadMappingSnapshotInstruction {
    fn load_snapshot(
        &self,
        context: &mut HitInstructionContext,
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
                if self.active_mappings_only && !m.is_active() {
                    return None;
                }
                let snapshot_value = get_snapshot_value(m).or_else(|| {
                    let default_value = self.default_value?;
                    // Sometimes we want to consider 0% as "on" and 100% as "off" when loading the
                    // default value. For example, it's quite common to unmute particular tracks,
                    // essentially activating them. So we have to reverse the "Track: Mute/unmute"
                    // target: It should mute at 0% and unmute at 100%.
                    let effective_value = if m.mode().settings().reverse {
                        default_value.inverse(None)
                    } else {
                        default_value
                    };
                    Some(effective_value)
                })?;
                context
                    .domain_event_handler
                    .notify_mapping_matched(m.compartment(), m.id());
                let res = m.control_from_target_directly(
                    context.control_context,
                    context.processor_context,
                    ControlValue::from_absolute(snapshot_value),
                    context.basic_settings.target_control_logger(
                        context.processor_context.control_context.unit,
                        ControlLogContext::LoadingMappingSnapshot,
                        m.qualified_id(),
                    ),
                );
                if res.at_least_one_target_was_reached {
                    m.update_last_non_performance_target_value(snapshot_value);
                }
                Some(res)
            })
            .collect()
    }

    fn mark_snapshot_as_active(&self, instance_state: &mut Unit) {
        instance_state.mark_snapshot_active(self.compartment, &self.scope, &self.snapshot);
    }
}

impl HitInstruction for LoadMappingSnapshotInstruction {
    fn execute(self: Box<Self>, mut context: HitInstructionContext) -> HitInstructionResponse {
        let results = match &self.snapshot {
            VirtualMappingSnapshotIdForLoad::Initial => {
                self.load_snapshot(&mut context, |m| m.initial_target_value())
            }
            VirtualMappingSnapshotIdForLoad::ById(id) => {
                let instance_state = context.control_context.unit.borrow();
                let snapshot_container =
                    instance_state.mapping_snapshot_container(self.compartment);
                let snapshot = snapshot_container.find_snapshot_by_id(id);
                self.load_snapshot(&mut context, |m| {
                    snapshot.and_then(|s| s.find_target_value_by_mapping_id(m.id()))
                })
            }
        };
        // Mark snapshot as active.
        let mut instance_state = context.control_context.unit.borrow_mut();
        self.mark_snapshot_as_active(&mut instance_state);
        HitInstructionResponse::CausedEffect(results)
    }
}

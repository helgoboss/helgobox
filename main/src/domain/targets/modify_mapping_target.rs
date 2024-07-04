use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_value_as_on_off, AdditionalFeedbackEvent, CompartmentKind, CompoundChangeEvent,
    ControlContext, DomainEvent, DomainEventHandler, ExtendedProcessorContext, HitInstruction,
    HitInstructionContext, HitInstructionResponse, HitResponse, MappingControlContext, MappingId,
    MappingKey, MappingModificationRequestedEvent, QualifiedMappingId, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, Unit, UnitEvent,
    UnitId, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use helgobox_api::persistence::MappingModification;
use std::borrow::Cow;
use std::rc::Rc;

#[derive(Debug)]
pub struct UnresolvedModifyMappingTarget {
    pub compartment: CompartmentKind,
    pub mapping_ref: MappingRef,
    pub modification: MappingModification,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum MappingRef {
    OwnMapping {
        mapping_id: MappingId,
    },
    ForeignMapping {
        // We can't use instance ID here because at the time when an unresolved target is created,
        // the other instance might not exist yet. We need to resolve the other instance at a later
        // point in time. Either at target resolve time (which would be in line with our existing
        // stuff) or at control time (which would be simpler and not a performance issue because
        // learning a mapping is not something done super frequently). Let's do the latter: Lazy
        // resolve at control time. Otherwise we would need to refresh all ReaLearn instances
        // whenever an instance was loaded. No important performance gain in this case,
        // not worth the added complexity.
        session_id: String,
        // For the same reason, we can't use mapping ID here.
        mapping_key: MappingKey,
    },
}

impl UnresolvedReaperTargetDef for UnresolvedModifyMappingTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::ModifyMapping(ModifyMappingTarget {
            compartment: self.compartment,
            modification: self.modification.clone(),
            mapping_ref: self.mapping_ref.clone(),
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModifyMappingTarget {
    /// This must always correspond to the compartment of the containing mapping, otherwise it will
    /// lead to strange behavior.
    pub compartment: CompartmentKind,
    pub modification: MappingModification,
    pub mapping_ref: MappingRef,
}

impl RealearnTarget for ModifyMappingTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        match &self.modification {
            MappingModification::LearnTarget(_) => {
                (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
            }
            MappingModification::SetTargetToLastTouched(_) => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        struct ModifyMappingInstruction {
            compartment: CompartmentKind,
            instance_id: Option<UnitId>,
            mapping_id: MappingId,
            modification: MappingModification,
            value: ControlValue,
        }
        impl HitInstruction for ModifyMappingInstruction {
            fn execute(self: Box<Self>, context: HitInstructionContext) -> HitInstructionResponse {
                let event =
                    DomainEvent::MappingModificationRequested(MappingModificationRequestedEvent {
                        compartment: self.compartment,
                        mapping_id: self.mapping_id,
                        modification: self.modification,
                        value: self.value,
                    });
                if let Some(instance_id) = self.instance_id {
                    if let Some(session) = context
                        .control_context
                        .unit_container
                        .find_session_by_instance_id(instance_id)
                    {
                        let session = Rc::downgrade(&session);
                        session.handle_event_ignoring_error(event)
                    }
                } else {
                    context
                        .domain_event_handler
                        .handle_event_ignoring_error(event)
                };
                HitInstructionResponse::CausedEffect(vec![])
            }
        }
        let (instance_id, mapping_id) = match &self.mapping_ref {
            MappingRef::OwnMapping { mapping_id } => (None, *mapping_id),
            MappingRef::ForeignMapping {
                session_id,
                mapping_key,
            } => {
                let session = context
                    .control_context
                    .unit_container
                    .find_session_by_id(session_id)
                    .ok_or("other ReaLearn unit not found")?;
                let session = session.borrow();
                let mapping_id = session
                    .find_mapping_id_by_key(self.compartment, mapping_key)
                    .ok_or("mapping in other ReaLearn unit not found")?;
                (Some(session.unit_id()), mapping_id)
            }
        };
        let instruction = ModifyMappingInstruction {
            compartment: self.compartment,
            modification: self.modification.clone(),
            instance_id,
            value,
            mapping_id,
        };
        Ok(HitResponse::hit_instruction(Box::new(instruction)))
    }

    fn can_report_current_value(&self) -> bool {
        matches!(&self.modification, MappingModification::LearnTarget(_))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match &self.modification {
            MappingModification::LearnTarget(_) => match evt {
                CompoundChangeEvent::Unit(UnitEvent::MappingWhichLearnsTargetChanged {
                    ..
                }) if matches!(&self.mapping_ref, MappingRef::OwnMapping { .. }) => (true, None),
                CompoundChangeEvent::Additional(AdditionalFeedbackEvent::Unit { .. })
                    if matches!(&self.mapping_ref, MappingRef::ForeignMapping { .. }) =>
                {
                    (true, None)
                }
                _ => (false, None),
            },
            MappingModification::SetTargetToLastTouched(_) => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        match &self.modification {
            MappingModification::LearnTarget(_) => {
                Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
            }
            MappingModification::SetTargetToLastTouched(_) => None,
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ModifyMapping)
    }
}

struct GetArgs<'a> {
    instance_state: &'a Unit,
    id: QualifiedMappingId,
}

impl ModifyMappingTarget {
    fn get_current_value(
        &self,
        context: ControlContext,
        get: impl FnOnce(GetArgs) -> Option<AbsoluteValue>,
    ) -> Option<AbsoluteValue> {
        match &self.mapping_ref {
            MappingRef::OwnMapping { mapping_id } => {
                let args = GetArgs {
                    instance_state: &context.unit.borrow(),
                    id: QualifiedMappingId::new(self.compartment, *mapping_id),
                };
                get(args)
            }
            MappingRef::ForeignMapping {
                session_id,
                mapping_key,
            } => {
                let session = context.unit_container.find_session_by_id(session_id)?;
                let session = session.borrow();
                let mapping_id = session.find_mapping_id_by_key(self.compartment, mapping_key)?;
                let args = GetArgs {
                    instance_state: &session.unit().borrow(),
                    id: QualifiedMappingId::new(self.compartment, mapping_id),
                };
                get(args)
            }
        }
    }
}

impl<'a> Target<'a> for ModifyMappingTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        match &self.modification {
            MappingModification::LearnTarget(_) => self.get_current_value(context, |args| {
                bool_to_current_value(args.instance_state.mapping_is_learning_target(args.id))
            }),
            MappingModification::SetTargetToLastTouched(_) => None,
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

fn bool_to_current_value(on: bool) -> Option<AbsoluteValue> {
    Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(on)))
}

pub const LEARN_MAPPING_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::ReaLearn,
    name: "Modify mapping",
    short_name: "Modify mapping",
    supports_included_targets: true,
    ..DEFAULT_TARGET
};

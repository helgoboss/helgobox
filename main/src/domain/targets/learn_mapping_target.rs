use crate::domain::{
    format_value_as_on_off, Compartment, CompoundChangeEvent, ControlContext, DomainEvent,
    DomainEventHandler, ExtendedProcessorContext, HitInstruction, HitInstructionContext,
    HitInstructionResponse, HitResponse, InstanceId, MappingControlContext, MappingId, MappingKey,
    MappingLearnRequestedEvent, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use realearn_api::persistence::LearnableMappingFeature;
use std::borrow::Cow;
use std::rc::Rc;

#[derive(Debug)]
pub struct UnresolvedLearnMappingTarget {
    pub compartment: Compartment,
    pub feature: LearnableMappingFeature,
    pub mapping_ref: MappingRef,
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

impl UnresolvedReaperTargetDef for UnresolvedLearnMappingTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::LearnMapping(LearnMappingTarget {
            compartment: self.compartment,
            feature: self.feature,
            mapping_ref: self.mapping_ref.clone(),
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnMappingTarget {
    /// This must always correspond to the compartment of the containing mapping, otherwise it will
    /// lead to strange behavior.
    pub compartment: Compartment,
    pub feature: LearnableMappingFeature,
    pub mapping_ref: MappingRef,
}

impl RealearnTarget for LearnMappingTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let on = value.is_on();
        struct LearnMappingInstruction {
            compartment: Compartment,
            feature: LearnableMappingFeature,
            instance_id: Option<InstanceId>,
            mapping_id: MappingId,
            on: bool,
        }
        impl HitInstruction for LearnMappingInstruction {
            fn execute(self: Box<Self>, context: HitInstructionContext) -> HitInstructionResponse {
                let event = DomainEvent::MappingLearnRequested(MappingLearnRequestedEvent {
                    compartment: self.compartment,
                    mapping_id: self.mapping_id,
                    feature: self.feature,
                    on: self.on,
                });
                if let Some(instance_id) = self.instance_id {
                    if let Some(session) = context
                        .control_context
                        .instance_container
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
                    .instance_container
                    .find_session_by_id(session_id)
                    .ok_or("other ReaLearn instance not found")?;
                let session = session.borrow();
                let mapping_id = session
                    .find_mapping_id_by_key(self.compartment, mapping_key)
                    .ok_or("mapping in other ReaLearn instance not found")?;
                (Some(*session.instance_id()), mapping_id)
            }
        };
        let instruction = LearnMappingInstruction {
            compartment: self.compartment,
            feature: self.feature,
            instance_id,
            on,
            mapping_id,
        };
        Ok(HitResponse::hit_instruction(Box::new(instruction)))
    }

    fn is_available(&self, c: ControlContext) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        // TODO-high CONTINUE Introduce a backbone event firing when learn source/target changes
        //  in a ReaLearn instance
        match evt {
            // CompoundChangeEvent::Instance(InstanceStateChanged::ActiveMappingTags {
            //     compartment,
            //     ..
            // }) if *compartment == self.compartment => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::LearnMapping)
    }
}

impl<'a> Target<'a> for LearnMappingTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let uv = if false {
            UnitValue::MAX
        } else {
            UnitValue::MIN
        };
        Some(AbsoluteValue::Continuous(uv))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const LEARN_MAPPING_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Learn mapping",
    short_name: "Learn mapping",
    ..DEFAULT_TARGET
};

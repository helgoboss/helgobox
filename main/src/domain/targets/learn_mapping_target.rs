use crate::domain::{
    format_value_as_on_off, Compartment, CompoundChangeEvent, ControlContext, DomainEvent,
    DomainEventHandler, ExtendedProcessorContext, HitInstruction, HitInstructionContext,
    HitInstructionResponse, HitResponse, InstanceId, MappingControlContext,
    MappingEnabledChangeRequestedEvent, MappingId, MappingLearnRequestedEvent, RealearnTarget,
    ReaperTarget, ReaperTargetType, TagScope, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use realearn_api::persistence::LearnableMappingFeature;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedLearnMappingTarget {
    pub compartment: Compartment,
    pub feature: LearnableMappingFeature,
    pub instance_id: Option<InstanceId>,
    pub mapping_id: MappingId,
}

impl UnresolvedReaperTargetDef for UnresolvedLearnMappingTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::LearnMapping(LearnMappingTarget {
            compartment: self.compartment,
            feature: self.feature,
            instance_id: self.instance_id,
            mapping_id: self.mapping_id,
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnMappingTarget {
    /// This must always correspond to the compartment of the containing mapping, otherwise it will
    /// lead to strange behavior.
    pub compartment: Compartment,
    pub feature: LearnableMappingFeature,
    pub instance_id: Option<InstanceId>,
    pub mapping_id: MappingId,
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
        let instruction = LearnMappingInstruction {
            compartment: self.compartment,
            feature: self.feature,
            instance_id: self.instance_id,
            on,
            mapping_id: self.mapping_id,
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

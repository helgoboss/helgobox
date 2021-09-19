use crate::domain::{
    ControlContext, DomainEvent, Exclusivity, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, InstanceFeedbackEvent, MappingCompartment, MappingControlContext,
    MappingControlResult, MappingData, MappingEnabledChangeRequestedEvent, RealearnTarget,
    TagScope, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct EnableMappingsTarget {
    /// This must always correspond to the compartment of the containing mapping, otherwise it will
    /// lead to strange behavior.
    pub compartment: MappingCompartment,
    pub scope: TagScope,
    pub exclusivity: Exclusivity,
}

impl RealearnTarget for EnableMappingsTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        let is_enable = !value.is_zero();
        struct EnableMappingsInstruction {
            compartment: MappingCompartment,
            scope: TagScope,
            mapping_data: MappingData,
            is_enable: bool,
            exclusivity: Exclusivity,
        }
        impl HitInstruction for EnableMappingsInstruction {
            fn execute(
                self: Box<Self>,
                context: HitInstructionContext,
            ) -> Vec<MappingControlResult> {
                let mut activated_inverse_tags = HashSet::new();
                for m in context.mappings.values_mut() {
                    // Don't touch ourselves.
                    if m.id() == self.mapping_data.mapping_id {
                        continue;
                    }
                    // Determine how to change the mappings.
                    let flag = match self.scope.determine_change(self.exclusivity, m.tags()) {
                        None => continue,
                        Some(f) => f,
                    };
                    if self.exclusivity == Exclusivity::Exclusive && !self.is_enable {
                        // Collect all *other* mapping tags because they are going to be activated
                        // and we have to know about them!
                        activated_inverse_tags.extend(m.tags().iter().cloned());
                    }
                    // Finally request change of mapping enabled state!
                    context.domain_event_handler.handle_event(
                        DomainEvent::MappingEnabledChangeRequested(
                            MappingEnabledChangeRequestedEvent {
                                compartment: m.compartment(),
                                mapping_id: m.id(),
                                is_enabled: if self.is_enable { flag } else { !flag },
                            },
                        ),
                    );
                }
                let mut instance_state = context.control_context.instance_state.borrow_mut();
                if self.exclusivity == Exclusivity::Exclusive {
                    // Completely replace
                    let new_active_tags = if self.is_enable {
                        self.scope.tags.clone()
                    } else {
                        activated_inverse_tags
                    };
                    instance_state.set_active_mapping_tags(self.compartment, new_active_tags);
                } else {
                    // Add or remove
                    instance_state.activate_or_deactivate_mapping_tags(
                        self.compartment,
                        &self.scope.tags,
                        self.is_enable,
                    );
                }
                vec![]
            }
        }
        let instruction = EnableMappingsInstruction {
            compartment: self.compartment,
            // So far this clone is okay because enabling/disable mappings is not something that
            // happens every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            mapping_data: context.mapping_data,
            is_enable,
            exclusivity: self.exclusivity,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            InstanceFeedbackEvent::ActiveMappingTagsChanged { compartment, .. }
                if *compartment == self.compartment =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for EnableMappingsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let active = match self.exclusivity {
            Exclusivity::NonExclusive => instance_state
                .at_least_those_mapping_tags_are_active(self.compartment, &self.scope.tags),
            Exclusivity::Exclusive => instance_state
                .only_these_mapping_tags_are_active(self.compartment, &self.scope.tags),
        };
        let uv = if active {
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

use crate::domain::{
    ControlContext, DomainEvent, Exclusivity, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, MappingControlContext, MappingControlResult, MappingData,
    MappingEnabledChangeRequestedEvent, MappingScope, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};

#[derive(Clone, Debug, PartialEq)]
pub struct EnableMappingsTarget {
    scope: MappingScope,
    // For making basic toggle control possible.
    artificial_value: UnitValue,
    exclusivity: Exclusivity,
}

impl EnableMappingsTarget {
    pub fn new(scope: MappingScope, exclusivity: Exclusivity) -> Self {
        Self {
            scope,
            artificial_value: UnitValue::MAX,
            exclusivity,
        }
    }
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
        self.artificial_value = value;
        let is_enable = !value.is_zero();
        struct EnableMappingInstruction {
            scope: MappingScope,
            mapping_data: MappingData,
            is_enable: bool,
            exclusivity: Exclusivity,
        }
        impl HitInstruction for EnableMappingInstruction {
            fn execute(&self, context: HitInstructionContext) -> Vec<MappingControlResult> {
                for m in context.mappings.values_mut() {
                    if m.id() == self.mapping_data.mapping_id {
                        // We don't want to enable/disable ourselves!
                        continue;
                    }
                    // Don't touch mappings which are not in the universe (e.g. not in the group or
                    // are not active).
                    if !self.scope.universe.matches(m, self.mapping_data.group_id) {
                        continue;
                    }
                    // Now determine how to change the mappings within that universe.
                    let change = if self.exclusivity == Exclusivity::Exclusive {
                        // Change mappings that match the tags and negate the rest.
                        Some(self.scope.matches_tags(m))
                    } else if self.scope.matches_tags(m) {
                        // Change mappings that match the tags.
                        Some(true)
                    } else {
                        // Don't touch mappings that don't match the tags.
                        None
                    };
                    if let Some(change) = change {
                        context.domain_event_handler.handle_event(
                            DomainEvent::MappingEnabledChangeRequested(
                                MappingEnabledChangeRequestedEvent {
                                    compartment: m.compartment(),
                                    mapping_id: m.id(),
                                    is_enabled: if self.is_enable { change } else { !change },
                                },
                            ),
                        );
                    }
                }
                vec![]
            }
        }
        let instruction = EnableMappingInstruction {
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

    fn supports_automatic_feedback(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for EnableMappingsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::Continuous(self.artificial_value))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

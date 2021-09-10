use crate::domain::{
    DomainEvent, FullMappingScope, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, MappingControlContext, MappingData,
    MappingEnabledChangeRequestedEvent, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Clone, Debug, PartialEq)]
pub struct EnableMappingsTarget {
    // TODO-high Add at least an artificial current target value so we can use "Toggle"
    pub scope: FullMappingScope,
}

impl RealearnTarget for EnableMappingsTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
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
        let is_enable = !value.to_unit_value()?.is_zero();
        struct EnableMappingInstruction {
            scope: FullMappingScope,
            mapping_data: MappingData,
            is_enable: bool,
        }
        impl HitInstruction for EnableMappingInstruction {
            fn execute(&self, context: HitInstructionContext) {
                for m in context.mappings.values_mut() {
                    if m.id() == self.mapping_data.mapping_id {
                        // We don't want to disable ourself!
                        continue;
                    }
                    if !self.scope.matches(m, self.mapping_data.group_id) {
                        continue;
                    }
                    context.domain_event_handler.handle_event(
                        DomainEvent::MappingEnabledChangeRequested(
                            MappingEnabledChangeRequestedEvent {
                                compartment: m.compartment(),
                                mapping_id: m.id(),
                                is_enabled: self.is_enable,
                            },
                        ),
                    );
                }
            }
        }
        let instruction = EnableMappingInstruction {
            // So far this clone is okay because enabling/disable mappings is not something that
            // happens every few milliseconds. No need to use a ref to this target.
            scope: self.scope.clone(),
            mapping_data: context.mapping_data,
            is_enable,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn can_report_current_value(&self) -> bool {
        // It would be cool if it could (by investigating if all of the affected mappings are
        // enabled), but for now this is a bit difficult and maybe costly ... let's see if we
        // need this one day.
        false
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl<'a> Target<'a> for EnableMappingsTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

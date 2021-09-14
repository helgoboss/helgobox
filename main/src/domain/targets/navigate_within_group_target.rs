use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value, convert_unit_to_discrete_value,
    ControlContext, Exclusivity, GroupId, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, InstanceFeedbackEvent, MappingCompartment, MappingControlContext,
    MappingControlResult, MappingId, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};

#[derive(Clone, Debug, PartialEq)]
pub struct NavigateWithinGroupTarget {
    pub compartment: MappingCompartment,
    pub group_id: GroupId,
    pub exclusivity: Exclusivity,
}

impl NavigateWithinGroupTarget {
    fn count(&self, context: ControlContext) -> Result<u32, &'static str> {
        let count = context
            .instance_state
            .borrow()
            .get_mappings_within_group(self.group_id)
            .ok_or("group doesn't exist")?
            .len();
        Ok(count as _)
    }
}

impl RealearnTarget for NavigateWithinGroupTarget {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: {
                    let count = self.count(context).unwrap_or(0);
                    convert_count_to_step_size(count as _)
                },
            },
            TargetCharacter::Discrete,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        // TODO-high Exclude control disabled or mapping disabled/inactive mappings
        let value = value.to_absolute_value()?;
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        let desired_mapping_id = {
            let mapping_ids = instance_state
                .get_mappings_within_group(self.group_id)
                .ok_or("group doesn't exist")?;
            let count = mapping_ids.len();
            let desired_index = match value {
                AbsoluteValue::Continuous(v) => convert_unit_to_discrete_value(v, count as _),
                AbsoluteValue::Discrete(f) => f.actual(),
            };
            *mapping_ids
                .get(desired_index as usize)
                .ok_or("mapping index out of bounds")?
        };
        instance_state.set_active_mapping_within_group(self.group_id, desired_mapping_id);
        struct CycleThroughGroupInstruction {
            group_id: GroupId,
            exclusivity: Exclusivity,
            desired_mapping_id: MappingId,
        }
        impl HitInstruction for CycleThroughGroupInstruction {
            fn execute(&self, context: HitInstructionContext) -> Vec<MappingControlResult> {
                let mut control_results = vec![];
                for m in context.mappings.values_mut() {
                    let v = if m.id() == self.desired_mapping_id {
                        m.mode().settings().target_value_interval.max_val()
                    } else if self.exclusivity == Exclusivity::Exclusive
                        && m.group_id() == self.group_id
                    {
                        m.mode().settings().target_value_interval.min_val()
                    } else {
                        continue;
                    };
                    let res = m.control_from_target_directly(
                        context.control_context,
                        context.logger,
                        context.processor_context,
                        AbsoluteValue::Continuous(v),
                    );
                    control_results.push(res);
                }
                control_results
            }
        }
        let instruction = CycleThroughGroupInstruction {
            group_id: self.group_id,
            exclusivity: self.exclusivity,
            desired_mapping_id,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        let count = self.count(context)?;
        Ok(convert_unit_to_discrete_value(input, count))
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let count = self.count(context)?;
        Ok(convert_discrete_to_unit_value(value, count as _))
    }

    fn is_available(&self, context: ControlContext) -> bool {
        context
            .instance_state
            .borrow()
            .get_mappings_within_group(self.group_id)
            .is_some()
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            InstanceFeedbackEvent::ActiveMappingWithinGroupChanged { group_id, .. }
                if *group_id == self.group_id =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for NavigateWithinGroupTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        if let Some(mapping_id) = instance_state.get_active_mapping_within_group(self.group_id) {
            if let Some(mapping_ids) = instance_state.get_mappings_within_group(self.group_id) {
                if mapping_ids.len() > 0 {
                    let max_value = mapping_ids.len() - 1;
                    if let Some(index) = mapping_ids.iter().position(|id| *id == mapping_id) {
                        return Some(AbsoluteValue::Discrete(Fraction::new(
                            index as _,
                            max_value as _,
                        )));
                    }
                }
            }
        }
        Some(AbsoluteValue::Continuous(UnitValue::MIN))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

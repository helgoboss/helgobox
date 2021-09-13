use crate::domain::{
    convert_count_to_step_size, ControlContext, DomainEvent, Exclusivity, GroupId, HitInstruction,
    HitInstructionContext, HitInstructionReturnValue, InstanceFeedbackEvent, MappingCompartment,
    MappingControlContext, MappingControlResult, MappingData, MappingEnabledChangeRequestedEvent,
    MappingId, MappingScope, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};

#[derive(Clone, Debug, PartialEq)]
pub struct NavigateWithinGroupTarget {
    pub compartment: MappingCompartment,
    pub group_id: GroupId,
    pub exclusivity: Exclusivity,
}

impl RealearnTarget for NavigateWithinGroupTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(todo!("Access group map")),
            },
            TargetCharacter::Discrete,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        // TODO-high Set value accordingly
        struct CycleThroughGroupInstruction {
            group_id: GroupId,
            exclusivity: Exclusivity,
        }
        impl HitInstruction for CycleThroughGroupInstruction {
            fn execute(&self, context: HitInstructionContext) -> Vec<MappingControlResult> {
                // TODO-high Implement
                // Get mapping corresponding to the given index (derived from value) within the
                // group and hit its target with target max ... just like autostart!
                // If exclusive, get all other mappings within that group and hit them with target
                // min.
                vec![]
            }
        }
        let instruction = CycleThroughGroupInstruction {
            group_id: self.group_id,
            exclusivity: self.exclusivity,
        };
        Ok(Some(Box::new(instruction)))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        todo!("Access group map")
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
        todo!()
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

/// Returns ID of the mapping in given group at given index.
fn get_mapping_id_at_index_within_group(group_id: GroupId, index: u32) -> Option<MappingId> {
    todo!()
}

/// Checks whether the given group exists.
fn group_exists(group_id: GroupId) -> bool {
    todo!()
}

/// Gets the number of mappings within the given group.
fn get_mapping_count_within_group(group_id: GroupId) -> Option<u32> {
    todo!()
}

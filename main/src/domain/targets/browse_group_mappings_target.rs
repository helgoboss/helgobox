use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value, convert_unit_to_discrete_value,
    Compartment, CompoundChangeEvent, ControlContext, ControlLogContext, ExtendedProcessorContext,
    GroupId, HitInstruction, HitInstructionContext, HitInstructionResponse, HitResponse,
    InstanceStateChanged, MappingControlContext, MappingId, QualifiedMappingId, RealearnTarget,
    ReaperTarget, ReaperTargetType, SimpleExclusivity, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedBrowseGroupTarget {
    pub compartment: Compartment,
    pub group_id: GroupId,
    pub exclusivity: SimpleExclusivity,
}

impl UnresolvedReaperTargetDef for UnresolvedBrowseGroupTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::BrowseGroupMappings(
            BrowseGroupMappingsTarget {
                compartment: self.compartment,
                group_id: self.group_id,
                exclusivity: self.exclusivity,
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowseGroupMappingsTarget {
    /// This must always correspond to the compartment of the containing mapping, otherwise it will
    /// not have any effect when controlling (only when querying the values).
    pub compartment: Compartment,
    pub group_id: GroupId,
    pub exclusivity: SimpleExclusivity,
}

impl BrowseGroupMappingsTarget {
    fn count(&self, context: ControlContext) -> u32 {
        context
            .instance_state
            .borrow()
            .get_on_mappings_within_group(self.compartment, self.group_id)
            .count() as _
    }
}

impl RealearnTarget for BrowseGroupMappingsTarget {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: {
                    let count = self.count(context);
                    convert_count_to_step_size(count)
                },
                is_retriggerable: false,
            },
            TargetCharacter::Discrete,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_absolute_value()?;
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        let desired_mapping_id = {
            let mapping_ids: Vec<_> = instance_state
                .get_on_mappings_within_group(self.compartment, self.group_id)
                .collect();
            let count = mapping_ids.len();
            let desired_index = match value {
                AbsoluteValue::Continuous(v) => convert_unit_to_discrete_value(v, count as _),
                AbsoluteValue::Discrete(f) => f.actual(),
            };
            *mapping_ids
                .get(desired_index as usize)
                .ok_or("mapping index out of bounds")?
        };
        instance_state.set_active_mapping_within_group(
            self.compartment,
            self.group_id,
            desired_mapping_id,
        );
        struct BrowseGroupMappingsInstruction {
            group_id: GroupId,
            exclusivity: SimpleExclusivity,
            desired_mapping_id: MappingId,
        }
        impl HitInstruction for BrowseGroupMappingsInstruction {
            fn execute(self: Box<Self>, context: HitInstructionContext) -> HitInstructionResponse {
                let mut control_results = vec![];
                for m in context.mappings.values_mut() {
                    let glue = m.mode().settings();
                    let v = if m.id() == self.desired_mapping_id {
                        if glue.reverse {
                            glue.target_value_interval.min_val()
                        } else {
                            glue.target_value_interval.max_val()
                        }
                    } else if self.exclusivity == SimpleExclusivity::Exclusive
                        && m.group_id() == self.group_id
                    {
                        if glue.reverse {
                            glue.target_value_interval.max_val()
                        } else {
                            glue.target_value_interval.min_val()
                        }
                    } else {
                        continue;
                    };
                    context
                        .domain_event_handler
                        .notify_mapping_matched(m.compartment(), m.id());
                    let res = m.control_from_target_directly(
                        context.control_context,
                        context.logger,
                        context.processor_context,
                        AbsoluteValue::Continuous(v),
                        context.basic_settings.target_control_logger(
                            context.processor_context.control_context.instance_state,
                            ControlLogContext::GroupNavigation,
                            m.qualified_id(),
                        ),
                    );
                    control_results.push(res);
                }
                HitInstructionResponse::CausedEffect(control_results)
            }
        }
        let instruction = BrowseGroupMappingsInstruction {
            group_id: self.group_id,
            exclusivity: self.exclusivity,
            desired_mapping_id,
        };
        Ok(HitResponse::hit_instruction(Box::new(instruction)))
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
        let count = self.count(context);
        Ok(convert_unit_to_discrete_value(input, count))
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let count = self.count(context);
        Ok(convert_discrete_to_unit_value(value, count as _))
    }

    fn is_available(&self, context: ControlContext) -> bool {
        context
            .instance_state
            .borrow()
            .get_on_mappings_within_group(self.compartment, self.group_id)
            .count()
            > 0
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::ActiveMappingWithinGroup {
                compartment,
                group_id,
                ..
            }) if *compartment == self.compartment && *group_id == self.group_id => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let (mapping_id, _) = self.current_mapping_with_position(context)?;
        let instance_state = context.instance_state.borrow();
        let info = instance_state
            .get_mapping_info(QualifiedMappingId::new(self.compartment, mapping_id))?;
        Some(info.name.clone().into())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let (_, fraction) = self.current_mapping_with_position(context)?;
        Some(NumericValue::Discrete(fraction.actual() as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::BrowseGroup)
    }
}

impl BrowseGroupMappingsTarget {
    fn current_mapping_with_position(
        &self,
        context: ControlContext,
    ) -> Option<(MappingId, Fraction)> {
        let instance_state = context.instance_state.borrow();
        if let Some(mapping_id) =
            instance_state.get_active_mapping_within_group(self.compartment, self.group_id)
        {
            let mapping_ids: Vec<_> = instance_state
                .get_on_mappings_within_group(self.compartment, self.group_id)
                .collect();
            if !mapping_ids.is_empty() {
                let max_value = mapping_ids.len() - 1;
                if let Some(index) = mapping_ids.iter().position(|id| *id == mapping_id) {
                    return Some((mapping_id, Fraction::new(index as _, max_value as _)));
                }
            }
        }
        None
    }
}

impl<'a> Target<'a> for BrowseGroupMappingsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext) -> Option<AbsoluteValue> {
        let fraction = self
            .current_mapping_with_position(context)
            .map(|(_, f)| f)
            .unwrap_or(Fraction::MIN);
        Some(AbsoluteValue::Discrete(fraction))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const BROWSE_GROUP_MAPPINGS_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Browse group mappings",
    short_name: "Browse group mappings",
    supports_exclusivity: true,
    ..DEFAULT_TARGET
};

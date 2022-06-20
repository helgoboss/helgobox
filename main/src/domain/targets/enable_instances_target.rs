use crate::domain::{
    format_value_as_on_off, Compartment, CompoundChangeEvent, ControlContext, EnableInstancesArgs,
    Exclusivity, ExtendedProcessorContext, HitInstructionReturnValue, InstanceStateChanged,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TagScope,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedEnableInstancesTarget {
    pub scope: TagScope,
    pub exclusivity: Exclusivity,
}

impl UnresolvedReaperTargetDef for UnresolvedEnableInstancesTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::EnableInstances(EnableInstancesTarget {
            scope: self.scope.clone(),
            exclusivity: self.exclusivity,
        })])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnableInstancesTarget {
    pub scope: TagScope,
    pub exclusivity: Exclusivity,
}

impl RealearnTarget for EnableInstancesTarget {
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
        let args = EnableInstancesArgs {
            common: context
                .control_context
                .instance_container_common_args(&self.scope),
            is_enable,
            exclusivity: self.exclusivity,
        };
        let tags = context
            .control_context
            .instance_container
            .enable_instances(args);
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        use Exclusivity::*;
        if self.exclusivity == Exclusive || (self.exclusivity == ExclusiveOnOnly && is_enable) {
            // Completely replace
            let new_active_tags = tags.unwrap_or_else(|| self.scope.tags.clone());
            instance_state.set_active_instance_tags(new_active_tags);
        } else {
            // Add or remove
            instance_state.activate_or_deactivate_instance_tags(&self.scope.tags, is_enable);
        }
        Ok(None)
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
            CompoundChangeEvent::Instance(InstanceStateChanged::ActiveInstanceTags) => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::EnableInstances)
    }
}

impl<'a> Target<'a> for EnableInstancesTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        use Exclusivity::*;
        let active = match self.exclusivity {
            NonExclusive => {
                instance_state.at_least_those_instance_tags_are_active(&self.scope.tags)
            }
            Exclusive | ExclusiveOnOnly => {
                instance_state.only_these_instance_tags_are_active(&self.scope.tags)
            }
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

pub const ENABLE_INSTANCES_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Enable/disable instances",
    short_name: "Enable/disable instances",
    supports_tags: true,
    supports_exclusivity: true,
    ..DEFAULT_TARGET
};

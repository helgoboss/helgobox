use crate::domain::{
    CompartmentKind, CompoundChangeEvent, ControlContext, DEFAULT_TARGET, EnableUnitsArgs,
    Exclusivity, ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget,
    ReaperTarget, ReaperTargetType, TagScope, TargetCharacter, TargetSection, TargetTypeDef,
    UnitEvent, UnresolvedReaperTargetDef, format_value_as_on_off,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedEnableUnitsTarget {
    pub scope: TagScope,
    pub exclusivity: Exclusivity,
}

impl UnresolvedReaperTargetDef for UnresolvedEnableUnitsTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = EnableUnitsTarget {
            scope: self.scope.clone(),
            exclusivity: self.exclusivity,
        };
        Ok(vec![ReaperTarget::EnableUnits(target)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnableUnitsTarget {
    pub scope: TagScope,
    pub exclusivity: Exclusivity,
}

impl RealearnTarget for EnableUnitsTarget {
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
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        let is_enable = !value.is_zero();
        let args = EnableUnitsArgs {
            common: context
                .control_context
                .create_modify_unit_container_common_args(&self.scope),
            is_enable,
            exclusivity: self.exclusivity,
        };
        let tags = context.control_context.unit_container.enable_units(args);
        let mut unit = context.control_context.unit.borrow_mut();
        if self.exclusivity == Exclusivity::Exclusive
            || (self.exclusivity == Exclusivity::ExclusiveOnOnly && is_enable)
        {
            // Completely replace
            let new_active_tags = tags.unwrap_or_else(|| self.scope.tags.clone());
            unit.set_active_unit_tags(new_active_tags);
        } else {
            // Add or remove
            unit.activate_or_deactivate_unit_tags(&self.scope.tags, is_enable);
        }
        Ok(HitResponse::processed_with_effect())
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
            CompoundChangeEvent::Unit(UnitEvent::ActiveUnitTags) => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::EnableUnits)
    }
}

impl<'a> Target<'a> for EnableUnitsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let unit_state = context.unit.borrow();
        use Exclusivity::*;
        let active = match self.exclusivity {
            NonExclusive => unit_state.at_least_those_unit_tags_are_active(&self.scope.tags),
            Exclusive | ExclusiveOnOnly => {
                unit_state.only_these_unit_tags_are_active(&self.scope.tags)
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

pub const ENABLE_UNITS_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::ReaLearn,
    name: "Enable/disable units",
    short_name: "Enable/disable units",
    supports_tags: true,
    supports_exclusivity: true,
    ..DEFAULT_TARGET
};

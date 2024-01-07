use crate::domain::{
    format_value_as_on_off, global_automation_mode_override_unit_value, Compartment,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::GlobalAutomationModeOverride;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedAutomationModeOverrideTarget {
    pub mode_override: Option<GlobalAutomationModeOverride>,
}

impl UnresolvedReaperTargetDef for UnresolvedAutomationModeOverrideTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::AutomationModeOverride(
            AutomationModeOverrideTarget {
                mode_override: self.mode_override,
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomationModeOverrideTarget {
    pub mode_override: Option<GlobalAutomationModeOverride>,
}

impl RealearnTarget for AutomationModeOverrideTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // Retriggerable because of #277
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if value.to_unit_value()?.is_zero() {
            Reaper::get().set_global_automation_override(None);
        } else {
            Reaper::get().set_global_automation_override(self.mode_override);
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
            CompoundChangeEvent::Reaper(ChangeEvent::GlobalAutomationOverrideChanged(e)) => (
                true,
                Some(AbsoluteValue::Continuous(
                    global_automation_mode_override_unit_value(self.mode_override, e.new_value),
                )),
            ),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::AutomationModeOverride)
    }
}

impl<'a> Target<'a> for AutomationModeOverrideTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let value = global_automation_mode_override_unit_value(
            self.mode_override,
            Reaper::get().global_automation_override(),
        );
        Some(AbsoluteValue::Continuous(value))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const AUTOMATION_MODE_OVERRIDE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Global,
    name: "Set automation mode override",
    short_name: "Automation override",
    ..DEFAULT_TARGET
};

use crate::domain::{
    automation_mode_unit_value, format_value_as_on_off, ControlContext, HitInstructionReturnValue,
    MappingControlContext, RealearnTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track, TrackRoute};
use reaper_medium::AutomationMode;

#[derive(Clone, Debug, PartialEq)]
pub struct RouteAutomationModeTarget {
    pub route: TrackRoute,
    pub poll_for_feedback: bool,
    pub mode: AutomationMode,
}

impl RealearnTarget for RouteAutomationModeTarget {
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            self.route.set_automation_mode(AutomationMode::TrimRead)
        } else {
            self.route.set_automation_mode(self.mode);
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.route.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.route.track().project())
    }

    fn track(&self) -> Option<&Track> {
        Some(self.route.track())
    }

    fn route(&self) -> Option<&TrackRoute> {
        Some(&self.route)
    }

    fn supports_automatic_feedback(&self) -> bool {
        self.poll_for_feedback
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackSendAutomationMode)
    }
}

impl<'a> Target<'a> for RouteAutomationModeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = automation_mode_unit_value(self.mode, self.route.automation_mode());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ROUTE_AUTOMATION_MODE_TARGET: TargetTypeDef = TargetTypeDef {
    short_name: "Send automation mode",
    hint: AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY,
    supports_poll_for_feedback: true,
    supports_track: true,
    supports_send: true,
    ..DEFAULT_TARGET
};

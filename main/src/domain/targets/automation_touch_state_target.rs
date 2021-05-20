use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, touched_unit_value, AdditionalFeedbackEvent, BackboneState,
    ControlContext, RealearnTarget, TargetCharacter, TouchedParameterType, TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationTouchStateTarget {
    pub track: Track,
    pub parameter_type: TouchedParameterType,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for AutomationTouchStateTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let mut ctx = BackboneState::target_context().borrow_mut();
        if value.as_unit_value()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| {
                ctx.touch_automation_parameter(t.raw(), self.parameter_type)
            });
            ctx.untouch_automation_parameter(self.track.raw(), self.parameter_type);
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| {
                ctx.untouch_automation_parameter(t.raw(), self.parameter_type)
            });
            ctx.touch_automation_parameter(self.track.raw(), self.parameter_type);
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.track.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        Some(self.exclusivity)
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(e)
                if e.track == self.track.raw() && e.parameter_type == self.parameter_type =>
            {
                (true, Some(touched_unit_value(e.new_value)))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for AutomationTouchStateTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        let is_touched = BackboneState::target_context()
            .borrow()
            .automation_parameter_is_touched(self.track.raw(), self.parameter_type);
        Some(touched_unit_value(is_touched))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

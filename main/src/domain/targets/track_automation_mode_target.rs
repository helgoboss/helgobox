use crate::domain::{
    format_value_as_on_off, handle_track_exclusivity, track_automation_mode_unit_value,
    ControlContext, HitInstructionReturnValue, MappingControlContext, RealearnTarget,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::AutomationMode;

#[derive(Clone, Debug, PartialEq)]
pub struct TrackAutomationModeTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub mode: AutomationMode,
}

impl RealearnTarget for TrackAutomationModeTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        // Retriggerable because of #277
        if self.exclusivity == TrackExclusivity::NonExclusive {
            (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Switch,
            )
        } else {
            (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            )
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| {
                t.set_automation_mode(self.mode)
            });
            self.track.set_automation_mode(AutomationMode::TrimRead);
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| {
                t.set_automation_mode(AutomationMode::TrimRead)
            });
            self.track.set_automation_mode(self.mode);
        }
        Ok(None)
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

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::TrackAutomationModeChanged(e) if e.track == self.track => (
                true,
                Some(AbsoluteValue::Continuous(track_automation_mode_unit_value(
                    self.mode,
                    e.new_value,
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackAutomationModeTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = track_automation_mode_unit_value(self.mode, self.track.automation_mode());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

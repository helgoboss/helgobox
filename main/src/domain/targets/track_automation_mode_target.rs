use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, mute_unit_value, track_arm_unit_value,
    track_automation_mode_unit_value, track_solo_unit_value, ControlContext, RealearnTarget,
    SoloBehavior, TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::{AutomationMode, SoloMode};

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

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_absolute()?.is_zero() {
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

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::TrackAutomationModeChanged(e) if e.track == self.track => (
                true,
                Some(track_automation_mode_unit_value(self.mode, e.new_value)),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackAutomationModeTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(track_automation_mode_unit_value(
            self.mode,
            self.track.automation_mode(),
        ))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

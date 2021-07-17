use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, track_arm_unit_value, ControlContext, RealearnTarget,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackArmTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackArmTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.to_unit_value()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.arm(false));
            self.track.disarm(false);
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.disarm(false));
            self.track.arm(false);
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
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::TrackArmChanged(e) if e.track == self.track => (
                true,
                Some(AbsoluteValue::Continuous(track_arm_unit_value(e.new_value))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackArmTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = track_arm_unit_value(self.track.is_armed(false));
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

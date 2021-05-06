use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, mute_unit_value, track_arm_unit_value, ControlContext,
    RealearnTarget, TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackMuteTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackMuteTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_absolute()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.mute());
            self.track.unmute();
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.unmute());
            self.track.mute();
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
            ChangeEvent::TrackMuteChanged(e) if e.track == self.track => {
                (true, Some(mute_unit_value(e.new_value)))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackMuteTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(mute_unit_value(self.track.is_muted()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

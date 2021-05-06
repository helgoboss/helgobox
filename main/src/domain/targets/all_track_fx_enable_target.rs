use crate::domain::{
    all_track_fx_enable_unit_value, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, handle_track_exclusivity,
    mute_unit_value, track_arm_unit_value, ControlContext, RealearnTarget, TargetCharacter,
    TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct AllTrackFxEnableTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for AllTrackFxEnableTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_absolute()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.enable_fx());
            self.track.disable_fx();
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.disable_fx());
            self.track.enable_fx();
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

    fn supports_automatic_feedback(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for AllTrackFxEnableTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(all_track_fx_enable_unit_value(self.track.fx_is_enabled()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

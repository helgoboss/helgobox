use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, track_solo_unit_value, ControlContext, RealearnTarget, SoloBehavior,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::SoloMode;

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSoloTarget {
    pub track: Track,
    pub behavior: SoloBehavior,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackSoloTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let solo_track = |t: &Track| {
            use SoloBehavior::*;
            match self.behavior {
                InPlace => t.set_solo_mode(SoloMode::SoloInPlace),
                IgnoreRouting => t.set_solo_mode(SoloMode::SoloIgnoreRouting),
                ReaperPreference => t.solo(),
            }
        };
        if value.as_unit_value()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, solo_track);
            self.track.unsolo();
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.unsolo());
            solo_track(&self.track);
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
            ChangeEvent::TrackSoloChanged(e) if e.track == self.track => {
                (true, Some(track_solo_unit_value(e.new_value)))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackSoloTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(track_solo_unit_value(self.track.is_solo()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

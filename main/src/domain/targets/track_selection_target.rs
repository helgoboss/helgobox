use crate::domain::ui_util::{
    format_as_percentage_without_unit, format_value_as_db, format_value_as_db_without_unit,
    parse_unit_value_from_percentage, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, track_selected_unit_value, ControlContext, RealearnTarget,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Reaper, Track, Volume};
use reaper_medium::CommandId;

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSelectionTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
}

impl RealearnTarget for TrackSelectionTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_absolute()?.is_zero() {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.select());
            self.track.unselect();
        } else if self.exclusivity == TrackExclusivity::ExclusiveAll {
            // We have a dedicated REAPER function to select the track exclusively.
            self.track.select_exclusively();
        } else {
            handle_track_exclusivity(&self.track, self.exclusivity, |t| t.unselect());
            self.track.select();
        }
        if self.scroll_arrange_view {
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(self.track.project()));
        }
        if self.scroll_mixer {
            self.track.scroll_mixer();
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
            ChangeEvent::TrackSelectedChanged(e) if e.track == self.track => {
                (true, Some(track_selected_unit_value(e.new_value)))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackSelectionTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(track_selected_unit_value(self.track.is_selected()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

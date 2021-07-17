use crate::domain::{
    format_value_as_on_off, get_control_type_and_character_for_track_exclusivity,
    handle_track_exclusivity, track_selected_unit_value, ControlContext, RealearnTarget,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Reaper, Track};
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

    fn control(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.to_unit_value()?.is_zero() {
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
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::TrackSelectedChanged(e) if e.track == self.track => (
                true,
                Some(AbsoluteValue::Continuous(track_selected_unit_value(
                    e.new_value,
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackSelectionTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = track_selected_unit_value(self.track.is_selected());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

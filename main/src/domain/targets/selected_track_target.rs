use crate::domain::{
    convert_count_to_step_size, convert_unit_value_to_track_index, selected_track_unit_value,
    ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Reaper};
use reaper_medium::{CommandId, MasterTrackBehavior};

#[derive(Clone, Debug, PartialEq)]
pub struct SelectedTrackTarget {
    pub project: Project,
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
}

impl RealearnTarget for SelectedTrackTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<Master track>" is also a possible value.
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(self.project.track_count() + 1),
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        let value = convert_unit_value_to_track_index(self.project, input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue) -> String {
        match convert_unit_value_to_track_index(self.project, value) {
            None => "<Master track>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let track_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => convert_unit_value_to_track_index(self.project, v),
            AbsoluteValue::Discrete(f) => {
                if f.actual() == 0 {
                    None
                } else {
                    Some(f.actual() - 1)
                }
            }
        };
        let track = match track_index {
            None => self.project.master_track(),
            Some(i) => self
                .project
                .track_by_index(i)
                .ok_or("track not available")?,
        };
        track.select_exclusively();
        if self.scroll_arrange_view {
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()));
        }
        if self.scroll_mixer {
            track.scroll_mixer();
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::TrackSelectedChanged(e)
                if e.new_value && e.track.project() == self.project =>
            {
                (
                    true,
                    Some(selected_track_unit_value(self.project, e.track.index())),
                )
            }
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        Ok(selected_track_unit_value(self.project, index))
    }
}

impl<'a> Target<'a> for SelectedTrackTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let track_count = self.project.track_count();
        // Because we count "<Master track>" as a possible value, this is equal.
        let max_value = track_count;
        let track_index = self
            .project
            .first_selected_track(MasterTrackBehavior::ExcludeMasterTrack)
            .and_then(|t| t.index());
        let actual_value = track_index.map(|i| i + 1).unwrap_or(0);
        Some(AbsoluteValue::Discrete(Fraction::new(
            actual_value,
            max_value,
        )))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

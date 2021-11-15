use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, track_selected_unit_value,
    CompoundChangeEvent, ControlContext, HitInstructionReturnValue, MappingControlContext,
    RealearnTarget, ReaperTargetType, TargetCharacter, TrackExclusivity,
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
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        use TrackExclusivity::*;
        let select_exclusively_within_project = !value.is_zero()
            && matches!(
                self.exclusivity,
                ExclusiveWithinProject | ExclusiveWithinProjectOnOnly
            );
        if select_exclusively_within_project {
            // We have a dedicated REAPER function to select the track exclusively.
            self.track.select_exclusively();
        } else {
            change_track_prop(
                &self.track,
                self.exclusivity,
                value,
                |t| t.select(),
                |t| t.unselect(),
            );
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
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
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
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackSelectedChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(track_selected_unit_value(
                        e.new_value,
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackSelection)
    }
}

impl<'a> Target<'a> for TrackSelectionTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = track_selected_unit_value(self.track.is_selected());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

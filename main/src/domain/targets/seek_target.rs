use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    current_value_of_seek, get_seek_info, ActionInvocationType, AdditionalFeedbackEvent,
    ControlContext, RealearnTarget, SeekOptions, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Action, ActionCharacter, Project, Reaper};
use reaper_medium::{CommandId, PositionInSeconds, SetEditCurPosOptions};

#[derive(Clone, Debug, PartialEq)]
pub struct SeekTarget {
    pub project: Project,
    pub options: SeekOptions,
}

impl RealearnTarget for SeekTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        // TODO-low "Seek" could support rounding/discrete (beats, measures, seconds, ...)
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let value = value.as_absolute()?;
        let info = get_seek_info(self.project, self.options);
        let desired_pos_within_range = value.get() * info.length();
        let desired_pos = info.start_pos.get() + desired_pos_within_range;
        self.project.set_edit_cursor_position(
            PositionInSeconds::new(desired_pos),
            SetEditCurPosOptions {
                move_view: self.options.move_view,
                seek_play: self.options.seek_play,
            },
        );
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            AdditionalFeedbackEvent::BeatChanged(e) if e.project == self.project => {
                let v = current_value_of_seek(self.project, self.options, e.new_value);
                (true, Some(v))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for SeekTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        let val = current_value_of_seek(
            self.project,
            self.options,
            self.project.play_or_edit_cursor_position(),
        );
        Some(val)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

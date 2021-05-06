use crate::domain::{
    format_step_size_as_playback_speed_factor_without_unit,
    format_value_as_playback_speed_factor_without_unit, parse_step_size_from_playback_speed_factor,
    parse_value_from_playback_speed_factor, playback_speed_factor_span, playrate_unit_value,
    ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, PlayRate, Project};
use reaper_medium::NormalizedPlayRate;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayrateTarget {
    pub project: Project,
}

impl RealearnTarget for PlayrateTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / (playback_speed_factor_span() * 100.0)),
            },
            TargetCharacter::Continuous,
        )
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_playback_speed_factor(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_step_size_from_playback_speed_factor(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_playback_speed_factor_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        format_step_size_as_playback_speed_factor_without_unit(step_size)
    }

    fn hide_formatted_value(&self) -> bool {
        true
    }

    fn hide_formatted_step_size(&self) -> bool {
        true
    }

    fn value_unit(&self) -> &'static str {
        "x"
    }

    fn step_size_unit(&self) -> &'static str {
        "x"
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let play_rate =
            PlayRate::from_normalized_value(NormalizedPlayRate::new(value.as_absolute()?.get()));
        self.project.set_play_rate(play_rate);
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
            ChangeEvent::MasterPlayrateChanged(e) if e.project == self.project => (
                true,
                Some(playrate_unit_value(PlayRate::from_playback_speed_factor(
                    e.new_value,
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for PlayrateTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(playrate_unit_value(self.project.play_rate()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

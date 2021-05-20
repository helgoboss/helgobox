use crate::domain::ui_util::{
    format_as_double_percentage_without_unit, format_as_symmetric_percentage_without_unit,
    parse_from_double_percentage, parse_from_symmetric_percentage,
};
use crate::domain::{width_unit_value, ControlContext, PanExt, RealearnTarget, TargetCharacter};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{AvailablePanValue, ChangeEvent, Project, Track, Width};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackWidthTarget {
    pub track: Track,
}

impl RealearnTarget for TrackWidthTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_from_symmetric_percentage(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_from_double_percentage(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_as_symmetric_percentage_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        format_as_double_percentage_without_unit(step_size)
    }

    fn is_available(&self) -> bool {
        self.track.is_available()
    }

    fn hide_formatted_value(&self) -> bool {
        true
    }

    fn hide_formatted_step_size(&self) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let width = Width::from_normalized_value(value.as_unit_value()?.get());
        self.track.set_width(width);
        Ok(())
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::TrackPanChanged(e) if e.track == self.track => (
                true,
                match e.new_value {
                    AvailablePanValue::Complete(v) => v
                        .width()
                        .map(|width| width_unit_value(Width::from_reaper_value(width))),
                    AvailablePanValue::Incomplete(_) => None,
                },
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackWidthTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(width_unit_value(self.track.width()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

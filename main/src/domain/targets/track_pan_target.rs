use crate::domain::ui_util::{
    format_as_percentage_without_unit, format_value_as_db, format_value_as_db_without_unit,
    parse_unit_value_from_percentage, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    format_value_as_pan, pan_unit_value, parse_value_from_pan, ControlContext, PanExt,
    RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{AvailablePanValue, ChangeEvent, Pan, Project, Track, Volume};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackPanTarget {
    pub track: Track,
}

impl RealearnTarget for TrackPanTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_pan(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_pan(value)
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

    fn value_unit(&self) -> &'static str {
        ""
    }

    fn step_size_unit(&self) -> &'static str {
        ""
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_pan(value)
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let pan = Pan::from_normalized_value(value.as_absolute()?.get());
        self.track.set_pan(pan);
        Ok(())
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::TrackPanChanged(e) if e.track == self.track => (true, {
                let pan = match e.new_value {
                    AvailablePanValue::Complete(v) => v.main_pan(),
                    AvailablePanValue::Incomplete(pan) => pan,
                };
                Some(pan_unit_value(Pan::from_reaper_value(pan)))
            }),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackPanTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(pan_unit_value(self.track.pan()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

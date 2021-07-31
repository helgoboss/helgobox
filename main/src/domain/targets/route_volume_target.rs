use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db, volume_unit_value,
};
use crate::domain::{ControlContext, RealearnTarget, TargetCharacter};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track, TrackRoute, Volume};

#[derive(Clone, Debug, PartialEq)]
pub struct RouteVolumeTarget {
    pub route: TrackRoute,
}

impl RealearnTarget for RouteVolumeTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_db_without_unit(value)
    }

    fn hide_formatted_value(&self) -> bool {
        true
    }

    fn hide_formatted_step_size(&self) -> bool {
        true
    }

    fn value_unit(&self) -> &'static str {
        "dB"
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_db(value)
    }

    fn hit(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get());
        self.route
            .set_volume(volume.unwrap_or(Volume::MIN))
            .map_err(|_| "couldn't set route volume")?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.route.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.route.track().project())
    }

    fn track(&self) -> Option<&Track> {
        Some(self.route.track())
    }

    fn route(&self) -> Option<&TrackRoute> {
        Some(&self.route)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::TrackRouteVolumeChanged(e) if e.route == self.route => (
                true,
                Some(AbsoluteValue::Continuous(volume_unit_value(
                    Volume::from_reaper_value(e.new_value),
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for RouteVolumeTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = volume_unit_value(self.route.volume());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    ControlContext, HitInstructionReturnValue, MappingControlContext, RealearnTarget,
    TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track, TrackRoute, Volume};

#[derive(Clone, Debug, PartialEq)]
pub struct RouteVolumeTarget {
    pub route: TrackRoute,
}

impl RealearnTarget for RouteVolumeTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db_without_unit(value)
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "dB"
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get());
        self.route
            .set_volume(volume.unwrap_or(Volume::MIN))
            .map_err(|_| "couldn't set route volume")?;
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
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

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(self.volume().to_string())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.volume().db().get()))
    }
}

impl RouteVolumeTarget {
    fn volume(&self) -> Volume {
        self.route.volume()
    }
}

impl<'a> Target<'a> for RouteVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = volume_unit_value(self.volume());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

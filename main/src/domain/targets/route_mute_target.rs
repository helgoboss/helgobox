use crate::domain::{
    format_value_as_on_off, mute_unit_value, ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track, TrackRoute};

#[derive(Clone, Debug, PartialEq)]
pub struct RouteMuteTarget {
    pub route: TrackRoute,
}

impl RealearnTarget for RouteMuteTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_absolute()?.is_zero() {
            self.route.unmute();
        } else {
            self.route.mute();
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.route.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.route.track().project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.route.track())
    }

    fn route(&self) -> Option<&TrackRoute> {
        Some(&self.route)
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for RouteMuteTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        Some(mute_unit_value(self.route.is_muted()))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

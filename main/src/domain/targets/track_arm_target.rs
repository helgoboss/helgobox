use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, track_arm_unit_value, ControlContext,
    HitInstructionReturnValue, MappingControlContext, RealearnTarget, TargetCharacter,
    TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackArmTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackArmTarget {
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
        change_track_prop(
            &self.track,
            self.exclusivity,
            value.to_unit_value()?,
            |t| t.arm(false),
            |t| t.disarm(false),
        );
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
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::TrackArmChanged(e) if e.track == self.track => (
                true,
                Some(AbsoluteValue::Continuous(track_arm_unit_value(e.new_value))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackArmTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = track_arm_unit_value(self.track.is_armed(false));
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

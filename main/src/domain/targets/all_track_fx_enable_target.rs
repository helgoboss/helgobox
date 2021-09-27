use crate::domain::{
    all_track_fx_enable_unit_value, change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, ControlContext,
    HitInstructionReturnValue, MappingControlContext, RealearnTarget, TargetCharacter,
    TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct AllTrackFxEnableTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub poll_for_feedback: bool,
}

impl RealearnTarget for AllTrackFxEnableTarget {
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
            |t| t.enable_fx(),
            |t| t.disable_fx(),
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

    fn supports_automatic_feedback(&self) -> bool {
        self.poll_for_feedback
    }
}

impl<'a> Target<'a> for AllTrackFxEnableTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = all_track_fx_enable_unit_value(self.track.fx_is_enabled());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

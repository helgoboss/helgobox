use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, track_solo_unit_value, ControlContext,
    HitInstructionReturnValue, MappingControlContext, RealearnTarget, SoloBehavior,
    TargetCharacter, TrackExclusivity,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::SoloMode;

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSoloTarget {
    pub track: Track,
    pub behavior: SoloBehavior,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackSoloTarget {
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
        let solo_track = |t: &Track| {
            use SoloBehavior::*;
            match self.behavior {
                InPlace => t.set_solo_mode(SoloMode::SoloInPlace),
                IgnoreRouting => t.set_solo_mode(SoloMode::SoloIgnoreRouting),
                ReaperPreference => t.solo(),
            }
        };
        change_track_prop(
            &self.track,
            self.exclusivity,
            value.to_unit_value()?,
            |t| solo_track(t),
            |t| t.unsolo(),
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
            ChangeEvent::TrackSoloChanged(e) if e.track == self.track => (
                true,
                Some(AbsoluteValue::Continuous(track_solo_unit_value(
                    e.new_value,
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TrackSoloTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = track_solo_unit_value(self.track.is_solo());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

use crate::domain::{
    get_track_name, percentage_for_track_within_project, ControlContext, RealearnTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, NumericValue, Target};
use reaper_high::{Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackToolTarget {
    pub track: Track,
}

impl RealearnTarget for TrackToolTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
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

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(get_track_name(&self.track))
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let position = match self.track.index() {
            None => 0,
            Some(i) => i + 1,
        };
        Some(NumericValue::Discrete(position as _))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackTool)
    }
}

impl<'a> Target<'a> for TrackToolTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let track_index = self.track.index();
        Some(percentage_for_track_within_project(
            self.track.project(),
            track_index,
        ))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_TOOL_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track",
    short_name: "Track",
    supports_track: true,
    ..DEFAULT_TARGET
};

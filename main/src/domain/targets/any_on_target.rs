use crate::domain::{
    format_value_as_on_off, ControlContext, HitInstructionReturnValue, MappingControlContext,
    RealearnTarget, ReaperTargetType, TargetCharacter,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{ChangeEvent, Project};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct AnyOnTarget {
    pub project: Project,
    pub parameter: AnyOnParameter,
}

impl RealearnTarget for AnyOnTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // Retriggerable because the logic of this target is unusual: Pressing a button (= receiving
        // on = 100%) is supposed to switch everything to *off*. So the desired target value doesn't
        // correspond to the incoming value.
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.is_on() {
            for t in self.project.tracks() {
                use AnyOnParameter::*;
                match self.parameter {
                    TrackSolo => t.unsolo(),
                    TrackMute => t.unmute(),
                    TrackArm => t.disarm(false),
                    TrackSelection => t.unselect(),
                }
            }
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use AnyOnParameter::*;
        match evt {
            ChangeEvent::TrackSoloChanged(e)
                if self.parameter == TrackSolo && e.track.project() == self.project =>
            {
                (true, None)
            }
            ChangeEvent::TrackMuteChanged(e)
                if self.parameter == TrackMute && e.track.project() == self.project =>
            {
                (true, None)
            }
            ChangeEvent::TrackArmChanged(e)
                if self.parameter == TrackArm && e.track.project() == self.project =>
            {
                (true, None)
            }
            ChangeEvent::TrackSelectedChanged(e)
                if self.parameter == TrackSelection && e.track.project() == self.project =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::AnyOn)
    }
}

impl<'a> Target<'a> for AnyOnTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        use AnyOnParameter::*;
        let on = match self.parameter {
            TrackSolo => self.project.any_solo(),
            TrackMute => self.project.tracks().any(|t| t.is_muted()),
            TrackArm => self.project.tracks().any(|t| t.is_armed(false)),
            TrackSelection => self.project.tracks().any(|t| t.is_selected()),
        };
        Some(AbsoluteValue::from_bool(on))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum AnyOnParameter {
    #[serde(rename = "track-solo")]
    #[display(fmt = "Track solo")]
    TrackSolo,
    #[serde(rename = "track-mute")]
    #[display(fmt = "Track mute")]
    TrackMute,
    #[serde(rename = "track-arm")]
    #[display(fmt = "Track arm")]
    TrackArm,
    #[serde(rename = "track-selection")]
    #[display(fmt = "Track selection")]
    TrackSelection,
}

impl Default for AnyOnParameter {
    fn default() -> Self {
        Self::TrackSolo
    }
}

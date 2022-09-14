use crate::domain::{
    format_value_as_on_off, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{ChangeEvent, Project};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedAnyOnTarget {
    pub parameter: AnyOnParameter,
}

impl UnresolvedReaperTargetDef for UnresolvedAnyOnTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::AnyOn(AnyOnTarget {
            project: context.context().project_or_current_project(),
            parameter: self.parameter,
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        for t in self.project.tracks() {
            use AnyOnParameter::*;
            match self.parameter {
                TrackSolo => t.unsolo(),
                TrackMute => t.unmute(),
                TrackArm => t.disarm(false),
                TrackSelection => t.unselect(),
            }
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use AnyOnParameter::*;
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::TrackSoloChanged(e))
                if self.parameter == TrackSolo && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackMuteChanged(e))
                if self.parameter == TrackMute && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackArmChanged(e))
                if self.parameter == TrackArm && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackSelectedChanged(e))
                if self.parameter == TrackSelection && e.track.project() == self.project =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
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
#[allow(clippy::enum_variant_names)]
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

pub const ANY_ON_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Project: Any on (solo/mute/...)",
    short_name: "Any on",
    ..DEFAULT_TARGET
};

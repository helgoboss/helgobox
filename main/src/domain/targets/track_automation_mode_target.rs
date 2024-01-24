use crate::domain::{
    automation_mode_unit_value, change_track_prop, format_value_as_on_off, get_effective_tracks,
    CompartmentKind, CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, TrackDescriptor, TrackExclusivity, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::AutomationMode;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackAutomationModeTarget {
    pub track_descriptor: TrackDescriptor,
    pub exclusivity: TrackExclusivity,
    pub mode: AutomationMode,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackAutomationModeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackAutomationMode(TrackAutomationModeTarget {
                        track,
                        exclusivity: self.exclusivity,
                        mode: self.mode,
                    })
                })
                .collect(),
        )
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackAutomationModeTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub mode: AutomationMode,
}

impl RealearnTarget for TrackAutomationModeTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // Retriggerable because of #277
        if self.exclusivity == TrackExclusivity::NonExclusive {
            (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Switch,
            )
        } else {
            (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            )
        }
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        change_track_prop(
            &self.track,
            self.exclusivity,
            value.to_unit_value()?,
            |t| t.set_automation_mode(self.mode),
            |t| t.set_automation_mode(AutomationMode::TrimRead),
        );
        Ok(HitResponse::processed_with_effect())
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
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackAutomationModeChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(automation_mode_unit_value(
                        self.mode,
                        e.new_value,
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackAutomationMode)
    }
}

impl<'a> Target<'a> for TrackAutomationModeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = automation_mode_unit_value(self.mode, self.track.automation_mode());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_AUTOMATION_MODE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Set automation mode",
    short_name: "Track automation mode",
    supports_track: true,
    supports_track_exclusivity: true,
    ..DEFAULT_TARGET
};

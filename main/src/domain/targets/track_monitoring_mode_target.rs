use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    change_track_prop, format_value_as_on_off, get_effective_tracks, with_gang_behavior,
    CompartmentKind, CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, TrackDescriptor, TrackExclusivity, TrackGangBehavior,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::InputMonitoringMode;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackMonitoringModeTarget {
    pub track_descriptor: TrackDescriptor,
    pub exclusivity: TrackExclusivity,
    pub mode: InputMonitoringMode,
    pub gang_behavior: TrackGangBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackMonitoringModeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackMonitoringMode(TrackMonitoringModeTarget {
                        track,
                        exclusivity: self.exclusivity,
                        mode: self.mode,
                        gang_behavior: self.gang_behavior,
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
pub struct TrackMonitoringModeTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub mode: InputMonitoringMode,
    pub gang_behavior: TrackGangBehavior,
}

impl RealearnTarget for TrackMonitoringModeTarget {
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
        let value = value.to_unit_value()?;
        with_gang_behavior(
            self.track.project(),
            self.gang_behavior,
            &TRACK_MONITORING_MODE_TARGET,
            |gang_behavior, grouping_behavior| {
                change_track_prop(
                    &self.track,
                    self.exclusivity,
                    value,
                    |t| t.set_input_monitoring_mode(self.mode, gang_behavior, grouping_behavior),
                    |t| {
                        t.set_input_monitoring_mode(
                            InputMonitoringMode::Off,
                            gang_behavior,
                            grouping_behavior,
                        )
                    },
                );
                Ok(())
            },
        )?;
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
            CompoundChangeEvent::Reaper(ChangeEvent::TrackInputMonitoringChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(monitoring_mode_unit_value(
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
        Some(ReaperTargetType::TrackMonitoringMode)
    }
}

impl<'a> Target<'a> for TrackMonitoringModeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = monitoring_mode_unit_value(self.mode, self.track.input_monitoring_mode());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_MONITORING_MODE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Set monitoring mode",
    short_name: "Track monitoring mode",
    hint: "grouping support since REAPER v6.70",
    supports_track: true,
    supports_track_exclusivity: true,
    supports_gang_selected: true,
    supports_gang_grouping: true,
    supports_track_grouping_only_gang_behavior: true,
    ..DEFAULT_TARGET
};

pub fn monitoring_mode_unit_value(
    desired_mode: InputMonitoringMode,
    actual_mode: InputMonitoringMode,
) -> UnitValue {
    let is_on = desired_mode == actual_mode;
    convert_bool_to_unit_value(is_on)
}

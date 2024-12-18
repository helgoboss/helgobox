use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, get_effective_tracks, mute_unit_value,
    with_gang_behavior, CompartmentKind, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, TrackDescriptor,
    TrackExclusivity, TrackGangBehavior, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackMuteTarget {
    pub track_descriptor: TrackDescriptor,
    pub exclusivity: TrackExclusivity,
    pub gang_behavior: TrackGangBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackMuteTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackMute(TrackMuteTarget {
                        track,
                        exclusivity: self.exclusivity,
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
pub struct TrackMuteTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub gang_behavior: TrackGangBehavior,
}

impl RealearnTarget for TrackMuteTarget {
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
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        with_gang_behavior(
            self.track.project(),
            self.gang_behavior,
            &TRACK_MUTE_TARGET,
            |gang_behavior, grouping_behavior| {
                change_track_prop(
                    &self.track,
                    self.exclusivity,
                    value,
                    |t| t.mute(gang_behavior, grouping_behavior),
                    |t| t.unmute(gang_behavior, grouping_behavior),
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
            CompoundChangeEvent::Reaper(ChangeEvent::TrackMuteChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(mute_unit_value(e.new_value))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackMute)
    }
}

impl<'a> Target<'a> for TrackMuteTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = mute_unit_value(self.track.is_muted());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_MUTE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Mute/unmute",
    short_name: "(Un)mute track",
    supports_track: true,
    supports_track_exclusivity: true,
    supports_gang_selected: true,
    supports_gang_grouping: true,
    supports_track_grouping_only_gang_behavior: true,
    ..DEFAULT_TARGET
};

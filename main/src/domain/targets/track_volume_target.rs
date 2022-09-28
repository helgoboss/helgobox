use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    get_effective_tracks, with_gang_behavior, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, TrackDescriptor, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use realearn_api::persistence::TrackGangBehavior;
use reaper_high::{ChangeEvent, Project, Track, Volume};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackVolumeTarget {
    pub track_descriptor: TrackDescriptor,
    pub gang_behavior: TrackGangBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackVolumeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackVolume(TrackVolumeTarget {
                        track,
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
pub struct TrackVolumeTarget {
    pub track: Track,
    pub gang_behavior: TrackGangBehavior,
}

impl RealearnTarget for TrackVolumeTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db_without_unit(value)
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "dB"
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get());
        with_gang_behavior(
            self.track.project(),
            self.gang_behavior,
            false,
            |gang_behavior| {
                self.track
                    .set_volume(volume.unwrap_or(Volume::MIN), gang_behavior);
            },
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

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackVolumeChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(volume_unit_value(
                        Volume::from_reaper_value(e.new_value),
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(self.volume().to_string().into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.volume().db().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackVolume)
    }
}

impl TrackVolumeTarget {
    fn volume(&self) -> Volume {
        self.track.volume()
    }
}

impl<'a> Target<'a> for TrackVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = volume_unit_value(self.volume());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_VOLUME_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track: Set volume",
    short_name: "Track volume",
    supports_track: true,
    supports_gang_selected: true,
    supports_gang_grouping: true,
    ..DEFAULT_TARGET
};

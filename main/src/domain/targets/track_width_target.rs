use crate::domain::ui_util::{
    format_as_double_percentage_without_unit, format_as_symmetric_percentage_without_unit,
    parse_from_double_percentage, parse_from_symmetric_percentage,
};
use crate::domain::{
    get_effective_tracks, width_unit_value, with_gang_behavior, CompartmentKind,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, TrackDescriptor, TrackGangBehavior, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, Target, UnitValue,
};
use reaper_high::{
    AvailablePanValue, ChangeEvent, PanExt, Project, Track, TrackSetSmartOpts, Width,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackWidthTarget {
    pub track_descriptor: TrackDescriptor,
    pub gang_behavior: TrackGangBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackWidthTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackWidth(TrackWidthTarget {
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
pub struct TrackWidthTarget {
    pub track: Track,
    pub gang_behavior: TrackGangBehavior,
}

impl RealearnTarget for TrackWidthTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_from_symmetric_percentage(text)
    }

    fn parse_as_step_size(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_from_double_percentage(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_as_symmetric_percentage_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue, _: ControlContext) -> String {
        format_as_double_percentage_without_unit(step_size)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.track.is_available()
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let width = Width::from_normalized_value(value.to_unit_value()?.get());
        with_gang_behavior(
            self.track.project(),
            self.gang_behavior,
            &TRACK_WIDTH_TARGET,
            |gang_behavior, grouping_behavior| {
                self.track.set_width_smart(
                    width.reaper_value(),
                    TrackSetSmartOpts {
                        gang_behavior,
                        grouping_behavior,
                        // Important because we don't want undo points for each little fader movement!
                        done: false,
                    },
                )
            },
        )?;
        Ok(HitResponse::processed_with_effect())
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackPanChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    match e.new_value {
                        AvailablePanValue::Complete(v) => v.width().map(|width| {
                            AbsoluteValue::Continuous(width_unit_value(Width::from_reaper_value(
                                width,
                            )))
                        }),
                        AvailablePanValue::Incomplete(_) => None,
                    },
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(format!("{:.2}", self.width().reaper_value().get()).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.width().reaper_value().get()))
    }

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        match key {
            "width.mcu" => {
                let width = self.width().reaper_value().get();
                let text = format!("{:>4.0}W", width * 100.0);
                Some(PropValue::Text(text.into()))
            }
            _ => None,
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackWidth)
    }
}

impl TrackWidthTarget {
    fn width(&self) -> Width {
        self.track.width()
    }
}

impl<'a> Target<'a> for TrackWidthTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = width_unit_value(self.width());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_WIDTH_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Set stereo pan width",
    short_name: "Track pan width",
    supports_track: true,
    supports_gang_selected: true,
    supports_gang_grouping: true,
    ..DEFAULT_TARGET
};

use crate::domain::{
    format_value_as_pan, get_effective_tracks, pan_unit_value, parse_value_from_pan,
    with_gang_behavior, CompartmentKind, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, TrackDescriptor,
    TrackGangBehavior, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, Target, UnitValue,
    BASE_EPSILON,
};
use reaper_high::{AvailablePanValue, ChangeEvent, Pan, PanExt, Project, Track, TrackSetSmartOpts};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackPanTarget {
    pub track_descriptor: TrackDescriptor,
    pub gang_behavior: TrackGangBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackPanTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackPan(TrackPanTarget {
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
pub struct TrackPanTarget {
    pub track: Track,
    pub gang_behavior: TrackGangBehavior,
}

impl RealearnTarget for TrackPanTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_pan(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_pan(value)
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

    fn value_unit(&self, _: ControlContext) -> &'static str {
        ""
    }

    fn step_size_unit(&self, _: ControlContext) -> &'static str {
        ""
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_pan(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let pan = Pan::from_normalized_value(value.to_unit_value()?.get());
        with_gang_behavior(
            self.track.project(),
            self.gang_behavior,
            &TRACK_PAN_TARGET,
            |gang_behavior, grouping_behavior| {
                self.track.set_pan_smart(
                    pan.reaper_value(),
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
                (true, {
                    let pan = match e.new_value {
                        AvailablePanValue::Complete(v) => v.main_pan(),
                        AvailablePanValue::Incomplete(pan) => pan,
                    };
                    Some(AbsoluteValue::Continuous(pan_unit_value(
                        Pan::from_reaper_value(pan),
                    )))
                })
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(self.pan().to_string().into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.pan().reaper_value().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackPan)
    }

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        match key {
            "pan.mcu" => {
                let pan = self.pan().reaper_value().get();
                let text: Cow<'static, str> = if pan.abs() < BASE_EPSILON {
                    "  <C>  ".into()
                } else if pan < 0.0 {
                    format!("<{:>3.0}   ", pan.abs() * 100.0).into()
                } else {
                    format!("   {:<3.0}>", pan * 100.0).into()
                };
                Some(PropValue::Text(text))
            }
            _ => None,
        }
    }
}

impl TrackPanTarget {
    fn pan(&self) -> Pan {
        self.track.pan()
    }
}

impl<'a> Target<'a> for TrackPanTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = pan_unit_value(self.pan());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_PAN_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Set pan",
    short_name: "Track pan",
    supports_track: true,
    supports_gang_selected: true,
    supports_gang_grouping: true,
    ..DEFAULT_TARGET
};

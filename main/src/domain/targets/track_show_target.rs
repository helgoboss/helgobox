use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, get_effective_tracks,
    track_solo_unit_value, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, FeedbackResolution, HitResponse, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    TrackDescriptor, TrackExclusivity, UnresolvedReaperTargetDef,
    AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};
use reaper_medium::TrackArea;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackShowTarget {
    pub track_descriptor: TrackDescriptor,
    pub exclusivity: TrackExclusivity,
    pub area: TrackArea,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackShowTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackShow(TrackShowTarget {
                        track,
                        exclusivity: self.exclusivity,
                        area: self.area,
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
pub struct TrackShowTarget {
    pub track: Track,
    pub exclusivity: TrackExclusivity,
    pub area: TrackArea,
}

impl RealearnTarget for TrackShowTarget {
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
        change_track_prop(
            &self.track,
            self.exclusivity,
            value.to_unit_value()?,
            |t| t.set_shown(self.area, true),
            |t| t.set_shown(self.area, false),
        );
        Ok(HitResponse::processed_with_effect())
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackVisibilityChanged(e))
                if &e.track == &self.track =>
            {
                let is_shown = match self.area {
                    TrackArea::Tcp => e.new_value.tcp,
                    TrackArea::Mcp => e.new_value.mcp,
                };
                let feedback_value =
                    AbsoluteValue::Continuous(convert_bool_to_unit_value(is_shown));
                (true, Some(feedback_value))
            }
            _ => (false, None),
        }
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

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackShow)
    }
}

impl<'a> Target<'a> for TrackShowTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let is_shown = self.track.is_shown(self.area);
        let val = convert_bool_to_unit_value(is_shown);
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_SHOW_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track: Show/hide",
    short_name: "Show/hide track",
    supports_track: true,
    supports_track_exclusivity: true,
    ..DEFAULT_TARGET
};

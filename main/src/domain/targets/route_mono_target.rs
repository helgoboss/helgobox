use crate::domain::{
    format_value_as_on_off, get_track_route, mute_unit_value, ControlContext,
    ExtendedProcessorContext, FeedbackResolution, HitInstructionReturnValue, MappingCompartment,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, TrackRouteDescriptor, UnresolvedReaperTargetDef,
    AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track, TrackRoute};

#[derive(Debug)]
pub struct UnresolvedRouteMonoTarget {
    pub descriptor: TrackRouteDescriptor,
    pub poll_for_feedback: bool,
}

impl UnresolvedReaperTargetDef for UnresolvedRouteMonoTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::TrackRouteMono(RouteMonoTarget {
            route: get_track_route(context, &self.descriptor, compartment)?,
            poll_for_feedback: self.poll_for_feedback,
        })])
    }

    fn route_descriptor(&self) -> Option<&TrackRouteDescriptor> {
        Some(&self.descriptor)
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        if self.poll_for_feedback {
            Some(FeedbackResolution::High)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteMonoTarget {
    pub route: TrackRoute,
    pub poll_for_feedback: bool,
}

impl RealearnTarget for RouteMonoTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            self.route.set_mono(false);
        } else {
            self.route.set_mono(true);
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.route.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.route.track().project())
    }

    fn track(&self) -> Option<&Track> {
        Some(self.route.track())
    }

    fn route(&self) -> Option<&TrackRoute> {
        Some(&self.route)
    }

    fn supports_automatic_feedback(&self) -> bool {
        self.poll_for_feedback
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackSendMono)
    }
}

impl<'a> Target<'a> for RouteMonoTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = mute_unit_value(self.route.is_mono());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ROUTE_MONO_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Send: Mono/stereo",
    short_name: "Send mono/stereo",
    hint: AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY,
    supports_poll_for_feedback: true,
    supports_track: true,
    supports_send: true,
    ..DEFAULT_TARGET
};

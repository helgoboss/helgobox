use crate::domain::{
    format_value_as_pan, get_track_route, pan_unit_value, parse_value_from_pan,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, TrackRouteDescriptor, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Pan, Project, Track, TrackRoute};
use reaper_medium::{EditMode, ReaperFunctionError};

#[derive(Debug)]
pub struct UnresolvedRoutePanTarget {
    pub descriptor: TrackRouteDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedRoutePanTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::TrackRoutePan(RoutePanTarget {
            route: get_track_route(context, &self.descriptor, compartment)?,
        })])
    }

    fn route_descriptor(&self) -> Option<&TrackRouteDescriptor> {
        Some(&self.descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutePanTarget {
    pub route: TrackRoute,
}

impl RealearnTarget for RoutePanTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_pan(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_pan(value)
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let pan = Pan::from_normalized_value(value.to_unit_value()?.get());
        self.route
            .set_pan(pan, EditMode::NormalTweak)
            .map_err(|_| "couldn't set route pan")?;
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

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackRoutePanChanged(e))
                if e.route == self.route =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(pan_unit_value(
                        Pan::from_reaper_value(e.new_value),
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(self.pan().ok()?.to_string())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.pan().ok()?.reaper_value().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackSendPan)
    }
}

impl RoutePanTarget {
    fn pan(&self) -> Result<Pan, ReaperFunctionError> {
        self.route.pan()
    }
}

impl<'a> Target<'a> for RoutePanTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = pan_unit_value(self.pan().ok()?);
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ROUTE_PAN_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Send: Set pan",
    short_name: "Send pan",
    supports_track: true,
    supports_send: true,
    ..DEFAULT_TARGET
};

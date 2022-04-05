use crate::domain::{
    format_value_as_on_off, get_track_route, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, MappingCompartment, MappingControlContext, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, TrackRouteDescriptor,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track, TrackRoute};
use reaper_medium::EditMode;

#[derive(Debug)]
pub struct UnresolvedRouteTouchStateTarget {
    pub descriptor: TrackRouteDescriptor,
    pub parameter_type: TouchedRouteParameterType,
}

impl UnresolvedReaperTargetDef for UnresolvedRouteTouchStateTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::RouteTouchState(RouteTouchStateTarget {
            route: get_track_route(context, &self.descriptor, compartment)?,
            parameter_type: self.parameter_type,
        })])
    }

    fn route_descriptor(&self) -> Option<&TrackRouteDescriptor> {
        Some(&self.descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteTouchStateTarget {
    pub route: TrackRoute,
    pub parameter_type: TouchedRouteParameterType,
}

impl RealearnTarget for RouteTouchStateTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if !value.is_on() {
            match self.parameter_type {
                TouchedRouteParameterType::Volume => {
                    let current_value = self.route.volume().map_err(|e| e.message())?;
                    self.route
                        .set_volume(current_value, EditMode::EndOfEdit)
                        .map_err(|e| e.message())?;
                }
                TouchedRouteParameterType::Pan => {
                    let current_value = self.route.pan().map_err(|e| e.message())?;
                    self.route
                        .set_pan(current_value, EditMode::EndOfEdit)
                        .map_err(|e| e.message())?;
                }
            }
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

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::RouteTouchState)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for RouteTouchStateTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ROUTE_TOUCH_STATE_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Send: Set automation touch state",
    short_name: "Send touch state",
    supports_track: true,
    supports_send: true,
    ..DEFAULT_TARGET
};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    enum_iterator::IntoEnumIterator,
    num_enum::TryFromPrimitive,
    num_enum::IntoPrimitive,
    derive_more::Display,
)]
#[repr(usize)]
pub enum TouchedRouteParameterType {
    Volume,
    Pan,
}

impl Default for TouchedRouteParameterType {
    fn default() -> Self {
        TouchedRouteParameterType::Volume
    }
}

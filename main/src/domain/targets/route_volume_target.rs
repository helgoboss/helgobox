use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    get_track_routes, Compartment, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, TrackRouteDescriptor,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track, TrackRoute, Volume};
use reaper_medium::{EditMode, ReaperFunctionError};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedRouteVolumeTarget {
    pub descriptor: TrackRouteDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedRouteVolumeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let routes = get_track_routes(context, &self.descriptor, compartment)?;
        let targets = routes
            .into_iter()
            .map(|route| ReaperTarget::TrackRouteVolume(RouteVolumeTarget { route }))
            .collect();
        Ok(targets)
    }

    fn route_descriptor(&self) -> Option<&TrackRouteDescriptor> {
        Some(&self.descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteVolumeTarget {
    pub route: TrackRoute,
}

impl RealearnTarget for RouteVolumeTarget {
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get());
        self.route
            .set_volume(volume.unwrap_or(Volume::MIN), EditMode::NormalTweak)
            .map_err(|_| "couldn't set route volume")?;
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
            CompoundChangeEvent::Reaper(ChangeEvent::TrackRouteVolumeChanged(e))
                if e.route == self.route =>
            {
                (
                    true,
                    None
                    // TODO-medium Weird: CSURF_EXT_SETSENDVOLUME exhibits volume values in the
                    //  VolumeSliderValue unit when automation mode is Write, Touch or similar.
                    //  This is a bug because it's supposed to use the ReaperVolumeValue unit.
                    //  As soon as this is fixed in REAPER (probably 6.44), we can use the value 
                    //  again, until then we shouldn't rely on it since it leads to weird feedback 
                    //  while writing automation (#513).
                    // Some(AbsoluteValue::Continuous(volume_unit_value(
                    //     Volume::from_reaper_value(e.new_value),
                    // ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(self.volume().ok()?.to_string().into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.volume().ok()?.db().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::RouteVolume)
    }
}

impl RouteVolumeTarget {
    fn volume(&self) -> Result<Volume, ReaperFunctionError> {
        self.route.volume()
    }
}

impl<'a> Target<'a> for RouteVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = volume_unit_value(self.volume().ok()?);
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ROUTE_VOLUME_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Send: Set volume",
    short_name: "Send volume",
    supports_track: true,
    supports_send: true,
    ..DEFAULT_TARGET
};

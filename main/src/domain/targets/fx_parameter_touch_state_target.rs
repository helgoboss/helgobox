use crate::domain::{
    format_value_as_on_off, get_fx_params, Compartment, ControlContext, ExtendedProcessorContext,
    FxParameterDescriptor, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Fx, FxParameter, Project, Track};

#[derive(Debug)]
pub struct UnresolvedFxParameterTouchStateTarget {
    pub fx_parameter_descriptor: FxParameterDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedFxParameterTouchStateTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let params = get_fx_params(context, &self.fx_parameter_descriptor, compartment)?;
        let targets = params
            .into_iter()
            .map(|param| ReaperTarget::FxParameterTouchState(FxParameterTouchStateTarget { param }))
            .collect();
        Ok(targets)
    }

    fn fx_parameter_descriptor(&self) -> Option<&FxParameterDescriptor> {
        Some(&self.fx_parameter_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxParameterTouchStateTarget {
    pub param: FxParameter,
}

impl RealearnTarget for FxParameterTouchStateTarget {
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
    ) -> Result<HitResponse, &'static str> {
        if value.is_on() {
            // Correct! Here, we only want an effect if the button is *released*.
            return Ok(HitResponse::ignored());
        }
        self.param.end_edit().map_err(|e| e.message())?;
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.param.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.param.fx().project()
    }

    fn track(&self) -> Option<&Track> {
        self.param.fx().track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(self.param.fx())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxParameterTouchState)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for FxParameterTouchStateTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_PARAMETER_TOUCH_STATE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::FxParameter,
    name: "Set automation touch state",
    short_name: "FX parameter touch state",
    supports_track: true,
    supports_fx: true,
    supports_fx_parameter: true,
    ..DEFAULT_TARGET
};

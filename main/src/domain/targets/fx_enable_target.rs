use crate::domain::{
    format_value_as_on_off, fx_enable_unit_value, CompartmentKind, CompoundChangeEvent,
    ControlContext, ExtendedProcessorContext, FxDescriptor, HitResponse, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};
use reaper_medium::ParamId;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedFxEnableTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedFxEnableTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(self
            .fx_descriptor
            .resolve(context, compartment)?
            .into_iter()
            .map(|fx| {
                ReaperTarget::FxEnable(FxEnableTarget {
                    bypass_param_index: fx.parameter_by_id(ParamId::Bypass).map(|p| p.index()),
                    fx,
                })
            })
            .collect())
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxEnableTarget {
    pub fx: Fx,
    pub bypass_param_index: Option<u32>,
}

impl RealearnTarget for FxEnableTarget {
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
    ) -> Result<HitResponse, &'static str> {
        if value.to_unit_value()?.is_zero() {
            self.fx.disable()?;
        } else {
            self.fx.enable()?;
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.fx.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(&self.fx)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::FxEnabledChanged(e)) if e.fx == self.fx => (
                true,
                Some(AbsoluteValue::Continuous(fx_enable_unit_value(e.new_value))),
            ),
            CompoundChangeEvent::Reaper(ChangeEvent::FxParameterValueChanged(e))
                if Some(e.parameter.index()) == self.bypass_param_index
                    && e.parameter.fx() == &self.fx =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxEnable)
    }
}

impl<'a> Target<'a> for FxEnableTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::Continuous(fx_enable_unit_value(
            self.fx.is_enabled(),
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_ENABLE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Fx,
    name: "Enable/disable",
    short_name: "Enable/disable FX",
    hint: "No feedback from automation",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

use crate::domain::{
    get_fx_name, get_fxs, percentage_for_fx_within_chain, Compartment, ControlContext,
    ExtendedProcessorContext, FxDescriptor, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, NumericValue, Target};
use reaper_high::{Fx, Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedFxToolTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedFxToolTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(get_fxs(context, &self.fx_descriptor, compartment)?
            .into_iter()
            .map(|fx| ReaperTarget::FxTool(FxToolTarget { fx }))
            .collect())
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxToolTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxToolTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
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

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_fx_name(&self.fx).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let position = self.fx.index() + 1;
        Some(NumericValue::Discrete(position as _))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxTool)
    }
}

impl<'a> Target<'a> for FxToolTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let fx_index = self.fx.index();
        percentage_for_fx_within_chain(self.fx.chain(), fx_index)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_TOOL_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Fx",
    short_name: "Fx",
    supports_fx: true,
    supports_track: true,
    ..DEFAULT_TARGET
};

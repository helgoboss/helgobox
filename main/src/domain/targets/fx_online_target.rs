use crate::domain::{
    format_value_as_on_off, fx_online_unit_value, get_fxs, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, FxDescriptor, HitInstructionReturnValue, MappingCompartment,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};

#[derive(Debug)]
pub struct UnresolvedFxOnlineTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedFxOnlineTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(get_fxs(context, &self.fx_descriptor, compartment)?
            .into_iter()
            .map(|fx| ReaperTarget::FxOnline(FxOnlineTarget { fx }))
            .collect())
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxOnlineTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxOnlineTarget {
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
        let online = !value.to_unit_value()?.is_zero();
        self.fx.set_online(online);
        Ok(None)
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
            CompoundChangeEvent::Reaper(ChangeEvent::FxEnabledChanged(e)) if e.fx == self.fx => {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxOnline)
    }
}

impl<'a> Target<'a> for FxOnlineTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::Continuous(fx_online_unit_value(
            self.fx.is_online(),
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_ONLINE_TARGET: TargetTypeDef = TargetTypeDef {
    name: "FX: Set online/offline",
    short_name: "On/off-line FX",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

use crate::domain::{
    convert_count_to_step_size, convert_unit_value_to_preset_index, fx_preset_unit_value,
    Compartment, CompoundChangeEvent, ControlContext, ExtendedProcessorContext, FxDescriptor,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use reaper_high::{ChangeEvent, Fx, Project, Track};
use reaper_medium::FxPresetRef;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedFxPresetTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedFxPresetTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(self
            .fx_descriptor
            .resolve(context, compartment)?
            .into_iter()
            .map(|fx| ReaperTarget::FxPreset(FxPresetTarget { fx }))
            .collect())
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxPresetTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxPresetTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<no preset>" is also a possible value.
        let preset_count = self.fx.preset_index_and_count().count;
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(preset_count + 1),
                is_retriggerable: false,
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        _: ControlContext,
    ) -> Result<u32, &'static str> {
        let value = convert_unit_value_to_preset_index(&self.fx, input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        match convert_unit_value_to_preset_index(&self.fx, value) {
            None => "<No preset>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let preset_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => convert_unit_value_to_preset_index(&self.fx, v),
            AbsoluteValue::Discrete(f) => {
                if f.actual() == 0 {
                    None
                } else {
                    Some(f.actual() - 1)
                }
            }
        };
        let preset_ref = match preset_index {
            None => FxPresetRef::FactoryPreset,
            Some(i) => FxPresetRef::Preset(i),
        };
        self.fx.activate_preset(preset_ref);
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
            CompoundChangeEvent::Reaper(ChangeEvent::FxPresetChanged(e)) if e.fx == self.fx => {
                (true, None)
            }
            CompoundChangeEvent::Reaper(ChangeEvent::FxParameterValueChanged(e))
                if e.parameter.fx() == &self.fx =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        Ok(fx_preset_unit_value(&self.fx, index))
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(self.fx.preset_name()?.into_string().into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let index = self.fx.preset_index_and_count().index?;
        Some(NumericValue::Discrete(index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxPreset)
    }
}

impl<'a> Target<'a> for FxPresetTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let res = self.fx.preset_index_and_count();
        // Because we count "<No preset>" as a possible value, this is equal.
        let max_value = res.count;
        let actual_value = res.index.map(|i| i + 1).unwrap_or(0);
        Some(AbsoluteValue::Discrete(Fraction::new(
            actual_value,
            max_value,
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "FX: Navigate between presets",
    short_name: "Navigate FX presets",
    hint: "Automatic feedback since REAPER v6.13",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

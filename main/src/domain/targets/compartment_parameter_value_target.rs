use crate::domain::ui_util::parse_unit_value_from_percentage;
use crate::domain::{
    convert_count_to_step_size, Compartment, CompartmentParamIndex, CompoundChangeEvent,
    ControlContext, EffectiveParamValue, ExtendedProcessorContext, HitResponse,
    MappingControlContext, PluginParamIndex, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
use reaper_medium::ReaperNormalizedFxParamValue;
use std::num::NonZeroU32;

#[derive(Debug)]
pub struct UnresolvedCompartmentParameterValueTarget {
    pub compartment: Compartment,
    pub index: CompartmentParamIndex,
}

impl UnresolvedReaperTargetDef for UnresolvedCompartmentParameterValueTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::CompartmentParameterValue(
            CompartmentParameterValueTarget {
                compartment: self.compartment,
                index: self.index,
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompartmentParameterValueTarget {
    pub compartment: Compartment,
    pub index: CompartmentParamIndex,
}

impl CompartmentParameterValueTarget {
    fn value_count(&self, context: ControlContext) -> Option<NonZeroU32> {
        context
            .unit
            .borrow()
            .parameter_manager()
            .params()
            .compartment_params(self.compartment)
            .at(self.index)
            .setting()
            .value_count
    }

    fn plugin_param_index(&self) -> PluginParamIndex {
        self.compartment.to_plugin_param_index(self.index)
    }
}

impl RealearnTarget for CompartmentParameterValueTarget {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        let value_count = self.value_count(context);
        if let Some(c) = value_count {
            (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(c.get()),
                    is_retriggerable: false,
                },
                TargetCharacter::Discrete,
            )
        } else {
            (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let unit_value = value.to_unit_value()?;
        let plugin_param_index = self.plugin_param_index();
        if context.control_context.unit.borrow().is_main_unit() {
            // The main unit of an instance is special in that its compartment parameters are
            // connected to the VST plug-in parameters. That's why we should change the VST plug-in
            // parameter directly for reasons of unidirectional data flow.
            context
                .control_context
                .processor_context
                .containing_fx()
                .parameter_by_index(plugin_param_index.get())
                .set_reaper_normalized_value(ReaperNormalizedFxParamValue::new(unit_value.get()))?;
        } else {
            // Compartment parameters of additional units are purely internal, so we need to
            // control them internally.
            context
                .control_context
                .unit
                .borrow()
                .parameter_manager()
                .set_single_parameter(plugin_param_index, unit_value.get() as _);
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::CompartmentParameter(index)
                if index == self.plugin_param_index() =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        match self.value_count(context) {
            None => parse_unit_value_from_percentage(text),
            Some(_) => self.parse_value_from_discrete_value(text, context),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let value_count = self.value_count(context).ok_or("not supported")?;
        let step_size = convert_count_to_step_size(value_count.get());
        let result = (value as f64 * step_size.get()).try_into()?;
        Ok(result)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        let value_count = self.value_count(context).ok_or("not supported")?;
        let step_size = convert_count_to_step_size(value_count.get());
        let val = (input.get() / step_size.get()).round() as u32;
        Ok(val)
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::CompartmentParameterValue)
    }
}

impl<'a> Target<'a> for CompartmentParameterValueTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let unit = context.unit.borrow();
        let params = unit.parameter_manager().params();
        let param = params.compartment_params(self.compartment).at(self.index);
        let value = param.effective_value();
        let abs_val = match value {
            EffectiveParamValue::Continuous(v) => {
                AbsoluteValue::Continuous(UnitValue::new_clamped(v))
            }
            EffectiveParamValue::Discrete(v) => {
                let value_count = param.setting().value_count.unwrap();
                AbsoluteValue::Discrete(Fraction::new(v, value_count.get() - 1))
            }
        };
        Some(abs_val)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const COMPARTMENT_PARAMETER_VALUE_TARGET: TargetTypeDef = TargetTypeDef {
    name: "ReaLearn: Set compartment parameter value",
    short_name: "Set compartment parameter value",
    ..DEFAULT_TARGET
};

use crate::domain::ui_util::{fx_parameter_unit_value, parse_unit_value_from_percentage};
use crate::domain::{
    AdditionalFeedbackEvent, ControlContext, HitInstructionReturnValue, MappingControlContext,
    RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, FxParameter, FxParameterCharacter, Project, Track};
use reaper_medium::{GetParameterStepSizesResult, ReaperNormalizedFxParamValue};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq)]
pub struct FxParameterTarget {
    pub param: FxParameter,
    pub poll_for_feedback: bool,
}

// TODO-high Implement textual feedback
impl RealearnTarget for FxParameterTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        use GetParameterStepSizesResult::*;
        match self.param.step_sizes() {
            None => (ControlType::AbsoluteContinuous, TargetCharacter::Continuous),
            Some(GetParameterStepSizesResult::Normal {
                normal_step,
                small_step,
                ..
            }) => {
                // The reported step sizes relate to the reported value range, which is not
                // always the unit interval! Easy to test with JS
                // FX.
                let range = self.param.value_range();
                // We are primarily interested in the smallest step size that makes sense.
                // We can always create multiples of it.
                let span = (range.max_val - range.min_val).abs();
                if span == 0.0 {
                    return (ControlType::AbsoluteContinuous, TargetCharacter::Continuous);
                }
                let pref_step_size = small_step.unwrap_or(normal_step);
                let step_size = pref_step_size / span;
                (
                    ControlType::AbsoluteDiscrete {
                        atomic_step_size: UnitValue::new(step_size),
                    },
                    TargetCharacter::Discrete,
                )
            }
            Some(Toggle) => (ControlType::AbsoluteContinuous, TargetCharacter::Switch),
        }
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        if self.param.character() == FxParameterCharacter::Discrete {
            self.parse_value_from_discrete_value(text, context)
        } else {
            parse_unit_value_from_percentage(text)
        }
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        if self.param.character() == FxParameterCharacter::Discrete {
            self.parse_value_from_discrete_value(text, context)
        } else {
            parse_unit_value_from_percentage(text)
        }
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        _: ControlContext,
    ) -> Result<u32, &'static str> {
        // Example (target step size = 0.10):
        // - 0    => 0
        // - 0.05 => 1
        // - 0.10 => 1
        // - 0.15 => 2
        // - 0.20 => 2
        let step_size = self.param.step_size().ok_or("not supported")?;
        let val = (input.get() / step_size).round() as _;
        Ok(val)
    }

    fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
        self.param
            // Even if a REAPER-normalized value can take numbers > 1.0, the usual value range
            // is in fact normalized in the classical sense (unit interval).
            .format_reaper_normalized_value(ReaperNormalizedFxParamValue::new(value.get()))
            .map(|s| s.into_string())
            .unwrap_or_else(|_| self.format_value_generic(value, context))
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        // It's okay to just convert this to a REAPER-normalized value. We don't support
        // values above the maximum (or buggy plug-ins).
        let v = ReaperNormalizedFxParamValue::new(value.to_unit_value()?.get());
        self.param
            .set_reaper_normalized_value(v)
            .map_err(|_| "couldn't set FX parameter value")?;
        Ok(None)
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

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        if self.poll_for_feedback {
            return (false, None);
        }
        match evt {
            ChangeEvent::FxParameterValueChanged(e) if e.parameter == self.param => (
                true,
                Some(AbsoluteValue::Continuous(fx_parameter_unit_value(
                    &e.parameter,
                    e.new_value,
                ))),
            ),
            _ => (false, None),
        }
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        if self.poll_for_feedback {
            return (false, None);
        }
        match evt {
            AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(e)
                if e.parameter == self.param =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(fx_parameter_unit_value(
                        &e.parameter,
                        e.new_value,
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let step_size = self.param.step_size().ok_or("not supported")?;
        let result = (value as f64 * step_size).try_into()?;
        Ok(result)
    }
}

impl<'a> Target<'a> for FxParameterTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = fx_parameter_unit_value(&self.param, self.param.reaper_normalized_value());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

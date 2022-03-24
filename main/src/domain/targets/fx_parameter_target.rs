use crate::domain::ui_util::parse_unit_value_from_percentage;
use crate::domain::{
    get_fx_param, AdditionalFeedbackEvent, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, FeedbackResolution, FxParameterDescriptor, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealTimeControlContext, RealTimeReaperTarget,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, FxParameter, FxParameterCharacter, Project, Reaper, Track};
use reaper_medium::{
    GetParamExResult, GetParameterStepSizesResult, MediaTrack, ProjectRef,
    ReaperNormalizedFxParamValue, ReaperVersion, TrackFxLocation,
};
use std::convert::TryInto;

#[derive(Debug)]
pub struct UnresolvedFxParameterTarget {
    pub fx_parameter_descriptor: FxParameterDescriptor,
    pub poll_for_feedback: bool,
}

impl UnresolvedReaperTargetDef for UnresolvedFxParameterTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let param = get_fx_param(context, &self.fx_parameter_descriptor, compartment)?;
        let is_real_time_ready = reaper_is_ready_for_real_time_fx_param_control()
            && fx_is_on_same_track_as_realearn(context, &param);
        let target = FxParameterTarget {
            is_real_time_ready,
            param,
            poll_for_feedback: self.poll_for_feedback,
        };
        Ok(vec![ReaperTarget::FxParameter(target)])
    }

    fn fx_parameter_descriptor(&self) -> Option<&FxParameterDescriptor> {
        Some(&self.fx_parameter_descriptor)
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        if self.poll_for_feedback {
            Some(FeedbackResolution::High)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxParameterTarget {
    pub is_real_time_ready: bool,
    pub param: FxParameter,
    pub poll_for_feedback: bool,
}

impl RealearnTarget for FxParameterTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        determine_param_control_type_and_character(
            || self.param.step_sizes(),
            || self.param.value_range(),
        )
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
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        if self.poll_for_feedback {
            return (false, None);
        }
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::FxParameterValueChanged(e))
                if e.parameter == self.param =>
            {
                (
                    true,
                    Some(fx_parameter_absolute_value(&e.parameter, e.new_value)),
                )
            }
            CompoundChangeEvent::Additional(
                AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(e),
            ) if e.parameter == self.param => (
                true,
                Some(fx_parameter_absolute_value(&e.parameter, e.new_value)),
            ),
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

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(self.param.formatted_value().ok()?.into_string())
    }

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        match key {
            "fx_parameter.index" => Some(PropValue::Index(self.param.index())),
            "fx_parameter.name" => Some(PropValue::Text(self.param.name().into_string())),
            _ => None,
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxParameter)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        if !self.is_real_time_ready {
            // Real-time controlling not possible.
            return None;
        }
        let target = RealTimeFxParameterTarget {
            track: self.param.fx().track()?.raw(),
            fx_location: self.param.fx().query_index(),
            param_index: self.param.index(),
        };
        Some(RealTimeReaperTarget::FxParameter(target))
    }
}

impl<'a> Target<'a> for FxParameterTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(fx_parameter_absolute_value(
            &self.param,
            self.param.reaper_normalized_value(),
        ))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeFxParameterTarget {
    track: MediaTrack,
    fx_location: TrackFxLocation,
    param_index: u32,
}

unsafe impl Send for RealTimeFxParameterTarget {}

impl RealTimeFxParameterTarget {
    pub fn should_control_in_real_time(&self, is_called_from_vst: bool) -> bool {
        if !is_called_from_vst {
            // Setting the target FX parameter value in real-time is only safe if we are in the
            // processing callstack of the target FX track. The resolve step of this target makes
            // sure that a real-time target doesn't even exist if the ReaLearn track doesn't match
            // the target FX track. But we still need to make sure here that we are in the same
            // processing callstack. This is the case if we are in a processing method of the
            // ReaLearn VST plug-in (control input = FX input). It's not the case if we are called
            // from the audio hook (control input = MIDI hardware device).
            return false;
        }
        let is_rendering = Reaper::get()
            .medium_reaper()
            .enum_projects(ProjectRef::CurrentlyRendering, 0)
            .is_some();
        if !is_rendering {
            // We want real-time control only during rendering. Because REAPER won't invoke the
            // change notifications when called in real-time (ReaLearn and maybe also other
            // control surface implementations relies on those during normal playing to make
            // feedback work).
            return false;
        }
        true
    }

    pub fn hit(&mut self, value: ControlValue) -> Result<(), &'static str> {
        // It's okay to just convert this to a REAPER-normalized value. We don't support
        // values above the maximum (or buggy plug-ins).
        let v = ReaperNormalizedFxParamValue::new(value.to_unit_value()?.get());
        let result = unsafe {
            Reaper::get().medium_reaper().track_fx_set_param_normalized(
                self.track,
                self.fx_location,
                self.param_index,
                v,
            )
        };
        result.map_err(|_| "couldn't set FX parameter value")?;
        Ok(())
    }
}

impl<'a> Target<'a> for RealTimeFxParameterTarget {
    type Context = RealTimeControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let reaper_normalize_value = unsafe {
            Reaper::get().medium_reaper().track_fx_get_param_normalized(
                self.track,
                self.fx_location,
                self.param_index,
            )
        };
        let unit_value = UnitValue::new_clamped(reaper_normalize_value.get());
        Some(AbsoluteValue::Continuous(unit_value))
    }

    fn control_type(&self, _: Self::Context) -> ControlType {
        determine_param_control_type_and_character(
            || unsafe {
                Reaper::get()
                    .medium_reaper()
                    .track_fx_get_parameter_step_sizes(
                        self.track,
                        self.fx_location,
                        self.param_index,
                    )
            },
            || unsafe {
                Reaper::get().medium_reaper().track_fx_get_param_ex(
                    self.track,
                    self.fx_location,
                    self.param_index,
                )
            },
        )
        .0
    }
}

pub const FX_PARAMETER_TARGET: TargetTypeDef = TargetTypeDef {
    name: "FX: Set parameter value",
    short_name: "FX parameter value",
    supports_poll_for_feedback: true,
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

fn determine_param_control_type_and_character(
    get_step_sizes: impl FnOnce() -> Option<GetParameterStepSizesResult>,
    get_value_range: impl FnOnce() -> GetParamExResult,
) -> (ControlType, TargetCharacter) {
    match get_step_sizes() {
        None => (ControlType::AbsoluteContinuous, TargetCharacter::Continuous),
        Some(GetParameterStepSizesResult::Normal {
            normal_step,
            small_step,
            ..
        }) => {
            // The reported step sizes relate to the reported value range, which is not
            // always the unit interval! Easy to test with JS
            // FX.
            let range = get_value_range();
            // We are primarily interested in the smallest step size that makes sense.
            // We can always create multiples of it.
            let span = (range.max_value - range.min_value).abs();
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
        Some(GetParameterStepSizesResult::Toggle) => {
            (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
        }
    }
}

fn fx_parameter_absolute_value(
    param: &FxParameter,
    value: ReaperNormalizedFxParamValue,
) -> AbsoluteValue {
    AbsoluteValue::Continuous(fx_parameter_unit_value(param, value))
}

fn fx_parameter_unit_value(param: &FxParameter, value: ReaperNormalizedFxParamValue) -> UnitValue {
    let v = value.get();
    if !UnitValue::is_valid(v) {
        // Either the FX reports a wrong value range (e.g. TAL Flanger Sync Speed)
        // or the value range exceeded a "normal" range (e.g. ReaPitch Wet). We can't
        // know. In future, we might offer further customization possibilities here.
        // For now, we just report it as 0.0 or 1.0 and log a warning.
        // TODO-medium As soon as we migrate to tracing, build this into the real-time target, too.
        slog::warn!(
            reaper_high::Reaper::get().logger(),
            "FX parameter reported normalized value {:?} which is not in unit interval: {:?}",
            v,
            param
        );
        return UnitValue::new_clamped(v);
    }
    UnitValue::new(v)
}

fn reaper_is_ready_for_real_time_fx_param_control() -> bool {
    Reaper::get().version() >= ReaperVersion::new("6.52+dev0323")
}

/// If ReaLearn is not on the same track as the FX whose parameters it should control,
/// controlling from a real-time thread is unsafe.
/// See here: https://forum.cockos.com/showpost.php?p=2525657&postcount=2009
fn fx_is_on_same_track_as_realearn(context: ExtendedProcessorContext, param: &FxParameter) -> bool {
    let realearn_track = context.context.track();
    realearn_track.is_some() && realearn_track == param.fx().track()
}

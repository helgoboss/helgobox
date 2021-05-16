use crate::base::{prop, Prop};
use crate::domain::{EelTransformation, Mode, OutputVariable};

use helgoboss_learn::{
    check_mode_applicability, full_unit_interval, AbsoluteMode, ButtonUsage,
    DetailedSourceCharacter, DiscreteIncrement, EncoderUsage, FireMode, GroupInteraction, Interval,
    ModeApplicabilityCheckInput, ModeParameter, OutOfRangeBehavior, PressDurationProcessor,
    SoftSymmetricUnitValue, TakeoverMode, UnitValue,
};

use rxrust::prelude::*;

use std::time::Duration;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    pub r#type: Prop<AbsoluteMode>,
    pub target_value_interval: Prop<Interval<UnitValue>>,
    pub source_value_interval: Prop<Interval<UnitValue>>,
    pub reverse: Prop<bool>,
    pub press_duration_interval: Prop<Interval<Duration>>,
    pub turbo_rate: Prop<Duration>,
    pub jump_interval: Prop<Interval<UnitValue>>,
    pub out_of_range_behavior: Prop<OutOfRangeBehavior>,
    pub fire_mode: Prop<FireMode>,
    pub round_target_value: Prop<bool>,
    pub takeover_mode: Prop<TakeoverMode>,
    pub button_usage: Prop<ButtonUsage>,
    pub encoder_usage: Prop<EncoderUsage>,
    pub eel_control_transformation: Prop<String>,
    pub eel_feedback_transformation: Prop<String>,
    // For relative control values.
    /// Depending on the target character, this is either a step count or a step size.
    ///
    /// A step count is a coefficient which multiplies the atomic step size. E.g. a step count of 2
    /// can be read as 2 * step_size which means double speed. When the step count is negative,
    /// it's interpreted as a fraction of 1. E.g. a step count of -2 is 1/2 * step_size which
    /// means half speed. The increment is fired only every nth time, which results in a
    /// slow-down, or in other words, less sensitivity.
    ///
    /// A step size is the positive, absolute size of an increment. 0.0 represents no increment,
    /// 1.0 represents an increment over the whole value range (not very useful).
    ///
    /// It's an interval. When using rotary encoders, the most important value is the interval
    /// minimum. There are some controllers which deliver higher increments if turned faster. This
    /// is where the maximum comes in. The maximum is also important if using the relative mode
    /// with buttons. The harder you press the button, the higher the increment. It's limited
    /// by the maximum value.
    pub step_interval: Prop<Interval<SoftSymmetricUnitValue>>,
    pub rotate: Prop<bool>,
    pub make_absolute: Prop<bool>,
    pub group_interaction: Prop<GroupInteraction>,
}

impl Default for ModeModel {
    fn default() -> Self {
        Self {
            r#type: prop(AbsoluteMode::Normal),
            target_value_interval: prop(full_unit_interval()),
            source_value_interval: prop(full_unit_interval()),
            reverse: prop(false),
            press_duration_interval: prop(Interval::new(
                Duration::from_millis(0),
                Duration::from_millis(0),
            )),
            turbo_rate: prop(Duration::from_millis(0)),
            jump_interval: prop(full_unit_interval()),
            out_of_range_behavior: prop(Default::default()),
            fire_mode: prop(Default::default()),
            round_target_value: prop(false),
            takeover_mode: prop(Default::default()),
            button_usage: prop(Default::default()),
            encoder_usage: prop(Default::default()),
            eel_control_transformation: prop(String::new()),
            eel_feedback_transformation: prop(String::new()),
            step_interval: prop(Self::default_step_size_interval()),
            rotate: prop(false),
            make_absolute: prop(false),
            group_interaction: prop(Default::default()),
        }
    }
}

impl ModeModel {
    pub fn default_step_size_interval() -> Interval<SoftSymmetricUnitValue> {
        // 0.01 has been chosen as default minimum step size because it corresponds to 1%.
        //
        // 0.05 has been chosen as default maximum step size in order to make users aware that
        // ReaLearn supports encoder acceleration ("dial harder = more increments") and
        // velocity-sensitive buttons ("press harder = more increments") but still is low
        // enough to not lead to surprising results such as ugly parameter jumps.
        Interval::new(
            SoftSymmetricUnitValue::new(0.01),
            SoftSymmetricUnitValue::new(0.05),
        )
    }

    /// This doesn't reset the mode type, just all the values.
    pub fn reset_within_type(&mut self) {
        let def = ModeModel::default();
        self.source_value_interval
            .set(def.source_value_interval.get());
        self.target_value_interval
            .set(def.target_value_interval.get());
        self.jump_interval.set(def.jump_interval.get());
        self.eel_control_transformation
            .set(def.eel_control_transformation.get_ref().clone());
        self.eel_feedback_transformation
            .set(def.eel_feedback_transformation.get_ref().clone());
        self.out_of_range_behavior
            .set(def.out_of_range_behavior.get());
        self.fire_mode.set(def.fire_mode.get());
        self.round_target_value.set(def.round_target_value.get());
        self.takeover_mode.set(def.takeover_mode.get());
        self.button_usage.set(def.button_usage.get());
        self.encoder_usage.set(def.encoder_usage.get());
        self.rotate.set(def.rotate.get());
        self.make_absolute.set(def.make_absolute.get());
        self.group_interaction.set(def.group_interaction.get());
        self.reverse.set(def.reverse.get());
        self.step_interval.set(def.step_interval.get());
        self.press_duration_interval
            .set(def.press_duration_interval.get());
        self.turbo_rate.set(def.turbo_rate.get());
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.r#type
            .changed()
            .merge(self.target_value_interval.changed())
            .merge(self.source_value_interval.changed())
            .merge(self.reverse.changed())
            .merge(self.jump_interval.changed())
            .merge(self.out_of_range_behavior.changed())
            .merge(self.fire_mode.changed())
            .merge(self.round_target_value.changed())
            .merge(self.takeover_mode.changed())
            .merge(self.button_usage.changed())
            .merge(self.encoder_usage.changed())
            .merge(self.eel_control_transformation.changed())
            .merge(self.eel_feedback_transformation.changed())
            .merge(self.step_interval.changed())
            .merge(self.rotate.changed())
            .merge(self.press_duration_interval.changed())
            .merge(self.turbo_rate.changed())
            .merge(self.make_absolute.changed())
            .merge(self.group_interaction.changed())
    }

    pub fn mode_parameter_is_relevant(
        &self,
        mode_parameter: ModeParameter,
        base_input: ModeApplicabilityCheckInput,
        possible_source_characters: &[DetailedSourceCharacter],
        control_is_relevant: bool,
        feedback_is_relevant: bool,
    ) -> bool {
        possible_source_characters.iter().any(|source_character| {
            let is_applicable = |is_feedback| {
                let input = ModeApplicabilityCheckInput {
                    is_feedback,
                    mode_parameter,
                    source_character: *source_character,
                    ..base_input
                };
                check_mode_applicability(input).is_relevant()
            };
            (control_is_relevant && is_applicable(false))
                || (feedback_is_relevant && is_applicable(true))
        })
    }

    /// Creates a mode reflecting this model's current values
    pub fn create_mode(
        &self,
        base_input: ModeApplicabilityCheckInput,
        possible_source_characters: &[DetailedSourceCharacter],
    ) -> Mode {
        let is_relevant = |mode_parameter: ModeParameter| {
            // We take both control and feedback into account to not accidentally get slightly
            // different behavior if feedback is not enabled.
            self.mode_parameter_is_relevant(
                mode_parameter,
                base_input,
                &possible_source_characters,
                true,
                true,
            )
        };
        // We know that just step max sometimes needs to be set to a sensible default (= step min)
        // and we know that step size and speed is mutually exclusive and therefore doesn't need
        // to be handled separately.
        let step_max_is_relevant =
            is_relevant(ModeParameter::StepSizeMax) || is_relevant(ModeParameter::SpeedMax);
        let min_step_count = convert_to_step_count(self.step_interval.get_ref().min_val());
        let min_step_size = self.step_interval.get_ref().min_val().abs();
        Mode {
            absolute_mode: if is_relevant(ModeParameter::AbsoluteMode) {
                self.r#type.get()
            } else {
                AbsoluteMode::default()
            },
            source_value_interval: if is_relevant(ModeParameter::SourceMinMax) {
                self.source_value_interval.get()
            } else {
                full_unit_interval()
            },
            target_value_interval: if is_relevant(ModeParameter::TargetMinMax) {
                self.target_value_interval.get()
            } else {
                full_unit_interval()
            },
            step_count_interval: Interval::new(
                min_step_count,
                if step_max_is_relevant {
                    convert_to_step_count(self.step_interval.get_ref().max_val())
                } else {
                    min_step_count
                },
            ),
            step_size_interval: Interval::new_auto(
                min_step_size,
                if step_max_is_relevant {
                    self.step_interval.get_ref().max_val().abs()
                } else {
                    min_step_size
                },
            ),
            jump_interval: if is_relevant(ModeParameter::JumpMinMax) {
                self.jump_interval.get()
            } else {
                full_unit_interval()
            },
            press_duration_processor: PressDurationProcessor::new(
                if is_relevant(ModeParameter::FireMode) {
                    self.fire_mode.get()
                } else {
                    FireMode::default()
                },
                self.press_duration_interval.get(),
                self.turbo_rate.get(),
            ),
            takeover_mode: if is_relevant(ModeParameter::TakeoverMode) {
                self.takeover_mode.get()
            } else {
                TakeoverMode::default()
            },
            encoder_usage: if is_relevant(ModeParameter::RelativeFilter) {
                self.encoder_usage.get()
            } else {
                EncoderUsage::default()
            },
            button_usage: if is_relevant(ModeParameter::ButtonFilter) {
                self.button_usage.get()
            } else {
                ButtonUsage::default()
            },
            reverse: if is_relevant(ModeParameter::Reverse) {
                self.reverse.get()
            } else {
                false
            },
            rotate: if is_relevant(ModeParameter::Rotate) {
                self.rotate.get()
            } else {
                false
            },
            increment_counter: 0,
            round_target_value: if is_relevant(ModeParameter::RoundTargetValue) {
                self.round_target_value.get()
            } else {
                false
            },
            out_of_range_behavior: if is_relevant(ModeParameter::OutOfRangeBehavior) {
                self.out_of_range_behavior.get()
            } else {
                OutOfRangeBehavior::default()
            },
            control_transformation: if is_relevant(ModeParameter::ControlTransformation) {
                EelTransformation::compile(
                    self.eel_control_transformation.get_ref(),
                    OutputVariable::Y,
                )
                .ok()
            } else {
                None
            },
            feedback_transformation: if is_relevant(ModeParameter::FeedbackTransformation) {
                EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                    OutputVariable::X,
                )
                .ok()
            } else {
                None
            },
            convert_relative_to_absolute: if is_relevant(ModeParameter::MakeAbsolute) {
                self.make_absolute.get()
            } else {
                false
            },
            current_absolute_value: UnitValue::MIN,
            previous_absolute_control_value: None,
        }
    }
}

pub fn convert_factor_to_unit_value(factor: i32) -> SoftSymmetricUnitValue {
    let result = if factor == 0 {
        0.01
    } else {
        factor as f64 / 100.0
    };
    SoftSymmetricUnitValue::new(result)
}

pub fn convert_unit_value_to_factor(value: SoftSymmetricUnitValue) -> i32 {
    // -1.00 => -100
    // -0.01 =>   -1
    //  0.00 =>    1
    //  0.01 =>    1
    //  1.00 =>  100
    let tmp = (value.get() * 100.0).round() as i32;
    if tmp == 0 {
        1
    } else {
        tmp
    }
}

fn convert_to_step_count(value: SoftSymmetricUnitValue) -> DiscreteIncrement {
    DiscreteIncrement::new(convert_unit_value_to_factor(value))
}

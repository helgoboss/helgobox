use crate::core::{prop, Prop};
use crate::domain::{EelTransformation, Mode, OutputVariable};

use helgoboss_learn::{
    full_unit_interval, AbsoluteMode, ButtonUsage, DiscreteIncrement, EncoderUsage, FireMode,
    Interval, OutOfRangeBehavior, PressDurationProcessor, SoftSymmetricUnitValue, TakeoverMode,
    UnitValue,
};

use rx_util::UnitEvent;

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
        }
    }
}

impl ModeModel {
    pub fn default_step_size_interval() -> Interval<SoftSymmetricUnitValue> {
        Interval::new(
            SoftSymmetricUnitValue::new(0.01),
            SoftSymmetricUnitValue::new(0.01),
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
        self.reverse.set(def.reverse.get());
        self.step_interval.set(def.step_interval.get());
        self.press_duration_interval
            .set(def.press_duration_interval.get());
        self.turbo_rate.set(def.turbo_rate.get());
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
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
    }

    /// Creates a mode reflecting this model's current values
    pub fn create_mode(&self, enforced_fire_mode: Option<FireMode>) -> Mode {
        Mode {
            absolute_mode: self.r#type.get(),
            source_value_interval: self.source_value_interval.get(),
            target_value_interval: self.target_value_interval.get(),
            step_count_interval: Interval::new(
                convert_to_step_count(self.step_interval.get_ref().min_val()),
                convert_to_step_count(self.step_interval.get_ref().max_val()),
            ),
            step_size_interval: self.positive_step_size_interval(),
            jump_interval: self.jump_interval.get(),
            press_duration_processor: PressDurationProcessor::new(
                enforced_fire_mode.unwrap_or_else(|| self.fire_mode.get()),
                self.press_duration_interval.get(),
                self.turbo_rate.get(),
            ),
            takeover_mode: self.takeover_mode.get(),
            encoder_usage: self.encoder_usage.get(),
            button_usage: self.button_usage.get(),
            reverse: self.reverse.get(),
            rotate: self.rotate.get(),
            increment_counter: 0,
            round_target_value: self.round_target_value.get(),
            out_of_range_behavior: self.out_of_range_behavior.get(),
            control_transformation: EelTransformation::compile(
                self.eel_control_transformation.get_ref(),
                OutputVariable::Y,
            )
            .ok(),
            feedback_transformation: EelTransformation::compile(
                self.eel_feedback_transformation.get_ref(),
                OutputVariable::X,
            )
            .ok(),
            convert_relative_to_absolute: self.make_absolute.get(),
            current_absolute_value: UnitValue::MIN,
            previous_absolute_control_value: None,
        }
    }

    pub fn supports_reverse(&self) -> bool {
        // For feedback always relevant
        true
    }

    pub fn supports_out_of_range_behavior(&self) -> bool {
        // For feedback always relevant
        true
    }

    pub fn supports_jump(&self) -> bool {
        self.r#type.get() == AbsoluteMode::Normal
    }

    pub fn supports_eel_control_transformation(&self) -> bool {
        self.r#type.get() == AbsoluteMode::Normal
    }

    pub fn supports_eel_feedback_transformation(&self) -> bool {
        true
    }

    pub fn supports_round_target_value(&self) -> bool {
        self.r#type.get() == AbsoluteMode::Normal
    }

    pub fn supports_takeover_mode(&self) -> bool {
        self.r#type.get() == AbsoluteMode::Normal
    }

    pub fn supports_steps(&self) -> bool {
        // No matter which absolute mode, incoming relative values always support this
        true
    }

    pub fn supports_rotate(&self) -> bool {
        // No matter which absolute mode, incoming relative values always support this
        true
    }

    pub fn supports_make_absolute(&self) -> bool {
        // No matter which absolute mode, incoming relative values always support this
        true
    }

    fn positive_step_size_interval(&self) -> Interval<UnitValue> {
        Interval::new_auto(
            self.step_interval.get_ref().min_val().abs(),
            self.step_interval.get_ref().max_val().abs(),
        )
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
    if tmp == 0 { 1 } else { tmp }
}

fn convert_to_step_count(value: SoftSymmetricUnitValue) -> DiscreteIncrement {
    DiscreteIncrement::new(convert_unit_value_to_factor(value))
}

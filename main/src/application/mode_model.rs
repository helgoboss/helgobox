use crate::core::{prop, Prop};
use crate::domain::{EelTransformation, Mode, OutputVariable};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    full_unit_interval, AbsoluteMode, DiscreteIncrement, Interval, PressDurationProcessor,
    RelativeMode, SymmetricUnitValue, ToggleMode, UnitValue,
};

use num_enum::{IntoPrimitive, TryFromPrimitive};
use rx_util::UnitEvent;
use serde_repr::*;
use std::time::Duration;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    // For all modes
    pub r#type: Prop<ModeType>,
    pub target_value_interval: Prop<Interval<UnitValue>>,
    // For absolute and relative mode
    pub source_value_interval: Prop<Interval<UnitValue>>,
    pub reverse: Prop<bool>,
    // For absolute and toggle mode
    pub press_duration_interval: Prop<Interval<Duration>>,
    // For absolute mode
    pub jump_interval: Prop<Interval<UnitValue>>,
    pub ignore_out_of_range_source_values: Prop<bool>,
    pub round_target_value: Prop<bool>,
    pub approach_target_value: Prop<bool>,
    pub eel_control_transformation: Prop<String>,
    pub eel_feedback_transformation: Prop<String>,
    // For relative mode
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
    pub step_interval: Prop<Interval<SymmetricUnitValue>>,
    pub rotate: Prop<bool>,
}

/// Type of a mode
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum ModeType {
    Absolute = 0,
    Relative = 1,
    Toggle = 2,
}

impl Default for ModeModel {
    fn default() -> Self {
        Self {
            r#type: prop(ModeType::Absolute),
            target_value_interval: prop(full_unit_interval()),
            source_value_interval: prop(full_unit_interval()),
            reverse: prop(false),
            press_duration_interval: prop(Interval::new(
                Duration::from_millis(0),
                Duration::from_millis(0),
            )),
            jump_interval: prop(full_unit_interval()),
            ignore_out_of_range_source_values: prop(false),
            round_target_value: prop(false),
            approach_target_value: prop(false),
            eel_control_transformation: prop(String::new()),
            eel_feedback_transformation: prop(String::new()),
            step_interval: prop(Self::default_step_size_interval()),
            rotate: prop(false),
        }
    }
}

impl ModeModel {
    pub fn default_step_size_interval() -> Interval<SymmetricUnitValue> {
        Interval::new(SymmetricUnitValue::new(0.01), SymmetricUnitValue::new(0.01))
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
        self.ignore_out_of_range_source_values
            .set(def.ignore_out_of_range_source_values.get());
        self.round_target_value.set(def.round_target_value.get());
        self.approach_target_value
            .set(def.approach_target_value.get());
        self.rotate.set(def.rotate.get());
        self.reverse.set(def.reverse.get());
        self.step_interval.set(def.step_interval.get());
        self.press_duration_interval
            .set(def.press_duration_interval.get());
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
        self.r#type
            .changed()
            .merge(self.target_value_interval.changed())
            .merge(self.source_value_interval.changed())
            .merge(self.reverse.changed())
            .merge(self.jump_interval.changed())
            .merge(self.ignore_out_of_range_source_values.changed())
            .merge(self.round_target_value.changed())
            .merge(self.approach_target_value.changed())
            .merge(self.eel_control_transformation.changed())
            .merge(self.eel_feedback_transformation.changed())
            .merge(self.step_interval.changed())
            .merge(self.rotate.changed())
            .merge(self.press_duration_interval.changed())
    }

    /// Creates a mode reflecting this model's current values
    pub fn create_mode(&self) -> Mode {
        use ModeType::*;
        match self.r#type.get() {
            Absolute => Mode::Absolute(AbsoluteMode {
                source_value_interval: self.source_value_interval.get(),
                target_value_interval: self.target_value_interval.get(),
                jump_interval: self.jump_interval.get(),
                press_duration_processor: PressDurationProcessor::new(
                    self.press_duration_interval.get(),
                ),
                approach_target_value: self.approach_target_value.get(),
                reverse_target_value: self.reverse.get(),
                round_target_value: self.round_target_value.get(),
                ignore_out_of_range_source_values: self.ignore_out_of_range_source_values.get(),
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
            }),
            Relative => Mode::Relative(RelativeMode {
                source_value_interval: self.source_value_interval.get(),
                step_count_interval: Interval::new(
                    convert_to_step_count(self.step_interval.get_ref().min_val()),
                    convert_to_step_count(self.step_interval.get_ref().max_val()),
                ),
                step_size_interval: self.positive_step_size_interval(),
                target_value_interval: self.target_value_interval.get(),
                reverse: self.reverse.get(),
                rotate: self.rotate.get(),
                increment_counter: 0,
                feedback_transformation: EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                    OutputVariable::X,
                )
                .ok(),
            }),
            Toggle => Mode::Toggle(ToggleMode {
                source_value_interval: self.source_value_interval.get(),
                target_value_interval: self.target_value_interval.get(),
                press_duration_processor: PressDurationProcessor::new(
                    self.press_duration_interval.get(),
                ),
                feedback_transformation: EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                    OutputVariable::X,
                )
                .ok(),
            }),
        }
    }

    pub fn supports_press_duration(&self) -> bool {
        use ModeType::*;
        matches!(self.r#type.get(), Absolute | Toggle)
    }

    pub fn supports_reverse(&self) -> bool {
        use ModeType::*;
        matches!(self.r#type.get(), Absolute | Relative)
    }

    pub fn supports_ignore_out_of_range_source_values(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_jump(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_eel_control_transformation(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_eel_feedback_transformation(&self) -> bool {
        true
    }

    pub fn supports_round_target_value(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_approach_target_value(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_steps(&self) -> bool {
        self.r#type.get() == ModeType::Relative
    }

    pub fn supports_rotate(&self) -> bool {
        self.r#type.get() == ModeType::Relative
    }

    fn positive_step_size_interval(&self) -> Interval<UnitValue> {
        Interval::new_auto(
            self.step_interval.get_ref().min_val().abs(),
            self.step_interval.get_ref().max_val().abs(),
        )
    }
}

pub fn convert_factor_to_unit_value(factor: i32) -> Result<SymmetricUnitValue, &'static str> {
    if factor < -100 || factor > 100 {
        return Err("invalid factor");
    }
    let result = if factor == 0 {
        0.01
    } else {
        factor as f64 / 100.0
    };
    Ok(SymmetricUnitValue::new(result))
}

pub fn convert_unit_value_to_factor(value: SymmetricUnitValue) -> i32 {
    // -1.00 => -100
    // -0.01 =>   -1
    //  0.00 =>    1
    //  0.01 =>    1
    //  1.00 =>  100
    let tmp = (value.get() * 100.0).round() as i32;
    if tmp == 0 { 1 } else { tmp }
}

fn convert_to_step_count(value: SymmetricUnitValue) -> DiscreteIncrement {
    DiscreteIncrement::new(convert_unit_value_to_factor(value))
}

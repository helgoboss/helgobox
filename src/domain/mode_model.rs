use crate::core::{prop, Prop};
use crate::domain::{EelTransformation, ReaperTarget, ResultVariable};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    full_unit_interval, AbsoluteMode, ControlValue, DiscreteIncrement, DiscreteValue, Interval,
    MidiClockTransportMessage, MidiSource, RelativeMode, SourceCharacter, Target, ToggleMode,
    Transformation, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde_repr::*;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    // For all modes
    pub r#type: Prop<ModeType>,
    pub target_value_interval: Prop<Interval<UnitValue>>,
    // For absolute and relative mode
    pub source_value_interval: Prop<Interval<UnitValue>>,
    pub reverse: Prop<bool>,
    // For absolute mode
    pub jump_interval: Prop<Interval<UnitValue>>,
    pub ignore_out_of_range_source_values: Prop<bool>,
    pub round_target_value: Prop<bool>,
    pub approach_target_value: Prop<bool>,
    pub eel_control_transformation: Prop<String>,
    pub eel_feedback_transformation: Prop<String>,
    // For relative mode
    pub step_size_interval: Prop<Interval<UnitValue>>,
    pub rotate: Prop<bool>,
    pub throttle: Prop<bool>,
}

// Represents a learn mode
#[derive(Clone, Debug)]
pub enum Mode {
    Absolute(AbsoluteMode<EelTransformation>),
    Relative(RelativeMode),
    Toggle(ToggleMode),
}

impl Mode {
    pub fn control(&mut self, value: ControlValue, target: &impl Target) -> Option<ControlValue> {
        use Mode::*;
        match self {
            Absolute(m) => m
                .control(value.as_absolute().ok()?, target)
                .map(ControlValue::Absolute),
            Relative(m) => m.control(value, target),
            Toggle(m) => m
                .control(value.as_absolute().ok()?, target)
                .map(ControlValue::Absolute),
        }
    }

    pub fn feedback(&self, value: UnitValue) -> UnitValue {
        use Mode::*;
        match self {
            Absolute(m) => m.feedback(value),
            Relative(m) => m.feedback(value),
            Toggle(m) => m.feedback(value),
        }
    }
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
            jump_interval: prop(full_unit_interval()),
            ignore_out_of_range_source_values: prop(false),
            round_target_value: prop(false),
            approach_target_value: prop(false),
            eel_control_transformation: prop(String::new()),
            eel_feedback_transformation: prop(String::new()),
            step_size_interval: prop(Self::default_step_size_interval()),
            rotate: prop(false),
            throttle: prop(false),
        }
    }
}

impl ModeModel {
    pub fn default_step_count_interval() -> Interval<DiscreteValue> {
        Interval::new(DiscreteValue::new(1), DiscreteValue::new(1))
    }

    pub fn default_step_size_interval() -> Interval<UnitValue> {
        Interval::new(UnitValue::new(0.01), UnitValue::new(0.01))
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
        self.step_size_interval.set(def.step_size_interval.get());
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
        self.r#type
            .changed()
            .merge(self.target_value_interval.changed())
            .merge(self.source_value_interval.changed())
            .merge(self.reverse.changed())
            .merge(self.throttle.changed())
            .merge(self.jump_interval.changed())
            .merge(self.ignore_out_of_range_source_values.changed())
            .merge(self.round_target_value.changed())
            .merge(self.approach_target_value.changed())
            .merge(self.eel_control_transformation.changed())
            .merge(self.eel_feedback_transformation.changed())
            .merge(self.step_size_interval.changed())
            .merge(self.rotate.changed())
    }

    /// Creates a mode reflecting this model's current values
    pub fn create_mode(&self, target: &ReaperTarget) -> Mode {
        use ModeType::*;
        match self.r#type.get() {
            Absolute => Mode::Absolute(AbsoluteMode {
                source_value_interval: self.source_value_interval.get(),
                target_value_interval: self.target_value_interval.get(),
                jump_interval: self.jump_interval.get(),
                approach_target_value: self.approach_target_value.get(),
                reverse_target_value: self.reverse.get(),
                round_target_value: self.round_target_value.get(),
                ignore_out_of_range_source_values: self.ignore_out_of_range_source_values.get(),
                control_transformation: EelTransformation::compile(
                    self.eel_control_transformation.get_ref(),
                    ResultVariable::Y,
                )
                .ok(),
                feedback_transformation: EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                    ResultVariable::X,
                )
                .ok(),
            }),
            Relative => Mode::Relative(RelativeMode {
                source_value_interval: self.source_value_interval.get(),
                step_count_interval: Interval::new(
                    self.convert_to_step_count(self.step_size_interval.get_ref().min()),
                    self.convert_to_step_count(self.step_size_interval.get_ref().max()),
                ),
                step_size_interval: self.step_size_interval.get(),
                target_value_interval: self.target_value_interval.get(),
                reverse: self.reverse.get(),
                rotate: self.rotate.get(),
                increment_counter: 0,
            }),
            Toggle => Mode::Toggle(ToggleMode {
                target_value_interval: self.target_value_interval.get(),
            }),
        }
    }

    pub fn convert_positive_factor_to_unit_value(
        &self,
        factor: u32,
    ) -> Result<UnitValue, &'static str> {
        if factor < 1 || factor > 100 {
            return Err("invalid step count");
        }
        // 1 to 100
        let values_count = 100;
        Ok(UnitValue::new(factor as f64 / (values_count - 1) as f64))
    }

    pub fn convert_unit_value_to_positive_factor(&self, unit_value: UnitValue) -> u32 {
        self.convert_to_step_count(unit_value).get().abs() as _
    }

    fn convert_to_step_count(&self, value: UnitValue) -> DiscreteIncrement {
        let inc = value.map_from_unit_interval_to_discrete_increment(&Interval::new(
            DiscreteIncrement::new(1),
            DiscreteIncrement::new(100),
        ));
        if self.throttle.get() {
            inc.inverse()
        } else {
            inc
        }
    }

    pub fn supports_reverse(&self) -> bool {
        use ModeType::*;
        matches!(self.r#type.get(), Absolute | Relative)
    }

    pub fn supports_throttle(&self, target_should_be_hit_with_increments: bool) -> bool {
        self.r#type.get() == ModeType::Relative && target_should_be_hit_with_increments
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
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_round_target_value(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_approach_target_value(&self) -> bool {
        self.r#type.get() == ModeType::Absolute
    }

    pub fn supports_step_size(&self) -> bool {
        self.r#type.get() == ModeType::Relative
    }

    pub fn supports_rotate(&self) -> bool {
        self.r#type.get() == ModeType::Relative
    }
}

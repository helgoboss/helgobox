use crate::domain::ReaperTarget;
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    full_unit_interval, AbsoluteMode, DiscreteValue, Interval, MidiClockTransportMessage,
    MidiSource, RelativeMode, SourceCharacter, ToggleMode, Transformation, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rx_util::{create_local_prop as p, LocalProp, LocalStaticProp, UnitEvent};
use rxrust::prelude::*;
use serde_repr::*;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    // For all modes
    pub r#type: LocalStaticProp<ModeType>,
    pub target_value_interval: LocalStaticProp<Interval<UnitValue>>,
    // For absolute and relative mode
    pub source_value_interval: LocalStaticProp<Interval<UnitValue>>,
    pub reverse: LocalStaticProp<bool>,
    // For absolute mode
    pub jump_interval: LocalStaticProp<Interval<UnitValue>>,
    pub ignore_out_of_range_source_values: LocalStaticProp<bool>,
    pub round_target_value: LocalStaticProp<bool>,
    pub approach_target_value: LocalStaticProp<bool>,
    pub eel_control_transformation: LocalStaticProp<String>,
    pub eel_feedback_transformation: LocalStaticProp<String>,
    // For relative mode
    pub step_size_interval: LocalStaticProp<Interval<UnitValue>>,
    pub rotate: LocalStaticProp<bool>,
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Debug)]
pub struct EelTransformation {}

impl EelTransformation {
    // Compiles the given script and creates an appropriate transformation.
    fn compile(eel_script: &str) -> Option<EelTransformation> {
        // TODO
        None
    }
}

impl Transformation for EelTransformation {
    fn transform(&self, input_value: UnitValue) -> Result<UnitValue, ()> {
        // TODO
        Err(())
    }
}

// Represents a learn mode
#[derive(Debug)]
pub enum Mode {
    Absolute(AbsoluteMode<EelTransformation>),
    Relative(RelativeMode),
    Toggle(ToggleMode),
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
            r#type: p(ModeType::Absolute),
            target_value_interval: p(full_unit_interval()),
            source_value_interval: p(full_unit_interval()),
            reverse: p(false),
            jump_interval: p(full_unit_interval()),
            ignore_out_of_range_source_values: p(false),
            round_target_value: p(false),
            approach_target_value: p(false),
            eel_control_transformation: p(String::new()),
            eel_feedback_transformation: p(String::new()),
            step_size_interval: p(Self::default_step_size_interval()),
            rotate: p(false),
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
                ),
                feedback_transformation: EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                ),
            }),
            Relative => Mode::Relative(RelativeMode {
                source_value_interval: self.source_value_interval.get(),
                step_count_interval: Interval::new(
                    make_discrete(self.step_size_interval.get_ref().min(), target),
                    make_discrete(self.step_size_interval.get_ref().max(), target),
                ),
                step_size_interval: self.step_size_interval.get(),
                target_value_interval: self.target_value_interval.get(),
                reverse: self.reverse.get(),
                rotate: self.rotate.get(),
            }),
            Toggle => Mode::Toggle(ToggleMode {
                target_value_interval: self.target_value_interval.get(),
            }),
        }
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

    pub fn supports_rotate_is_enabled(&self) -> bool {
        self.r#type.get() == ModeType::Relative
    }
}

fn make_discrete(value: UnitValue, target: &ReaperTarget) -> DiscreteValue {
    let discrete = target
        .convert_unit_value_to_discrete_value(value)
        .unwrap_or(1);
    DiscreteValue::new(discrete)
}

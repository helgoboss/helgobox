use helgoboss_learn::{
    AbsoluteMode, Interval, MidiClockTransportMessage, MidiSource, RelativeMode, SourceCharacter,
    ToggleMode, Transformation, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use rx_util::{create_local_prop as p, LocalProp, LocalStaticProp, SharedItemEvent};
use rxrust::prelude::*;
use serde_repr::*;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    // For all modes
    pub r#type: LocalStaticProp<ModeType>,
    pub min_target_value: LocalStaticProp<UnitValue>,
    pub max_target_value: LocalStaticProp<UnitValue>,
    // For absolute and relative mode
    pub min_source_value: LocalStaticProp<UnitValue>,
    pub max_source_value: LocalStaticProp<UnitValue>,
    pub reverse: LocalStaticProp<bool>,
    // For absolute mode
    pub min_jump: LocalStaticProp<UnitValue>,
    pub max_jump: LocalStaticProp<UnitValue>,
    pub ignore_out_of_range_source_values: LocalStaticProp<bool>,
    pub round_target_value: LocalStaticProp<bool>,
    pub approach_target_value: LocalStaticProp<bool>,
    pub eel_control_transformation: LocalStaticProp<String>,
    pub eel_feedback_transformation: LocalStaticProp<String>,
    // For relative mode
    pub min_step_size: LocalStaticProp<UnitValue>,
    pub max_step_size: LocalStaticProp<UnitValue>,
    pub rotate: LocalStaticProp<bool>,
}

/// Represents a value transformation done via EEL scripting language.
pub struct EelTransformation {}

impl EelTransformation {
    // Compiles the given script and creates an appropriate transformation.
    fn compile(eel_script: &str) -> Option<EelTransformation> {
        todo!()
    }
}

impl Transformation for EelTransformation {
    fn transform(&self, input_value: UnitValue) -> Result<UnitValue, ()> {
        todo!()
    }
}

// Represents a learn mode
pub enum Mode {
    Absolute(AbsoluteMode<EelTransformation>),
    Relative(RelativeMode),
    Toggle(ToggleMode),
}

/// Type of a mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ModeType {
    Absolute = 0,
    Relative = 1,
    Toggle = 2,
}

impl Default for ModeModel {
    fn default() -> Self {
        Self {
            r#type: p(ModeType::Absolute),
            min_target_value: p(UnitValue::MIN),
            max_target_value: p(UnitValue::MAX),
            min_source_value: p(UnitValue::MIN),
            max_source_value: p(UnitValue::MAX),
            reverse: p(false),
            min_jump: p(UnitValue::MIN),
            max_jump: p(UnitValue::MAX),
            ignore_out_of_range_source_values: p(false),
            round_target_value: p(false),
            approach_target_value: p(false),
            eel_control_transformation: p(String::new()),
            eel_feedback_transformation: p(String::new()),
            min_step_size: p(UnitValue::new(0.01)),
            max_step_size: p(UnitValue::new(0.01)),
            rotate: p(false),
        }
    }
}

impl ModeModel {
    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl SharedItemEvent<()> {
        self.r#type
            .changed()
            .merge(self.min_target_value.changed())
            .merge(self.max_target_value.changed())
            .merge(self.min_source_value.changed())
            .merge(self.max_source_value.changed())
            .merge(self.reverse.changed())
            .merge(self.min_jump.changed())
            .merge(self.max_jump.changed())
            .merge(self.ignore_out_of_range_source_values.changed())
            .merge(self.round_target_value.changed())
            .merge(self.approach_target_value.changed())
            .merge(self.eel_control_transformation.changed())
            .merge(self.eel_feedback_transformation.changed())
            .merge(self.min_step_size.changed())
            .merge(self.max_step_size.changed())
            .merge(self.rotate.changed())
    }

    /// Creates a mode reflecting this model's current values
    pub fn create_mode(&self) -> Mode {
        use ModeType::*;
        match self.r#type.get() {
            Absolute => Mode::Absolute(AbsoluteMode {
                source_value_interval: Interval::new(
                    self.min_source_value.get(),
                    self.max_source_value.get(),
                ),
                target_value_interval: Interval::new(
                    self.min_target_value.get(),
                    self.max_target_value.get(),
                ),
                jump_interval: Interval::new(self.min_jump.get(), self.max_jump.get()),
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
                source_value_interval: Interval::new(
                    self.min_source_value.get(),
                    self.max_source_value.get(),
                ),
                step_count_interval: todo!("needs to transform step size "),
                step_size_interval: Interval::new(
                    self.min_step_size.get(),
                    self.max_step_size.get(),
                ),
                target_value_interval: Interval::new(
                    self.min_target_value.get(),
                    self.max_target_value.get(),
                ),
                reverse: self.reverse.get(),
                rotate: self.rotate.get(),
            }),
            Toggle => Mode::Toggle(ToggleMode {
                target_value_interval: Interval::new(
                    self.min_target_value.get(),
                    self.max_target_value.get(),
                ),
            }),
        }
    }
}

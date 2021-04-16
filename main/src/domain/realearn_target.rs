use crate::domain::{
    FeedbackAudioHookTask, FeedbackOutput, InstanceState, OscFeedbackTask, RealTimeSender,
    TargetCharacter,
};
use helgoboss_learn::{ControlValue, UnitValue};
use std::cell::RefCell;
use std::rc::Rc;

pub trait RealearnTarget {
    fn character(&self) -> TargetCharacter;
    fn open(&self);
    /// Parses the given text as a target value and returns it as unit value.
    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str>;
    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str>;
    /// This converts the given normalized value to a discrete value.
    ///
    /// Used for displaying discrete target values in edit fields.
    /// Must be implemented for discrete targets only which don't support parsing according to
    /// `can_parse_values()`, e.g. FX preset. This target reports a step size. If we want to
    /// display an increment or a particular value in an edit field, we don't show normalized
    /// values of course but a discrete number, by using this function. Should be the reverse of
    /// `convert_discrete_value_to_unit_value()` because latter is used for parsing.
    ///
    /// In case the target wants increments, this takes 63 as the highest possible value.
    ///
    /// # Errors
    ///
    /// Returns an error if this target doesn't report a step size.
    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str>;
    /// Formats the given value without unit.
    fn format_value_without_unit(&self, value: UnitValue) -> String;
    /// Formats the given step size without unit.
    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String;
    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    fn hide_formatted_value(&self) -> bool;
    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    fn hide_formatted_step_size(&self) -> bool;
    fn value_unit(&self) -> &'static str;
    fn step_size_unit(&self) -> &'static str;
    /// Formats the value completely (including a possible unit).
    fn format_value(&self, value: UnitValue) -> String;
    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str>;
    fn can_report_current_value(&self) -> bool;
}

#[derive(Copy, Clone, Debug)]
pub struct ControlContext<'a> {
    pub feedback_audio_hook_task_sender: &'a RealTimeSender<FeedbackAudioHookTask>,
    pub osc_feedback_task_sender: &'a crossbeam_channel::Sender<OscFeedbackTask>,
    pub feedback_output: Option<FeedbackOutput>,
    pub instance_state: &'a Rc<RefCell<InstanceState>>,
}

use crate::domain::ui_util::{format_as_percentage_without_unit, parse_unit_value_from_percentage};
use crate::domain::{
    AdditionalFeedbackEvent, FeedbackAudioHookTask, FeedbackOutput, InstanceFeedbackEvent,
    InstanceId, MainMapping, OrderedMappingMap, OscFeedbackTask, RealTimeReaperTarget,
    RealTimeSender, SharedInstanceState, TargetCharacter, TrackExclusivity,
};
use enum_dispatch::enum_dispatch;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Reaper, Track, TrackRoute};
use reaper_medium::CommandId;
use std::convert::TryInto;

#[enum_dispatch(ReaperTarget)]
pub trait RealearnTarget {
    fn character(&self) -> TargetCharacter {
        self.control_type_and_character().1
    }
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter);
    fn open(&self) {
        if let Some(fx) = self.fx() {
            fx.show_in_floating_window();
            return;
        }
        if let Some(track) = self.track() {
            track.select_exclusively();
            // Scroll to track
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()));
        }
    }
    /// Parses the given text as a target value and returns it as unit value.
    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_unit_value_from_percentage(text)
    }
    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_unit_value_from_percentage(text)
    }
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
    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        if self.control_type_and_character().0.is_relative() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        let _ = input;
        Err("not supported")
    }
    /// Formats the given value without unit.
    fn format_value_without_unit(&self, value: UnitValue) -> String {
        self.format_as_discrete_or_percentage(value)
    }
    /// Formats the given step size without unit.
    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        self.format_as_discrete_or_percentage(step_size)
    }
    /// Reusable function
    fn format_as_discrete_or_percentage(&self, value: UnitValue) -> String {
        if self.character() == TargetCharacter::Discrete {
            self.convert_unit_value_to_discrete_value(value)
                .map(|v| v.to_string())
                .unwrap_or_default()
        } else {
            format_as_percentage_without_unit(value)
        }
    }
    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    fn hide_formatted_value(&self) -> bool {
        false
    }
    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    fn hide_formatted_step_size(&self) -> bool {
        false
    }
    fn value_unit(&self) -> &'static str {
        if self.character() == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    fn step_size_unit(&self) -> &'static str {
        if self.character() == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    /// Formats the value completely (including a possible unit).
    fn format_value(&self, value: UnitValue) -> String {
        self.format_value_generic(value)
    }

    fn format_value_generic(&self, value: UnitValue) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value),
            self.value_unit()
        )
    }
    fn hit(
        &mut self,
        value: ControlValue,
        context: ControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let (_, _) = (value, context);
        Err("not supported")
    }
    fn can_report_current_value(&self) -> bool {
        // We will quickly realize if not.
        true
    }
    fn is_available(&self) -> bool;
    fn project(&self) -> Option<Project> {
        None
    }
    fn track(&self) -> Option<&Track> {
        None
    }
    fn fx(&self) -> Option<&Fx> {
        None
    }
    fn route(&self) -> Option<&TrackRoute> {
        None
    }
    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        None
    }
    fn supports_automatic_feedback(&self) -> bool {
        // Usually yes. We will quickly realize if not.
        true
    }

    /// Might return the new value if changed.
    ///
    /// Is called in any case (even if feedback not enabled). So we can use it for general-purpose
    /// change event reactions such as reacting to transport stop.
    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        control_context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        let (_, _) = (evt, control_context);
        (false, None)
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        let _ = evt;
        (false, None)
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        let _ = evt;
        (false, None)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        None
    }

    /// Like `convert_unit_value_to_discrete_value()` but in the other direction.
    ///
    /// Used for parsing discrete values of discrete targets that can't do real parsing according to
    /// `can_parse_values()`.
    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
        if self.control_type_and_character().0.is_relative() {
            return (value as f64 / 63.0).try_into();
        }
        let _ = value;
        Err("not supported")
    }

    fn parse_value_from_discrete_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(text.parse().map_err(|_| "not a discrete value")?)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ControlContext<'a> {
    pub feedback_audio_hook_task_sender: &'a RealTimeSender<FeedbackAudioHookTask>,
    pub osc_feedback_task_sender: &'a crossbeam_channel::Sender<OscFeedbackTask>,
    pub feedback_output: Option<FeedbackOutput>,
    pub instance_state: &'a SharedInstanceState,
    pub instance_id: &'a InstanceId,
    pub output_logging_enabled: bool,
}

pub type HitInstructionReturnValue = Option<Box<dyn HitInstruction>>;

pub trait HitInstruction {
    fn execute(&self, context: HitInstructionContext);
}

pub struct HitInstructionContext<'a> {
    pub mappings: &'a mut OrderedMappingMap<MainMapping>,
    pub control_context: ControlContext<'a>,
}

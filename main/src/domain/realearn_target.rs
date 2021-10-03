use crate::domain::ui_util::{
    format_as_percentage_without_unit, format_raw_midi, log_output,
    parse_unit_value_from_percentage, OutputReason,
};
use crate::domain::{
    AdditionalFeedbackEvent, DomainEventHandler, Exclusivity, ExtendedProcessorContext,
    FeedbackAudioHookTask, FeedbackOutput, GroupId, InstanceId, InstanceStateChanged, MainMapping,
    MappingControlResult, MappingId, OrderedMappingMap, OscFeedbackTask, ProcessorContext,
    RealTimeReaperTarget, RealTimeSender, SharedInstanceState, Tag, TagScope, TargetCharacter,
    TrackExclusivity,
};
use enum_dispatch::enum_dispatch;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, RawMidiEvent, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Reaper, Track, TrackRoute};
use reaper_medium::{CommandId, MidiOutputDeviceId};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt::Debug;

#[enum_dispatch(ReaperTarget)]
pub trait RealearnTarget {
    // TODO-low Instead of taking the ControlContext as parameter in each method, we could also
    //  choose to implement RealearnTarget for a wrapper that contains the control context.
    //  We did this with ValueFormatter and ValueParser.
    fn character(&self, context: ControlContext) -> TargetCharacter {
        self.control_type_and_character(context).1
    }

    fn control_type_and_character(&self, context: ControlContext)
        -> (ControlType, TargetCharacter);

    fn open(&self, context: ControlContext) {
        let _ = context;
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
    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
        parse_unit_value_from_percentage(text)
    }
    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
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
    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        if self.control_type_and_character(context).0.is_relative() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        let _ = input;
        Err("not supported")
    }
    /// Formats the given value without unit.
    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_as_discrete_or_percentage(value, context)
    }
    /// Formats the given step size without unit.
    fn format_step_size_without_unit(
        &self,
        step_size: UnitValue,
        context: ControlContext,
    ) -> String {
        self.format_as_discrete_or_percentage(step_size, context)
    }
    /// Reusable function
    fn format_as_discrete_or_percentage(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> String {
        if self.character(context) == TargetCharacter::Discrete {
            self.convert_unit_value_to_discrete_value(value, context)
                .map(|v| v.to_string())
                .unwrap_or_default()
        } else {
            format_as_percentage_without_unit(value)
        }
    }
    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    fn hide_formatted_value(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }
    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    fn hide_formatted_step_size(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }
    fn value_unit(&self, context: ControlContext) -> &'static str {
        if self.character(context) == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    fn step_size_unit(&self, context: ControlContext) -> &'static str {
        if self.character(context) == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    /// Formats the value completely (including a possible unit).
    fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_value_generic(value, context)
    }

    fn format_value_generic(&self, value: UnitValue, context: ControlContext) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value, context),
            self.value_unit(context)
        )
    }
    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let (_, _) = (value, context);
        Err("not supported")
    }

    fn can_report_current_value(&self) -> bool {
        // We will quickly realize if not.
        true
    }

    fn is_available(&self, context: ControlContext) -> bool;

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

    /// Whether the target supports automatic feedback in response to some events or polling.
    ///
    /// If the target supports automatic feedback, you are left with a choice:
    ///
    /// - a) Using polling (continuously poll the target value).
    /// - b) Setting this to `false`.
    ///
    /// Choose (a) if the target value is a real, global target value that also can affect
    /// other mappings. Polling is obviously not the optimal choice because of the performance
    /// drawback ... but at least multiple mappings can participate.
    ///
    /// Choose (b) is if the target value is not global but artificial, that is, attached to the
    /// mapping itself - and can therefore not have any effect on other mappings. This is also
    /// not the optimal choice because other mappings can't participate in the feedback value ...
    /// but at least it's fast.
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
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        let (_, _) = (evt, context);
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
        evt: &InstanceStateChanged,
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
    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        if self.control_type_and_character(context).0.is_relative() {
            return (value as f64 / 63.0).try_into();
        }
        let _ = value;
        Err("not supported")
    }

    fn parse_value_from_discrete_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(
            text.parse().map_err(|_| "not a discrete value")?,
            context,
        )
    }
}

pub trait InstanceContainer: Debug {
    /// Returns activated tags if they don't correspond to the tags in the args.
    fn enable_instances(&self, args: EnableInstancesArgs) -> Option<HashSet<Tag>>;
}

pub struct EnableInstancesArgs<'a> {
    pub initiator_instance_id: InstanceId,
    /// `None` if monitoring FX.
    pub initiator_project: Option<Project>,
    pub scope: &'a TagScope,
    pub is_enable: bool,
    pub exclusivity: Exclusivity,
}

#[derive(Copy, Clone, Debug)]
pub struct ControlContext<'a> {
    pub feedback_audio_hook_task_sender: &'a RealTimeSender<FeedbackAudioHookTask>,
    pub osc_feedback_task_sender: &'a crossbeam_channel::Sender<OscFeedbackTask>,
    pub feedback_output: Option<FeedbackOutput>,
    pub instance_container: &'a dyn InstanceContainer,
    pub instance_state: &'a SharedInstanceState,
    pub instance_id: &'a InstanceId,
    pub output_logging_enabled: bool,
    pub processor_context: &'a ProcessorContext,
}

impl<'a> ControlContext<'a> {
    pub fn send_raw_midi(
        &self,
        reason: OutputReason,
        dev_id: MidiOutputDeviceId,
        events: Vec<RawMidiEvent>,
    ) {
        if self.output_logging_enabled {
            for e in &events {
                log_output(self.instance_id, reason, format_raw_midi(e.bytes()));
            }
        }
        let _ = self
            .feedback_audio_hook_task_sender
            .send(FeedbackAudioHookTask::SendMidi(dev_id, events))
            .unwrap();
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingControlContext<'a> {
    pub control_context: ControlContext<'a>,
    pub mapping_data: MappingData,
}

#[derive(Copy, Clone, Debug)]
pub struct MappingData {
    pub mapping_id: MappingId,
    pub group_id: GroupId,
}

pub type HitInstructionReturnValue = Option<Box<dyn HitInstruction>>;

pub trait HitInstruction {
    fn execute(self: Box<Self>, context: HitInstructionContext) -> Vec<MappingControlResult>;
}

pub struct HitInstructionContext<'a> {
    /// All mappings in the relevant compartment.
    pub mappings: &'a mut OrderedMappingMap<MainMapping>,
    // TODO-medium This became part of ExtendedProcessorContext, so redundant (not just here BTW)
    pub control_context: ControlContext<'a>,
    pub domain_event_handler: &'a dyn DomainEventHandler,
    pub logger: &'a slog::Logger,
    pub processor_context: ExtendedProcessorContext<'a>,
}

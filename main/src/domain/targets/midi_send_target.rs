use crate::base::NamedChannelSender;
use crate::domain::{
    Compartment, ControlContext, ExtendedProcessorContext, FeedbackAudioHookTask, FeedbackOutput,
    FeedbackRealTimeTask, HitInstructionReturnValue, MappingControlContext, MidiDestination,
    RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType, SendMidiDestination,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    create_raw_midi_events_singleton, AbsoluteValue, ControlType, ControlValue, Fraction,
    MidiSourceValue, RawMidiPattern, Target, UnitValue,
};
use std::convert::TryInto;

#[derive(Debug)]
pub struct UnresolvedMidiSendTarget {
    pub pattern: RawMidiPattern,
    pub destination: SendMidiDestination,
}

impl UnresolvedReaperTargetDef for UnresolvedMidiSendTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::SendMidi(MidiSendTarget::new(
            self.pattern.clone(),
            self.destination,
        ))])
    }

    fn can_be_affected_by_change_events(&self) -> bool {
        // We don't want to be refreshed because we maintain an artificial value.
        false
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiSendTarget {
    pattern: RawMidiPattern,
    destination: SendMidiDestination,
    // For making basic toggle/relative control possible.
    artificial_value: AbsoluteValue,
}

impl MidiSendTarget {
    pub fn new(pattern: RawMidiPattern, destination: SendMidiDestination) -> Self {
        let max_discrete_value = pattern.max_discrete_value();
        Self {
            pattern,
            destination,
            artificial_value: AbsoluteValue::Discrete(Fraction::new(0, max_discrete_value as _)),
        }
    }

    pub fn pattern(&self) -> &RawMidiPattern {
        &self.pattern
    }

    pub fn destination(&self) -> SendMidiDestination {
        self.destination
    }

    pub fn set_artificial_value(&mut self, value: AbsoluteValue) {
        self.artificial_value = value;
    }

    fn control_type_and_character_simple(&self) -> (ControlType, TargetCharacter) {
        match self.pattern.step_size() {
            None => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            Some(step_size) => {
                if self.pattern.resolution() == 1 {
                    (
                        ControlType::AbsoluteContinuousRetriggerable,
                        TargetCharacter::Switch,
                    )
                } else {
                    (
                        ControlType::AbsoluteDiscrete {
                            atomic_step_size: step_size,
                            is_retriggerable: true,
                        },
                        TargetCharacter::Discrete,
                    )
                }
            }
        }
    }
}

impl RealearnTarget for MidiSendTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        self.control_type_and_character_simple()
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        _: ControlContext,
    ) -> Result<u32, &'static str> {
        let step_size = self.pattern.step_size().ok_or("not supported")?;
        let discrete_value = (input.get() / step_size.get()).round() as _;
        Ok(discrete_value)
    }

    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(value, context) {
            discrete_value.to_string()
        } else {
            "0".to_owned()
        }
    }

    fn format_step_size_without_unit(
        &self,
        step_size: UnitValue,
        context: ControlContext,
    ) -> String {
        if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(step_size, context) {
            discrete_value.to_string()
        } else {
            "0".to_owned()
        }
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        ""
    }

    fn step_size_unit(&self, _: ControlContext) -> &'static str {
        ""
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_absolute_value()?;
        // We arrive here only if controlled via OSC, group interaction (as follower), mapping
        // snapshot or autoload. Sending MIDI in response to incoming MIDI messages is handled
        // directly in the real-time processor.
        let resolved_destination = match self.destination {
            SendMidiDestination::FxOutput => MidiDestination::FxOutput,
            SendMidiDestination::FeedbackOutput => {
                let feedback_output = context
                    .control_context
                    .feedback_output
                    .ok_or("no feedback output set")?;
                if let FeedbackOutput::Midi(dest) = feedback_output {
                    dest
                } else {
                    return Err("feedback output is not MIDI");
                }
            }
        };
        self.artificial_value = value;
        let raw_midi_events =
            create_raw_midi_events_singleton(self.pattern.to_concrete_midi_event(value));
        context
            .control_context
            .log_outgoing_target_midi(&raw_midi_events);
        match resolved_destination {
            MidiDestination::FxOutput => {
                let source_value = MidiSourceValue::Raw {
                    feedback_address_info: None,
                    events: raw_midi_events,
                };
                context
                    .control_context
                    .feedback_real_time_task_sender
                    .send_complaining(FeedbackRealTimeTask::FxOutputFeedback(source_value));
            }
            MidiDestination::Device(dev_id) => {
                context
                    .control_context
                    .feedback_audio_hook_task_sender
                    .send_complaining(FeedbackAudioHookTask::SendMidi(dev_id, raw_midi_events));
            }
        };
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let unit_value = if let Some(step_size) = self.pattern.step_size() {
            (value as f64 * step_size.get()).try_into()?
        } else {
            UnitValue::MIN
        };
        Ok(unit_value)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        Some(RealTimeReaperTarget::SendMidi(self.clone()))
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

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::SendMidi)
    }
}

impl<'a> Target<'a> for MidiSendTarget {
    type Context = ();

    fn current_value(&self, _context: ()) -> Option<AbsoluteValue> {
        Some(self.artificial_value)
    }

    fn control_type(&self, _: Self::Context) -> ControlType {
        self.control_type_and_character_simple().0
    }
}

pub const MIDI_SEND_TARGET: TargetTypeDef = TargetTypeDef {
    name: "MIDI: Send message",
    short_name: "Send MIDI",
    supports_feedback: false,
    ..DEFAULT_TARGET
};

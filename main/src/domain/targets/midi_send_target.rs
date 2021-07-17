use crate::domain::ui_util::{format_raw_midi_event, log_target_output};
use crate::domain::{
    ControlContext, FeedbackAudioHookTask, FeedbackOutput, MidiDestination, RealTimeReaperTarget,
    RealearnTarget, SendMidiDestination, TargetCharacter,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, RawMidiPattern, Target, UnitValue,
};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq)]
pub struct MidiSendTarget {
    pub pattern: RawMidiPattern,
    pub destination: SendMidiDestination,
}

impl RealearnTarget for MidiSendTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
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
                        },
                        TargetCharacter::Discrete,
                    )
                }
            }
        }
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        let step_size = self.pattern.step_size().ok_or("not supported")?;
        let discrete_value = (input.get() / step_size.get()).round() as _;
        Ok(discrete_value)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(value) {
            discrete_value.to_string()
        } else {
            "0".to_owned()
        }
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(step_size) {
            discrete_value.to_string()
        } else {
            "0".to_owned()
        }
    }

    fn value_unit(&self) -> &'static str {
        ""
    }

    fn step_size_unit(&self) -> &'static str {
        ""
    }

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        // We arrive here only if controlled via OSC. Sending MIDI in response to incoming
        // MIDI messages is handled directly in the real-time processor.
        let raw_midi_event = self
            .pattern
            .to_concrete_midi_event(value.to_absolute_value()?);
        match self.destination {
            SendMidiDestination::FxOutput => Err("OSC => MIDI FX output not supported"),
            SendMidiDestination::FeedbackOutput => {
                let feedback_output = context.feedback_output.ok_or("no feedback output set")?;
                if let FeedbackOutput::Midi(MidiDestination::Device(dev_id)) = feedback_output {
                    if context.output_logging_enabled {
                        log_target_output(
                            context.instance_id,
                            format_raw_midi_event(&raw_midi_event),
                        );
                    }
                    let _ = context
                        .feedback_audio_hook_task_sender
                        .send(FeedbackAudioHookTask::SendMidi(
                            dev_id,
                            Box::new(raw_midi_event),
                        ))
                        .unwrap();
                    Ok(())
                } else {
                    Err("feedback output is not a MIDI device")
                }
            }
        }
    }

    fn can_report_current_value(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        true
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }

    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
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

    fn parse_value_from_discrete_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(text.parse().map_err(|_| "not a discrete value")?)
    }
}

impl<'a> Target<'a> for MidiSendTarget {
    type Context = ();

    fn current_value(&self, _context: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

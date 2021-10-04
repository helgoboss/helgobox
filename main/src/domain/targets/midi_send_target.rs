use crate::domain::ui_util::OutputReason;
use crate::domain::{
    ControlContext, FeedbackOutput, HitInstructionReturnValue, MappingControlContext,
    MidiDestination, RealTimeReaperTarget, RealearnTarget, SendMidiDestination, TargetCharacter,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, RawMidiPattern, Target, UnitValue,
};
use std::convert::TryInto;

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
        let raw_midi_event = self.pattern.to_concrete_midi_event(value);
        let result = match self.destination {
            SendMidiDestination::FxOutput => Err("OSC => MIDI FX output not supported"),
            SendMidiDestination::FeedbackOutput => {
                let feedback_output = context
                    .control_context
                    .feedback_output
                    .ok_or("no feedback output set")?;
                if let FeedbackOutput::Midi(MidiDestination::Device(dev_id)) = feedback_output {
                    context.control_context.send_raw_midi(
                        OutputReason::Target,
                        dev_id,
                        vec![raw_midi_event],
                    );
                    Ok(None)
                } else {
                    Err("feedback output is not a MIDI device")
                }
            }
        };
        if result.is_ok() {
            self.artificial_value = value;
        }
        result
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

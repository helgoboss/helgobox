use crate::domain::{
    real_time_processor, Caller, CompartmentKind, ControlContext, ControlMainTask,
    ExtendedProcessorContext, FeedbackAudioHookTask, FeedbackOutput, FeedbackRealTimeTask,
    HitResponse, LogOptions, MappingControlContext, MidiDestination, MidiEvent,
    MidiTransformationContainer, RealTimeReaperTarget, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use base::{NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use helgoboss_learn::{
    create_raw_midi_events_singleton, AbsoluteValue, ControlType, ControlValue, Fraction,
    MidiSourceValue, RawMidiPattern, Target, UnitValue,
};
use helgobox_allocator::permit_alloc;
use helgobox_api::persistence::SendMidiDestination;
use reaper_high::MidiOutputDevice;
use reaper_medium::{MidiInputDeviceId, SendMidiTime};
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
        _: CompartmentKind,
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

#[derive(Clone, Debug, Eq, PartialEq)]
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

    #[allow(clippy::too_many_arguments)]
    pub fn midi_send_target_send_midi_in_rt_thread(
        &mut self,
        caller: Caller,
        control_value: ControlValue,
        midi_feedback_output: Option<MidiDestination>,
        log_options: LogOptions,
        main_task_sender: &SenderToNormalThread<ControlMainTask>,
        rt_feedback_sender: &SenderToRealTimeThread<FeedbackRealTimeTask>,
        value_event: MidiEvent<ControlValue>,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> Result<(), &'static str> {
        let v = control_value.to_absolute_value()?;
        // This is a type of mapping that we should process right here because we want to
        // send a MIDI message and this needs to happen in the audio thread.
        // Going to the main thread and back would be such a waste!
        let raw_midi_event = match self.destination {
            SendMidiDestination::FxOutput | SendMidiDestination::FeedbackOutput => {
                // The frame offset of the RawMidiEvent is irrelevant in this case. We pass it in other ways.
                let raw_midi_event = self.pattern().to_concrete_midi_event(0, v);
                let midi_destination = match caller {
                    Caller::Vst(_) => match self.destination() {
                        SendMidiDestination::FxOutput => MidiDestination::FxOutput,
                        SendMidiDestination::FeedbackOutput => {
                            midi_feedback_output.ok_or("no feedback output set")?
                        }
                        _ => unreachable!(),
                    },
                    Caller::AudioHook => match self.destination() {
                        SendMidiDestination::FxOutput => MidiDestination::FxOutput,
                        SendMidiDestination::FeedbackOutput => {
                            midi_feedback_output.ok_or("no feedback output set")?
                        }
                        _ => unreachable!(),
                    },
                };
                match midi_destination {
                    MidiDestination::FxOutput => {
                        match caller {
                            Caller::Vst(_) => {
                                real_time_processor::send_raw_midi_to_fx_output(
                                    raw_midi_event.bytes(),
                                    value_event.offset(),
                                    caller,
                                );
                            }
                            Caller::AudioHook => {
                                // We can't send to FX output here directly. Need to wait until VST processing
                                // starts (same processing cycle).
                                rt_feedback_sender.send_complaining(
                                    FeedbackRealTimeTask::NonAllocatingFxOutputFeedback(
                                        raw_midi_event,
                                        value_event.offset(),
                                    ),
                                );
                            }
                        }
                    }
                    MidiDestination::Device(dev_id) => {
                        MidiOutputDevice::new(dev_id).with_midi_output(
                            |mo| -> Result<(), &'static str> {
                                let mo = mo.ok_or("couldn't open MIDI output device")?;
                                let sample_offset = value_event.offset().get() as u32;
                                mo.send_msg(
                                    raw_midi_event,
                                    SendMidiTime::AtFrameOffset(sample_offset),
                                );
                                Ok(())
                            },
                        )?;
                    }
                };
                raw_midi_event
            }
            SendMidiDestination::InputDevice(d) => {
                // The frame offset of the RawMidiEvent is relevant here. It is supposed to be provided in
                // MIDI input device frames (1/1024000s of a second), *not* in sample frames! At the very beginning of
                // the signal flow we normalized the MIDI input device frames to sample frames according to the
                // current device sample rate. Simply because in most cases that's what we need. But now we need
                // MIDI input device frames again, so we need to convert it back LOL.
                let container = transformation_container.as_mut().ok_or(
                    "can't send to device input when MIDI doesn't come from device directly",
                )?;
                let midi_frame = value_event
                    .offset()
                    .to_midi_input_frame_offset(container.current_device_sample_rate());
                let raw_midi_event = self.pattern().to_concrete_midi_event(midi_frame, v);
                let dev_id = d.device_id.map(MidiInputDeviceId::new);
                container.push(dev_id, raw_midi_event);
                raw_midi_event
            }
        };
        // We end up here only if the message was successfully sent
        self.artificial_value = v;
        if log_options.output_logging_enabled {
            permit_alloc(|| {
                main_task_sender.send_complaining(ControlMainTask::LogTargetOutput {
                    event: Box::new(raw_midi_event),
                });
            });
        }
        Ok(())
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
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_absolute_value()?;
        // We arrive here only if controlled via OSC, group interaction (as follower), mapping
        // snapshot or autoload. Sending MIDI in response to incoming MIDI messages is handled
        // directly in the real-time processor.
        let resolved_destination =
            match self.destination {
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
                SendMidiDestination::InputDevice(_) => return Err(
                    "sending to device input is only possible in response to a MIDI source event coming from a MIDI device",
                ),
            };
        self.artificial_value = value;
        let raw_midi_events =
            create_raw_midi_events_singleton(self.pattern.to_concrete_midi_event(0, value));
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
        Ok(HitResponse::processed_with_effect())
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

impl Target<'_> for MidiSendTarget {
    type Context = ();

    fn current_value(&self, _context: ()) -> Option<AbsoluteValue> {
        Some(self.artificial_value)
    }

    fn control_type(&self, _: Self::Context) -> ControlType {
        self.control_type_and_character_simple().0
    }
}

pub const MIDI_SEND_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Midi,
    name: "Send message",
    short_name: "Send MIDI",
    supports_feedback: false,
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

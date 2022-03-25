use crate::application::{MidiSourceType, ReaperSourceType, SourceCategory};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind, convert_osc_argument, ConversionStyle,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::SourceModelData;
use helgoboss_learn::{
    DisplayType, MackieSevenSegmentDisplayScope, MidiClockTransportMessage, SourceCharacter,
};
use helgoboss_midi::{Channel, U14};
use realearn_api::schema;
use std::convert::TryInto;

pub struct NewSourceProps {
    pub prevent_echo_feedback: bool,
    pub send_feedback_after_control: bool,
}

pub fn convert_source(
    data: SourceModelData,
    new_source_props: NewSourceProps,
    style: ConversionStyle,
) -> ConversionResult<schema::Source> {
    let feedback_behavior = {
        use schema::FeedbackBehavior as T;
        let v = if new_source_props.prevent_echo_feedback {
            // Took precedence if both checkboxes were ticked (was possible in ReaLearn < 2.10.0).
            T::PreventEchoFeedback
        } else if new_source_props.send_feedback_after_control {
            T::SendFeedbackAfterControl
        } else {
            T::Normal
        };
        style.required_value(v)
    };
    use SourceCategory::*;
    let source = match data.category {
        Never => schema::Source::NoneSource,
        Midi => {
            use MidiSourceType::*;
            match data.r#type {
                ControlChangeValue => {
                    let s = schema::MidiControlChangeValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        controller_number: convert_controller_number(data.number),
                        character: convert_character(data.character, style),
                        fourteen_bit: data.is_14_bit,
                    };
                    schema::Source::MidiControlChangeValue(s)
                }
                NoteVelocity => {
                    let s = schema::MidiNoteVelocitySource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        key_number: convert_key_number(data.number),
                    };
                    schema::Source::MidiNoteVelocity(s)
                }
                NoteKeyNumber => {
                    let s = schema::MidiNoteKeyNumberSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiNoteKeyNumber(s)
                }
                PitchBendChangeValue => {
                    let s = schema::MidiPitchBendChangeValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiPitchBendChangeValue(s)
                }
                ChannelPressureAmount => {
                    let s = schema::MidiChannelPressureAmountSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiChannelPressureAmount(s)
                }
                ProgramChangeNumber => {
                    let s = schema::MidiProgramChangeNumberSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiProgramChangeNumber(s)
                }
                ParameterNumberValue => {
                    let s = schema::MidiParameterNumberValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        number: convert_parameter_number(data.number),
                        fourteen_bit: data.is_14_bit,
                        registered: data.is_registered,
                        character: convert_character(data.character, style),
                    };
                    schema::Source::MidiParameterNumberValue(s)
                }
                PolyphonicKeyPressureAmount => {
                    let s = schema::MidiPolyphonicKeyPressureAmountSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        key_number: convert_key_number(data.number),
                    };
                    schema::Source::MidiPolyphonicKeyPressureAmount(s)
                }
                ClockTempo => {
                    let s = schema::MidiClockTempoSource;
                    schema::Source::MidiClockTempo(s)
                }
                ClockTransport => {
                    let s = schema::MidiClockTransportSource {
                        message: convert_transport_msg(data.message),
                    };
                    schema::Source::MidiClockTransport(s)
                }
                Raw => {
                    let s = schema::MidiRawSource {
                        feedback_behavior,
                        pattern: style.required_value(data.raw_midi_pattern),
                        character: convert_character(data.character, style),
                    };
                    schema::Source::MidiRaw(s)
                }
                Script => {
                    let s = schema::MidiScriptSource {
                        script: style.required_value(data.midi_script),
                    };
                    schema::Source::MidiScript(s)
                }
                Display => {
                    use DisplayType::*;
                    match data.display_type {
                        MackieLcd => {
                            let s = schema::MackieLcdSource {
                                channel: data.display_id,
                                line: data.line,
                            };
                            schema::Source::MackieLcd(s)
                        }
                        MackieSevenSegmentDisplay => {
                            let s = schema::MackieSevenSegmentDisplaySource {
                                scope: data.display_id.and_then(|id| {
                                    convert_mackie_seven_segment_display_scope(
                                        (id as usize).try_into().ok()?,
                                    )
                                }),
                            };
                            schema::Source::MackieSevenSegmentDisplay(s)
                        }
                        SiniConE24 => {
                            let s = schema::SiniConE24DisplaySource {
                                cell_index: data.display_id,
                                item_index: data.line,
                            };
                            schema::Source::SiniConE24Display(s)
                        }
                        LaunchpadProScrollingText => {
                            let s = schema::LaunchpadProScrollingTextDisplaySource;
                            schema::Source::LaunchpadProScrollingTextDisplay(s)
                        }
                    }
                }
            }
        }
        Osc => {
            let s = schema::OscSource {
                feedback_behavior,
                address: style.required_value(data.osc_address_pattern),
                argument: convert_osc_argument(data.osc_arg_index, data.osc_arg_type, style),
                relative: style.required_value_with_default(
                    data.osc_arg_is_relative,
                    defaults::SOURCE_OSC_IS_RELATIVE,
                ),
                feedback_arguments: style.required_value(data.osc_feedback_args),
            };
            schema::Source::Osc(s)
        }
        Reaper => {
            use ReaperSourceType::*;
            match data.reaper_source_type {
                MidiDeviceChanges => {
                    schema::Source::MidiDeviceChanges(schema::MidiDeviceChangesSource)
                }
                RealearnInstanceStart => {
                    schema::Source::RealearnInstanceStart(schema::RealearnInstanceStartSource)
                }
                Timer => schema::Source::Timer(schema::TimerSource {
                    duration: data.timer_millis,
                }),
            }
        }
        Virtual => {
            let s = schema::VirtualSource {
                id: convert_control_element_id(data.control_element_index),
                character: convert_control_element_kind(data.control_element_type, style),
            };
            schema::Source::Virtual(s)
        }
        Keyboard => todo!(),
    };
    Ok(source)
}

fn convert_channel(v: Option<Channel>) -> Option<u8> {
    Some(v?.get())
}

fn convert_controller_number(v: Option<U14>) -> Option<u8> {
    Some(v?.get() as _)
}

fn convert_parameter_number(v: Option<U14>) -> Option<u16> {
    Some(v?.get() as _)
}

fn convert_key_number(v: Option<U14>) -> Option<u8> {
    Some(v?.get() as _)
}

fn convert_character(
    v: SourceCharacter,
    style: ConversionStyle,
) -> Option<schema::SourceCharacter> {
    use schema::SourceCharacter as T;
    use SourceCharacter::*;
    let res = match v {
        RangeElement => T::Range,
        MomentaryButton => T::Button,
        Encoder1 => T::Relative1,
        Encoder2 => T::Relative2,
        Encoder3 => T::Relative3,
        ToggleButton => T::StatefulButton,
    };
    style.required_value(res)
}

fn convert_transport_msg(
    v: MidiClockTransportMessage,
) -> Option<schema::MidiClockTransportMessage> {
    use schema::MidiClockTransportMessage as T;
    use MidiClockTransportMessage::*;
    let res = match v {
        Start => T::Start,
        Continue => T::Continue,
        Stop => T::Stop,
    };
    Some(res)
}

fn convert_mackie_seven_segment_display_scope(
    v: MackieSevenSegmentDisplayScope,
) -> Option<schema::MackieSevenSegmentDisplayScope> {
    use schema::MackieSevenSegmentDisplayScope as T;
    use MackieSevenSegmentDisplayScope::*;
    let res = match v {
        All => T::All,
        Assignment => T::Assignment,
        Tc => T::Tc,
        TcHoursBars => T::TcHoursBars,
        TcMinutesBeats => T::TcMinutesBeats,
        TcSecondsSub => T::TcSecondsSub,
        TcFramesTicks => T::TcFramesTicks,
    };
    Some(res)
}

use crate::application::{MidiSourceType, ReaperSourceType, SourceCategory};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind, convert_keystroke,
    convert_osc_argument, ConversionStyle,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::SourceModelData;
use helgoboss_learn::{
    DisplayType, MackieSevenSegmentDisplayScope, MidiClockTransportMessage, SourceCharacter,
};
use helgoboss_midi::{Channel, U14};
use realearn_api::persistence;
use std::convert::TryInto;

pub struct NewSourceProps {
    pub prevent_echo_feedback: bool,
    pub send_feedback_after_control: bool,
}

pub fn convert_source(
    data: SourceModelData,
    new_source_props: NewSourceProps,
    style: ConversionStyle,
) -> ConversionResult<persistence::Source> {
    let feedback_behavior = {
        use persistence::FeedbackBehavior as T;
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
        Never => persistence::Source::NoneSource,
        Midi => {
            use MidiSourceType::*;
            match data.r#type {
                ControlChangeValue => {
                    let s = persistence::MidiControlChangeValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        controller_number: convert_controller_number(data.number),
                        character: convert_character(data.character, style),
                        fourteen_bit: data.is_14_bit,
                    };
                    persistence::Source::MidiControlChangeValue(s)
                }
                NoteVelocity => {
                    let s = persistence::MidiNoteVelocitySource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        key_number: convert_key_number(data.number),
                    };
                    persistence::Source::MidiNoteVelocity(s)
                }
                NoteKeyNumber => {
                    let s = persistence::MidiNoteKeyNumberSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    persistence::Source::MidiNoteKeyNumber(s)
                }
                PitchBendChangeValue => {
                    let s = persistence::MidiPitchBendChangeValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    persistence::Source::MidiPitchBendChangeValue(s)
                }
                ChannelPressureAmount => {
                    let s = persistence::MidiChannelPressureAmountSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    persistence::Source::MidiChannelPressureAmount(s)
                }
                ProgramChangeNumber => {
                    let s = persistence::MidiProgramChangeNumberSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                    };
                    persistence::Source::MidiProgramChangeNumber(s)
                }
                ParameterNumberValue => {
                    let s = persistence::MidiParameterNumberValueSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        number: convert_parameter_number(data.number),
                        fourteen_bit: data.is_14_bit,
                        registered: data.is_registered,
                        character: convert_character(data.character, style),
                    };
                    persistence::Source::MidiParameterNumberValue(s)
                }
                PolyphonicKeyPressureAmount => {
                    let s = persistence::MidiPolyphonicKeyPressureAmountSource {
                        feedback_behavior,
                        channel: convert_channel(data.channel),
                        key_number: convert_key_number(data.number),
                    };
                    persistence::Source::MidiPolyphonicKeyPressureAmount(s)
                }
                ClockTempo => {
                    let s = persistence::MidiClockTempoSource;
                    persistence::Source::MidiClockTempo(s)
                }
                ClockTransport => {
                    let s = persistence::MidiClockTransportSource {
                        message: convert_transport_msg(data.message),
                    };
                    persistence::Source::MidiClockTransport(s)
                }
                Raw => {
                    let s = persistence::MidiRawSource {
                        feedback_behavior,
                        pattern: style.required_value(data.raw_midi_pattern),
                        character: convert_character(data.character, style),
                    };
                    persistence::Source::MidiRaw(s)
                }
                Script => {
                    let s = persistence::MidiScriptSource {
                        kind: style.required_value(data.midi_script_kind),
                        script: style.required_value(data.midi_script),
                    };
                    persistence::Source::MidiScript(s)
                }
                Display => {
                    use DisplayType::*;
                    match data.display_type {
                        MackieLcd => {
                            let s = persistence::MackieLcdSource {
                                extender_index: style.required_value_with_default(
                                    0,
                                    defaults::SOURCE_MACKIE_LCD_EXTENDER_INDEX,
                                ),
                                channel: data.display_id,
                                line: data.line,
                            };
                            persistence::Source::MackieLcd(s)
                        }
                        MackieXtLcd => {
                            let s = persistence::MackieLcdSource {
                                extender_index: style.required_value_with_default(
                                    1,
                                    defaults::SOURCE_MACKIE_LCD_EXTENDER_INDEX,
                                ),
                                channel: data.display_id,
                                line: data.line,
                            };
                            persistence::Source::MackieLcd(s)
                        }
                        XTouchMackieLcd => {
                            let s = persistence::XTouchMackieLcdSource {
                                extender_index: style.required_value_with_default(
                                    0,
                                    defaults::SOURCE_MACKIE_LCD_EXTENDER_INDEX,
                                ),
                                channel: data.display_id,
                                line: data.line,
                            };
                            persistence::Source::XTouchMackieLcd(s)
                        }
                        XTouchMackieXtLcd => {
                            let s = persistence::XTouchMackieLcdSource {
                                extender_index: style.required_value_with_default(
                                    1,
                                    defaults::SOURCE_MACKIE_LCD_EXTENDER_INDEX,
                                ),
                                channel: data.display_id,
                                line: data.line,
                            };
                            persistence::Source::XTouchMackieLcd(s)
                        }
                        MackieSevenSegmentDisplay => {
                            let s = persistence::MackieSevenSegmentDisplaySource {
                                scope: data.display_id.and_then(|id| {
                                    convert_mackie_seven_segment_display_scope(
                                        (id as usize).try_into().ok()?,
                                    )
                                }),
                            };
                            persistence::Source::MackieSevenSegmentDisplay(s)
                        }
                        SlKeyboardDisplay => {
                            let s = persistence::SlKeyboardDisplaySource {
                                section: data.display_id,
                                line: data.line,
                            };
                            persistence::Source::SlKeyboardDisplay(s)
                        }
                        SiniConE24 => {
                            let s = persistence::SiniConE24DisplaySource {
                                cell_index: data.display_id,
                                item_index: data.line,
                            };
                            persistence::Source::SiniConE24Display(s)
                        }
                        LaunchpadProScrollingText => {
                            let s = persistence::LaunchpadProScrollingTextDisplaySource;
                            persistence::Source::LaunchpadProScrollingTextDisplay(s)
                        }
                    }
                }
            }
        }
        Osc => {
            let s = persistence::OscSource {
                feedback_behavior,
                address: style.required_value(data.osc_address_pattern),
                argument: convert_osc_argument(
                    data.osc_arg_index,
                    data.osc_arg_type,
                    data.osc_arg_value_range,
                    style,
                ),
                relative: style.required_value_with_default(
                    data.osc_arg_is_relative,
                    defaults::SOURCE_OSC_IS_RELATIVE,
                ),
                feedback_arguments: style.required_value(data.osc_feedback_args),
            };
            persistence::Source::Osc(s)
        }
        Reaper => {
            use ReaperSourceType::*;
            match data.reaper_source_type {
                MidiDeviceChanges => {
                    persistence::Source::MidiDeviceChanges(persistence::MidiDeviceChangesSource)
                }
                RealearnInstanceStart => persistence::Source::RealearnInstanceStart(
                    persistence::RealearnInstanceStartSource,
                ),
                Timer => persistence::Source::Timer(persistence::TimerSource {
                    duration: data.timer_millis,
                }),
                RealearnParameter => {
                    persistence::Source::RealearnParameter(persistence::RealearnParameterSource {
                        parameter_index: data.parameter_index.get(),
                    })
                }
            }
        }
        Virtual => {
            let s = persistence::VirtualSource {
                id: convert_control_element_id(data.control_element_index),
                character: convert_control_element_kind(data.control_element_type, style),
            };
            persistence::Source::Virtual(s)
        }
        Keyboard => {
            let s = persistence::KeySource {
                keystroke: data.keystroke.map(convert_keystroke),
            };
            persistence::Source::Key(s)
        }
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
) -> Option<persistence::SourceCharacter> {
    use persistence::SourceCharacter as T;
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
) -> Option<persistence::MidiClockTransportMessage> {
    use persistence::MidiClockTransportMessage as T;
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
) -> Option<persistence::MackieSevenSegmentDisplayScope> {
    use persistence::MackieSevenSegmentDisplayScope as T;
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

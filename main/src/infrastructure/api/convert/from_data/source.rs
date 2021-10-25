use crate::application::{MidiSourceType, ReaperSourceType, SourceCategory};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::data::SourceModelData;
use helgoboss_learn::{
    DisplayType, MackieSevenSegmentDisplayScope, MidiClockTransportMessage, OscTypeTag,
    SourceCharacter,
};
use helgoboss_midi::{Channel, U14};
use std::convert::TryInto;

pub fn convert_source(data: SourceModelData) -> ConversionResult<schema::Source> {
    use SourceCategory::*;
    let source = match data.category {
        Never => schema::Source::NoneSource,
        Midi => {
            use MidiSourceType::*;
            match data.r#type {
                ControlChangeValue => {
                    let s = schema::MidiControlChangeValueSource {
                        channel: convert_channel(data.channel),
                        controller_number: convert_controller_number(data.number),
                        character: convert_character(data.character),
                        fourteen_bit: data.is_14_bit,
                    };
                    schema::Source::MidiControlChangeValue(s)
                }
                NoteVelocity => {
                    let s = schema::MidiNoteVelocitySource {
                        channel: convert_channel(data.channel),
                        key_number: convert_key_number(data.number),
                    };
                    schema::Source::MidiNoteVelocity(s)
                }
                NoteKeyNumber => {
                    let s = schema::MidiNoteKeyNumberSource {
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiNoteKeyNumber(s)
                }
                PitchBendChangeValue => {
                    let s = schema::MidiPitchBendChangeValueSource {
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiPitchBendChangeValue(s)
                }
                ChannelPressureAmount => {
                    let s = schema::MidiChannelPressureAmountSource {
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiChannelPressureAmount(s)
                }
                ProgramChangeNumber => {
                    let s = schema::MidiProgramChangeNumberSource {
                        channel: convert_channel(data.channel),
                    };
                    schema::Source::MidiProgramChangeNumber(s)
                }
                ParameterNumberValue => {
                    let s = schema::MidiParameterNumberValueSource {
                        channel: convert_channel(data.channel),
                        number: convert_parameter_number(data.number),
                        fourteen_bit: data.is_14_bit,
                        registered: data.is_registered,
                        character: convert_character(data.character),
                    };
                    schema::Source::MidiParameterNumberValue(s)
                }
                PolyphonicKeyPressureAmount => {
                    let s = schema::MidiPolyphonicKeyPressureAmountSource {
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
                        pattern: Some(data.raw_midi_pattern),
                        character: convert_character(data.character),
                    };
                    schema::Source::MidiRaw(s)
                }
                Script => {
                    let s = schema::MidiScriptSource {
                        script: Some(data.midi_script),
                    };
                    schema::Source::MidiScript(s)
                }
                Display => {
                    use DisplayType::*;
                    match data.display_type {
                        MackieLcd => {
                            let s = schema::MackieLcd {
                                channel: data.display_id,
                                line: data.line,
                            };
                            schema::Source::MackieLcd(s)
                        }
                        MackieSevenSegmentDisplay => {
                            let s = schema::MackieSevenSegmentDisplay {
                                scope: data.display_id.and_then(|id| {
                                    convert_mackie_seven_segment_display_scope(
                                        (id as usize).try_into().ok()?,
                                    )
                                }),
                            };
                            schema::Source::MackieSevenSegmentDisplay(s)
                        }
                        SiniConE24 => {
                            let s = schema::SiniConE24Display {
                                cell_index: data.display_id,
                                item_index: data.line,
                            };
                            schema::Source::SiniConE24Display(s)
                        }
                        LaunchpadProScrollingText => {
                            let s = schema::LaunchpadProScrollingTextDisplay;
                            schema::Source::LaunchpadProScrollingTextDisplay(s)
                        }
                    }
                }
            }
        }
        Osc => {
            let s = schema::OscSource {
                address: Some(data.osc_address_pattern),
                argument: convert_osc_argument(
                    data.osc_arg_index,
                    data.osc_arg_type,
                    data.osc_arg_is_relative,
                ),
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
            }
        }
        Virtual => {
            let s = schema::VirtualSource {
                id: convert_control_element_id(data.control_element_index),
                kind: convert_control_element_kind(data.control_element_type),
            };
            schema::Source::Virtual(s)
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

fn convert_character(v: SourceCharacter) -> Option<schema::SourceCharacter> {
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
    Some(res)
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

fn convert_osc_argument(
    arg_index: Option<u32>,
    arg_type: OscTypeTag,
    arg_is_relative: bool,
) -> Option<schema::OscArgument> {
    let arg_index = arg_index?;
    let arg = schema::OscArgument {
        index: Some(arg_index),
        kind: Some(convert_osc_arg_kind(arg_type)),
        relative: Some(arg_is_relative),
    };
    Some(arg)
}

fn convert_osc_arg_kind(v: OscTypeTag) -> schema::OscArgKind {
    use schema::OscArgKind as T;
    use OscTypeTag::*;
    match v {
        Float => T::Float,
        Double => T::Double,
        Bool => T::Bool,
        Nil => T::Nil,
        Inf => T::Inf,
        Int => T::Int,
        String => T::String,
        Blob => T::Blob,
        Time => T::Time,
        Long => T::Long,
        Char => T::Char,
        Color => T::Color,
        Midi => T::Midi,
        Array => T::Array,
    }
}

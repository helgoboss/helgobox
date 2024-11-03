use crate::application::{MidiSourceType, ReaperSourceType, SourceCategory};
use crate::infrastructure::api::convert::to_data::{
    convert_control_element_id, convert_keystroke, convert_osc_arg_type, convert_osc_value_range,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::SourceModelData;
use anyhow::bail;
use helgoboss_learn::DisplayType;
use helgoboss_midi::{Channel, U14};
use helgobox_api::persistence::*;
use std::convert::TryInto;

pub fn convert_source(s: Source) -> ConversionResult<SourceModelData> {
    let data = SourceModelData {
        category: convert_category(&s),
        r#type: convert_midi_source_type(&s),
        channel: convert_midi_channel(&s)?,
        number: convert_midi_number(&s)?,
        character: match &s {
            Source::MidiControlChangeValue(s) => convert_character(s.character),
            Source::MidiParameterNumberValue(s) => convert_character(s.character),
            Source::MidiRaw(s) => convert_character(s.character),
            _ => Default::default(),
        },
        is_registered: match &s {
            Source::MidiParameterNumberValue(s) => s.registered,
            _ => None,
        },
        is_14_bit: match &s {
            Source::MidiControlChangeValue(s) => s.fourteen_bit,
            Source::MidiParameterNumberValue(s) => s.fourteen_bit,
            _ => None,
        },
        message: match &s {
            Source::MidiClockTransport(s) => convert_midi_clock_transport_message(s.message),
            _ => Default::default(),
        },
        raw_midi_pattern: match &s {
            Source::MidiRaw(s) => s.pattern.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        midi_script_kind: match &s {
            Source::MidiScript(s) => s.script_kind.unwrap_or_default(),
            _ => Default::default(),
        },
        midi_script: match &s {
            Source::MidiScript(s) => s.script.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        display_type: match &s {
            Source::MackieLcd(s) => {
                let extender_index = s
                    .extender_index
                    .unwrap_or(defaults::SOURCE_MACKIE_LCD_EXTENDER_INDEX);
                match extender_index {
                    0 => DisplayType::MackieLcd,
                    1 => DisplayType::MackieXtLcd,
                    _ => {
                        bail!("at the moment, only extender indexes 0 and 1 are supported");
                    }
                }
            }
            Source::XTouchMackieLcd(s) => {
                let extender_index = s
                    .extender_index
                    .unwrap_or(defaults::SOURCE_X_TOUCH_MACKIE_LCD_EXTENDER_INDEX);
                match extender_index {
                    0 => DisplayType::XTouchMackieLcd,
                    1 => DisplayType::XTouchMackieXtLcd,
                    _ => {
                        bail!("at the moment, only extender indexes 0 and 1 are supported");
                    }
                }
            }
            Source::MackieSevenSegmentDisplay(_) => DisplayType::MackieSevenSegmentDisplay,
            Source::SiniConE24Display(_) => DisplayType::SiniConE24,
            Source::LaunchpadProScrollingTextDisplay => DisplayType::LaunchpadProScrollingText,
            _ => Default::default(),
        },
        display_id: match &s {
            Source::MackieLcd(s) => s.channel,
            Source::XTouchMackieLcd(s) => s.channel,
            Source::MackieSevenSegmentDisplay(s) => s
                .scope
                .map(convert_mackie_seven_segment_display_scope)
                .map(|s| usize::from(s) as _),
            Source::SiniConE24Display(s) => s.cell_index,
            _ => None,
        },
        line: match &s {
            Source::MackieLcd(s) => s.line,
            Source::XTouchMackieLcd(s) => s.line,
            Source::MackieSevenSegmentDisplay(_) => None,
            Source::SiniConE24Display(s) => s.item_index,
            _ => None,
        },
        osc_address_pattern: match &s {
            Source::Osc(s) => s.address.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        osc_arg_index: match &s {
            Source::Osc(s) => s.argument.and_then(|arg| arg.index),
            _ => None,
        },
        osc_arg_type: match &s {
            Source::Osc(s) => s
                .argument
                .map(|arg| convert_osc_arg_type(arg.arg_kind.unwrap_or_default()))
                .unwrap_or_default(),
            _ => Default::default(),
        },
        osc_arg_is_relative: match &s {
            Source::Osc(s) => s.relative.unwrap_or(defaults::SOURCE_OSC_IS_RELATIVE),
            _ => false,
        },
        osc_arg_value_range: match &s {
            Source::Osc(s) => convert_osc_value_range(s.argument.and_then(|a| a.value_range)),
            _ => Default::default(),
        },
        osc_feedback_args: match &s {
            Source::Osc(s) => s.feedback_arguments.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        keystroke: match &s {
            Source::Key(s) => s.keystroke.map(convert_keystroke),
            _ => Default::default(),
        },
        button_index: match &s {
            Source::StreamDeck(s) => s.button_index,
            _ => Default::default(),
        },
        button_design: match &s {
            Source::StreamDeck(s) => s.button_design.clone(),
            _ => Default::default(),
        },
        control_element_type: match &s {
            Source::Virtual(s) => s.character.unwrap_or_default(),
            _ => Default::default(),
        },
        control_element_index: match &s {
            Source::Virtual(s) => convert_control_element_id(s.id.clone()),
            _ => Default::default(),
        },
        reaper_source_type: match &s {
            Source::MidiDeviceChanges => ReaperSourceType::MidiDeviceChanges,
            Source::RealearnInstanceStart => ReaperSourceType::RealearnUnitStart,
            Source::Timer(_) => ReaperSourceType::Timer,
            Source::RealearnParameter(_) => ReaperSourceType::RealearnParameter,
            _ => Default::default(),
        },
        timer_millis: match &s {
            Source::Timer(t) => t.duration,
            _ => Default::default(),
        },
        parameter_index: match &s {
            Source::RealearnParameter(s) => {
                s.parameter_index.try_into().map_err(anyhow::Error::msg)?
            }
            _ => Default::default(),
        },
    };
    Ok(data)
}

fn convert_category(s: &Source) -> SourceCategory {
    use Source::*;
    match s {
        None => SourceCategory::Never,
        MidiDeviceChanges | RealearnInstanceStart | Timer(_) | RealearnParameter(_) | Speech => {
            SourceCategory::Reaper
        }
        MidiNoteVelocity(_)
        | MidiNoteKeyNumber(_)
        | MidiPolyphonicKeyPressureAmount(_)
        | MidiControlChangeValue(_)
        | MidiProgramChangeNumber(_)
        | MidiSpecificProgramChange(_)
        | MidiChannelPressureAmount(_)
        | MidiPitchBendChangeValue(_)
        | MidiParameterNumberValue(_)
        | MidiClockTempo
        | MidiClockTransport(_)
        | MidiRaw(_)
        | MidiScript(_)
        | MackieLcd(_)
        | XTouchMackieLcd(_)
        | MackieSevenSegmentDisplay(_)
        | SiniConE24Display(_)
        | SlKeyboardDisplay(_)
        | LaunchpadProScrollingTextDisplay => SourceCategory::Midi,
        Osc(_) => SourceCategory::Osc,
        Key(_) => SourceCategory::Keyboard,
        StreamDeck(_) => SourceCategory::StreamDeck,
        Virtual(_) => SourceCategory::Virtual,
    }
}

fn convert_midi_source_type(s: &Source) -> MidiSourceType {
    use Source::*;
    match s {
        MidiNoteVelocity(_) => MidiSourceType::NoteVelocity,
        MidiNoteKeyNumber(_) => MidiSourceType::PolyphonicKeyPressureAmount,
        MidiPolyphonicKeyPressureAmount(_) => MidiSourceType::PolyphonicKeyPressureAmount,
        MidiControlChangeValue(_) => MidiSourceType::ControlChangeValue,
        MidiProgramChangeNumber(_) => MidiSourceType::ProgramChangeNumber,
        MidiSpecificProgramChange(_) => MidiSourceType::SpecificProgramChange,
        MidiChannelPressureAmount(_) => MidiSourceType::ChannelPressureAmount,
        MidiPitchBendChangeValue(_) => MidiSourceType::PitchBendChangeValue,
        MidiParameterNumberValue(_) => MidiSourceType::ParameterNumberValue,
        MidiClockTempo => MidiSourceType::ClockTempo,
        MidiClockTransport(_) => MidiSourceType::ClockTransport,
        MidiRaw(_) => MidiSourceType::Raw,
        MidiScript(_) => MidiSourceType::Script,
        MackieLcd(_) | XTouchMackieLcd(_) | MackieSevenSegmentDisplay(_) | SiniConE24Display(_) => {
            MidiSourceType::Display
        }
        _ => MidiSourceType::default(),
    }
}

fn convert_midi_channel(s: &Source) -> ConversionResult<Option<Channel>> {
    let ch = match s {
        Source::MidiNoteVelocity(s) => s.channel,
        Source::MidiNoteKeyNumber(s) => s.channel,
        Source::MidiPolyphonicKeyPressureAmount(s) => s.channel,
        Source::MidiControlChangeValue(s) => s.channel,
        Source::MidiProgramChangeNumber(s) => s.channel,
        Source::MidiSpecificProgramChange(s) => s.channel,
        Source::MidiChannelPressureAmount(s) => s.channel,
        Source::MidiPitchBendChangeValue(s) => s.channel,
        Source::MidiParameterNumberValue(s) => s.channel,
        _ => None,
    };
    if let Some(ch) = ch {
        let ch: Channel = ch.try_into()?;
        Ok(Some(ch))
    } else {
        Ok(None)
    }
}

fn convert_midi_number(s: &Source) -> ConversionResult<Option<U14>> {
    let n = match s {
        Source::MidiNoteVelocity(s) => s.key_number.map(|n| n as u16),
        Source::MidiPolyphonicKeyPressureAmount(s) => s.key_number.map(|n| n as u16),
        Source::MidiControlChangeValue(s) => s.controller_number.map(|n| n as u16),
        Source::MidiSpecificProgramChange(s) => s.program_number.map(|n| n as u16),
        Source::MidiParameterNumberValue(s) => s.number,
        _ => None,
    };
    if let Some(n) = n {
        let n: U14 = n.try_into()?;
        Ok(Some(n))
    } else {
        Ok(None)
    }
}

fn convert_character(s: Option<SourceCharacter>) -> helgoboss_learn::SourceCharacter {
    use helgoboss_learn::SourceCharacter as T;
    use SourceCharacter::*;
    match s.unwrap_or_default() {
        Range => T::RangeElement,
        Button => T::MomentaryButton,
        Relative1 => T::Encoder1,
        Relative2 => T::Encoder2,
        Relative3 => T::Encoder3,
        StatefulButton => T::ToggleButton,
    }
}

fn convert_midi_clock_transport_message(
    s: Option<MidiClockTransportMessage>,
) -> helgoboss_learn::MidiClockTransportMessage {
    use helgoboss_learn::MidiClockTransportMessage as T;
    use MidiClockTransportMessage::*;
    match s.unwrap_or_default() {
        Start => T::Start,
        Continue => T::Continue,
        Stop => T::Stop,
    }
}

fn convert_mackie_seven_segment_display_scope(
    s: MackieSevenSegmentDisplayScope,
) -> helgoboss_learn::MackieSevenSegmentDisplayScope {
    use helgoboss_learn::MackieSevenSegmentDisplayScope as T;
    use MackieSevenSegmentDisplayScope::*;
    match s {
        All => T::All,
        Assignment => T::Assignment,
        Tc => T::Tc,
        TcHoursBars => T::TcHoursBars,
        TcMinutesBeats => T::TcMinutesBeats,
        TcSecondsSub => T::TcSecondsSub,
        TcFramesTicks => T::TcFramesTicks,
    }
}

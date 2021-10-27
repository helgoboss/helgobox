use crate::application::{MidiSourceType, ReaperSourceType, SourceCategory};
use crate::infrastructure::api::convert::to_data::{
    convert_control_element_id, convert_control_element_type, convert_osc_arg_type,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::SourceModelData;
use helgoboss_learn::DisplayType;
use helgoboss_midi::{Channel, U14};
use std::convert::TryInto;

pub fn convert_source(s: Source) -> ConversionResult<SourceModelData> {
    use Source::*;
    let data = SourceModelData {
        category: convert_category(&s),
        r#type: convert_midi_source_type(&s),
        channel: convert_midi_channel(&s)?,
        number: convert_midi_number(&s)?,
        character: match &s {
            MidiControlChangeValue(s) => convert_character(s.character),
            MidiParameterNumberValue(s) => convert_character(s.character),
            MidiRaw(s) => convert_character(s.character),
            _ => Default::default(),
        },
        is_registered: match &s {
            MidiParameterNumberValue(s) => s.registered,
            _ => None,
        },
        is_14_bit: match &s {
            MidiControlChangeValue(s) => s.fourteen_bit,
            MidiParameterNumberValue(s) => s.fourteen_bit,
            _ => None,
        },
        message: match &s {
            MidiClockTransport(s) => convert_midi_clock_transport_message(s.message),
            _ => Default::default(),
        },
        raw_midi_pattern: match &s {
            MidiRaw(s) => s.pattern.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        midi_script: match &s {
            MidiScript(s) => s.script.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        display_type: match &s {
            MackieLcd(_) => DisplayType::MackieLcd,
            MackieSevenSegmentDisplay(_) => DisplayType::MackieSevenSegmentDisplay,
            SiniConE24Display(_) => DisplayType::SiniConE24,
            LaunchpadProScrollingTextDisplay(_) => DisplayType::LaunchpadProScrollingText,
            _ => Default::default(),
        },
        display_id: match &s {
            MackieLcd(s) => s.channel,
            MackieSevenSegmentDisplay(s) => s
                .scope
                .map(convert_mackie_seven_segment_display_scope)
                .map(|s| usize::from(s) as _),
            SiniConE24Display(s) => s.cell_index,
            _ => None,
        },
        line: match &s {
            MackieLcd(s) => s.line,
            MackieSevenSegmentDisplay(_) => None,
            SiniConE24Display(s) => s.item_index,
            _ => None,
        },
        osc_address_pattern: match &s {
            Osc(s) => s.address.as_ref().cloned().unwrap_or_default(),
            _ => Default::default(),
        },
        osc_arg_index: match &s {
            Osc(s) => s.argument.and_then(|arg| arg.index),
            _ => None,
        },
        osc_arg_type: match &s {
            Osc(s) => s
                .argument
                .map(|arg| convert_osc_arg_type(arg.kind.unwrap_or_default()))
                .unwrap_or_default(),
            _ => Default::default(),
        },
        osc_arg_is_relative: match &s {
            Osc(s) => s.argument.and_then(|arg| arg.relative).unwrap_or(false),
            _ => false,
        },
        control_element_type: match &s {
            Virtual(s) => convert_control_element_type(s.kind.unwrap_or_default()),
            _ => Default::default(),
        },
        control_element_index: match &s {
            Virtual(s) => convert_control_element_id(s.id.clone()),
            _ => Default::default(),
        },
        reaper_source_type: match &s {
            MidiDeviceChanges(_) => ReaperSourceType::MidiDeviceChanges,
            RealearnInstanceStart(_) => ReaperSourceType::RealearnInstanceStart,
            _ => Default::default(),
        },
    };
    Ok(data)
}

fn convert_category(s: &Source) -> SourceCategory {
    use Source::*;
    match s {
        NoneSource => SourceCategory::Never,
        MidiDeviceChanges(_) | RealearnInstanceStart(_) => SourceCategory::Reaper,
        MidiNoteVelocity(_)
        | MidiNoteKeyNumber(_)
        | MidiPolyphonicKeyPressureAmount(_)
        | MidiControlChangeValue(_)
        | MidiProgramChangeNumber(_)
        | MidiChannelPressureAmount(_)
        | MidiPitchBendChangeValue(_)
        | MidiParameterNumberValue(_)
        | MidiClockTempo(_)
        | MidiClockTransport(_)
        | MidiRaw(_)
        | MidiScript(_)
        | MackieLcd(_)
        | MackieSevenSegmentDisplay(_)
        | SiniConE24Display(_)
        | LaunchpadProScrollingTextDisplay(_) => SourceCategory::Midi,
        Osc(_) => SourceCategory::Osc,
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
        MidiChannelPressureAmount(_) => MidiSourceType::ChannelPressureAmount,
        MidiPitchBendChangeValue(_) => MidiSourceType::PitchBendChangeValue,
        MidiParameterNumberValue(_) => MidiSourceType::ParameterNumberValue,
        MidiClockTempo(_) => MidiSourceType::ClockTempo,
        MidiClockTransport(_) => MidiSourceType::ClockTransport,
        MidiRaw(_) => MidiSourceType::Raw,
        MidiScript(_) => MidiSourceType::Script,
        MackieLcd(_) | MackieSevenSegmentDisplay(_) | SiniConE24Display(_) => {
            MidiSourceType::Display
        }
        _ => MidiSourceType::default(),
    }
}

fn convert_midi_channel(s: &Source) -> ConversionResult<Option<Channel>> {
    use Source::*;
    let ch = match s {
        MidiNoteVelocity(s) => s.channel,
        MidiNoteKeyNumber(s) => s.channel,
        MidiPolyphonicKeyPressureAmount(s) => s.channel,
        MidiControlChangeValue(s) => s.channel,
        MidiProgramChangeNumber(s) => s.channel,
        MidiChannelPressureAmount(s) => s.channel,
        MidiPitchBendChangeValue(s) => s.channel,
        MidiParameterNumberValue(s) => s.channel,
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
    use Source::*;
    let n = match s {
        MidiNoteVelocity(s) => s.key_number.map(|n| n as u16),
        MidiPolyphonicKeyPressureAmount(s) => s.key_number.map(|n| n as u16),
        MidiControlChangeValue(s) => s.controller_number.map(|n| n as u16),
        MidiParameterNumberValue(s) => s.number,
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

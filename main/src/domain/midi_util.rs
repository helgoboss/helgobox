use helgoboss_midi::{RawShortMessage, ShortMessage, ShortMessageType};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiMessageClassification {
    Normal,
    Ignored,
    Timing,
}

pub fn classify_midi_message(msg: RawShortMessage) -> MidiMessageClassification {
    use ShortMessageType::*;
    match msg.r#type() {
        NoteOff
        | NoteOn
        | PolyphonicKeyPressure
        | ControlChange
        | ProgramChange
        | ChannelPressure
        | PitchBendChange
        | Start
        | Continue
        | Stop => MidiMessageClassification::Normal,
        SystemExclusiveStart
        | TimeCodeQuarterFrame
        | SongPositionPointer
        | SongSelect
        | SystemCommonUndefined1
        | SystemCommonUndefined2
        | TuneRequest
        | SystemExclusiveEnd
        | SystemRealTimeUndefined1
        | SystemRealTimeUndefined2
        | ActiveSensing
        | SystemReset => MidiMessageClassification::Ignored,
        TimingClock => MidiMessageClassification::Timing,
    }
}

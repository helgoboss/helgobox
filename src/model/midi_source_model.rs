use crate::model::Property;
use helgoboss_learn::{MidiClockTransportMessageKind, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use rxrust::prelude::*;

/// A model for creating MIDI sources
#[derive(Clone, Debug)]
pub struct MidiSourceModel<'a> {
    pub kind: Property<'a, MidiSourceKind>,
    pub channel: Property<'a, Option<Channel>>,
    pub midi_message_number: Property<'a, Option<U7>>,
    pub parameter_number_message_number: Property<'a, Option<U14>>,
    pub custom_character: Property<'a, SourceCharacter>,
    pub midi_clock_transport_message_kind: Property<'a, MidiClockTransportMessageKind>,
    pub is_registered: Property<'a, Option<bool>>,
    pub is_14_bit: Property<'a, Option<bool>>,
}

impl<'a> Default for MidiSourceModel<'a> {
    fn default() -> Self {
        Self {
            kind: Property::new(MidiSourceKind::ControlChangeValue),
            channel: Default::default(),
            midi_message_number: Default::default(),
            parameter_number_message_number: Default::default(),
            custom_character: Property::new(SourceCharacter::Range),
            midi_clock_transport_message_kind: Property::new(MidiClockTransportMessageKind::Start),
            is_registered: Default::default(),
            is_14_bit: Default::default(),
        }
    }
}

/// Represents possible MIDI sources
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiSourceKind {
    NoteVelocity,
    NoteKeyNumber,
    PolyphonicKeyPressureAmount,
    ControlChangeValue,
    ProgramChangeNumber,
    ChannelPressureAmount,
    PitchBendChangeValue,
    ParameterNumberValue,
    ClockTempo,
    ClockTransport,
}

impl<'a> MidiSourceModel<'a> {
    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'a, Item = (), Err = ()> {
        self.kind
            .changed()
            .merge(self.channel.changed())
            .merge(self.midi_message_number.changed())
            .merge(self.parameter_number_message_number.changed())
            .merge(self.custom_character.changed())
            .merge(self.midi_clock_transport_message_kind.changed())
            .merge(self.is_registered.changed())
            .merge(self.is_14_bit.changed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::channel;

    #[test]
    fn changed() {
        // Given
        let mut invocation_count = 0;
        // When
        {
            let mut m = MidiSourceModel::default();
            m.changed().subscribe(|v| invocation_count += 1);
            m.kind.set(MidiSourceKind::NoteVelocity);
            m.channel.set(Some(channel(5)));
            m.kind.set(MidiSourceKind::ClockTransport);
            m.kind.set(MidiSourceKind::ClockTransport);
            m.channel.set(Some(channel(4)));
        }
        // Then
        assert_eq!(invocation_count, 4);
    }
}

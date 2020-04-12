use crate::domain::Property;
use helgoboss_learn::{MidiClockTransportMessage, MidiSource, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use rxrust::prelude::*;
use serde_repr::*;

/// A model for creating MIDI sources
#[derive(Clone, Debug)]
pub struct MidiSourceModel<'a> {
    pub r#type: Property<'a, MidiSourceType>,
    pub channel: Property<'a, Option<Channel>>,
    pub midi_message_number: Property<'a, Option<U7>>,
    pub parameter_number_message_number: Property<'a, Option<U14>>,
    pub custom_character: Property<'a, SourceCharacter>,
    pub midi_clock_transport_message: Property<'a, MidiClockTransportMessage>,
    pub is_registered: Property<'a, Option<bool>>,
    pub is_14_bit: Property<'a, Option<bool>>,
}

impl<'a> Default for MidiSourceModel<'a> {
    fn default() -> Self {
        Self {
            r#type: Property::new(MidiSourceType::ControlChangeValue),
            channel: Default::default(),
            midi_message_number: Default::default(),
            parameter_number_message_number: Default::default(),
            custom_character: Property::new(SourceCharacter::Range),
            midi_clock_transport_message: Property::new(MidiClockTransportMessage::Start),
            is_registered: Default::default(),
            is_14_bit: Default::default(),
        }
    }
}

impl<'a> MidiSourceModel<'a> {
    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'a, Item = (), Err = ()> {
        self.r#type
            .changed()
            .merge(self.channel.changed())
            .merge(self.midi_message_number.changed())
            .merge(self.parameter_number_message_number.changed())
            .merge(self.custom_character.changed())
            .merge(self.midi_clock_transport_message.changed())
            .merge(self.is_registered.changed())
            .merge(self.is_14_bit.changed())
    }

    /// Creates a source reflecting this model's current values
    pub fn create_source(&self) -> MidiSource {
        use MidiSourceType::*;
        let channel = *self.channel.get();
        let key_number = self.midi_message_number.get().map(|n| n.into());
        match self.r#type.get() {
            NoteVelocity => MidiSource::NoteVelocity {
                channel,
                key_number,
            },
            NoteKeyNumber => MidiSource::NoteKeyNumber { channel },
            PolyphonicKeyPressureAmount => MidiSource::PolyphonicKeyPressureAmount {
                channel,
                key_number,
            },
            ControlChangeValue => {
                if *self.is_14_bit.get() == Some(true) {
                    MidiSource::ControlChange14BitValue {
                        channel,
                        msb_controller_number: self.midi_message_number.get().map(|n| n.into()),
                    }
                } else {
                    MidiSource::ControlChangeValue {
                        channel,
                        controller_number: self.midi_message_number.get().map(|n| n.into()),
                        custom_character: *self.custom_character.get(),
                    }
                }
            }
            ProgramChangeNumber => MidiSource::ProgramChangeNumber { channel },
            ChannelPressureAmount => MidiSource::ChannelPressureAmount { channel },
            PitchBendChangeValue => MidiSource::PitchBendChangeValue { channel },
            ParameterNumberValue => MidiSource::ParameterNumberValue {
                channel,
                number: *self.parameter_number_message_number.get(),
                is_14_bit: *self.is_14_bit.get(),
                is_registered: *self.is_registered.get(),
            },
            ClockTempo => MidiSource::ClockTempo,
            ClockTransport => MidiSource::ClockTransport {
                message: *self.midi_clock_transport_message.get(),
            },
        }
    }
}

/// Represents possible MIDI sources
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum MidiSourceType {
    ControlChangeValue = 0,
    NoteVelocity = 1,
    NoteKeyNumber = 2,
    PitchBendChangeValue = 3,
    ChannelPressureAmount = 4,
    ProgramChangeNumber = 5,
    ParameterNumberValue = 6,
    PolyphonicKeyPressureAmount = 7,
    ClockTempo = 8,
    ClockTransport = 9,
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::test_util::*;

    #[test]
    fn changed() {
        // Given
        let mut invocation_count = 0;
        // When
        {
            let mut m = MidiSourceModel::default();
            m.changed().subscribe(|v| invocation_count += 1);
            m.r#type.set(MidiSourceType::NoteVelocity);
            m.channel.set(Some(channel(5)));
            m.r#type.set(MidiSourceType::ClockTransport);
            m.r#type.set(MidiSourceType::ClockTransport);
            m.channel.set(Some(channel(4)));
        }
        // Then
        assert_eq!(invocation_count, 4);
    }

    #[test]
    fn create_source() {
        // Given
        let m = MidiSourceModel::default();
        // When
        let s = m.create_source();
        // Then
        assert_eq!(
            s,
            MidiSource::ControlChangeValue {
                channel: None,
                controller_number: None,
                custom_character: SourceCharacter::Range,
            }
        );
    }
}

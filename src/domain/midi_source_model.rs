use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlValue, MidiClockTransportMessage, MidiSource, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rx_util::{create_local_prop as p, LocalProp, LocalStaticProp, UnitEvent};
use rxrust::prelude::*;
use serde::export::Formatter;
use serde_repr::*;
use std::borrow::Cow;
use std::fmt::{format, Display};

/// A model for creating MIDI sources
#[derive(Clone, Debug)]
pub struct MidiSourceModel {
    pub r#type: LocalStaticProp<MidiSourceType>,
    pub channel: LocalStaticProp<Option<Channel>>,
    pub midi_message_number: LocalStaticProp<Option<U7>>,
    pub parameter_number_message_number: LocalStaticProp<Option<U14>>,
    pub custom_character: LocalStaticProp<SourceCharacter>,
    pub midi_clock_transport_message: LocalStaticProp<MidiClockTransportMessage>,
    pub is_registered: LocalStaticProp<Option<bool>>,
    pub is_14_bit: LocalStaticProp<Option<bool>>,
}

impl Default for MidiSourceModel {
    fn default() -> Self {
        Self {
            r#type: p(MidiSourceType::ControlChangeValue),
            channel: p(None),
            midi_message_number: p(None),
            parameter_number_message_number: p(None),
            custom_character: p(SourceCharacter::Range),
            midi_clock_transport_message: p(MidiClockTransportMessage::Start),
            is_registered: p(Some(false)),
            is_14_bit: p(Some(false)),
        }
    }
}

impl MidiSourceModel {
    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
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

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        // TODO-low use cached
        self.create_source().format_control_value(value)
    }

    pub fn emits_increments(&self) -> bool {
        // TODO-low use cached
        self.create_source().emits_increments()
    }

    /// Creates a source reflecting this model's current values
    pub fn create_source(&self) -> MidiSource {
        use MidiSourceType::*;
        let channel = self.channel.get();
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
                if self.is_14_bit.get() == Some(true) {
                    MidiSource::ControlChange14BitValue {
                        channel,
                        msb_controller_number: self.midi_message_number.get().map(|n| n.into()),
                    }
                } else {
                    MidiSource::ControlChangeValue {
                        channel,
                        controller_number: self.midi_message_number.get().map(|n| n.into()),
                        custom_character: self.custom_character.get(),
                    }
                }
            }
            ProgramChangeNumber => MidiSource::ProgramChangeNumber { channel },
            ChannelPressureAmount => MidiSource::ChannelPressureAmount { channel },
            PitchBendChangeValue => MidiSource::PitchBendChangeValue { channel },
            ParameterNumberValue => MidiSource::ParameterNumberValue {
                channel,
                number: self.parameter_number_message_number.get(),
                is_14_bit: self.is_14_bit.get(),
                is_registered: self.is_registered.get(),
            },
            ClockTempo => MidiSource::ClockTempo,
            ClockTransport => MidiSource::ClockTransport {
                message: self.midi_clock_transport_message.get(),
            },
        }
    }

    pub fn supports_channel(&self) -> bool {
        use MidiSourceType::*;
        matches!(
            self.r#type.get(),
            ChannelPressureAmount
                | ControlChangeValue
                | NoteVelocity
                | PolyphonicKeyPressureAmount
                | NoteKeyNumber
                | ParameterNumberValue
                | PitchBendChangeValue
                | ProgramChangeNumber
        )
    }

    pub fn supports_midi_message_number(&self) -> bool {
        use MidiSourceType::*;
        matches!(
            self.r#type.get(),
            ControlChangeValue | NoteVelocity | PolyphonicKeyPressureAmount
        )
    }

    pub fn supports_14_bit(&self) -> bool {
        use MidiSourceType::*;
        matches!(self.r#type.get(), ControlChangeValue | ParameterNumberValue)
    }

    pub fn supports_parameter_number_message_number(&self) -> bool {
        self.supports_parameter_number_message_props()
    }

    pub fn supports_is_registered(&self) -> bool {
        self.supports_parameter_number_message_props()
    }

    pub fn supports_custom_character(&self) -> bool {
        self.r#type.get() == MidiSourceType::ControlChangeValue
            && self.is_14_bit.get().contains(&false)
    }

    pub fn supports_midi_clock_transport_message_type(&self) -> bool {
        self.r#type.get() == MidiSourceType::ClockTransport
    }

    fn supports_parameter_number_message_props(&self) -> bool {
        self.r#type.get() == MidiSourceType::ParameterNumberValue
    }

    fn primary_label(&self) -> Cow<str> {
        use MidiSourceType::*;
        match self.r#type.get() {
            ParameterNumberValue => match self.is_registered.get() {
                None => ParameterNumberValue.to_string().into(),
                Some(is_registered) => {
                    if is_registered {
                        "RPN".into()
                    } else {
                        "NRPN".into()
                    }
                }
            },
            PolyphonicKeyPressureAmount => "Poly after touch".into(),
            ClockTempo => "MIDI clock\nTempo".into(),
            ClockTransport => {
                format!("MIDI clock\n{}", self.midi_clock_transport_message.get()).into()
            }
            t @ _ => t.to_string().into(),
        }
    }

    fn channel_label(&self) -> Cow<str> {
        if self.supports_channel() {
            match self.channel.get() {
                None => "Any channel".into(),
                Some(ch) => format!("Channel {}", ch.get() + 1).into(),
            }
        } else {
            "".into()
        }
    }

    fn secondary_label(&self) -> Cow<str> {
        use MidiSourceType::*;
        match self.r#type.get() {
            NoteVelocity | PolyphonicKeyPressureAmount => match self.midi_message_number.get() {
                None => "Any note".into(),
                Some(n) => format!("Note number {}", n.get()).into(),
            },
            ControlChangeValue => match self.midi_message_number.get() {
                None => "Any CC".into(),
                Some(n) => format!("CC number {}", n.get()).into(),
            },
            ParameterNumberValue => match self.parameter_number_message_number.get() {
                None => "Any number".into(),
                Some(n) => format!("Number {}", n.get()).into(),
            },
            _ => "".into(),
        }
    }
}

impl Display for MidiSourceModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let compartments = [
            self.primary_label(),
            self.channel_label(),
            self.secondary_label(),
        ];
        write!(f, "{}", compartments.join("\n"))
    }
}

/// Type of a MIDI source
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum MidiSourceType {
    #[display(fmt = "CC value")]
    ControlChangeValue = 0,
    #[display(fmt = "Note velocity")]
    NoteVelocity = 1,
    #[display(fmt = "Note number")]
    NoteKeyNumber = 2,
    #[display(fmt = "Pitch wheel")]
    PitchBendChangeValue = 3,
    #[display(fmt = "Channel after touch")]
    ChannelPressureAmount = 4,
    #[display(fmt = "Program change")]
    ProgramChangeNumber = 5,
    #[display(fmt = "(N)RPN value (no feedback)")]
    ParameterNumberValue = 6,
    #[display(fmt = "Polyphonic after touch")]
    PolyphonicKeyPressureAmount = 7,
    #[display(fmt = "MIDI clock tempo (experimental)")]
    ClockTempo = 8,
    #[display(fmt = "MIDI clock transport")]
    ClockTransport = 9,
}

impl MidiSourceType {
    pub fn number_label(&self) -> &'static str {
        use MidiSourceType::*;
        match self {
            ControlChangeValue => "CC number",
            NoteVelocity | PolyphonicKeyPressureAmount => "Note number",
            ParameterNumberValue => "Number",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::test_util::*;
    use rx_util::create_invocation_mock;

    #[test]
    fn changed() {
        // Given
        let mut m = MidiSourceModel::default();
        let (mock, mock_mirror) = create_invocation_mock();
        // When
        m.changed().subscribe(move |_| mock.invoke(()));
        m.r#type.set(MidiSourceType::NoteVelocity);
        m.channel.set(Some(channel(5)));
        m.r#type.set(MidiSourceType::ClockTransport);
        m.r#type.set(MidiSourceType::ClockTransport);
        m.channel.set(Some(channel(4)));
        // Then
        assert_eq!(mock_mirror.invocation_count(), 4);
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

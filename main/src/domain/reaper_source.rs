use derive_more::Display;
use helgoboss_learn::{
    format_percentage_without_unit, parse_percentage_without_unit, ControlValue,
    DetailedSourceCharacter, SourceCharacter, UnitValue,
};
use std::convert::TryInto;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ReaperSource {
    MidiDeviceChanges,
    RealearnInstanceStart,
}

impl ReaperSource {
    pub fn possible_detailed_characters(&self) -> Vec<DetailedSourceCharacter> {
        use ReaperSource::*;
        match self {
            MidiDeviceChanges => vec![DetailedSourceCharacter::MomentaryOnOffButton],
            RealearnInstanceStart => vec![DetailedSourceCharacter::MomentaryOnOffButton],
        }
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        let formatted = format_percentage_without_unit(value.to_unit_value()?.get());
        Ok(formatted)
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_percentage_without_unit(text)?.try_into()
    }

    pub fn character(&self) -> SourceCharacter {
        SourceCharacter::MomentaryButton
    }

    pub fn control(&self, msg: &ReaperMessage) -> Option<ControlValue> {
        use ReaperMessage::*;
        let control_value = match msg {
            MidiDevicesConnected => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MAX),
                _ => return None,
            },
            MidiDevicesDisconnected => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MIN),
                _ => return None,
            },
            RealearnInstanceStarted => match self {
                ReaperSource::RealearnInstanceStart => {
                    ControlValue::AbsoluteContinuous(UnitValue::MAX)
                }
                _ => return None,
            },
        };
        Some(control_value)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Hash, Display)]
pub enum ReaperMessage {
    MidiDevicesConnected,
    MidiDevicesDisconnected,
    RealearnInstanceStarted,
}

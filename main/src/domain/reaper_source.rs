use crate::domain::{Compartment, CompartmentParamIndex, RawParamValue};
use core::fmt;
use derive_more::Display;
use helgoboss_learn::{
    format_percentage_without_unit, parse_percentage_without_unit, ControlValue,
    DetailedSourceCharacter, SourceCharacter, UnitValue,
};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ReaperSource {
    MidiDeviceChanges,
    RealearnInstanceStart,
    Timer(TimerSource),
    RealearnParameter(RealearnParameterSource),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RealearnParameterSource {
    pub parameter_index: CompartmentParamIndex,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct TimerSource {
    duration: Duration,
    last_fire: Option<Instant>,
}

impl TimerSource {
    pub fn new(interval: Duration) -> Self {
        Self {
            duration: interval,
            last_fire: None,
        }
    }

    pub fn on_deactivate(&mut self) {
        self.last_fire = None;
    }

    pub fn poll(&mut self) -> Option<ControlValue> {
        let now = Instant::now();
        if let Some(last_fire) = self.last_fire {
            let elapsed = now - last_fire;
            if elapsed >= self.duration {
                Some(self.fire(now))
            } else {
                None
            }
        } else {
            Some(self.fire(now))
        }
    }

    fn fire(&mut self, now: Instant) -> ControlValue {
        self.last_fire = Some(now);
        ControlValue::AbsoluteContinuous(UnitValue::MAX)
    }
}

impl ReaperSource {
    #[allow(clippy::single_match)]
    pub fn on_deactivate(&mut self) {
        match self {
            ReaperSource::Timer(s) => s.on_deactivate(),
            _ => {}
        }
    }

    /// If this returns `true`, the `poll` method should be called, on a regular basis.
    pub fn wants_to_be_polled(&self) -> bool {
        matches!(self, ReaperSource::Timer(_))
    }

    pub fn possible_detailed_characters(&self) -> Vec<DetailedSourceCharacter> {
        use ReaperSource::*;
        match self {
            MidiDeviceChanges => vec![DetailedSourceCharacter::MomentaryOnOffButton],
            RealearnInstanceStart => vec![DetailedSourceCharacter::MomentaryOnOffButton],
            Timer(_) => vec![DetailedSourceCharacter::PressOnlyButton],
            RealearnParameter(_) => vec![
                DetailedSourceCharacter::RangeControl,
                DetailedSourceCharacter::MomentaryVelocitySensitiveButton,
                DetailedSourceCharacter::MomentaryOnOffButton,
                DetailedSourceCharacter::PressOnlyButton,
            ],
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
        use ReaperSource::*;
        match self {
            MidiDeviceChanges | RealearnInstanceStart | Timer(_) => {
                SourceCharacter::MomentaryButton
            }
            RealearnParameter(_) => SourceCharacter::RangeElement,
        }
    }

    pub fn poll(&mut self) -> Option<ControlValue> {
        if let ReaperSource::Timer(t) = self {
            t.poll()
        } else {
            None
        }
    }

    pub fn control(
        &mut self,
        msg: &ReaperMessage,
        compartment: Compartment,
    ) -> Option<ControlValue> {
        use ReaperMessage::*;
        let control_value = match msg {
            MidiDevicesConnected(_) => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MAX),
                _ => return None,
            },
            MidiDevicesDisconnected(_) => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MIN),
                _ => return None,
            },
            RealearnInstanceStarted => match self {
                ReaperSource::RealearnInstanceStart => {
                    ControlValue::AbsoluteContinuous(UnitValue::MAX)
                }
                _ => return None,
            },
            RealearnParameterChange(c) => match self {
                ReaperSource::RealearnParameter(s)
                    if c.compartment == compartment && c.parameter_index == s.parameter_index =>
                {
                    ControlValue::AbsoluteContinuous(UnitValue::new_clamped(c.value as f64))
                }
                _ => return None,
            },
        };
        Some(control_value)
    }
}

#[derive(PartialEq, Debug, Display)]
pub enum ReaperMessage {
    #[display(fmt = "MidiDevicesConnected ({})", _0)]
    MidiDevicesConnected(MidiDeviceChangePayload),
    #[display(fmt = "MidiDevicesDisconnected ({})", _0)]
    MidiDevicesDisconnected(MidiDeviceChangePayload),
    RealearnInstanceStarted,
    RealearnParameterChange(RealearnParameterChangePayload),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RealearnParameterChangePayload {
    pub compartment: Compartment,
    pub parameter_index: CompartmentParamIndex,
    pub value: RawParamValue,
}

impl Display for RealearnParameterChangePayload {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Parameter {} with value {}",
            self.parameter_index, self.value
        )
    }
}

#[derive(PartialEq, Debug)]
pub struct MidiDeviceChangePayload {
    pub input_devices: HashSet<MidiInputDeviceId>,
    pub output_devices: HashSet<MidiOutputDeviceId>,
}

impl Display for MidiDeviceChangePayload {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Input devices: {:?}, Output devices: {:?}",
            &self.input_devices, &self.output_devices
        )
    }
}

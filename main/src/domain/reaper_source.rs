use crate::domain::{
    CompartmentKind, CompartmentParamIndex, RawParamValue, ReaperSourceAddress, StreamDeckDeviceId,
};
use base::hash_util::NonCryptoHashSet;
use core::fmt;
use derive_more::Display;
use helgoboss_learn::{
    format_percentage_without_unit, parse_percentage_without_unit, ControlValue,
    DetailedSourceCharacter, FeedbackValue, SourceCharacter, UnitValue,
};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::convert::TryInto;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReaperSource {
    MidiDeviceChanges,
    RealearnInstanceStart,
    RealearnCompartmentLoaded,
    Timer(TimerSource),
    RealearnParameter(RealearnParameterSource),
    Speech(SpeechSource),
}

#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct SpeechSource {}

impl SpeechSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feedback(&self, feedback_value: &FeedbackValue) -> SpeechSourceFeedbackValue {
        SpeechSourceFeedbackValue {
            text: feedback_value.to_textual().text.to_string(),
        }
    }
}

pub fn say(feedback_value: SpeechSourceFeedbackValue) -> Result<(), Box<dyn Error>> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        fn get_default_tts() -> Result<tts::Tts, tts::Error> {
            #[cfg(target_os = "macos")]
            {
                let mut tts = tts::Tts::default()?;
                // On macOS, at least with AVFoundation, it's necessary to set a voice first.
                // Prefer an English voice as default.
                if let Ok(voices) = tts.voices() {
                    let voice = voices
                        .iter()
                        .find(|v| v.language().language.as_str() == "en")
                        .or_else(|| voices.first());
                    if let Some(v) = voice {
                        tts.set_voice(v)?;
                    }
                }
                Ok(tts)
            }
            #[cfg(target_os = "windows")]
            {
                tts::Tts::default()
            }
        }
        use once_cell::sync::Lazy;
        use std::sync::Mutex;
        static TTS: Lazy<Result<Mutex<tts::Tts>, tts::Error>> =
            Lazy::new(|| get_default_tts().map(Mutex::new));
        let tts = TTS.as_ref()?;
        // TODO-medium This is only necessary because tts exposes a non-optimal API.
        let mut tts = tts.lock()?;
        // TODO-medium This cloning is totally unnecessary but ... non-optimal API.
        tts.speak(feedback_value.text, true)?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let _ = feedback_value;
        Err("speech source not yet supported on Linux".into())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RealearnParameterSource {
    pub parameter_index: CompartmentParamIndex,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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
    pub fn extract_feedback_address(&self) -> Option<ReaperSourceAddress> {
        use ReaperSource::*;
        match self {
            Speech(_) => Some(ReaperSourceAddress::GlobalSpeech),
            _ => None,
        }
    }

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
            RealearnCompartmentLoaded => vec![DetailedSourceCharacter::Trigger],
            Timer(_) => vec![DetailedSourceCharacter::Trigger],
            RealearnParameter(_) => vec![
                DetailedSourceCharacter::RangeControl,
                DetailedSourceCharacter::MomentaryVelocitySensitiveButton,
                DetailedSourceCharacter::MomentaryOnOffButton,
                DetailedSourceCharacter::Trigger,
            ],
            Speech(_) => vec![DetailedSourceCharacter::RangeControl],
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
            MidiDeviceChanges | RealearnInstanceStart | RealearnCompartmentLoaded | Timer(_) => {
                SourceCharacter::MomentaryButton
            }
            RealearnParameter(_) => SourceCharacter::RangeElement,
            Speech(_) => SourceCharacter::RangeElement,
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
        compartment: CompartmentKind,
    ) -> Option<ControlValue> {
        use ReaperMessage::*;
        let control_value = match msg {
            StreamDeckDevicesConnected(_) => {
                // We don't have a corresponding source yet
                return None;
            }
            MidiDevicesConnected(_) => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MAX),
                _ => return None,
            },
            MidiDevicesDisconnected(_) => match self {
                ReaperSource::MidiDeviceChanges => ControlValue::AbsoluteContinuous(UnitValue::MIN),
                _ => return None,
            },
            RealearnUnitStarted => match self {
                ReaperSource::RealearnInstanceStart => {
                    ControlValue::AbsoluteContinuous(UnitValue::MAX)
                }
                _ => return None,
            },
            RealearnCompartmentLoaded(kind) => match self {
                ReaperSource::RealearnCompartmentLoaded if *kind == compartment => {
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

    pub fn feedback(&self, feedback_value: &FeedbackValue) -> Option<ReaperSourceFeedbackValue> {
        use ReaperSource::*;
        match self {
            MidiDeviceChanges
            | RealearnInstanceStart
            | RealearnCompartmentLoaded
            | Timer(_)
            | RealearnParameter(_) => None,
            Speech(s) => Some(ReaperSourceFeedbackValue::Speech(
                s.feedback(feedback_value),
            )),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReaperSourceFeedbackValue {
    Speech(SpeechSourceFeedbackValue),
}

impl ReaperSourceFeedbackValue {
    pub fn extract_feedback_address(&self) -> Option<ReaperSourceAddress> {
        match self {
            ReaperSourceFeedbackValue::Speech(_) => Some(ReaperSourceAddress::GlobalSpeech),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SpeechSourceFeedbackValue {
    pub text: String,
}

#[derive(PartialEq, Debug, Display)]
pub enum ReaperMessage {
    #[display(fmt = "MidiDevicesConnected ({_0})")]
    MidiDevicesConnected(MidiDeviceChangePayload),
    #[display(fmt = "MidiDevicesDisconnected ({_0})")]
    MidiDevicesDisconnected(MidiDeviceChangePayload),
    RealearnUnitStarted,
    RealearnCompartmentLoaded(CompartmentKind),
    RealearnParameterChange(RealearnParameterChangePayload),
    StreamDeckDevicesConnected(StreamDeckDevicePayload),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RealearnParameterChangePayload {
    pub compartment: CompartmentKind,
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

#[derive(Eq, PartialEq, Debug)]
pub struct StreamDeckDevicePayload {
    pub devices: NonCryptoHashSet<StreamDeckDeviceId>,
}

impl Display for StreamDeckDevicePayload {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Devices: {:?}", &self.devices,)
    }
}

#[derive(Eq, PartialEq, Debug)]
pub struct MidiDeviceChangePayload {
    pub input_devices: NonCryptoHashSet<MidiInputDeviceId>,
    pub output_devices: NonCryptoHashSet<MidiOutputDeviceId>,
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

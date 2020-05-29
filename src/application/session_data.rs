use crate::application::MappingModelData;
use crate::domain::{MappingModel, MidiControlInput, MidiFeedbackOutput, Session};
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::ops::Deref;
use validator::{Validate, ValidationError, ValidationErrors};
use validator_derive::*;

/// This is the structure for loading and saving a ReaLearn session.
///
/// It's optimized for being represented as JSON. The JSON representation must be 100%
/// backward-compatible.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase", default)]
// #[validate(schema(function = "validate_schema"))]
pub struct SessionData {
    pub let_matched_events_through: bool,
    pub let_unmatched_events_through: bool,
    pub always_auto_detect_mode: bool,
    pub send_feedback_only_if_armed: bool,
    // None = FxInput
    pub control_device_id: Option<String>,
    // None = None
    pub feedback_device_id: Option<String>,
    pub mappings: Vec<MappingModelData>,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            let_matched_events_through: false,
            let_unmatched_events_through: true,
            always_auto_detect_mode: true,
            // In older versions, feedback was always sent no matter if armed or not
            send_feedback_only_if_armed: false,
            control_device_id: None,
            feedback_device_id: None,
            mappings: vec![],
        }
    }
}

impl SessionData {
    pub fn from_model(session: &Session) -> SessionData {
        SessionData {
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            always_auto_detect_mode: session.always_auto_detect.get(),
            send_feedback_only_if_armed: session.send_feedback_only_if_armed.get(),
            control_device_id: {
                use MidiControlInput::*;
                match session.midi_control_input.get() {
                    FxInput => None,
                    Device(dev) => Some(dev.id().to_string()),
                }
            },
            feedback_device_id: {
                use MidiFeedbackOutput::*;
                session.midi_feedback_output.get().map(|o| match o {
                    Device(dev) => dev.id().to_string(),
                    FxOutput => todo!("feedback to FX output not yet supported"),
                })
            },
            mappings: session
                .mappings()
                .map(|m| MappingModelData::from_model(m.borrow().deref()))
                .collect(),
        }
    }

    pub fn apply_to_model(&self, session: &mut Session) -> Result<(), &'static str> {
        let control_input = match self.control_device_id.as_ref() {
            None => MidiControlInput::FxInput,
            Some(dev_id_string) => {
                let raw_dev_id: u8 = dev_id_string
                    .parse()
                    .map_err(|_| "MIDI input device ID must be a number")?;
                let dev_id: MidiInputDeviceId = raw_dev_id
                    .try_into()
                    .map_err(|_| "invalid MIDI input device ID")?;
                MidiControlInput::Device(MidiInputDevice::new(dev_id))
            }
        };
        let feedback_output = match self.feedback_device_id.as_ref() {
            None => None,
            Some(dev_id_string) => {
                let raw_dev_id: u8 = dev_id_string
                    .parse()
                    .map_err(|_| "MIDI output device ID must be a number")?;
                let dev_id = MidiOutputDeviceId::new(raw_dev_id);
                Some(MidiFeedbackOutput::Device(MidiOutputDevice::new(dev_id)))
            }
        };
        session
            .let_matched_events_through
            .set(self.let_matched_events_through);
        session
            .let_unmatched_events_through
            .set(self.let_unmatched_events_through);
        session.always_auto_detect.set(self.always_auto_detect_mode);
        session
            .send_feedback_only_if_armed
            .set(self.send_feedback_only_if_armed);
        session.midi_control_input.set(control_input);
        session.midi_feedback_output.set(feedback_output);
        session.set_mappings(self.mappings.iter().map(MappingModelData::to_model));
        Ok(())
    }
}

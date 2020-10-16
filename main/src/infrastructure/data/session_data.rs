use crate::application::{ParameterSetting, Session};
use crate::domain::{
    MappingCompartment, MidiControlInput, MidiFeedbackOutput, PLUGIN_PARAMETER_COUNT,
};
use crate::infrastructure::data::{MappingModelData, ParameterData};
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Deref;

/// This is the structure for loading and saving a ReaLearn session.
///
/// It's optimized for being represented as JSON. The JSON representation must be 100%
/// backward-compatible.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionData {
    // Since ReaLearn 1.12.0
    id: Option<String>,
    let_matched_events_through: bool,
    let_unmatched_events_through: bool,
    always_auto_detect_mode: bool,
    send_feedback_only_if_armed: bool,
    /// `None` means "<FX input>"
    control_device_id: Option<String>,
    ///
    /// - `None` means "\<None>"
    /// - `Some("fx-output")` means "\<FX output>"
    feedback_device_id: Option<String>,
    mappings: Vec<MappingModelData>,
    controller_mappings: Vec<MappingModelData>,
    active_controller_id: Option<String>,
    parameters: HashMap<u32, ParameterData>,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            id: None,
            let_matched_events_through: false,
            let_unmatched_events_through: true,
            always_auto_detect_mode: true,
            // In older versions, feedback was always sent no matter if armed or not
            send_feedback_only_if_armed: false,
            control_device_id: None,
            feedback_device_id: None,
            mappings: vec![],
            controller_mappings: vec![],
            active_controller_id: None,
            parameters: Default::default(),
        }
    }
}

impl SessionData {
    pub fn from_model(session: &Session) -> SessionData {
        let from_mappings = |compartment| {
            session
                .mappings(compartment)
                .map(|m| MappingModelData::from_model(m.borrow().deref()))
                .collect()
        };
        SessionData {
            id: Some(session.id().to_string()),
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
                    FxOutput => "fx-output".to_string(),
                })
            },
            mappings: from_mappings(MappingCompartment::PrimaryMappings),
            controller_mappings: from_mappings(MappingCompartment::ControllerMappings),
            active_controller_id: session.active_controller_id().map(|id| id.to_string()),
            parameters: (0..PLUGIN_PARAMETER_COUNT)
                .filter_map(|i| {
                    let value = session.get_parameter(i);
                    let settings = session.get_parameter_settings(i);
                    if value == 0.0 && settings.custom_name.is_none() {
                        return None;
                    }
                    let data = ParameterData {
                        value,
                        name: settings.custom_name.clone(),
                    };
                    Some((i, data))
                })
                .collect(),
        }
    }

    /// Applies this session data to the given session.
    ///
    /// Doesn't notify listeners! Consumers must inform session that everything has changed.
    ///
    /// # Errors
    ///
    /// Returns and error if this session data is invalid.
    pub fn apply_to_model(&self, session: &mut Session) -> Result<(), &'static str> {
        // Validation
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
            Some(id) => {
                if id == "fx-output" {
                    Some(MidiFeedbackOutput::FxOutput)
                } else {
                    let raw_dev_id: u8 = id
                        .parse()
                        .map_err(|_| "MIDI output device ID must be a number")?;
                    let dev_id = MidiOutputDeviceId::new(raw_dev_id);
                    Some(MidiFeedbackOutput::Device(MidiOutputDevice::new(dev_id)))
                }
            }
        };
        // Mutation
        if let Some(id) = &self.id {
            session.id.set_without_notification(id.clone())
        };
        session
            .let_matched_events_through
            .set_without_notification(self.let_matched_events_through);
        session
            .let_unmatched_events_through
            .set_without_notification(self.let_unmatched_events_through);
        session.always_auto_detect.set(self.always_auto_detect_mode);
        session
            .send_feedback_only_if_armed
            .set_without_notification(self.send_feedback_only_if_armed);
        session
            .midi_control_input
            .set_without_notification(control_input);
        session
            .midi_feedback_output
            .set_without_notification(feedback_output);
        // Mappings
        let processor_context = session.context().clone();
        let mut apply_mappings = |compartment, mappings: &Vec<MappingModelData>| {
            session.set_mappings_without_notification(
                compartment,
                mappings
                    .iter()
                    .map(|m| m.to_model(compartment, Some(&processor_context))),
            );
        };
        apply_mappings(MappingCompartment::PrimaryMappings, &self.mappings);
        apply_mappings(
            MappingCompartment::ControllerMappings,
            &self.controller_mappings,
        );
        session.set_active_controller_id_without_notification(self.active_controller_id.clone());
        // Parameters
        let mut parameters = [0.0f32; PLUGIN_PARAMETER_COUNT as usize];
        let mut parameter_settings = vec![Default::default(); PLUGIN_PARAMETER_COUNT as usize];
        for (i, p) in self.parameters.iter() {
            parameters[*i as usize] = p.value;
            parameter_settings[*i as usize] = ParameterSetting {
                custom_name: p.name.clone(),
            };
        }
        session.set_parameters_without_notification(parameters);
        session.set_parameter_settings_without_notification(parameter_settings);
        Ok(())
    }
}

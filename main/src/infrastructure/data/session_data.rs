use crate::application::{MainPresetAutoLoadMode, ParameterSetting, Session};
use crate::core::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{
    MappingCompartment, MidiControlInput, MidiFeedbackOutput, ParameterArray,
    PLUGIN_PARAMETER_COUNT, ZEROED_PLUGIN_PARAMETERS,
};
use crate::infrastructure::data::{
    GroupModelData, MappingModelData, MigrationDescriptor, ParameterData,
};
use crate::infrastructure::plugin::App;
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Deref;

/// This is the structure for loading and saving a ReaLearn session.
///
/// It's optimized for being represented as JSON. The JSON representation must be 100%
/// backward-compatible.
// TODO-low Maybe call PluginData because it also contains parameter values (which are not part of
// the session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    // Since ReaLearn 1.12.0-pre18
    #[serde(default, skip_serializing_if = "is_default")]
    version: Option<Version>,
    // Since ReaLearn 1.12.0-pre?
    #[serde(default, skip_serializing_if = "is_default")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    let_matched_events_through: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    let_unmatched_events_through: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    always_auto_detect_mode: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    // false by default because in older versions, feedback was always sent no matter if armed or
    // not
    send_feedback_only_if_armed: bool,
    /// `None` means "<FX input>"
    #[serde(default, skip_serializing_if = "is_default")]
    // TODO-high Implement OSC device persistence
    control_device_id: Option<String>,
    ///
    /// - `None` means "\<None>"
    /// - `Some("fx-output")` means "\<FX output>"
    #[serde(default, skip_serializing_if = "is_default")]
    feedback_device_id: Option<String>,
    // Not set before 1.12.0-pre9
    #[serde(default, skip_serializing_if = "is_default")]
    default_group: Option<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    groups: Vec<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    controller_mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    active_controller_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    active_main_preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    main_preset_auto_load_mode: MainPresetAutoLoadMode,
    #[serde(default, skip_serializing_if = "is_default")]
    parameters: HashMap<u32, ParameterData>,
}

impl Default for SessionData {
    fn default() -> Self {
        use crate::application::session_defaults;
        Self {
            version: Some(App::version().clone()),
            id: None,
            let_matched_events_through: session_defaults::LET_MATCHED_EVENTS_THROUGH,
            let_unmatched_events_through: session_defaults::LET_UNMATCHED_EVENTS_THROUGH,
            always_auto_detect_mode: session_defaults::AUTO_CORRECT_SETTINGS,
            send_feedback_only_if_armed: session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED,
            control_device_id: None,
            feedback_device_id: None,
            default_group: None,
            groups: vec![],
            mappings: vec![],
            controller_mappings: vec![],
            active_controller_id: None,
            active_main_preset_id: None,
            main_preset_auto_load_mode: session_defaults::MAIN_PRESET_AUTO_LOAD_MODE,
            parameters: Default::default(),
        }
    }
}

impl SessionData {
    pub fn was_saved_with_newer_version(&self) -> bool {
        App::given_version_is_newer_than_app_version(self.version.as_ref())
    }

    pub fn from_model(session: &Session, parameters: &ParameterArray) -> SessionData {
        let from_mappings = |compartment| {
            session
                .mappings(compartment)
                .map(|m| MappingModelData::from_model(m.borrow().deref()))
                .collect()
        };
        SessionData {
            version: Some(App::version().clone()),
            id: Some(session.id().to_string()),
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            always_auto_detect_mode: session.auto_correct_settings.get(),
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
            default_group: Some(GroupModelData::from_model(
                session.default_group().borrow().deref(),
            )),
            groups: session
                .groups()
                .map(|m| GroupModelData::from_model(m.borrow().deref()))
                .collect(),
            mappings: from_mappings(MappingCompartment::MainMappings),
            controller_mappings: from_mappings(MappingCompartment::ControllerMappings),
            active_controller_id: session.active_controller_id().map(|id| id.to_string()),
            active_main_preset_id: session.active_main_preset_id().map(|id| id.to_string()),
            main_preset_auto_load_mode: session.main_preset_auto_load_mode.get(),
            parameters: (0..PLUGIN_PARAMETER_COUNT)
                .filter_map(|i| {
                    let value = parameters[i as usize];
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
        let migration_descriptor = MigrationDescriptor::new(self.version.as_ref());
        if let Some(id) = &self.id {
            session.id.set_without_notification(id.clone())
        };
        session
            .let_matched_events_through
            .set_without_notification(self.let_matched_events_through);
        session
            .let_unmatched_events_through
            .set_without_notification(self.let_unmatched_events_through);
        session
            .auto_correct_settings
            .set(self.always_auto_detect_mode);
        session
            .send_feedback_only_if_armed
            .set_without_notification(self.send_feedback_only_if_armed);
        session
            .midi_control_input
            .set_without_notification(control_input);
        session
            .midi_feedback_output
            .set_without_notification(feedback_output);
        // Groups
        let final_default_group = self
            .default_group
            .as_ref()
            .map(|g| g.to_model())
            .unwrap_or_default();
        session.default_group().replace(final_default_group);
        session.set_groups_without_notification(self.groups.iter().map(|g| g.to_model()));
        // Mappings
        let processor_context = session.context().clone();
        let mut apply_mappings = |compartment, mappings: &Vec<MappingModelData>| {
            session.set_mappings_without_notification(
                compartment,
                mappings.iter().map(|m| {
                    m.to_model(compartment, Some(&processor_context), &migration_descriptor)
                }),
            );
        };
        apply_mappings(MappingCompartment::MainMappings, &self.mappings);
        apply_mappings(
            MappingCompartment::ControllerMappings,
            &self.controller_mappings,
        );
        session.set_active_controller_id_without_notification(self.active_controller_id.clone());
        session.set_active_main_preset_id_without_notification(self.active_main_preset_id.clone());
        session
            .main_preset_auto_load_mode
            .set_without_notification(self.main_preset_auto_load_mode);
        // Parameters
        let mut parameter_settings = vec![Default::default(); PLUGIN_PARAMETER_COUNT as usize];
        for (i, p) in self.parameters.iter() {
            parameter_settings[*i as usize] = ParameterSetting {
                custom_name: p.name.clone(),
            };
        }
        session.set_parameter_settings_without_notification(parameter_settings);
        Ok(())
    }

    pub fn parameters_as_array(&self) -> ParameterArray {
        let mut parameters = ZEROED_PLUGIN_PARAMETERS;
        for (i, p) in self.parameters.iter() {
            parameters[*i as usize] = p.value;
        }
        parameters
    }
}

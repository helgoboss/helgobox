use crate::application::{
    empty_parameter_settings, reaper_supports_global_midi_filter, GroupModel,
    MainPresetAutoLoadMode, ParameterSetting, Session,
};
use crate::base::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{
    ExtendedProcessorContext, MappingCompartment, MidiControlInput, MidiDestination, OscDeviceId,
    ParameterArray, QualifiedSlotDescriptor, COMPARTMENT_PARAMETER_COUNT, ZEROED_PLUGIN_PARAMETERS,
};
use crate::infrastructure::data::{
    GroupModelData, MappingModelData, MigrationDescriptor, ParameterData,
};
use crate::infrastructure::plugin::App;

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
    lives_on_upper_floor: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    // false by default because in older versions, feedback was always sent no matter if armed or
    // not
    send_feedback_only_if_armed: bool,
    /// `None` means "<FX input>"
    #[serde(default, skip_serializing_if = "is_default")]
    control_device_id: Option<ControlDeviceId>,
    ///
    /// - `None` means "\<None>"
    /// - `Some("fx-output")` means "\<FX output>"
    #[serde(default, skip_serializing_if = "is_default")]
    feedback_device_id: Option<FeedbackDeviceId>,
    // Not set before 1.12.0-pre9
    #[serde(default, skip_serializing_if = "is_default")]
    default_group: Option<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    groups: Vec<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    default_controller_group: Option<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    controller_groups: Vec<GroupModelData>,
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
    #[serde(default, skip_serializing_if = "is_default")]
    controller_parameters: HashMap<u32, ParameterData>,
    #[serde(default, skip_serializing_if = "is_default")]
    clip_slots: Vec<QualifiedSlotDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum ControlDeviceId {
    Osc(OscDeviceId),
    Midi(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum FeedbackDeviceId {
    Osc(OscDeviceId),
    MidiOrFxOutput(String),
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
            lives_on_upper_floor: session_defaults::LIVES_ON_UPPER_FLOOR,
            send_feedback_only_if_armed: session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED,
            control_device_id: None,
            feedback_device_id: None,
            default_group: None,
            default_controller_group: None,
            groups: vec![],
            controller_groups: vec![],
            mappings: vec![],
            controller_mappings: vec![],
            active_controller_id: None,
            active_main_preset_id: None,
            main_preset_auto_load_mode: session_defaults::MAIN_PRESET_AUTO_LOAD_MODE,
            parameters: Default::default(),
            controller_parameters: Default::default(),
            clip_slots: vec![],
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
        let from_groups = |compartment| {
            session
                .groups(compartment)
                .map(|m| GroupModelData::from_model(m.borrow().deref()))
                .collect()
        };
        let from_group = |compartment| {
            Some(GroupModelData::from_model(
                session.default_group(compartment).borrow().deref(),
            ))
        };
        SessionData {
            version: Some(App::version().clone()),
            id: Some(session.id().to_string()),
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            always_auto_detect_mode: session.auto_correct_settings.get(),
            lives_on_upper_floor: session.lives_on_upper_floor.get(),
            send_feedback_only_if_armed: session.send_feedback_only_if_armed.get(),
            control_device_id: if let Some(osc_dev_id) = session.osc_input_device_id.get() {
                Some(ControlDeviceId::Osc(osc_dev_id))
            } else {
                use MidiControlInput::*;
                match session.midi_control_input.get() {
                    FxInput => None,
                    Device(dev_id) => Some(ControlDeviceId::Midi(dev_id.to_string())),
                }
            },
            feedback_device_id: if let Some(osc_dev_id) = session.osc_output_device_id.get() {
                Some(FeedbackDeviceId::Osc(osc_dev_id))
            } else {
                use MidiDestination::*;
                session.midi_feedback_output.get().map(|o| match o {
                    Device(dev_id) => FeedbackDeviceId::MidiOrFxOutput(dev_id.to_string()),
                    FxOutput => FeedbackDeviceId::MidiOrFxOutput("fx-output".to_owned()),
                })
            },
            default_group: from_group(MappingCompartment::MainMappings),
            default_controller_group: from_group(MappingCompartment::ControllerMappings),
            groups: from_groups(MappingCompartment::MainMappings),
            controller_groups: from_groups(MappingCompartment::ControllerMappings),
            mappings: from_mappings(MappingCompartment::MainMappings),
            controller_mappings: from_mappings(MappingCompartment::ControllerMappings),
            active_controller_id: session
                .active_controller_preset_id()
                .map(|id| id.to_string()),
            active_main_preset_id: session.active_main_preset_id().map(|id| id.to_string()),
            main_preset_auto_load_mode: session.main_preset_auto_load_mode.get(),
            parameters: get_parameter_data_map(
                session,
                parameters,
                MappingCompartment::MainMappings,
            ),
            controller_parameters: get_parameter_data_map(
                session,
                parameters,
                MappingCompartment::ControllerMappings,
            ),
            clip_slots: { session.instance_state().borrow().filled_slot_descriptors() },
        }
    }

    /// Applies this session data to the given session.
    ///
    /// Doesn't notify listeners! Consumers must inform session that everything has changed.
    ///
    /// # Errors
    ///
    /// Returns and error if this session data is invalid.
    pub fn apply_to_model(
        &self,
        session: &mut Session,
        params: &ParameterArray,
    ) -> Result<(), &'static str> {
        // Validation
        let (midi_control_input, osc_control_input) = match self.control_device_id.as_ref() {
            None => (MidiControlInput::FxInput, None),
            Some(dev_id) => {
                use ControlDeviceId::*;
                match dev_id {
                    Midi(midi_dev_id_string) => {
                        let raw_midi_dev_id = midi_dev_id_string
                            .parse::<u8>()
                            .map_err(|_| "invalid MIDI input device ID")?;
                        let midi_dev_id: MidiInputDeviceId = raw_midi_dev_id
                            .try_into()
                            .map_err(|_| "MIDI input device ID out of range")?;
                        (MidiControlInput::Device(midi_dev_id), None)
                    }
                    Osc(osc_dev_id) => (MidiControlInput::FxInput, Some(*osc_dev_id)),
                }
            }
        };
        let (midi_feedback_output, osc_feedback_output) = match self.feedback_device_id.as_ref() {
            None => (None, None),
            Some(dev_id) => {
                use FeedbackDeviceId::*;
                match dev_id {
                    MidiOrFxOutput(s) if s == "fx-output" => {
                        (Some(MidiDestination::FxOutput), None)
                    }
                    MidiOrFxOutput(midi_dev_id_string) => {
                        let midi_dev_id = midi_dev_id_string
                            .parse::<u8>()
                            .map(MidiOutputDeviceId::new)
                            .map_err(|_| "invalid MIDI output device ID")?;
                        (Some(MidiDestination::Device(midi_dev_id)), None)
                    }
                    Osc(osc_dev_id) => (None, Some(*osc_dev_id)),
                }
            }
        };
        // Mutation
        let migration_descriptor = MigrationDescriptor::new(self.version.as_ref());
        if let Some(id) = &self.id {
            session.id.set_without_notification(id.clone())
        };
        session
            .auto_correct_settings
            .set(self.always_auto_detect_mode);
        session.lives_on_upper_floor.set(self.lives_on_upper_floor);
        session
            .send_feedback_only_if_armed
            .set_without_notification(self.send_feedback_only_if_armed);
        session
            .midi_control_input
            .set_without_notification(midi_control_input);
        session
            .osc_input_device_id
            .set_without_notification(osc_control_input);
        session
            .midi_feedback_output
            .set_without_notification(midi_feedback_output);
        session
            .osc_output_device_id
            .set_without_notification(osc_feedback_output);
        // Let events through or not
        {
            let is_old_preset = self
                .version
                .as_ref()
                .map(|v| v < &Version::parse("2.10.0-pre.10").unwrap())
                .unwrap_or(true);
            let (matched, unmatched) = if is_old_preset && session.control_input().is_midi_device()
            {
                // Old presets using MIDI device input didn't support global MIDI filtering. For
                // backward compatibility, make sure that all messages are let through then!
                (true, true)
            } else if reaper_supports_global_midi_filter() {
                // This is a new preset and REAPER supports global MIDI filtering.
                (
                    self.let_matched_events_through,
                    self.let_unmatched_events_through,
                )
            } else {
                // This is a new preset but REAPER doesn't support global MIDI filtering.
                (true, true)
            };
            session
                .let_matched_events_through
                .set_without_notification(matched);
            session
                .let_unmatched_events_through
                .set_without_notification(unmatched);
        }
        // Groups
        let get_final_default_group =
            |def_group: Option<&GroupModelData>, compartment: MappingCompartment| {
                def_group
                    .map(|g| g.to_model(compartment))
                    .unwrap_or_else(|| GroupModel::default_for_compartment(compartment))
            };
        session
            .default_group(MappingCompartment::MainMappings)
            .replace(get_final_default_group(
                self.default_group.as_ref(),
                MappingCompartment::MainMappings,
            ));
        session.set_groups_without_notification(
            MappingCompartment::MainMappings,
            self.groups
                .iter()
                .map(|g| g.to_model(MappingCompartment::MainMappings)),
        );
        session
            .default_group(MappingCompartment::ControllerMappings)
            .replace(get_final_default_group(
                self.default_controller_group.as_ref(),
                MappingCompartment::ControllerMappings,
            ));
        session.set_groups_without_notification(
            MappingCompartment::ControllerMappings,
            self.controller_groups
                .iter()
                .map(|g| g.to_model(MappingCompartment::ControllerMappings)),
        );
        // Mappings
        let context = session.context().clone();
        let extended_context = ExtendedProcessorContext::new(&context, params);
        let mut apply_mappings = |compartment, mappings: &Vec<MappingModelData>| {
            session.set_mappings_without_notification(
                compartment,
                mappings.iter().map(|m| {
                    m.to_model_flexible(
                        compartment,
                        Some(extended_context),
                        &migration_descriptor,
                        self.version.as_ref(),
                    )
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
        session.set_parameter_settings_without_notification(
            MappingCompartment::MainMappings,
            get_parameter_settings(&self.parameters),
        );
        session.set_parameter_settings_without_notification(
            MappingCompartment::ControllerMappings,
            get_parameter_settings(&self.controller_parameters),
        );
        // Clip slots
        {
            let mut instance_state = session.instance_state().borrow_mut();
            instance_state.load_slots(
                self.clip_slots.clone(),
                Some(session.context().project_or_current_project()),
            )?;
        }
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

fn get_parameter_data_map(
    session: &Session,
    parameters: &ParameterArray,
    compartment: MappingCompartment,
) -> HashMap<u32, ParameterData> {
    (0..COMPARTMENT_PARAMETER_COUNT)
        .filter_map(|i| {
            let parameter_slice = compartment.slice_params(parameters);
            let value = parameter_slice[i as usize];
            let settings = session.get_parameter_settings(compartment, i);
            if value == 0.0 && settings.name.is_empty() {
                return None;
            }
            let data = ParameterData {
                value,
                name: settings.name.clone(),
            };
            Some((i, data))
        })
        .collect()
}

fn get_parameter_settings(data_map: &HashMap<u32, ParameterData>) -> Vec<ParameterSetting> {
    let mut settings = empty_parameter_settings();
    for (i, p) in data_map.iter() {
        settings[*i as usize] = ParameterSetting {
            name: p.name.clone(),
        };
    }
    settings
}

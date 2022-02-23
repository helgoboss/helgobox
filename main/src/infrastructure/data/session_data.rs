use crate::application::{
    empty_parameter_settings, reaper_supports_global_midi_filter, CompartmentInSession, GroupModel,
    MainPresetAutoLoadMode, ParameterSetting, Session, SessionState, TargetCategory,
    VirtualTrackType,
};
use crate::base::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{
    GroupId, GroupKey, InstanceState, MappingCompartment, MappingId, MidiControlInput,
    MidiDestination, OscDeviceId, ParameterArray, ReaperTargetType, Tag, TransportAction,
    COMPARTMENT_PARAMETER_COUNT, ZEROED_PLUGIN_PARAMETERS,
};
use crate::infrastructure::data::{
    deserialize_track, ensure_no_duplicate_compartment_data, GroupModelData, MappingModelData,
    MigrationDescriptor, ParameterData,
};
use crate::infrastructure::plugin::App;

use crate::base::notification;
use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use playtime_api::Matrix;
use playtime_clip_engine::main::{LegacyClipOutput, LegacySlotDescriptor, QualifiedSlotDescriptor};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::error::Error;
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
    pub version: Option<Version>,
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
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(default, skip_serializing_if = "is_default")]
    parameters: HashMap<String, ParameterData>,
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(default, skip_serializing_if = "is_default")]
    controller_parameters: HashMap<String, ParameterData>,
    // Legacy (ReaLearn <= 2.12.0-pre.4)
    #[serde(default, skip_serializing_if = "is_default")]
    clip_slots: Vec<QualifiedSlotDescriptor>,
    // New since 2.12.0-pre.5
    #[serde(default, skip_serializing_if = "is_default")]
    clip_matrix: Option<Matrix>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "is_default")]
    controller: CompartmentState,
    #[serde(default, skip_serializing_if = "is_default")]
    main: CompartmentState,
    #[serde(default, skip_serializing_if = "is_default")]
    active_instance_tags: HashSet<Tag>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CompartmentState {
    #[serde(default, skip_serializing_if = "is_default")]
    active_mapping_by_group: HashMap<GroupId, MappingId>,
    #[serde(default, skip_serializing_if = "is_default")]
    active_mapping_tags: HashSet<Tag>,
}

impl CompartmentState {
    fn from_instance_state(
        instance_state: &InstanceState,
        compartment: MappingCompartment,
    ) -> Self {
        CompartmentState {
            active_mapping_by_group: instance_state.active_mapping_by_group(compartment).clone(),
            active_mapping_tags: instance_state.active_mapping_tags(compartment).clone(),
        }
    }
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
            clip_matrix: None,
            tags: vec![],
            controller: Default::default(),
            main: Default::default(),
            active_instance_tags: Default::default(),
        }
    }
}

impl SessionData {
    pub fn from_model(session: &Session, parameters: &ParameterArray) -> SessionData {
        let from_mappings = |compartment| {
            let compartment_in_session = CompartmentInSession {
                session,
                compartment,
            };
            session
                .mappings(compartment)
                .map(|m| MappingModelData::from_model(m.borrow().deref(), &compartment_in_session))
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
        let instance_state = session.instance_state().borrow();
        let session_state = session.state().borrow();
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
                .active_preset_id(MappingCompartment::ControllerMappings)
                .map(|id| id.to_string()),
            active_main_preset_id: session
                .active_preset_id(MappingCompartment::MainMappings)
                .map(|id| id.to_string()),
            main_preset_auto_load_mode: session.main_preset_auto_load_mode.get(),
            parameters: get_parameter_data_map(
                &session_state,
                parameters,
                MappingCompartment::MainMappings,
            ),
            controller_parameters: get_parameter_data_map(
                &session_state,
                parameters,
                MappingCompartment::ControllerMappings,
            ),
            clip_slots: vec![],
            clip_matrix: instance_state.clip_matrix().map(|m| m.save()),
            tags: session.tags.get_ref().clone(),
            controller: CompartmentState::from_instance_state(
                &instance_state,
                MappingCompartment::ControllerMappings,
            ),
            main: CompartmentState::from_instance_state(
                &instance_state,
                MappingCompartment::MainMappings,
            ),
            active_instance_tags: instance_state.active_instance_tags().clone(),
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
    ) -> Result<(), Box<dyn Error>> {
        // Validation
        ensure_no_duplicate_compartment_data(
            &self.mappings,
            &self.groups,
            self.parameters.values().map(|p| &p.settings),
        )?;
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
                    .map(|g| g.to_model(compartment, true))
                    .unwrap_or_else(|| GroupModel::default_for_compartment(compartment))
            };
        let main_default_group = get_final_default_group(
            self.default_group.as_ref(),
            MappingCompartment::MainMappings,
        );
        let controller_default_group = get_final_default_group(
            self.default_controller_group.as_ref(),
            MappingCompartment::ControllerMappings,
        );
        session
            .default_group(MappingCompartment::MainMappings)
            .replace(main_default_group);
        let main_groups: Vec<_> = self
            .groups
            .iter()
            .map(|g| g.to_model(MappingCompartment::MainMappings, false))
            .collect();
        let controller_groups: Vec<_> = self
            .controller_groups
            .iter()
            .map(|g| g.to_model(MappingCompartment::ControllerMappings, false))
            .collect();
        session.set_groups_without_notification(MappingCompartment::MainMappings, main_groups);
        session
            .default_group(MappingCompartment::ControllerMappings)
            .replace(controller_default_group);
        session.set_groups_without_notification(
            MappingCompartment::ControllerMappings,
            controller_groups,
        );
        // Mappings
        let mut apply_mappings = |compartment, mappings: &Vec<MappingModelData>| {
            let mappings: Vec<_> = mappings
                .iter()
                .map(|m| {
                    m.to_model_flexible(
                        compartment,
                        &migration_descriptor,
                        self.version.as_ref(),
                        session.compartment_in_session(compartment),
                        Some(session.extended_context_with_params(params)),
                    )
                })
                .collect();
            session.set_mappings_without_notification(compartment, mappings);
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
        session.tags.set_without_notification(self.tags.clone());
        // Parameters
        {
            let mut session_state = session.state().borrow_mut();
            session_state.set_parameter_settings_without_notification(
                MappingCompartment::MainMappings,
                get_parameter_settings(&self.parameters),
            );
            session_state.set_parameter_settings_without_notification(
                MappingCompartment::ControllerMappings,
                get_parameter_settings(&self.controller_parameters),
            );
        }
        // Instance state
        {
            let mut instance_state = session.instance_state().borrow_mut();
            // Legacy clips
            if let Some(matrix) = &self.clip_matrix {
                instance_state
                    .require_clip_matrix_mut()
                    .load(matrix.clone())?;
            } else if !self.clip_slots.is_empty() {
                instance_state.require_clip_matrix_mut().load_slots_legacy(
                    self.clip_slots
                        .iter()
                        .map(|desc| LegacySlotDescriptor {
                            output: self.determine_legacy_clip_track(desc.index),
                            index: desc.index,
                            clip: desc.descriptor.clone(),
                        })
                        .collect(),
                    Some(session.context().project_or_current_project()),
                )?;
            } else {
                instance_state.shut_down_clip_matrix();
            }
            instance_state
                .set_active_instance_tags_without_notification(self.active_instance_tags.clone());
            // Compartment-specific
            instance_state.set_active_mapping_by_group(
                MappingCompartment::ControllerMappings,
                self.controller.active_mapping_by_group.clone(),
            );
            instance_state.set_active_mapping_by_group(
                MappingCompartment::MainMappings,
                self.main.active_mapping_by_group.clone(),
            );
            instance_state.set_active_mapping_tags(
                MappingCompartment::ControllerMappings,
                self.controller.active_mapping_tags.clone(),
            );
            instance_state.set_active_mapping_tags(
                MappingCompartment::MainMappings,
                self.main.active_mapping_tags.clone(),
            );
        }
        Ok(())
    }

    fn determine_legacy_clip_track(&self, slot_index: usize) -> LegacyClipOutput {
        let mut candidates: Vec<_> = self
            .mappings
            .iter()
            .chain(self.controller_mappings.iter())
            .filter_map(|m| {
                if m.target.category == TargetCategory::Reaper
                    && m.target.r#type == ReaperTargetType::ClipTransport
                    && matches!(m.target.transport_action, TransportAction::PlayPause | TransportAction::PlayStop)
                    && m.target.slot_index == slot_index
                {
                    let prop_values = deserialize_track(&m.target.track_data);
                    use VirtualTrackType::*;
                    let t = match prop_values.r#type {
                        This => LegacyClipOutput::ThisTrack,
                        Selected | AllSelected | Dynamic => {
                            warn_about_legacy_clip_loss(slot_index, "The clip play target used track \"Selected\", \"All selected\" or \"Dynamic\" which is not supported anymore. Falling back to playing slot on \"This\" track.");
                            LegacyClipOutput::ThisTrack
                        },
                        Master => LegacyClipOutput::MasterTrack,
                        ById => if let Some(id) = prop_values.id { LegacyClipOutput::TrackById(id) } else {
                            LegacyClipOutput::ThisTrack
                        },
                        ByName => LegacyClipOutput::TrackByName(prop_values.name),
                        AllByName => {
                            warn_about_legacy_clip_loss(slot_index, "The clip play target used track \"All by name\" which is not supported anymore. Falling back to identifying track by name.");
                            LegacyClipOutput::TrackByName(prop_values.name)
                        },
                        ByIndex => LegacyClipOutput::TrackByIndex(prop_values.index),
                        ByIdOrName => if let Some(id) = prop_values.id {
                            LegacyClipOutput::TrackById(id)
                        } else {
                            LegacyClipOutput::TrackByName(prop_values.name)
                        }
                    };
                    Some(t)
                } else {
                    None
                }
            }).collect();
        if candidates.len() > 1 {
            warn_about_legacy_clip_loss(slot_index, "This clip was referred to by multiple clip play targets. Only the first one will be taken into account.");
        }
        let res = if let Some(first) = candidates.drain(..).next() {
            first
        } else {
            warn_about_legacy_clip_loss(slot_index, "There was no corresponding play target for this clip. Clip will be played on the master track instead.");
            LegacyClipOutput::MasterTrack
        };
        res
    }

    pub fn parameters_as_array(&self) -> ParameterArray {
        let mut parameters = ZEROED_PLUGIN_PARAMETERS;
        for (i, p) in self.parameters.iter() {
            if let Ok(i) = i.parse::<u32>() {
                parameters[i as usize] = p.value;
            }
        }
        parameters
    }
}

fn get_parameter_data_map(
    session_state: &SessionState,
    parameters: &ParameterArray,
    compartment: MappingCompartment,
) -> HashMap<String, ParameterData> {
    (0..COMPARTMENT_PARAMETER_COUNT)
        .filter_map(|i| {
            let parameter_slice = compartment.slice_params(parameters);
            let value = parameter_slice[i as usize];
            let settings = session_state.get_parameter_settings(compartment, i);
            if value == 0.0 && settings.name.is_empty() {
                return None;
            }
            let data = ParameterData {
                settings: settings.clone(),
                value,
            };
            Some((i.to_string(), data))
        })
        .collect()
}

fn get_parameter_settings(data_map: &HashMap<String, ParameterData>) -> Vec<ParameterSetting> {
    let mut settings = empty_parameter_settings();
    for (i, p) in data_map.iter() {
        if let Ok(i) = i.parse::<u32>() {
            settings[i as usize] = p.settings.clone();
        }
    }
    settings
}

impl<'a> ModelToDataConversionContext for CompartmentInSession<'a> {
    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        let group = self.session.find_group_by_id(self.compartment, group_id)?;
        Some(group.borrow().key().clone())
    }
}

impl<'a> DataToModelConversionContext for CompartmentInSession<'a> {
    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        let group = self.session.find_group_by_key(self.compartment, key)?;
        Some(group.borrow().id())
    }
}

impl<'a> ApiToDataConversionContext for CompartmentInSession<'a> {
    fn param_index_by_key(&self, key: &str) -> Option<u32> {
        let (i, _) = self
            .session
            .state()
            .borrow()
            .find_parameter_setting_by_key(self.compartment, key)?;
        Some(i)
    }
}

pub trait ModelToDataConversionContext {
    fn group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        if group_id.is_default() {
            return Some(GroupKey::default());
        }
        self.non_default_group_key_by_id(group_id)
    }

    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey>;
}

pub trait DataToModelConversionContext {
    fn group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        if key.is_empty() {
            return Some(GroupId::default());
        }
        self.non_default_group_id_by_key(key)
    }

    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId>;
}

fn warn_about_legacy_clip_loss(slot_index: usize, msg: &str) {
    notification::warn(format!(
        "\
        You have loaded a preset that makes use of the experimental clip engine contained in \
        older ReaLearn versions. This engine is now legacy and replaced by a new engine. The new \
        engine is better in many ways but doesn't support some of the more exotic features of the \
        old engine. In particular, clip {} will probably behave differently now. Details: {}
        ",
        slot_index + 1,
        msg
    ))
}

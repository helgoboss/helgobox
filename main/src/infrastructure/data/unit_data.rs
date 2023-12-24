use crate::application::{
    reaper_supports_global_midi_filter, CompartmentCommand, CompartmentInSession,
    FxPresetLinkConfig, GroupModel, InstanceModel, MainPresetAutoLoadMode, SessionCommand,
    WeakInstanceModel,
};
use crate::domain::{
    compartment_param_index_iter, Compartment, CompartmentParamIndex, CompartmentParams,
    ControlInput, FeedbackOutput, GroupId, GroupKey, MappingId, MappingKey,
    MappingSnapshotContainer, MappingSnapshotId, MidiControlInput, MidiDestination, OscDeviceId,
    Param, PluginParams, StayActiveWhenProjectInBackground, Tag, Unit, UnitId,
};
use crate::infrastructure::data::{
    convert_target_value_to_api, convert_target_value_to_model,
    ensure_no_duplicate_compartment_data, CompartmentModelData, GroupModelData, MappingModelData,
    MigrationDescriptor, ParameterData,
};
use crate::infrastructure::plugin::BackboneShell;
use base::default_util::{bool_true, deserialize_null_default, is_bool_true, is_default};

use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use realearn_api::persistence::{
    FxDescriptor, MappingInSnapshot, MappingSnapshot, TrackDescriptor,
};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ops::Deref;

/// This is the structure for loading and saving a ReaLearn session.
///
/// It's optimized for being represented as JSON. The JSON representation must be 100%
/// backward-compatible.
// TODO-low Maybe call PluginData because it also contains parameter values (which are not part of
// the session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitData {
    // Since ReaLearn 1.12.0-pre18
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub version: Option<Version>,
    // Since ReaLearn 1.12.0-pre?
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    let_matched_events_through: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    let_unmatched_events_through: bool,
    /// Introduced with ReaLearn 2.14.0-pre.1. Before that "Always".
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    stay_active_when_project_in_background: Option<StayActiveWhenProjectInBackground>,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    always_auto_detect_mode: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    lives_on_upper_floor: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    // false by default because in older versions, feedback was always sent no matter if armed or
    // not
    send_feedback_only_if_armed: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    reset_feedback_when_releasing_source: bool,
    /// `None` means "<FX input>"
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    control_device_id: Option<ControlDeviceId>,
    ///
    /// - `None` means "\<None>"
    /// - `Some("fx-output")` means "\<FX output>"
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    feedback_device_id: Option<FeedbackDeviceId>,
    // Not set before 1.12.0-pre9
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    default_group: Option<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    groups: Vec<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    default_controller_group: Option<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_groups: Vec<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    mappings: Vec<MappingModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_mappings: Vec<MappingModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_custom_data: HashMap<String, serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_notes: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    main_notes: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_controller_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_main_preset_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    main_preset_auto_load_mode: MainPresetAutoLoadMode,
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    parameters: HashMap<String, ParameterData>,
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_parameters: HashMap<String, ParameterData>,
    // Legacy (ReaLearn <= 2.12.0-pre.4)
    #[cfg(feature = "playtime")]
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    clip_slots: Vec<crate::infrastructure::data::clip_legacy::QualifiedSlotDescriptor>,
    // New since 2.12.0-pre.5
    #[cfg(feature = "playtime")]
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    clip_matrix: Option<ClipMatrixRefData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub tags: Vec<Tag>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller: CompartmentState,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    main: CompartmentState,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_instance_tags: HashSet<Tag>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    instance_preset_link_config: FxPresetLinkConfig,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    use_instance_preset_links_only: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    instance_track: TrackDescriptor,
    #[serde(default = "focused_fx_descriptor")]
    instance_fx: FxDescriptor,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    mapping_snapshots: Vec<MappingSnapshot>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_mapping_snapshots: Vec<MappingSnapshot>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pot_state: pot::PersistentState,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    memorized_main_compartment: Option<CompartmentModelData>,
}

fn focused_fx_descriptor() -> FxDescriptor {
    FxDescriptor::Focused
}

#[cfg(feature = "playtime")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum ClipMatrixRefData {
    Own(Box<playtime_api::persistence::FlexibleMatrix>),
    Foreign(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CompartmentState {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_mapping_by_group: HashMap<GroupId, MappingId>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_mapping_tags: HashSet<Tag>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_mapping_snapshots: HashMap<Tag, MappingSnapshotId>,
}

impl CompartmentState {
    fn from_instance_state(instance_state: &Unit, compartment: Compartment) -> Self {
        CompartmentState {
            active_mapping_by_group: instance_state.active_mapping_by_group(compartment).clone(),
            active_mapping_tags: instance_state.active_mapping_tags(compartment).clone(),
            active_mapping_snapshots: instance_state
                .mapping_snapshot_container(compartment)
                .active_snapshot_id_by_tag()
                .clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum ControlDeviceId {
    Keyboard(KeyboardDevice),
    Osc(OscDeviceId),
    Midi(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum KeyboardDevice {
    #[serde(rename = "keyboard")]
    TheKeyboard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum FeedbackDeviceId {
    Osc(OscDeviceId),
    MidiOrFxOutput(String),
}

impl Default for UnitData {
    fn default() -> Self {
        use crate::application::session_defaults;
        Self {
            version: Some(BackboneShell::version().clone()),
            id: None,
            let_matched_events_through: session_defaults::LET_MATCHED_EVENTS_THROUGH,
            let_unmatched_events_through: session_defaults::LET_UNMATCHED_EVENTS_THROUGH,
            stay_active_when_project_in_background: Some(
                session_defaults::STAY_ACTIVE_WHEN_PROJECT_IN_BACKGROUND,
            ),
            always_auto_detect_mode: session_defaults::AUTO_CORRECT_SETTINGS,
            lives_on_upper_floor: session_defaults::LIVES_ON_UPPER_FLOOR,
            send_feedback_only_if_armed: session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED,
            reset_feedback_when_releasing_source:
                session_defaults::RESET_FEEDBACK_WHEN_RELEASING_SOURCE,
            control_device_id: None,
            feedback_device_id: None,
            default_group: None,
            default_controller_group: None,
            groups: vec![],
            controller_groups: vec![],
            mappings: vec![],
            controller_mappings: vec![],
            controller_custom_data: Default::default(),
            controller_notes: Default::default(),
            main_notes: Default::default(),
            active_controller_id: None,
            active_main_preset_id: None,
            main_preset_auto_load_mode: session_defaults::MAIN_PRESET_AUTO_LOAD_MODE,
            parameters: Default::default(),
            controller_parameters: Default::default(),
            #[cfg(feature = "playtime")]
            clip_slots: vec![],
            #[cfg(feature = "playtime")]
            clip_matrix: None,
            tags: vec![],
            controller: Default::default(),
            main: Default::default(),
            active_instance_tags: Default::default(),
            instance_preset_link_config: Default::default(),
            use_instance_preset_links_only: false,
            instance_track: Default::default(),
            instance_fx: session_defaults::INSTANCE_FX_DESCRIPTOR,
            mapping_snapshots: vec![],
            controller_mapping_snapshots: vec![],
            pot_state: Default::default(),
            memorized_main_compartment: None,
        }
    }
}

impl UnitData {
    /// The given parameters are the canonical ones from `RealearnPluginParameters`.
    pub fn from_model(session: &InstanceModel, plugin_params: &PluginParams) -> UnitData {
        let from_mappings = |compartment| {
            let compartment_in_session = CompartmentInSession::new(session, compartment);
            session
                .mappings(compartment)
                .map(|m| MappingModelData::from_model(m.borrow().deref(), &compartment_in_session))
                .collect()
        };
        let from_groups = |compartment| {
            session
                .groups(compartment)
                .map(|m| {
                    let compartment_in_session = CompartmentInSession::new(session, compartment);
                    GroupModelData::from_model(m.borrow().deref(), &compartment_in_session)
                })
                .collect()
        };
        let from_group = |compartment| {
            let compartment_in_session = CompartmentInSession::new(session, compartment);
            let group_model_data = GroupModelData::from_model(
                session.default_group(compartment).borrow().deref(),
                &compartment_in_session,
            );
            Some(group_model_data)
        };
        let main_preset_auto_load_mode = session.main_preset_auto_load_mode.get();
        let instance_state = session.instance_state().borrow();
        UnitData {
            version: Some(BackboneShell::version().clone()),
            id: Some(session.id().to_string()),
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            stay_active_when_project_in_background: Some(
                session.stay_active_when_project_in_background.get(),
            ),
            always_auto_detect_mode: session.auto_correct_settings.get(),
            lives_on_upper_floor: session.lives_on_upper_floor.get(),
            send_feedback_only_if_armed: session.send_feedback_only_if_armed.get(),
            reset_feedback_when_releasing_source: session
                .reset_feedback_when_releasing_source
                .get(),
            control_device_id: {
                match session.control_input() {
                    ControlInput::Midi(MidiControlInput::FxInput) => None,
                    ControlInput::Midi(MidiControlInput::Device(dev_id)) => {
                        Some(ControlDeviceId::Midi(dev_id.to_string()))
                    }
                    ControlInput::Osc(dev_id) => Some(ControlDeviceId::Osc(dev_id)),
                    ControlInput::Keyboard => {
                        Some(ControlDeviceId::Keyboard(KeyboardDevice::TheKeyboard))
                    }
                }
            },
            feedback_device_id: {
                session.feedback_output().map(|output| match output {
                    FeedbackOutput::Midi(MidiDestination::FxOutput) => {
                        FeedbackDeviceId::MidiOrFxOutput("fx-output".to_owned())
                    }
                    FeedbackOutput::Midi(MidiDestination::Device(dev_id)) => {
                        FeedbackDeviceId::MidiOrFxOutput(dev_id.to_string())
                    }
                    FeedbackOutput::Osc(dev_id) => FeedbackDeviceId::Osc(dev_id),
                })
            },
            default_group: from_group(Compartment::Main),
            default_controller_group: from_group(Compartment::Controller),
            groups: from_groups(Compartment::Main),
            controller_groups: from_groups(Compartment::Controller),
            mappings: from_mappings(Compartment::Main),
            controller_mappings: from_mappings(Compartment::Controller),
            controller_custom_data: session
                .custom_compartment_data(Compartment::Controller)
                .clone(),
            controller_notes: session
                .compartment_notes(Compartment::Controller)
                .to_owned(),
            main_notes: session.compartment_notes(Compartment::Main).to_owned(),
            active_controller_id: session
                .active_preset_id(Compartment::Controller)
                .map(|id| id.to_string()),
            active_main_preset_id: session
                .active_preset_id(Compartment::Main)
                .map(|id| id.to_string()),
            main_preset_auto_load_mode,
            parameters: get_parameter_data_map(plugin_params, Compartment::Main),
            controller_parameters: get_parameter_data_map(plugin_params, Compartment::Controller),
            #[cfg(feature = "playtime")]
            clip_slots: vec![],
            #[cfg(feature = "playtime")]
            clip_matrix: {
                instance_state
                    .clip_matrix_ref()
                    .and_then(|matrix_ref| match matrix_ref {
                        crate::domain::ClipMatrixRef::Own(m) => {
                            Some(ClipMatrixRefData::Own(Box::new(m.save())))
                        }
                        crate::domain::ClipMatrixRef::Foreign(instance_id) => {
                            let foreign_session = BackboneShell::get()
                                .find_session_by_instance_id_ignoring_borrowed_ones(*instance_id)?;
                            let foreign_id = foreign_session.borrow().id().to_owned();
                            Some(ClipMatrixRefData::Foreign(foreign_id))
                        }
                    })
            },
            tags: session.tags.get_ref().clone(),
            controller: CompartmentState::from_instance_state(
                &instance_state,
                Compartment::Controller,
            ),
            main: CompartmentState::from_instance_state(&instance_state, Compartment::Main),
            active_instance_tags: instance_state.active_instance_tags().clone(),
            instance_preset_link_config: session.instance_preset_link_config().clone(),
            use_instance_preset_links_only: session.use_instance_preset_links_only(),
            instance_track: session.instance_track_descriptor().clone(),
            instance_fx: session.instance_fx_descriptor().clone(),
            mapping_snapshots: convert_mapping_snapshots_to_api(
                session,
                &instance_state,
                Compartment::Main,
            ),
            controller_mapping_snapshots: convert_mapping_snapshots_to_api(
                session,
                &instance_state,
                Compartment::Controller,
            ),
            pot_state: instance_state.save_pot_unit(),
            memorized_main_compartment: session
                .memorized_main_compartment()
                .map(CompartmentModelData::from_model),
        }
    }

    /// Applies this session data to the given session.
    ///
    /// Doesn't notify listeners! Consumers must inform session that everything has changed.
    ///
    /// # Errors
    ///
    /// Returns and error if this session data is invalid.
    #[allow(unused_variables)]
    pub fn apply_to_model(
        &self,
        session: &mut InstanceModel,
        params: &PluginParams,
        weak_session: WeakInstanceModel,
    ) -> Result<(), Box<dyn Error>> {
        // Validation
        let main_conversion_context = SimpleDataToModelConversionContext::from_session_or_random(
            &self.groups,
            &self.mappings,
            Some(CompartmentInSession::new(session, Compartment::Main)),
        );
        ensure_no_duplicate_compartment_data(
            &self.mappings,
            &self.groups,
            self.parameters.values().map(|p| &p.setting),
        )?;
        ensure_no_duplicate_compartment_data(
            &self.controller_mappings,
            &self.controller_groups,
            self.controller_parameters.values().map(|p| &p.setting),
        )?;
        let control_input = match self.control_device_id.as_ref() {
            None => ControlInput::Midi(MidiControlInput::FxInput),
            Some(dev_id) => {
                use ControlDeviceId::*;
                match dev_id {
                    Keyboard(_) => ControlInput::Keyboard,
                    Midi(midi_dev_id_string) => {
                        let raw_midi_dev_id = midi_dev_id_string
                            .parse::<u8>()
                            .map_err(|_| "invalid MIDI input device ID")?;
                        let midi_dev_id = MidiInputDeviceId::new(raw_midi_dev_id);
                        ControlInput::Midi(MidiControlInput::Device(midi_dev_id))
                    }
                    Osc(osc_dev_id) => ControlInput::Osc(*osc_dev_id),
                }
            }
        };
        let feedback_output = match self.feedback_device_id.as_ref() {
            None => None,
            Some(dev_id) => {
                use FeedbackDeviceId::*;
                let output = match dev_id {
                    MidiOrFxOutput(s) if s == "fx-output" => {
                        FeedbackOutput::Midi(MidiDestination::FxOutput)
                    }
                    MidiOrFxOutput(midi_dev_id_string) => {
                        let midi_dev_id = midi_dev_id_string
                            .parse::<u8>()
                            .map(MidiOutputDeviceId::new)
                            .map_err(|_| "invalid MIDI output device ID")?;
                        FeedbackOutput::Midi(MidiDestination::Device(midi_dev_id))
                    }
                    Osc(osc_dev_id) => FeedbackOutput::Osc(*osc_dev_id),
                };
                Some(output)
            }
        };
        let main_mapping_snapshot_container = convert_mapping_snapshots_to_model(
            &self.mapping_snapshots,
            &self.main.active_mapping_snapshots,
            &main_conversion_context,
        )?;
        let controller_mapping_snapshot_container = convert_mapping_snapshots_to_model(
            &self.controller_mapping_snapshots,
            &self.controller.active_mapping_snapshots,
            &main_conversion_context,
        )?;
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
            .reset_feedback_when_releasing_source
            .set_without_notification(self.reset_feedback_when_releasing_source);
        session
            .control_input
            .set_without_notification(control_input);
        session
            .feedback_output
            .set_without_notification(feedback_output);
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
            let stay_active_when_project_in_background = self
                .stay_active_when_project_in_background
                .unwrap_or(StayActiveWhenProjectInBackground::Always);
            session
                .stay_active_when_project_in_background
                .set_without_notification(stay_active_when_project_in_background);
        }
        // Groups
        let controller_conversion_context =
            SimpleDataToModelConversionContext::from_session_or_random(
                &self.controller_groups,
                &self.controller_mappings,
                Some(CompartmentInSession::new(session, Compartment::Controller)),
            );
        let conversion_context = |compartment: Compartment| match compartment {
            Compartment::Controller => &controller_conversion_context,
            Compartment::Main => &main_conversion_context,
        };
        let get_final_default_group =
            |def_group: Option<&GroupModelData>, compartment: Compartment| {
                def_group
                    .map(|g| g.to_model(compartment, true, conversion_context(compartment)))
                    .unwrap_or_else(|| GroupModel::default_for_compartment(compartment))
            };
        let main_default_group =
            get_final_default_group(self.default_group.as_ref(), Compartment::Main);
        let controller_default_group = get_final_default_group(
            self.default_controller_group.as_ref(),
            Compartment::Controller,
        );
        session
            .default_group(Compartment::Main)
            .replace(main_default_group);
        let main_groups: Vec<_> = self
            .groups
            .iter()
            .map(|g| {
                g.to_model(
                    Compartment::Main,
                    false,
                    conversion_context(Compartment::Main),
                )
            })
            .collect();
        let controller_groups: Vec<_> = self
            .controller_groups
            .iter()
            .map(|g| {
                g.to_model(
                    Compartment::Controller,
                    false,
                    conversion_context(Compartment::Controller),
                )
            })
            .collect();
        session.set_groups_without_notification(Compartment::Main, main_groups);
        session
            .default_group(Compartment::Controller)
            .replace(controller_default_group);
        session.set_groups_without_notification(Compartment::Controller, controller_groups);
        // Mappings

        let mut apply_mappings =
            |compartment, mappings: &Vec<MappingModelData>| -> Result<(), &'static str> {
                let mappings: Result<Vec<_>, _> = mappings
                    .iter()
                    .map(|m| {
                        m.to_model_flexible(
                            compartment,
                            &migration_descriptor,
                            self.version.as_ref(),
                            conversion_context(compartment),
                            Some(session.extended_context_with_params(params)),
                        )
                    })
                    .collect();
                session.set_mappings_without_notification(compartment, mappings?);
                Ok(())
            };
        apply_mappings(Compartment::Main, &self.mappings)?;
        apply_mappings(Compartment::Controller, &self.controller_mappings)?;
        session.set_custom_compartment_data(
            Compartment::Controller,
            self.controller_custom_data.clone(),
        );
        let _ = session.change(SessionCommand::ChangeCompartment(
            Compartment::Controller,
            CompartmentCommand::SetNotes(self.controller_notes.clone()),
        ));
        let _ = session.change(SessionCommand::ChangeCompartment(
            Compartment::Main,
            CompartmentCommand::SetNotes(self.main_notes.clone()),
        ));
        session.set_active_controller_id_without_notification(self.active_controller_id.clone());
        session.set_active_main_preset_id_without_notification(self.active_main_preset_id.clone());
        session
            .main_preset_auto_load_mode
            .set_without_notification(self.main_preset_auto_load_mode);
        session.tags.set_without_notification(self.tags.clone());
        session.set_instance_preset_link_config(self.instance_preset_link_config.clone());
        session.set_use_instance_preset_links_only(self.use_instance_preset_links_only);
        let _ = session.change(SessionCommand::SetInstanceTrack(
            self.instance_track.clone(),
        ));
        let _ = session.change(SessionCommand::SetInstanceFx(self.instance_fx.clone()));
        let memorized_main_compartment =
            if let Some(data) = self.memorized_main_compartment.as_ref() {
                Some(data.to_model(self.version.as_ref(), Compartment::Main, Some(session))?)
            } else {
                None
            };
        session.set_memorized_main_compartment_without_notification(memorized_main_compartment);
        // Instance state (don't borrow sooner because the session methods might also borrow it)
        {
            let instance_state = session.instance_state().clone();
            let mut instance_state = instance_state.borrow_mut();
            #[cfg(feature = "playtime")]
            {
                use crate::domain::Backbone;
                if let Some(matrix_ref) = &self.clip_matrix {
                    use ClipMatrixRefData::*;
                    match matrix_ref {
                        Own(m) => {
                            crate::application::get_or_insert_owned_clip_matrix(
                                weak_session,
                                &mut instance_state,
                            )
                            .load(*m.clone())?;
                        }
                        Foreign(session_id) => {
                            // Check if a session with that ID already exists.
                            let foreign_instance_id = BackboneShell::get()
                                .find_session_by_id_ignoring_borrowed_ones(session_id)
                                .and_then(|session| {
                                    session.try_borrow().map(|s| *s.instance_id()).ok()
                                });
                            if let Some(id) = foreign_instance_id {
                                // Referenced ReaLearn instance exists already.
                                Backbone::get().set_instance_clip_matrix_to_foreign_matrix(
                                    &mut instance_state,
                                    id,
                                );
                            } else {
                                // Referenced ReaLearn instance doesn't exist yet.
                                session.memorize_unresolved_foreign_clip_matrix_session_id(
                                    session_id.clone(),
                                );
                            }
                        }
                    };
                } else if !self.clip_slots.is_empty() {
                    // Legacy
                    let matrix =
                        crate::infrastructure::data::clip_legacy::create_clip_matrix_from_legacy_slots(
                            &self.clip_slots,
                            &self.mappings,
                            &self.controller_mappings,
                            session.processor_context().track(),
                        )?;
                    crate::application::get_or_insert_owned_clip_matrix(
                        weak_session,
                        &mut instance_state,
                    )
                    .load(
                        playtime_api::persistence::FlexibleMatrix::Unsigned(Box::new(matrix)),
                    )?;
                } else {
                    Backbone::get().clear_clip_matrix_from_instance_state(&mut instance_state);
                }
            }
            instance_state
                .set_active_instance_tags_without_notification(self.active_instance_tags.clone());
            // Compartment-specific
            // Active mapping by group
            instance_state.set_active_mapping_by_group(
                Compartment::Controller,
                self.controller.active_mapping_by_group.clone(),
            );
            instance_state.set_active_mapping_by_group(
                Compartment::Main,
                self.main.active_mapping_by_group.clone(),
            );
            // Active mapping tags
            instance_state.set_active_mapping_tags(
                Compartment::Controller,
                self.controller.active_mapping_tags.clone(),
            );
            instance_state
                .set_active_mapping_tags(Compartment::Main, self.main.active_mapping_tags.clone());
            // Mapping snapshots (contents) and IDs
            instance_state
                .set_mapping_snapshot_container(Compartment::Main, main_mapping_snapshot_container);
            instance_state.set_mapping_snapshot_container(
                Compartment::Controller,
                controller_mapping_snapshot_container,
            );
            // Pot state
            instance_state.restore_pot_unit(self.pot_state.clone());
        }
        // Check if some other instances waited for the clip matrix of this instance.
        // (important to do after instance state released).
        #[cfg(feature = "playtime")]
        BackboneShell::get().with_instances(|instances| {
            use crate::domain::Backbone;
            // Gather other sessions that have a foreign clip matrix ID set.
            let relevant_other_sessions = instances.iter().filter_map(|other_session| {
                let other_session = other_session.session.upgrade()?;
                let other_session_foreign_clip_matrix_id = other_session
                    .try_borrow()
                    .ok()?
                    .unresolved_foreign_clip_matrix_session_id()?
                    .clone();
                let this_session_id = self.id.as_ref()?;
                if &other_session_foreign_clip_matrix_id == this_session_id {
                    Some(other_session)
                } else {
                    None
                }
            });
            // Let the other session's instance state reference the clip matrix of *this*
            // session's instance state.
            for other_session in relevant_other_sessions {
                let mut other_session = other_session.borrow_mut();
                let other_instance_state = other_session.instance_state();
                Backbone::get().set_instance_clip_matrix_to_foreign_matrix(
                    &mut other_instance_state.borrow_mut(),
                    *session.instance_id(),
                );
                other_session.notify_foreign_clip_matrix_resolved();
            }
        });
        Ok(())
    }

    pub fn create_params(&self) -> PluginParams {
        let mut params = PluginParams::default();
        fill_compartment_params(
            &self.parameters,
            params.compartment_params_mut(Compartment::Main),
        );
        fill_compartment_params(
            &self.controller_parameters,
            params.compartment_params_mut(Compartment::Controller),
        );
        params
    }
}

fn fill_compartment_params(data: &HashMap<String, ParameterData>, model: &mut CompartmentParams) {
    for (index_string, p) in data.iter() {
        let index = index_string
            .parse::<u32>()
            .ok()
            .and_then(|i| CompartmentParamIndex::try_from(i).ok());
        if let Some(i) = index {
            let param = Param::new(p.setting.clone(), p.value);
            *model.at_mut(i) = param;
        }
    }
}

fn get_parameter_data_map(
    plugin_params: &PluginParams,
    compartment: Compartment,
) -> HashMap<String, ParameterData> {
    let compartment_params = plugin_params.compartment_params(compartment);
    compartment_param_index_iter()
        .filter_map(|i| {
            let param = compartment_params.at(i);
            let value = param.raw_value();
            let setting = param.setting();
            if value == 0.0 && setting.name.is_empty() {
                return None;
            }
            let data = ParameterData {
                setting: setting.clone(),
                value,
            };
            Some((i.to_string(), data))
        })
        .collect()
}

impl<'a> ModelToDataConversionContext for CompartmentInSession<'a> {
    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        let group = self.session.find_group_by_id(self.compartment, group_id)?;
        Some(group.borrow().key().clone())
    }

    fn mapping_key_by_id(&self, mapping_id: MappingId) -> Option<MappingKey> {
        let mapping = self
            .session
            .find_mapping_by_id(self.compartment, mapping_id)?;
        Some(mapping.borrow().key().clone())
    }

    fn session_id_by_instance_id(&self, instance_id: UnitId) -> Option<String> {
        BackboneShell::get().find_session_id_by_instance_id(instance_id)
    }
}

impl<'a> DataToModelConversionContext for CompartmentInSession<'a> {
    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        let group = self.session.find_group_by_key(self.compartment, key)?;
        Some(group.borrow().id())
    }

    fn mapping_id_by_key(&self, key: &MappingKey) -> Option<MappingId> {
        self.session.find_mapping_id_by_key(self.compartment, key)
    }

    fn instance_id_by_session_id(&self, session_id: &str) -> Option<UnitId> {
        BackboneShell::get().find_instance_id_by_session_id(session_id)
    }
}

impl<'a> ApiToDataConversionContext for CompartmentInSession<'a> {
    fn param_index_by_key(&self, key: &str) -> Option<CompartmentParamIndex> {
        let (i, _) = self
            .session
            .params()
            .compartment_params(self.compartment)
            .find_setting_by_key(key)?;
        Some(i)
    }
}

/// Consists of methods that return a persistent business ID ("key") for a given transient technical
/// ID ("ID").
pub trait ModelToDataConversionContext {
    fn group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        if group_id.is_default() {
            return Some(GroupKey::default());
        }
        self.non_default_group_key_by_id(group_id)
    }

    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey>;

    fn mapping_key_by_id(&self, mapping_id: MappingId) -> Option<MappingKey>;

    fn session_id_by_instance_id(&self, instance_id: UnitId) -> Option<String>;
}

/// Consists of methods that return a transient technical ID ("ID") for a given persistent
/// business ID ("key").
pub trait DataToModelConversionContext {
    fn group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        if key.is_empty() {
            return Some(GroupId::default());
        }
        self.non_default_group_id_by_key(key)
    }

    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId>;

    fn mapping_id_by_key(&self, key: &MappingKey) -> Option<MappingId>;

    fn instance_id_by_session_id(&self, session_id: &str) -> Option<UnitId>;
}

/// Defines a direct translation from keys to IDs.
pub struct SimpleDataToModelConversionContext {
    group_id_by_key: HashMap<GroupKey, GroupId>,
    mapping_id_by_key: HashMap<MappingKey, MappingId>,
}

impl SimpleDataToModelConversionContext {
    /// Prefers IDs from existing session if it can find a group/mapping with the same key but
    /// generates new, random IDs if it doesn't.
    ///
    /// It's important that IDs are picked up from the session because we have some data that is
    /// not part of the compartment but the session, which is not going to replaced but refers to
    /// existing technical mapping IDs, for example mapping snapshots. If we would always come up
    /// with random technical IDs, these snapshots would immediately get orphans. See
    /// https://github.com/helgoboss/realearn/issues/652.
    pub fn from_session_or_random(
        groups: &[GroupModelData],
        mappings: &[MappingModelData],
        compartment_in_session: Option<CompartmentInSession>,
    ) -> Self {
        Self {
            group_id_by_key: groups
                .iter()
                .map(|g| {
                    let technical_id = compartment_in_session
                        .and_then(|cs| cs.group_id_by_key(&g.id))
                        .unwrap_or_else(GroupId::random);
                    (g.id.clone(), technical_id)
                })
                .collect(),
            mapping_id_by_key: mappings
                .iter()
                .filter_map(|m| {
                    let key = m.id.as_ref()?;
                    let technical_id = compartment_in_session
                        .and_then(|cs| cs.mapping_id_by_key(key))
                        .unwrap_or_else(MappingId::random);
                    Some((key.clone(), technical_id))
                })
                .collect(),
        }
    }
}

impl DataToModelConversionContext for SimpleDataToModelConversionContext {
    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        self.group_id_by_key.get(key).copied()
    }

    fn mapping_id_by_key(&self, key: &MappingKey) -> Option<MappingId> {
        self.mapping_id_by_key.get(key).copied()
    }

    fn instance_id_by_session_id(&self, session_id: &str) -> Option<UnitId> {
        BackboneShell::get().find_instance_id_by_session_id(session_id)
    }
}

fn convert_mapping_snapshots_to_api(
    session: &InstanceModel,
    instance_state: &Unit,
    compartment: Compartment,
) -> Vec<MappingSnapshot> {
    let compartment_in_session = CompartmentInSession::new(session, compartment);
    convert_mapping_snapshots_to_api_internal(
        instance_state.mapping_snapshot_container(compartment),
        &compartment_in_session,
    )
}

fn convert_mapping_snapshots_to_api_internal(
    container: &MappingSnapshotContainer,
    conversion_context: &impl ModelToDataConversionContext,
) -> Vec<MappingSnapshot> {
    container
        .snapshots()
        .map(|(snapshot_id, snapshot)| MappingSnapshot {
            id: snapshot_id.to_string(),
            mappings: snapshot
                .target_values()
                .filter_map(|(mapping_id, target_value)| {
                    let m = MappingInSnapshot {
                        id: conversion_context.mapping_key_by_id(mapping_id)?.into(),
                        target_value: convert_target_value_to_api(target_value),
                    };
                    Some(m)
                })
                .collect(),
        })
        .collect()
}

fn convert_mapping_snapshots_to_model(
    api_snapshots: &[MappingSnapshot],
    active_snapshot_id_by_tag: &HashMap<Tag, MappingSnapshotId>,
    conversion_context: &impl DataToModelConversionContext,
) -> Result<MappingSnapshotContainer, &'static str> {
    let snapshots: Result<
        HashMap<MappingSnapshotId, crate::domain::MappingSnapshot>,
        &'static str,
    > = api_snapshots
        .iter()
        .map(|api_snapshot| {
            let id: MappingSnapshotId = api_snapshot.id.parse()?;
            let target_values: Result<HashMap<_, _>, &'static str> = api_snapshot
                .mappings
                .iter()
                .map(|api_mapping| {
                    let mapping_key: MappingKey = api_mapping.id.clone().into();
                    let id: MappingId = conversion_context
                        .mapping_id_by_key(&mapping_key)
                        .ok_or("couldn't find mapping with key")?;
                    let absolute_value = convert_target_value_to_model(&api_mapping.target_value)?;
                    Ok((id, absolute_value))
                })
                .collect();
            let snapshot = crate::domain::MappingSnapshot::new(target_values?);
            Ok((id, snapshot))
        })
        .collect();
    Ok(MappingSnapshotContainer::new(
        snapshots?,
        active_snapshot_id_by_tag.clone(),
    ))
}

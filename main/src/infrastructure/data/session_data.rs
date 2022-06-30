use crate::application::{
    reaper_supports_global_midi_filter, CompartmentInSession, FxPresetLinkConfig, GroupModel,
    MainPresetAutoLoadMode, Session, SessionCommand,
};
use crate::base::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{
    compartment_param_index_iter, BackboneState, ClipMatrixRef, Compartment, CompartmentParamIndex,
    ControlInput, FeedbackOutput, GroupId, GroupKey, InstanceState, MappingId, MappingKey,
    MappingSnapshotContainer, MappingSnapshotId, MidiControlInput, MidiDestination, OscDeviceId,
    Param, PluginParamIndex, PluginParams, Tag,
};
use crate::infrastructure::data::{
    ensure_no_duplicate_compartment_data, GroupModelData, MappingModelData, MigrationDescriptor,
    ParameterData,
};
use crate::infrastructure::plugin::App;

use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use crate::infrastructure::data::clip_legacy::{
    create_clip_matrix_from_legacy_slots, QualifiedSlotDescriptor,
};
use helgoboss_learn::{AbsoluteValue, Fraction, UnitValue};
use playtime_api::persistence::Matrix;
use realearn_api::persistence::{
    FxDescriptor, MappingInSnapshot, MappingSnapshot, TargetValue, TrackDescriptor,
};
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    controller_custom_data: HashMap<String, serde_json::Value>,
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
    clip_matrix: Option<ClipMatrixRefData>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "is_default")]
    controller: CompartmentState,
    #[serde(default, skip_serializing_if = "is_default")]
    main: CompartmentState,
    #[serde(default, skip_serializing_if = "is_default")]
    active_instance_tags: HashSet<Tag>,
    #[serde(default, skip_serializing_if = "is_default")]
    instance_preset_link_config: FxPresetLinkConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    use_instance_preset_links_only: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    instance_track: TrackDescriptor,
    #[serde(default, skip_serializing_if = "is_default")]
    instance_fx: FxDescriptor,
    #[serde(default, skip_serializing_if = "is_default")]
    mapping_snapshots: Vec<MappingSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum ClipMatrixRefData {
    Own(Matrix),
    Foreign(String),
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
    fn from_instance_state(instance_state: &InstanceState, compartment: Compartment) -> Self {
        CompartmentState {
            active_mapping_by_group: instance_state.active_mapping_by_group(compartment).clone(),
            active_mapping_tags: instance_state.active_mapping_tags(compartment).clone(),
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
            controller_custom_data: Default::default(),
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
            instance_preset_link_config: Default::default(),
            use_instance_preset_links_only: false,
            instance_track: Default::default(),
            instance_fx: session_defaults::INSTANCE_FX_DESCRIPTOR,
            mapping_snapshots: vec![],
        }
    }
}

impl SessionData {
    /// The given parameters are the canonical ones from `RealearnPluginParameters`.
    pub fn from_model(session: &Session, plugin_params: &PluginParams) -> SessionData {
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
        let instance_state = session.instance_state().borrow();
        SessionData {
            version: Some(App::version().clone()),
            id: Some(session.id().to_string()),
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            always_auto_detect_mode: session.auto_correct_settings.get(),
            lives_on_upper_floor: session.lives_on_upper_floor.get(),
            send_feedback_only_if_armed: session.send_feedback_only_if_armed.get(),
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
            active_controller_id: session
                .active_preset_id(Compartment::Controller)
                .map(|id| id.to_string()),
            active_main_preset_id: session
                .active_preset_id(Compartment::Main)
                .map(|id| id.to_string()),
            main_preset_auto_load_mode: session.main_preset_auto_load_mode.get(),
            parameters: get_parameter_data_map(plugin_params, Compartment::Main),
            controller_parameters: get_parameter_data_map(plugin_params, Compartment::Controller),
            clip_slots: vec![],
            clip_matrix: {
                instance_state
                    .clip_matrix_ref()
                    .and_then(|matrix_ref| match matrix_ref {
                        ClipMatrixRef::Own(m) => Some(ClipMatrixRefData::Own(m.save())),
                        ClipMatrixRef::Foreign(instance_id) => {
                            let foreign_session = App::get()
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
            mapping_snapshots: {
                let compartment_in_session = CompartmentInSession::new(session, Compartment::Main);
                convert_mapping_snapshots_to_api(
                    instance_state.mapping_snapshot_container(),
                    &compartment_in_session,
                )
            },
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
        params: &PluginParams,
    ) -> Result<(), Box<dyn Error>> {
        // Validation
        let main_conversion_context =
            SimpleDataToModelConversionContext::new(&self.groups, &self.mappings);
        ensure_no_duplicate_compartment_data(
            &self.mappings,
            &self.groups,
            self.parameters.values().map(|p| &p.setting),
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
                        let midi_dev_id: MidiInputDeviceId = raw_midi_dev_id
                            .try_into()
                            .map_err(|_| "MIDI input device ID out of range")?;
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
        let mapping_snapshot_container =
            convert_mapping_snapshots_to_model(&self.mapping_snapshots, &main_conversion_context)?;
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
        }
        // Groups
        let controller_conversion_context = SimpleDataToModelConversionContext::new(
            &self.controller_groups,
            &self.controller_mappings,
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
        // Instance state (don't borrow sooner because the session methods might also borrow it)
        {
            let instance_state = session.instance_state().clone();
            let mut instance_state = instance_state.borrow_mut();
            if let Some(matrix_ref) = &self.clip_matrix {
                use ClipMatrixRefData::*;
                match matrix_ref {
                    Own(m) => {
                        BackboneState::get()
                            .get_or_insert_owned_clip_matrix_from_instance_state(
                                &mut instance_state,
                            )
                            .load(m.clone())?;
                    }
                    Foreign(session_id) => {
                        // Check if a session with that ID already exists.
                        let foreign_instance_id = App::get()
                            .find_session_by_id_ignoring_borrowed_ones(session_id)
                            .and_then(|session| {
                                session.try_borrow().map(|s| *s.instance_id()).ok()
                            });
                        if let Some(id) = foreign_instance_id {
                            // Referenced ReaLearn instance exists already.
                            BackboneState::get().set_instance_clip_matrix_to_foreign_matrix(
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
                let matrix = create_clip_matrix_from_legacy_slots(
                    &self.clip_slots,
                    &self.mappings,
                    &self.controller_mappings,
                    session.processor_context().track(),
                )?;
                BackboneState::get()
                    .get_or_insert_owned_clip_matrix_from_instance_state(&mut instance_state)
                    .load(matrix)?;
            } else {
                BackboneState::get().clear_clip_matrix_from_instance_state(&mut instance_state);
            }
            instance_state
                .set_active_instance_tags_without_notification(self.active_instance_tags.clone());
            // Compartment-specific
            instance_state.set_active_mapping_by_group(
                Compartment::Controller,
                self.controller.active_mapping_by_group.clone(),
            );
            instance_state.set_active_mapping_by_group(
                Compartment::Main,
                self.main.active_mapping_by_group.clone(),
            );
            instance_state.set_active_mapping_tags(
                Compartment::Controller,
                self.controller.active_mapping_tags.clone(),
            );
            instance_state
                .set_active_mapping_tags(Compartment::Main, self.main.active_mapping_tags.clone());
            instance_state.set_mapping_snapshot_container(mapping_snapshot_container);
            // Check if some other instances waited for the clip matrix of this instance.
            App::get().with_weak_sessions(|sessions| {
                let relevant_other_sessions = sessions.iter().filter_map(|other_session| {
                    let other_session = other_session.upgrade()?;
                    if other_session
                        .try_borrow()
                        .ok()?
                        .unresolved_foreign_clip_matrix_session_id()
                        == self.id.as_ref()
                    {
                        Some(other_session)
                    } else {
                        None
                    }
                });
                for other_session in relevant_other_sessions {
                    // Let the other session's instance state reference the clip matrix of this
                    // session's instance state.
                    let mut other_session = other_session.borrow_mut();
                    let other_instance_state = other_session.instance_state();
                    BackboneState::get().set_instance_clip_matrix_to_foreign_matrix(
                        &mut other_instance_state.borrow_mut(),
                        *session.instance_id(),
                    );
                    other_session.notify_foreign_clip_matrix_resolved();
                }
            });
        }
        Ok(())
    }

    pub fn create_params(&self) -> PluginParams {
        let mut params = PluginParams::default();
        for (i, p) in self.parameters.iter() {
            if let Some(i) = i
                .parse::<u32>()
                .ok()
                .and_then(|i| PluginParamIndex::try_from(i).ok())
            {
                let param = Param::new(p.setting.clone(), p.value);
                *params.at_mut(i) = param;
            }
        }
        params
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
        let (_, mapping) = self
            .session
            .find_mapping_and_index_by_id(self.compartment, mapping_id)?;
        Some(mapping.borrow().key().clone())
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

pub trait ModelToDataConversionContext {
    fn group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        if group_id.is_default() {
            return Some(GroupKey::default());
        }
        self.non_default_group_key_by_id(group_id)
    }

    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey>;

    fn mapping_key_by_id(&self, mapping_id: MappingId) -> Option<MappingKey>;
}

pub trait DataToModelConversionContext {
    fn group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        if key.is_empty() {
            return Some(GroupId::default());
        }
        self.non_default_group_id_by_key(key)
    }

    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId>;

    fn mapping_id_by_key(&self, key: &MappingKey) -> Option<MappingId>;
}

/// Defines a translation from keys to random IDs.
pub struct SimpleDataToModelConversionContext {
    group_id_by_key: HashMap<GroupKey, GroupId>,
    mapping_id_by_key: HashMap<MappingKey, MappingId>,
}

impl SimpleDataToModelConversionContext {
    pub fn new(groups: &[GroupModelData], mappings: &[MappingModelData]) -> Self {
        Self {
            group_id_by_key: groups
                .iter()
                .map(|g| (g.id.clone(), GroupId::random()))
                .collect(),
            mapping_id_by_key: mappings
                .iter()
                .filter_map(|m| {
                    let key = m.id.as_ref()?;
                    Some((key.clone(), MappingId::random()))
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
}

fn convert_mapping_snapshots_to_api(
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
    Ok(MappingSnapshotContainer::new(snapshots?))
}

fn convert_target_value_to_api(value: AbsoluteValue) -> TargetValue {
    match value {
        AbsoluteValue::Continuous(v) => TargetValue::Normalized { value: v.get() },
        AbsoluteValue::Discrete(v) => TargetValue::Discrete { value: v.actual() },
    }
}

fn convert_target_value_to_model(value: &TargetValue) -> Result<AbsoluteValue, &'static str> {
    match value {
        TargetValue::Normalized { value } => {
            Ok(AbsoluteValue::Continuous(UnitValue::try_from(*value)?))
        }
        TargetValue::Discrete { value } => Ok(AbsoluteValue::Discrete(Fraction::new_max(*value))),
    }
}

#![allow(deprecated)]
use crate::application::{
    reaper_supports_global_midi_filter, AutoLoadMode, CompartmentCommand, CompartmentInSession,
    CompartmentModel, FxPresetLinkConfig, GroupModel, SessionCommand, SharedUnitModel, UnitModel,
    WeakUnitModel,
};
use crate::domain::{
    compartment_param_index_iter, CompartmentKind, CompartmentParamIndex, CompartmentParams,
    ControlInput, FeedbackOutput, GroupId, GroupKey, MappingId, MappingKey,
    MappingSnapshotContainer, MappingSnapshotId, MidiControlInput, MidiDestination, OscDeviceId,
    Param, PluginParams, StayActiveWhenProjectInBackground, StreamDeckDeviceId, Tag, Unit,
};
use crate::infrastructure::data::{
    convert_target_value_to_api, convert_target_value_to_model,
    ensure_no_duplicate_compartment_data, CompartmentModelData, GroupModelData, MappingModelData,
    MigrationDescriptor, ParameterData,
};
use crate::infrastructure::plugin::{update_auto_units_async, BackboneShell};
use base::default_util::{bool_true, deserialize_null_default, is_bool_true, is_default};

use crate::base::notification;
use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use helgobox_api::persistence::{
    FxDescriptor, MappingInSnapshot, MappingSnapshot, TrackDescriptor,
};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::ops::Deref;
use std::rc::Rc;

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
    // Since ReaLearn 2.16.0-pre9
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    name: Option<String>,
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
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    wants_keyboard_input: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    stream_deck_device_id: Option<StreamDeckDeviceId>,
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
    controller_custom_data: NonCryptoHashMap<String, serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    main_custom_data: NonCryptoHashMap<String, serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_common_lua: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    main_common_lua: String,
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
    main_preset_auto_load_mode: AutoLoadMode,
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    parameters: NonCryptoHashMap<String, ParameterData>,
    // String key workaround because otherwise deserialization doesn't work with flattening,
    // which is used in CompartmentModelData.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    controller_parameters: NonCryptoHashMap<String, ParameterData>,
    // New since 2.12.0-pre.5
    #[deprecated(note = "Moved to InstanceData")]
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_matrix: Option<ClipMatrixRefData>,
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
    active_instance_tags: NonCryptoHashSet<Tag>,
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
    #[deprecated(note = "Moved to InstanceData")]
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub pot_state: Option<pot::PersistentState>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default",
        alias = "memorizedMainCompartment"
    )]
    auto_load_fallback: Option<AutoLoadFallbackData>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
enum AutoLoadFallbackData {
    Compartment(CompartmentModelData),
    Preset(String),
}

impl AutoLoadFallbackData {
    fn from_model(
        fallback_preset_id: Option<&str>,
        fallback_compartment_model: Option<&CompartmentModel>,
    ) -> Option<Self> {
        fallback_preset_id
            .map(|id| Self::Preset(id.to_string()))
            .or_else(|| {
                fallback_compartment_model
                    .map(|m| Self::Compartment(CompartmentModelData::from_model(m)))
            })
    }
}

fn focused_fx_descriptor() -> FxDescriptor {
    FxDescriptor::Focused
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClipMatrixRefData {
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
    active_mapping_by_group: NonCryptoHashMap<GroupId, MappingId>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_mapping_tags: NonCryptoHashSet<Tag>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    active_mapping_snapshots: NonCryptoHashMap<Tag, MappingSnapshotId>,
}

impl CompartmentState {
    fn from_instance_state(instance_state: &Unit, compartment: CompartmentKind) -> Self {
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
    #[deprecated(
        since = "2.16.4",
        note = "Keyboard input can now be enabled in addition to the input device"
    )]
    Keyboard(KeyboardDevice),
    Osc(OscDeviceId),
    Midi(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[deprecated(
    since = "2.16.4",
    note = "Keyboard input can now be enabled in addition to the input device"
)]
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
    #[allow(deprecated)]
    fn default() -> Self {
        use crate::application::session_defaults;
        Self {
            version: Some(BackboneShell::version().clone()),
            id: None,
            name: None,
            let_matched_events_through: session_defaults::LET_MATCHED_EVENTS_THROUGH,
            let_unmatched_events_through: session_defaults::LET_UNMATCHED_EVENTS_THROUGH,
            stay_active_when_project_in_background: Some(
                session_defaults::STAY_ACTIVE_WHEN_PROJECT_IN_BACKGROUND,
            ),
            lives_on_upper_floor: session_defaults::LIVES_ON_UPPER_FLOOR,
            send_feedback_only_if_armed: session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED,
            reset_feedback_when_releasing_source:
                session_defaults::RESET_FEEDBACK_WHEN_RELEASING_SOURCE,
            control_device_id: None,
            wants_keyboard_input: session_defaults::WANTS_KEYBOARD_INPUT,
            stream_deck_device_id: None,
            feedback_device_id: None,
            default_group: None,
            default_controller_group: None,
            groups: vec![],
            controller_groups: vec![],
            mappings: vec![],
            controller_mappings: vec![],
            controller_custom_data: Default::default(),
            main_custom_data: Default::default(),
            controller_common_lua: Default::default(),
            main_common_lua: Default::default(),
            controller_notes: Default::default(),
            main_notes: Default::default(),
            active_controller_id: None,
            active_main_preset_id: None,
            main_preset_auto_load_mode: session_defaults::MAIN_PRESET_AUTO_LOAD_MODE,
            parameters: Default::default(),
            controller_parameters: Default::default(),
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
            auto_load_fallback: None,
        }
    }
}

impl UnitData {
    /// The given parameters are the canonical ones from `RealearnPluginParameters`.
    #[allow(deprecated)]
    pub fn from_model(session: &UnitModel) -> UnitData {
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
        let main_preset_auto_load_mode = session.auto_load_mode.get();
        let unit = session.unit().borrow();
        let plugin_params = unit.parameter_manager().params();
        UnitData {
            version: Some(BackboneShell::version().clone()),
            id: Some(session.unit_key().to_string()),
            name: session.name.clone(),
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            stay_active_when_project_in_background: Some(
                session.stay_active_when_project_in_background.get(),
            ),
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
                }
            },
            wants_keyboard_input: session.wants_keyboard_input(),
            stream_deck_device_id: session.stream_deck_device_id(),
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
            default_group: from_group(CompartmentKind::Main),
            default_controller_group: from_group(CompartmentKind::Controller),
            groups: from_groups(CompartmentKind::Main),
            controller_groups: from_groups(CompartmentKind::Controller),
            mappings: from_mappings(CompartmentKind::Main),
            controller_mappings: from_mappings(CompartmentKind::Controller),
            controller_custom_data: unit
                .custom_compartment_data(CompartmentKind::Controller)
                .clone(),
            main_custom_data: unit.custom_compartment_data(CompartmentKind::Main).clone(),
            controller_common_lua: session
                .compartment_common_lua(CompartmentKind::Controller)
                .to_owned(),
            main_common_lua: session
                .compartment_common_lua(CompartmentKind::Main)
                .to_owned(),
            controller_notes: session
                .compartment_notes(CompartmentKind::Controller)
                .to_owned(),
            main_notes: session.compartment_notes(CompartmentKind::Main).to_owned(),
            active_controller_id: session
                .active_preset_id(CompartmentKind::Controller)
                .map(|id| id.to_string()),
            active_main_preset_id: session
                .active_preset_id(CompartmentKind::Main)
                .map(|id| id.to_string()),
            main_preset_auto_load_mode,
            parameters: get_parameter_data_map(&plugin_params, CompartmentKind::Main),
            controller_parameters: get_parameter_data_map(
                &plugin_params,
                CompartmentKind::Controller,
            ),
            clip_matrix: None,
            tags: session.tags.get_ref().clone(),
            controller: CompartmentState::from_instance_state(&unit, CompartmentKind::Controller),
            main: CompartmentState::from_instance_state(&unit, CompartmentKind::Main),
            active_instance_tags: unit.active_instance_tags().clone(),
            instance_preset_link_config: session.instance_preset_link_config().clone(),
            use_instance_preset_links_only: session.use_unit_preset_links_only(),
            instance_track: session.instance_track_descriptor().clone(),
            instance_fx: session.instance_fx_descriptor().clone(),
            mapping_snapshots: convert_mapping_snapshots_to_api(
                session,
                &unit,
                CompartmentKind::Main,
            ),
            controller_mapping_snapshots: convert_mapping_snapshots_to_api(
                session,
                &unit,
                CompartmentKind::Controller,
            ),
            pot_state: None,
            auto_load_fallback: AutoLoadFallbackData::from_model(
                session.auto_load_fallback_preset_id(),
                session.auto_load_fallback_compartment(),
            ),
        }
    }

    /// Applies this session data to the given session.
    #[allow(unused_variables)]
    pub fn apply_to_model(&self, shared_session: &SharedUnitModel) -> anyhow::Result<()> {
        let mut session = shared_session.borrow_mut();
        if let Some(v) = self.version.as_ref() {
            if BackboneShell::version() < v {
                notification::warn(format!(
                    "The session that is about to load was saved with ReaLearn {}, which is \
                         newer than the installed version {}. Things might not work as expected. \
                         Even more importantly: Saving might result in loss of the data that was \
                         saved with the new ReaLearn version! Please consider upgrading your \
                         ReaLearn installation to the latest version.",
                    v,
                    BackboneShell::version()
                ));
            }
        }
        if let Err(e) = self.apply_to_model_internal(&mut session, Rc::downgrade(shared_session)) {
            notification::warn(e.to_string());
        }
        // Notify
        session.notify_everything_has_changed();
        session.notify_compartment_loaded(CompartmentKind::Main);
        session.notify_compartment_loaded(CompartmentKind::Controller);
        Ok(())
    }

    /// Applies this session data to the given session.
    ///
    /// Doesn't notify listeners! Consumers must inform session that everything has changed.
    ///
    /// # Errors
    ///
    /// Returns and error if this session data is invalid.
    #[allow(unused_variables)]
    fn apply_to_model_internal(
        &self,
        session: &mut UnitModel,
        weak_session: WeakUnitModel,
    ) -> Result<(), Box<dyn Error>> {
        // Validation
        let params = self.create_params();
        let main_conversion_context = SimpleDataToModelConversionContext::from_session_or_random(
            &self.groups,
            &self.mappings,
            Some(CompartmentInSession::new(session, CompartmentKind::Main)),
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
        let (control_input, wants_keyboard_input_legacy) = match self.control_device_id.as_ref() {
            None => (ControlInput::Midi(MidiControlInput::FxInput), false),
            Some(dev_id) => {
                use ControlDeviceId::*;
                match dev_id {
                    Keyboard(_) => (ControlInput::Midi(MidiControlInput::FxInput), true),
                    Midi(midi_dev_id_string) => {
                        let raw_midi_dev_id = midi_dev_id_string
                            .parse::<u8>()
                            .map_err(|_| "invalid MIDI input device ID")?;
                        let midi_dev_id = MidiInputDeviceId::new(raw_midi_dev_id);
                        (
                            ControlInput::Midi(MidiControlInput::Device(midi_dev_id)),
                            false,
                        )
                    }
                    Osc(osc_dev_id) => (ControlInput::Osc(*osc_dev_id), false),
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
        let _ = session.change(SessionCommand::SetWantsKeyboardInput(
            self.wants_keyboard_input || wants_keyboard_input_legacy,
        ));
        let _ = session.change(SessionCommand::SetStreamDeckDevice(
            self.stream_deck_device_id,
        ));
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
                Some(CompartmentInSession::new(
                    session,
                    CompartmentKind::Controller,
                )),
            );
        let conversion_context = |compartment: CompartmentKind| match compartment {
            CompartmentKind::Controller => &controller_conversion_context,
            CompartmentKind::Main => &main_conversion_context,
        };
        let get_final_default_group =
            |def_group: Option<&GroupModelData>, compartment: CompartmentKind| {
                def_group
                    .map(|g| g.to_model(compartment, true, conversion_context(compartment)))
                    .unwrap_or_else(|| GroupModel::default_for_compartment(compartment))
            };
        let main_default_group =
            get_final_default_group(self.default_group.as_ref(), CompartmentKind::Main);
        let controller_default_group = get_final_default_group(
            self.default_controller_group.as_ref(),
            CompartmentKind::Controller,
        );
        session
            .default_group(CompartmentKind::Main)
            .replace(main_default_group);
        let main_groups: Vec<_> = self
            .groups
            .iter()
            .map(|g| {
                g.to_model(
                    CompartmentKind::Main,
                    false,
                    conversion_context(CompartmentKind::Main),
                )
            })
            .collect();
        let controller_groups: Vec<_> = self
            .controller_groups
            .iter()
            .map(|g| {
                g.to_model(
                    CompartmentKind::Controller,
                    false,
                    conversion_context(CompartmentKind::Controller),
                )
            })
            .collect();
        session.set_groups_without_notification(CompartmentKind::Main, main_groups);
        session
            .default_group(CompartmentKind::Controller)
            .replace(controller_default_group);
        session.set_groups_without_notification(CompartmentKind::Controller, controller_groups);
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
                            Some(session.extended_context_with_params(&params)),
                        )
                    })
                    .collect();
                session.set_mappings_without_notification(compartment, mappings?);
                Ok(())
            };
        apply_mappings(CompartmentKind::Main, &self.mappings)?;
        apply_mappings(CompartmentKind::Controller, &self.controller_mappings)?;
        let _ = session.change(SessionCommand::ChangeCompartment(
            CompartmentKind::Controller,
            CompartmentCommand::SetCommonLua(self.controller_common_lua.clone()),
        ));
        let _ = session.change(SessionCommand::ChangeCompartment(
            CompartmentKind::Main,
            CompartmentCommand::SetCommonLua(self.main_common_lua.clone()),
        ));
        let _ = session.change(SessionCommand::ChangeCompartment(
            CompartmentKind::Controller,
            CompartmentCommand::SetNotes(self.controller_notes.clone()),
        ));
        let _ = session.change(SessionCommand::ChangeCompartment(
            CompartmentKind::Main,
            CompartmentCommand::SetNotes(self.main_notes.clone()),
        ));
        session.set_active_controller_id_without_notification(self.active_controller_id.clone());
        session.set_active_main_preset_id_without_notification(self.active_main_preset_id.clone());
        session
            .auto_load_mode
            .set_without_notification(self.main_preset_auto_load_mode);
        session.tags.set_without_notification(self.tags.clone());
        session.set_instance_preset_link_config(self.instance_preset_link_config.clone());
        session.set_use_unit_preset_links_only(self.use_instance_preset_links_only);
        if let Some(id) = &self.id {
            let _ = session.change(SessionCommand::SetUnitKey(id.clone()));
        };
        let _ = session.change(SessionCommand::SetUnitName(self.name.clone()));
        let _ = session.change(SessionCommand::SetInstanceTrack(
            self.instance_track.clone(),
        ));
        let _ = session.change(SessionCommand::SetInstanceFx(self.instance_fx.clone()));
        let (auto_load_fallback_preset_id, auto_load_fallback_compartment_model) =
            match &self.auto_load_fallback {
                None => (None, None),
                Some(AutoLoadFallbackData::Preset(id)) => (Some(id.clone()), None),
                Some(AutoLoadFallbackData::Compartment(data)) => {
                    let compartment_model =
                        data.to_model(self.version.as_ref(), CompartmentKind::Main, Some(session))?;
                    (None, Some(compartment_model))
                }
            };
        session.set_auto_load_fallback_preset_id(auto_load_fallback_preset_id);
        session.set_auto_load_fallback_compartment_model(auto_load_fallback_compartment_model);
        // Instance state (don't borrow sooner because the session methods might also borrow it)
        {
            let unit = session.unit().clone();
            let mut unit = unit.borrow_mut();
            unit.set_custom_compartment_data(
                CompartmentKind::Controller,
                self.controller_custom_data.clone(),
            );
            unit.set_custom_compartment_data(CompartmentKind::Main, self.main_custom_data.clone());
            unit.parameter_manager().set_all_parameters(params);
            unit.set_active_instance_tags_without_notification(self.active_instance_tags.clone());
            // Compartment-specific
            // Active mapping by group
            unit.set_active_mapping_by_group(
                CompartmentKind::Controller,
                self.controller.active_mapping_by_group.clone(),
            );
            unit.set_active_mapping_by_group(
                CompartmentKind::Main,
                self.main.active_mapping_by_group.clone(),
            );
            // Active mapping tags
            unit.set_active_mapping_tags(
                CompartmentKind::Controller,
                self.controller.active_mapping_tags.clone(),
            );
            unit.set_active_mapping_tags(
                CompartmentKind::Main,
                self.main.active_mapping_tags.clone(),
            );
            // Mapping snapshots (contents) and IDs
            unit.set_mapping_snapshot_container(
                CompartmentKind::Main,
                main_mapping_snapshot_container,
            );
            unit.set_mapping_snapshot_container(
                CompartmentKind::Controller,
                controller_mapping_snapshot_container,
            );
        }
        update_auto_units_async();
        Ok(())
    }

    fn create_params(&self) -> PluginParams {
        let mut params = PluginParams::default();
        fill_compartment_params(
            &self.parameters,
            params.compartment_params_mut(CompartmentKind::Main),
        );
        fill_compartment_params(
            &self.controller_parameters,
            params.compartment_params_mut(CompartmentKind::Controller),
        );
        params
    }
}

fn fill_compartment_params(
    data: &NonCryptoHashMap<String, ParameterData>,
    model: &mut CompartmentParams,
) {
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
    compartment: CompartmentKind,
) -> NonCryptoHashMap<String, ParameterData> {
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

impl ModelToDataConversionContext for CompartmentInSession<'_> {
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
}

impl DataToModelConversionContext for CompartmentInSession<'_> {
    fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
        let group = self.session.find_group_by_key(self.compartment, key)?;
        Some(group.borrow().id())
    }

    fn mapping_id_by_key(&self, key: &MappingKey) -> Option<MappingId> {
        self.session.find_mapping_id_by_key(self.compartment, key)
    }
}

impl ApiToDataConversionContext for CompartmentInSession<'_> {
    fn compartment(&self) -> CompartmentKind {
        self.compartment
    }

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
}

/// Defines a direct translation from keys to IDs.
pub struct SimpleDataToModelConversionContext {
    group_id_by_key: NonCryptoHashMap<GroupKey, GroupId>,
    mapping_id_by_key: NonCryptoHashMap<MappingKey, MappingId>,
}

impl SimpleDataToModelConversionContext {
    /// Prefers IDs from existing session if it can find a group/mapping with the same key but
    /// generates new, random IDs if it doesn't.
    ///
    /// It's important that IDs are picked up from the session because we have some data that is
    /// not part of the compartment but the session, which is not going to replaced but refers to
    /// existing technical mapping IDs, for example mapping snapshots. If we would always come up
    /// with random technical IDs, these snapshots would immediately get orphans. See
    /// https://github.com/helgoboss/helgobox/issues/652.
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
}

fn convert_mapping_snapshots_to_api(
    session: &UnitModel,
    instance_state: &Unit,
    compartment: CompartmentKind,
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
    active_snapshot_id_by_tag: &NonCryptoHashMap<Tag, MappingSnapshotId>,
    conversion_context: &impl DataToModelConversionContext,
) -> Result<MappingSnapshotContainer, &'static str> {
    let snapshots: Result<
        NonCryptoHashMap<MappingSnapshotId, crate::domain::MappingSnapshot>,
        &'static str,
    > = api_snapshots
        .iter()
        .map(|api_snapshot| {
            let id: MappingSnapshotId = api_snapshot.id.parse()?;
            let target_values: Result<NonCryptoHashMap<_, _>, &'static str> = api_snapshot
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

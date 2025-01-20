use crate::application::{
    get_track_label, share_group, share_mapping, Affected, AutoLoadMode, AutoUnitData, Change,
    ChangeResult, CompartmentCommand, CompartmentModel, CompartmentPresetManager,
    CompartmentPresetModel, CompartmentProp, FxId, FxPresetLinkConfig, GroupCommand, GroupModel,
    MappingCommand, MappingModel, MappingProp, ModeCommand, PresetLinkManager, ProcessingRelevance,
    SharedGroup, SharedMapping, SourceModel, TargetCategory, TargetModel, TargetProp,
    MASTER_TRACK_LABEL,
};
use crate::base::{notification, prop, when, AsyncNotifier, Prop};
use crate::domain::{
    convert_plugin_param_index_range_to_iter, create_lua_feedback_script_runtime,
    create_lua_midi_script_source_runtime, lua_module_path_without_ext, Backbone, BasicSettings,
    CompartmentKind, CompartmentParamIndex, CompartmentSettings, CompoundMappingSource,
    ControlContext, ControlInput, DomainEvent, DomainEventHandler, ExtendedProcessorContext,
    FeedbackAudioHookTask, FeedbackOutput, FeedbackRealTimeTask, FinalSourceFeedbackValue, GroupId,
    GroupKey, IncomingCompoundSourceValue, InputDescriptor, InstanceId, InternalInfoEvent,
    LastTouchedTargetFilter, MainMapping, MappingId, MappingKey, MappingMatchedEvent,
    MessageCaptureEvent, MidiControlInput, NormalMainTask, OscFeedbackTask, ParamSetting,
    PluginParams, ProcessorContext, ProjectionFeedbackValue, QualifiedMappingId,
    RealearnControlSurfaceMainTask, RealearnTarget, ReaperTarget, ReaperTargetType, SharedInstance,
    SharedUnit, SourceFeedbackEvent, StayActiveWhenProjectInBackground, StreamDeckDeviceId, Tag,
    TargetControlEvent, TargetTouchEvent, TargetValueChangedEvent, Unit, UnitContainer, UnitId,
    VirtualControlElementId, VirtualFx, VirtualSource, VirtualSourceValue,
    LUA_FEEDBACK_SCRIPT_RUNTIME_NAME, LUA_MIDI_SCRIPT_SOURCE_RUNTIME_NAME,
};
use base::{Global, NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use derivative::Derivative;
use enum_map::EnumMap;

use reaper_high::Reaper;
use rx_util::Notifier;
use rxrust::prelude::*;
use std::cell::{OnceCell, Ref, RefCell};
use std::fmt::{Debug, Display, Formatter};

use crate::domain;
use anyhow::{ensure, Context};
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use core::iter;
use helgoboss_learn::{AbsoluteMode, ControlResult, ControlValue, UnitValue};
use helgobox_api::persistence::{
    FxDescriptor, MappingModification, TargetTouchCause, TrackDescriptor,
    VirtualControlElementCharacter,
};
use helgobox_api::runtime::InstanceInfoEvent;
use itertools::Itertools;
use reaper_medium::{InputMonitoringMode, RecordingInput};
use std::error::Error;
use std::fmt;
use std::rc::{Rc, Weak};
use tracing::{debug, trace};

pub trait SessionUi {
    fn show_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId);
    fn show_pot_browser(&self);
    fn target_value_changed(&self, event: TargetValueChangedEvent);
    fn parameters_changed(&self, session: &UnitModel);
    fn midi_devices_changed(&self);
    fn celebrate_success(&self);
    fn conditions_changed(&self);
    fn send_projection_feedback(&self, session: &UnitModel, value: ProjectionFeedbackValue);
    fn mapping_matched(&self, event: MappingMatchedEvent);
    fn handle_target_control(&self, event: TargetControlEvent);
    fn handle_source_feedback(&self, event: SourceFeedbackEvent);
    fn handle_internal_info_event(&self, event: &InternalInfoEvent);
    fn handle_external_info_event(&self, event: InstanceInfoEvent);
    fn handle_everything_changed(&self, unit_model: &UnitModel);
    fn handle_global_control_and_feedback_state_changed(&self);
    fn handle_affected(
        &self,
        session: &UnitModel,
        affected: Affected<SessionProp>,
        initiator: Option<u32>,
    );
}

/// Just the old term as alias for easier class search.
pub type _Session = UnitModel;

/// This represents the user session with one ReaLearn instance.
///
/// It's ReaLearn's main object which keeps everything together.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct UnitModel {
    instance_id: InstanceId,
    unit_id: UnitId,
    /// Persisted and can be user-customized. Should be
    /// unique but if not it's not a big deal, then it won't crash but the user can't be sure which
    /// session will be picked. Most relevant for HTTP/WS API.
    unit_key: String,
    pub name: Option<String>,
    pub let_matched_events_through: Prop<bool>,
    pub let_unmatched_events_through: Prop<bool>,
    pub stay_active_when_project_in_background: Prop<StayActiveWhenProjectInBackground>,
    pub real_input_logging_enabled: Prop<bool>,
    pub real_output_logging_enabled: Prop<bool>,
    pub virtual_input_logging_enabled: Prop<bool>,
    pub virtual_output_logging_enabled: Prop<bool>,
    pub target_control_logging_enabled: Prop<bool>,
    pub send_feedback_only_if_armed: Prop<bool>,
    pub reset_feedback_when_releasing_source: Prop<bool>,
    pub control_input: Prop<ControlInput>,
    wants_keyboard_input: bool,
    stream_deck_device_id: Option<StreamDeckDeviceId>,
    pub feedback_output: Prop<Option<FeedbackOutput>>,
    pub auto_load_mode: Prop<AutoLoadMode>,
    pub auto_load_fallback_compartment: Option<CompartmentModel>,
    pub auto_load_fallback_preset_id: Option<String>,
    // --
    pub lives_on_upper_floor: Prop<bool>,
    pub tags: Prop<Vec<Tag>>,
    pub compartment_is_dirty: EnumMap<CompartmentKind, Prop<bool>>,
    // Is set when in the state of learning multiple mappings ("batch learn")
    learn_many_state: Prop<Option<LearnManyState>>,
    // We want that learn works independently of the UI, so they are session properties.
    active_controller_preset_id: Option<String>,
    active_main_preset_id: Option<String>,
    processor_context: ProcessorContext,
    mappings: EnumMap<CompartmentKind, Vec<SharedMapping>>,
    compartment_common_lua: EnumMap<CompartmentKind, String>,
    compartment_notes: EnumMap<CompartmentKind, String>,
    default_main_group: SharedGroup,
    default_controller_group: SharedGroup,
    groups: EnumMap<CompartmentKind, Vec<SharedGroup>>,
    everything_changed_subject: LocalSubject<'static, (), ()>,
    mapping_list_changed_subject: LocalSubject<'static, (CompartmentKind, Option<MappingId>), ()>,
    group_list_changed_subject: LocalSubject<'static, CompartmentKind, ()>,
    incoming_msg_captured_subject: LocalSubject<'static, MessageCaptureEvent, ()>,
    mapping_subscriptions: EnumMap<CompartmentKind, Vec<SubscriptionGuard<LocalSubscription>>>,
    group_subscriptions: EnumMap<CompartmentKind, Vec<SubscriptionGuard<LocalSubscription>>>,
    normal_main_task_sender: SenderToNormalThread<NormalMainTask>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    #[derivative(Debug = "ignore")]
    ui: OnceCell<Box<dyn SessionUi>>,
    unit_container: &'static dyn UnitContainer,
    // TODO-medium I already made an attempt to remove this because we share parameters via
    //  Arc<ParameterManager> now. However, the extended_context() method borrows this and it's
    //  used in many places. This is harder to replace because we can't just return something
    //  borrowed from the Arc.
    /// Copy of all parameters (`ParameterManager` is the rightful owner).
    params: PluginParams,
    controller_preset_manager: Box<dyn CompartmentPresetManager>,
    main_preset_manager: Box<dyn CompartmentPresetManager>,
    global_preset_link_manager: Box<dyn PresetLinkManager>,
    unit_preset_link_config: FxPresetLinkConfig,
    use_unit_preset_links_only: bool,
    // It's okay not to use Weak here because the instance lives longer than the unit.
    instance: SharedInstance,
    unit: SharedUnit,
    global_feedback_audio_hook_task_sender: &'static SenderToRealTimeThread<FeedbackAudioHookTask>,
    feedback_real_time_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
    global_osc_feedback_task_sender: &'static SenderToNormalThread<OscFeedbackTask>,
    control_surface_main_task_sender: &'static RealearnControlSurfaceMainTaskSender,
    instance_track_descriptor: TrackDescriptor,
    instance_fx_descriptor: FxDescriptor,
    /// Set if this unit was added automatically.
    auto_unit: Option<AutoUnitData>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct LearnManyState {
    pub compartment: CompartmentKind,
    pub current_mapping_id: MappingId,
    pub sub_state: LearnManySubState,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum LearnManySubState {
    LearningSource {
        // Only relevant in controller compartment
        control_element_character: VirtualControlElementCharacter,
    },
    LearningTarget,
}

impl LearnManyState {
    pub fn learning_source(
        compartment: CompartmentKind,
        current_mapping_id: MappingId,
        control_element_character: VirtualControlElementCharacter,
    ) -> LearnManyState {
        LearnManyState {
            compartment,
            current_mapping_id,
            sub_state: LearnManySubState::LearningSource {
                control_element_character,
            },
        }
    }

    pub fn learning_target(
        compartment: CompartmentKind,
        current_mapping_id: MappingId,
    ) -> LearnManyState {
        LearnManyState {
            compartment,
            current_mapping_id,
            sub_state: LearnManySubState::LearningTarget,
        }
    }
}

pub mod session_defaults {
    use crate::application::AutoLoadMode;
    use crate::domain::StayActiveWhenProjectInBackground;
    use helgobox_api::persistence::FxDescriptor;

    pub const LET_MATCHED_EVENTS_THROUGH: bool = false;
    pub const LET_UNMATCHED_EVENTS_THROUGH: bool = true;
    pub const STAY_ACTIVE_WHEN_PROJECT_IN_BACKGROUND: StayActiveWhenProjectInBackground =
        StayActiveWhenProjectInBackground::OnlyIfBackgroundProjectIsRunning;
    pub const WANTS_KEYBOARD_INPUT: bool = false;
    pub const LIVES_ON_UPPER_FLOOR: bool = false;
    pub const SEND_FEEDBACK_ONLY_IF_ARMED: bool = false;
    pub const RESET_FEEDBACK_WHEN_RELEASING_SOURCE: bool = true;
    pub const MAIN_PRESET_AUTO_LOAD_MODE: AutoLoadMode = AutoLoadMode::Off;
    /// This is mainly for backward-compatibility with "Auto-load: Depending on focused FX"
    /// but also is a quite common use case, so why not.
    pub const INSTANCE_FX_DESCRIPTOR: FxDescriptor = FxDescriptor::Focused;
}

impl UnitModel {
    /// Creates an empty unit model.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: InstanceId,
        unit_id: UnitId,
        initial_name: Option<String>,
        context: ProcessorContext,
        normal_main_task_sender: SenderToNormalThread<NormalMainTask>,
        instance_container: &'static dyn UnitContainer,
        controller_manager: impl CompartmentPresetManager + 'static,
        main_preset_manager: impl CompartmentPresetManager + 'static,
        preset_link_manager: impl PresetLinkManager + 'static,
        instance: SharedInstance,
        unit: SharedUnit,
        global_feedback_audio_hook_task_sender: &'static SenderToRealTimeThread<
            FeedbackAudioHookTask,
        >,
        feedback_real_time_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
        global_osc_feedback_task_sender: &'static SenderToNormalThread<OscFeedbackTask>,
        control_surface_main_task_sender: &'static RealearnControlSurfaceMainTaskSender,
        auto_unit: Option<AutoUnitData>,
    ) -> UnitModel {
        let initial_input = auto_unit
            .as_ref()
            .map(|au| au.control_input())
            .unwrap_or_default();
        let initial_output = auto_unit.as_ref().and_then(|au| au.feedback_output());
        // Make unit key deterministic if we have an auto unit. That can help later if we want to let other units
        // (or anything really) refer to automatically loaded units.
        let initial_unit_key = auto_unit
            .as_ref()
            .map(|au| au.controller_id.clone())
            .unwrap_or_else(|| nanoid::nanoid!(8));
        let mut model = Self {
            unit_key: initial_unit_key,
            instance_id,
            unit_id,
            let_matched_events_through: prop(session_defaults::LET_MATCHED_EVENTS_THROUGH),
            let_unmatched_events_through: prop(session_defaults::LET_UNMATCHED_EVENTS_THROUGH),
            stay_active_when_project_in_background: prop(
                session_defaults::STAY_ACTIVE_WHEN_PROJECT_IN_BACKGROUND,
            ),
            real_input_logging_enabled: prop(false),
            real_output_logging_enabled: prop(false),
            virtual_input_logging_enabled: prop(false),
            virtual_output_logging_enabled: prop(false),
            target_control_logging_enabled: prop(false),
            send_feedback_only_if_armed: prop(session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED),
            reset_feedback_when_releasing_source: prop(
                session_defaults::RESET_FEEDBACK_WHEN_RELEASING_SOURCE,
            ),
            control_input: prop(initial_input),
            wants_keyboard_input: session_defaults::WANTS_KEYBOARD_INPUT,
            stream_deck_device_id: None,
            feedback_output: prop(initial_output),
            auto_load_mode: prop(session_defaults::MAIN_PRESET_AUTO_LOAD_MODE),
            auto_load_fallback_compartment: None,
            auto_load_fallback_preset_id: None,
            lives_on_upper_floor: prop(false),
            tags: Default::default(),
            compartment_is_dirty: Default::default(),
            learn_many_state: prop(None),
            active_controller_preset_id: None,
            active_main_preset_id: None,
            processor_context: context,
            mappings: Default::default(),
            compartment_common_lua: Default::default(),
            compartment_notes: Default::default(),
            default_main_group: Rc::new(RefCell::new(GroupModel::default_for_compartment(
                CompartmentKind::Main,
            ))),
            default_controller_group: Rc::new(RefCell::new(GroupModel::default_for_compartment(
                CompartmentKind::Controller,
            ))),
            groups: Default::default(),
            everything_changed_subject: Default::default(),
            mapping_list_changed_subject: Default::default(),
            group_list_changed_subject: Default::default(),
            incoming_msg_captured_subject: Default::default(),
            mapping_subscriptions: Default::default(),
            group_subscriptions: Default::default(),
            normal_main_task_sender,
            party_is_over_subject: Default::default(),
            ui: OnceCell::new(),
            unit_container: instance_container,
            params: Default::default(),
            controller_preset_manager: Box::new(controller_manager),
            main_preset_manager: Box::new(main_preset_manager),
            global_preset_link_manager: Box::new(preset_link_manager),
            unit_preset_link_config: Default::default(),
            use_unit_preset_links_only: false,
            instance,
            unit,
            global_feedback_audio_hook_task_sender,
            feedback_real_time_task_sender,
            global_osc_feedback_task_sender,
            control_surface_main_task_sender,
            instance_track_descriptor: Default::default(),
            instance_fx_descriptor: session_defaults::INSTANCE_FX_DESCRIPTOR,
            auto_unit: auto_unit.clone(),
            name: initial_name,
        };
        if let Some(auto_unit) = auto_unit {
            let initial_controller_preset_id = auto_unit
                .controller_preset_usage
                .map(|u| u.controller_preset_id);
            if let Some(id) = initial_controller_preset_id {
                model.activate_controller_preset(Some(id));
            }
            model.activate_main_preset(Some(auto_unit.main_preset_id));
            model
                .unit()
                .borrow_mut()
                .set_control_unit_palette_color(auto_unit.controller_palette_color);
        }
        model
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn name_or_key(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.unit_key)
    }

    pub fn auto_unit(&self) -> Option<&AutoUnitData> {
        self.auto_unit.as_ref()
    }

    /// # Preconditions
    ///
    /// This unit model must have an auto unit whose ID matches the given one.
    pub fn update_auto_unit(&mut self, new_unit: AutoUnitData) {
        // Check preconditions
        let old_auto_unit = self.auto_unit.take().expect("auto unit must be set");
        assert_eq!(old_auto_unit.controller_id, new_unit.controller_id);
        // Update actual properties of unit
        if new_unit.input != old_auto_unit.input {
            self.control_input.set(new_unit.control_input());
        }
        if new_unit.output != old_auto_unit.output {
            self.feedback_output.set(new_unit.feedback_output());
        }
        if new_unit.controller_preset_usage != old_auto_unit.controller_preset_usage {
            let controller_preset_id = new_unit
                .controller_preset_usage
                .as_ref()
                .map(|usage| usage.controller_preset_id.clone());
            self.activate_controller_preset(controller_preset_id)
        }
        if new_unit.main_preset_id != old_auto_unit.main_preset_id {
            self.activate_main_preset(Some(new_unit.main_preset_id.clone()))
        }
        if new_unit.main_preset_id != old_auto_unit.main_preset_id {
            self.activate_main_preset(Some(new_unit.main_preset_id.clone()))
        }
        if new_unit.controller_palette_color != old_auto_unit.controller_palette_color {
            self.unit
                .borrow_mut()
                .set_control_unit_palette_color(new_unit.controller_palette_color);
        }
        // Update unit data itself
        self.auto_unit.replace(new_unit);
    }

    pub fn set_ui(&mut self, ui: impl SessionUi + 'static) {
        if self.ui.set(Box::new(ui)).is_err() {
            panic!("can set ui only once");
        }
    }

    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn unit_id(&self) -> UnitId {
        self.unit_id
    }

    pub fn unit_key(&self) -> &str {
        &self.unit_key
    }

    pub fn instance_track_descriptor(&self) -> &TrackDescriptor {
        &self.instance_track_descriptor
    }

    pub fn instance_fx_descriptor(&self) -> &FxDescriptor {
        &self.instance_fx_descriptor
    }

    /// Returns whether this unit allows the given global auto unit to be created (not necessarily
    /// and even most likely not in this instance by the way). It can give its veto by returning
    /// `false`.
    pub fn is_fine_with_global_auto_unit(&self, auto_unit_candidate: &AutoUnitData) -> bool {
        if self.auto_unit.is_some() {
            // Only manual units give veto
            return true;
        }
        // Only active units give veto
        let state = self.unit.borrow().global_control_and_feedback_state();
        // If the manual unit uses the same connection as the controller, the manual unit wins. We
        // do this to prevent conflicts of manual units with automatic ones. Manual units need to
        // take care of that manually, but with auto units we can easily avoid it.
        if state.control_active && auto_unit_candidate.control_input() == self.control_input() {
            return false;
        }
        if state.feedback_active && auto_unit_candidate.feedback_output() == self.feedback_output()
        {
            return false;
        }
        true
    }

    pub fn receives_input_from(&self, input_descriptor: &InputDescriptor) -> bool {
        match input_descriptor {
            InputDescriptor::Midi { device_id, channel } => match self.control_input() {
                ControlInput::Midi(MidiControlInput::FxInput) => {
                    if let Some(track) = self.processor_context().track() {
                        if !track.is_armed(true) {
                            return false;
                        }
                        if let Some(RecordingInput::Midi {
                            device_id: dev_id,
                            channel: ch,
                        }) = track.recording_input()
                        {
                            (dev_id.is_none() || dev_id == Some(*device_id))
                                && (ch.is_none() || ch == *channel)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                ControlInput::Midi(MidiControlInput::Device(dev_id)) => dev_id == *device_id,
                _ => false,
            },
            InputDescriptor::Osc { device_id } => match self.control_input() {
                ControlInput::Osc(dev_id) => dev_id == *device_id,
                _ => false,
            },
            InputDescriptor::Keyboard => self.wants_keyboard_input,
            InputDescriptor::StreamDeck { device_id } => {
                self.stream_deck_device_id == Some(*device_id)
            }
        }
    }

    pub fn find_mapping_with_source(
        &self,
        compartment: CompartmentKind,
        source_value: IncomingCompoundSourceValue,
    ) -> Option<FindMappingOutcome> {
        use CompoundMappingSource::*;
        let virtualization = self.virtualize_source_value(source_value);
        let instance_state = self.unit.borrow();
        let mapping = self.mappings(compartment).find(|m| {
            let m = m.borrow();
            if !instance_state.mapping_is_on(m.qualified_id()) {
                return false;
            }
            let mapping_source = m.source_model.create_source();
            if let (Virtual(virtual_source), Some(v)) = (&mapping_source, &virtualization) {
                virtual_source.control(&v.virtual_source_value).is_some()
            } else {
                mapping_source
                    .reacts_to_source_value_with(source_value)
                    .is_some()
            }
        });
        let outcome = FindMappingOutcome {
            mapping: mapping.cloned()?,
            source_is_learnable: virtualization.map(|v| v.learnable).unwrap_or(true),
        };
        Some(outcome)
    }

    pub fn mappings_have_project_references(&self, compartment: CompartmentKind) -> bool {
        mappings_have_project_references(self.mappings[compartment].iter())
    }

    pub fn virtualize_main_mappings(&mut self) -> Result<(), String> {
        let count = self.mappings[CompartmentKind::Main]
            .iter()
            .filter(|m| {
                let mut m = m.borrow_mut();
                if let Some(virtual_source_model) = self.virtualize_source_model(&m.source_model) {
                    m.source_model = virtual_source_model;
                    false
                } else {
                    true
                }
            })
            .count();
        self.notify_everything_has_changed();
        if count > 0 {
            return Err(format!("Couldn't virtualize {count} mappings."));
        }
        Ok(())
    }

    pub fn mappings_are_read_only(&self, compartment: CompartmentKind) -> bool {
        self.is_learning_many_mappings()
            || (compartment == CompartmentKind::Main && self.main_preset_is_auto_loaded())
    }

    fn full_sync(&mut self) {
        for compartment in CompartmentKind::enum_iter() {
            self.sync_compartment_settings(compartment);
        }
        // It's important to sync feedback device first, otherwise the initial feedback messages
        // won't arrive!
        self.sync_settings();
        self.sync_upper_floor_membership();
        // Now sync mappings - which includes initial feedback.
        for compartment in CompartmentKind::enum_iter() {
            self.sync_all_mappings_full(compartment);
        }
    }

    /// Makes all autostart mappings hit the target.
    pub fn notify_realearn_unit_started(&self) {
        debug!("Sending NotifyRealearnUnitStarted");
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::NotifyRealearnUnitStarted);
    }

    /// Instructs the main processor to hit the target directly.
    ///
    /// This doesn't invoke group interaction because it's meant to totally skip the mode.
    pub fn hit_target(&self, id: QualifiedMappingId, value: ControlValue) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::HitTarget { id, value });
    }

    /// Connects the dots.
    // TODO-low Too large. Split this into several methods.
    pub fn activate(&mut self, weak_session: WeakUnitModel) {
        // Initial sync
        self.full_sync();
        // Whenever something in the group list changes, resubscribe to those groups and sync
        // (because a mapping could have changed its group).
        when(self.group_list_changed())
            .with(weak_session.clone())
            .do_async(|shared_session, compartment| {
                let mut session = shared_session.borrow_mut();
                session.sync_all_mappings_full(compartment);
                session.mark_compartment_dirty(compartment);
            });
        // Whenever anything in a mapping list changes and other things which affect all
        // processors (including the real-time processor which takes care of sources only), resync
        // all mappings to *all* processors.
        when(self.mapping_list_changed())
            .with(weak_session.clone())
            .do_async(move |session, (compartment, _)| {
                session.borrow_mut().sync_all_mappings_full(compartment);
            });
        // Marking project as dirty if certain things are changed. Should only contain events that
        // are triggered by the user.
        when(self.settings_changed())
            .with(weak_session.clone())
            .do_sync(move |s, _| {
                s.borrow().mark_dirty();
            });
        when(self.mapping_list_changed())
            .with(weak_session.clone())
            .do_sync(move |s, (compartment, _)| {
                s.borrow_mut().mark_compartment_dirty(compartment);
            });
        // Keep adding/removing instance to/from upper floor.
        when(self.lives_on_upper_floor.changed())
            .with(weak_session.clone())
            .do_sync(move |s, _| {
                s.borrow().sync_upper_floor_membership();
            });
        // Keep syncing some general settings to real-time processor.
        when(self.settings_changed())
            .with(weak_session.clone())
            .do_async(move |s, _| {
                s.borrow().sync_settings();
            });
        // When FX is reordered, invalidate FX indexes. This is primarily for the GUI.
        // Existing GUID-tracked `Fx` instances will detect wrong index automatically.
        when(
            Global::control_surface_rx()
                .fx_reordered()
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session)
        .do_async(|s, _| {
            s.borrow_mut()
                .invalidate_fx_indexes_of_mapping_targets(Rc::downgrade(&s));
        });
    }

    pub fn activate_auto_load_mode(&mut self, mode: AutoLoadMode) -> anyhow::Result<()> {
        // Memorize or clear current preset ID for fallback purposes
        self.auto_load_fallback_preset_id = match mode {
            AutoLoadMode::Off => None,
            AutoLoadMode::UnitFx => {
                ensure!(
                    Reaper::get().version().revision() >= "6.37",
                    "Auto-load needs at least REAPER version 6.37!"
                );
                self.active_main_preset_id.clone()
            }
        };
        // Set auto-load mode
        self.auto_load_mode.set(mode);
        Ok(())
    }

    pub fn main_preset_is_auto_loaded(&self) -> bool {
        self.auto_load_mode.get().is_on() && self.active_main_preset_id.is_some()
    }

    /// This returns an early `false` if the desired preset is already active.
    fn auto_load_preset_linked_to_fx(&mut self, fx_id: Option<FxId>) -> bool {
        let final_preset_id = fx_id.and_then(|fx_id| self.find_preset_linked_to_fx(fx_id));
        self.auto_load_preset(final_preset_id)
    }

    fn find_preset_linked_to_fx(&self, fx_id: FxId) -> Option<String> {
        if let Some(preset_id) = self
            .unit_preset_link_config
            .find_preset_linked_to_fx(&fx_id)
        {
            return Some(preset_id);
        }
        if self.use_unit_preset_links_only {
            return None;
        }
        self.global_preset_link_manager
            .find_preset_linked_to_fx(&fx_id)
    }

    fn invalidate_fx_indexes_of_mapping_targets(&mut self, weak_session: WeakUnitModel) {
        let ids: Vec<_> = self
            .all_mappings()
            .map(|m| m.borrow().qualified_id())
            .collect();
        for id in ids {
            self.change_mapping_by_id_with_closure(id, None, weak_session.clone(), |ctx| {
                let affected = ctx
                    .mapping
                    .target_model
                    .invalidate_fx_index(ctx.extended_context, ctx.mapping.compartment())
                    .map(|affected| Affected::One(MappingProp::InTarget(affected)));
                Ok(affected)
            })
            .expect("error when invalidating FX indexes");
        }
    }

    /// Settings are all the things displayed in the ReaLearn header panel.
    fn settings_changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.let_matched_events_through
            .changed()
            .merge(self.let_unmatched_events_through.changed())
            .merge(self.stay_active_when_project_in_background.changed())
            .merge(self.control_input.changed())
            .merge(self.feedback_output.changed())
            .merge(self.send_feedback_only_if_armed.changed())
            .merge(self.reset_feedback_when_releasing_source.changed())
            .merge(self.auto_load_mode.changed())
            .merge(self.real_input_logging_enabled.changed())
            .merge(self.real_output_logging_enabled.changed())
            .merge(self.virtual_input_logging_enabled.changed())
            .merge(self.virtual_output_logging_enabled.changed())
            .merge(self.target_control_logging_enabled.changed())
    }

    pub fn captured_incoming_message(&mut self, event: MessageCaptureEvent) {
        self.incoming_msg_captured_subject.next(event);
    }

    pub fn create_compound_source_for_learning(
        &self,
        event: MessageCaptureEvent,
    ) -> Option<CompoundMappingSource> {
        // At first, try virtualized
        if event.allow_virtual_sources {
            if let Some(virtualization) = self.virtualize_source_value(event.result.message()) {
                if !virtualization.learnable {
                    return None;
                }
                let virtual_source =
                    VirtualSource::from_source_value(virtualization.virtual_source_value);
                return Some(CompoundMappingSource::Virtual(virtual_source));
            }
        }
        // Then direct
        CompoundMappingSource::from_message_capture_event(event)
    }

    /// Attention: If a mapping matches but given source value is a relative-zero (and matches), it
    /// will return a control value of "zero". This should be fine for most use cases, mostly it's
    /// only the control element which matters, not the concrete control value.
    pub fn virtualize_source_value(
        &self,
        source_value: IncomingCompoundSourceValue,
    ) -> Option<SourceValueVirtualization> {
        let instance_state = self.unit.borrow();
        let res = self
            .active_virtual_controller_mappings(&instance_state)
            .find_map(|m| {
                let m = m.borrow();
                let control_result = m
                    .source_model
                    .create_source()
                    .reacts_to_source_value_with(source_value)?;
                let control_value = match control_result {
                    ControlResult::Consumed => ControlValue::AbsoluteContinuous(UnitValue::MIN),
                    ControlResult::Processed(v) => v,
                };
                let virtualization = SourceValueVirtualization {
                    virtual_source_value: VirtualSourceValue::new(
                        m.target_model.create_control_element(),
                        control_value,
                    ),
                    learnable: m.target_model.learnable(),
                };
                Some(virtualization)
            });
        res
    }

    pub fn virtualize_source_model(&self, source_model: &SourceModel) -> Option<SourceModel> {
        let instance_state = self.unit.borrow();
        let res = self
            .active_virtual_controller_mappings(&instance_state)
            .find_map(|m| {
                let m = m.borrow();
                if m.source_model.create_source() == source_model.create_source() {
                    let element = m.target_model.create_control_element();
                    let virtual_source =
                        CompoundMappingSource::Virtual(VirtualSource::new(element));
                    let mut virtual_model = SourceModel::new();
                    let _ = virtual_model.apply_from_source(&virtual_source);
                    Some(virtual_model)
                } else {
                    None
                }
            });
        res
    }

    pub fn stream_deck_device_id(&self) -> Option<StreamDeckDeviceId> {
        self.stream_deck_device_id
    }

    pub fn wants_keyboard_input(&self) -> bool {
        self.wants_keyboard_input
    }

    fn active_virtual_controller_mappings<'a>(
        &'a self,
        instance_state: &'a Unit,
    ) -> impl Iterator<Item = &SharedMapping> {
        self.mappings(CompartmentKind::Controller).filter(move |m| {
            let m = m.borrow();
            if !m.control_is_enabled() {
                return false;
            }
            if m.target_model.category() != TargetCategory::Virtual {
                return false;
            }
            if !instance_state.mapping_is_on(m.qualified_id()) {
                // Since virtual mappings support conditional activation, too!
                return false;
            }
            true
        })
    }

    pub fn incoming_msg_captured(
        &self,
        reenable_control_after_touched: bool,
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    ) -> impl LocalObservable<'static, Item = MessageCaptureEvent, Err = ()> + 'static {
        // TODO-low We should migrate this to the nice async-await mechanism that we use for global
        //  learning (via REAPER action). That way we don't need the subject and also don't need
        //  to pass the information through multiple processors whether we allow virtual sources.
        // TODO-low Would be nicer to do this on subscription instead of immediately. from_fn()?
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::StartLearnSource {
                allow_virtual_sources,
                osc_arg_index_hint,
            });
        let main_sender = self.normal_main_task_sender.clone();
        self.incoming_msg_captured_subject
            .clone()
            .finalize(move || {
                if reenable_control_after_touched {
                    main_sender.send_complaining(NormalMainTask::ReturnToControlMode);
                }
            })
    }

    fn learn_target(&mut self, target: &ReaperTarget, weak_session: WeakUnitModel) {
        // Prevent learning targets from other project tabs (leads to weird effects, just think
        // about it)
        if let Some(p) = target.project() {
            if p != self.processor_context.project_or_current_project() {
                return;
            }
        }
        let qualified_id = self.unit.borrow_mut().set_mapping_which_learns_target(None);
        if let Some(qualified_id) = qualified_id {
            if let Some(mapping) = self.find_mapping_by_qualified_id(qualified_id).cloned() {
                let mut mapping = mapping.borrow_mut();
                let compartment = mapping.compartment();
                self.change_target_with_closure(&mut mapping, None, weak_session, |ctx| {
                    ctx.mapping.target_model.apply_from_target(
                        target,
                        ctx.extended_context,
                        compartment,
                    )
                });
            }
        }
    }

    pub fn processor_context(&self) -> &ProcessorContext {
        &self.processor_context
    }

    pub fn extended_context(&self) -> ExtendedProcessorContext {
        self.extended_context_with_params(&self.params)
    }

    pub fn extended_context_with_params<'a>(
        &'a self,
        params: &'a PluginParams,
    ) -> ExtendedProcessorContext<'a> {
        ExtendedProcessorContext::new(&self.processor_context, params, self.control_context())
    }

    pub fn control_context(&self) -> ControlContext {
        ControlContext {
            feedback_audio_hook_task_sender: self.global_feedback_audio_hook_task_sender,
            feedback_real_time_task_sender: &self.feedback_real_time_task_sender,
            osc_feedback_task_sender: self.global_osc_feedback_task_sender,
            feedback_output: self.feedback_output(),
            stream_deck_dev_id: self.stream_deck_device_id,
            unit_container: self.unit_container,
            instance: &self.instance,
            unit: self.unit(),
            unit_id: self.unit_id(),
            output_logging_enabled: self.real_output_logging_enabled.get(),
            source_context: Default::default(),
            mode_context: Default::default(),
            processor_context: &self.processor_context,
        }
    }

    pub fn add_group_with_default_values(
        &mut self,
        compartment: CompartmentKind,
        name: String,
    ) -> GroupId {
        let group = GroupModel::new_from_ui(compartment, name);
        self.add_group(compartment, group)
    }

    fn add_group(&mut self, compartment: CompartmentKind, group: GroupModel) -> GroupId {
        let id = group.id();
        let shared_group = Rc::new(RefCell::new(group));
        self.groups[compartment].push(shared_group);
        self.notify_group_list_changed(compartment);
        id
    }

    /// Also finds default group.
    pub fn find_group_index_by_id_sorted(
        &self,
        compartment: CompartmentKind,
        id: GroupId,
    ) -> Option<usize> {
        self.groups_sorted(compartment)
            .position(|g| g.borrow().id() == id)
    }

    pub fn group_contains_mappings(&self, compartment: CompartmentKind, id: GroupId) -> bool {
        self.mappings(compartment)
            .filter(|m| m.borrow().group_id() == id)
            .count()
            > 0
    }

    /// Doesn't find default group.
    pub fn find_group_by_id(
        &self,
        compartment: CompartmentKind,
        id: GroupId,
    ) -> Option<&SharedGroup> {
        self.groups[compartment]
            .iter()
            .find(|g| g.borrow().id() == id)
    }

    pub fn find_group_by_key(
        &self,
        compartment: CompartmentKind,
        key: &GroupKey,
    ) -> Option<&SharedGroup> {
        self.groups[compartment]
            .iter()
            .find(|g| g.borrow().key() == key)
    }

    pub fn find_group_by_id_including_default_group(
        &self,
        compartment: CompartmentKind,
        id: GroupId,
    ) -> Option<&SharedGroup> {
        if id.is_default() {
            Some(self.default_group(compartment))
        } else {
            self.find_group_by_id(compartment, id)
        }
    }

    pub fn find_group_by_index_sorted(
        &self,
        compartment: CompartmentKind,
        index: usize,
    ) -> Option<&SharedGroup> {
        self.groups_sorted(compartment).nth(index)
    }

    pub fn groups_sorted(
        &self,
        compartment: CompartmentKind,
    ) -> impl Iterator<Item = &SharedGroup> {
        iter::once(self.default_group(compartment)).chain(
            self.groups[compartment]
                .iter()
                .sorted_by_key(|g| g.borrow().effective_name().to_owned()),
        )
    }

    pub fn move_mappings_to_group(
        &mut self,
        compartment: CompartmentKind,
        mapping_ids: &[MappingId],
        group_id: GroupId,
        weak_session: WeakUnitModel,
    ) -> Result<(), &'static str> {
        for mapping_id in mapping_ids.iter() {
            let id = QualifiedMappingId::new(compartment, *mapping_id);
            self.change_mapping_from_session(
                id,
                MappingCommand::SetGroupId(group_id),
                weak_session.clone(),
            );
        }
        self.notify_group_list_changed(compartment);
        Ok(())
    }

    pub fn remove_group(
        &mut self,
        compartment: CompartmentKind,
        id: GroupId,
        delete_mappings: bool,
    ) {
        self.groups[compartment].retain(|g| g.borrow().id() != id);
        if delete_mappings {
            self.mappings[compartment].retain(|m| m.borrow().group_id() != id);
        } else {
            for m in self.mappings(compartment) {
                let mut m = m.borrow_mut();
                if m.group_id() == id {
                    let _ = m.change(MappingCommand::SetGroupId(GroupId::default()));
                }
            }
        }
        self.notify_group_list_changed(compartment);
    }

    /// Changes a mapping with notification and without initiator, expecting the mutable mapping
    /// itself to be passed as parameter.
    ///
    /// # Panics
    ///
    /// Panics if mapping not found.
    pub fn change_mapping_from_ui_simple(
        weak_session: WeakUnitModel,
        mapping: &mut MappingModel,
        cmd: MappingCommand,
        initiator: Option<u32>,
    ) {
        let Some(session) = weak_session.upgrade() else {
            // If session is gone, it doesn't matter anyway
            return;
        };
        let mut session = session.borrow_mut();
        session.change_mapping_from_ui_expert(mapping, cmd, initiator, weak_session);
    }

    pub fn change_mapping_from_ui_expert(
        &mut self,
        mapping: &mut MappingModel,
        cmd: MappingCommand,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
    ) {
        if let Some(affected) = mapping.change(cmd) {
            use Affected::*;
            let affected = One(SessionProp::InCompartment(
                mapping.compartment(),
                One(CompartmentProp::InMapping(mapping.id(), affected)),
            ));
            self.handle_affected(affected, initiator, weak_session);
        }
    }

    pub fn change_group_from_ui_simple(
        weak_session: WeakUnitModel,
        group: &mut GroupModel,
        cmd: GroupCommand,
        initiator: Option<u32>,
    ) {
        let session = weak_session.upgrade().expect("session gone");
        let mut session = session.borrow_mut();
        session.change_group_from_ui_expert(group, cmd, initiator, weak_session);
    }

    pub fn change_group_from_ui_expert(
        &mut self,
        group: &mut GroupModel,
        cmd: GroupCommand,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
    ) {
        if let Some(affected) = group.change(cmd) {
            use Affected::*;
            let affected = One(SessionProp::InCompartment(
                group.compartment(),
                One(CompartmentProp::InGroup(group.id(), affected)),
            ));
            self.handle_affected(affected, initiator, weak_session);
        }
    }

    /// Changes a mapping with notification and without initiator.
    ///
    /// # Panics
    ///
    /// Panics if mapping not found.
    fn change_mapping_from_session(
        &mut self,
        id: QualifiedMappingId,
        val: MappingCommand,
        weak_session: WeakUnitModel,
    ) {
        self.change_with_notification(
            SessionCommand::ChangeCompartment(
                id.compartment,
                CompartmentCommand::ChangeMapping(id.id, Box::new(val)),
            ),
            None,
            weak_session,
        );
    }

    /// The gateway point to change something in the session just using commands, also deeply nested
    /// things such as target properties.
    ///
    /// Reasoning: With this single point of entry for changing something in the session, we can
    /// easily intercept certain changes, notify the UI and so on. Without magic and without rxRust!
    pub fn change_with_notification(
        &mut self,
        cmd: SessionCommand,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
    ) {
        let _ = self.change_with_closure(initiator, weak_session, |session| session.change(cmd));
    }

    pub fn change(&mut self, cmd: SessionCommand) -> ChangeResult<SessionProp> {
        use Affected::*;
        use SessionCommand as C;
        use SessionProp as P;
        let affected = match cmd {
            C::SetUnitName(unit_name) => {
                self.name = unit_name;
                Some(One(SessionProp::UnitName))
            }
            C::SetUnitKey(unit_key) => {
                self.unit_key = unit_key;
                Some(One(SessionProp::UnitKey))
            }
            C::SetInstanceTrack(api_desc) => {
                let virtual_track =
                    domain::TrackDescriptor::from_api(api_desc.clone()).unwrap_or_default();
                let virtual_track = domain::TrackDescriptor {
                    enable_only_if_track_selected: false,
                    ..virtual_track
                };
                self.instance_track_descriptor = api_desc;
                self.unit
                    .borrow_mut()
                    .set_instance_track_descriptor(virtual_track);
                self.normal_main_task_sender
                    .send_complaining(NormalMainTask::NotifyConditionsChanged);
                Some(One(P::InstanceTrack))
            }
            C::SetInstanceFx(api_desc) => {
                let virtual_fx =
                    domain::FxDescriptor::from_api(api_desc.clone()).unwrap_or_default();
                let virtual_fx = domain::FxDescriptor {
                    enable_only_if_fx_has_focus: false,
                    ..virtual_fx
                };
                self.instance_fx_descriptor = api_desc;
                self.unit
                    .borrow_mut()
                    .set_instance_fx_descriptor(virtual_fx);
                self.normal_main_task_sender
                    .send_complaining(NormalMainTask::NotifyConditionsChanged);
                Some(One(P::InstanceFx))
            }
            C::SetWantsKeyboardInput(value) => {
                self.wants_keyboard_input = value;
                Some(One(P::WantsKeyboardInput))
            }
            C::SetStreamDeckDevice(value) => {
                self.stream_deck_device_id = value;
                Some(One(P::StreamDeckDeviceId))
            }
            C::ChangeCompartment(compartment, cmd) => self
                .change_compartment_internal(compartment, cmd)?
                .map(|affected| One(P::InCompartment(compartment, affected))),
            C::AdjustMappingModeIfNecessary(id) => self
                .changing_mapping_by_id(id, |ctx| {
                    Ok(ctx.mapping.adjust_mode_if_necessary(ctx.extended_context))
                })?
                .map(|affected| One(P::InCompartment(id.compartment, affected))),
        };
        Ok(affected)
    }

    pub fn change_mapping_by_id_with_closure(
        &mut self,
        id: QualifiedMappingId,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
        f: impl FnOnce(MappingChangeContext) -> ChangeResult<MappingProp>,
    ) -> Result<(), String> {
        use Affected::*;
        use SessionProp as P;
        let affected = self
            .changing_mapping_by_id(id, f)?
            .map(|affected| One(P::InCompartment(id.compartment, affected)));
        if let Some(affected) = affected {
            self.handle_affected(affected, initiator, weak_session);
        }
        Ok(())
    }

    pub fn change_target_with_closure(
        &mut self,
        mapping: &mut MappingModel,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
        f: impl FnOnce(MappingChangeContext) -> Option<Affected<TargetProp>>,
    ) {
        let _ = self.change_mapping_with_closure(mapping, initiator, weak_session, |ctx| {
            Ok(f(ctx).map(|affected| Affected::One(MappingProp::InTarget(affected))))
        });
    }

    pub fn change_mapping_with_closure(
        &mut self,
        mapping: &mut MappingModel,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
        f: impl FnOnce(MappingChangeContext) -> ChangeResult<MappingProp>,
    ) -> Result<(), String> {
        use Affected::*;
        use SessionProp as P;
        let affected = self
            .changing_mapping(mapping, f)?
            .map(|affected| One(P::InCompartment(mapping.compartment(), affected)));
        if let Some(affected) = affected {
            self.handle_affected(affected, initiator, weak_session);
        }
        Ok(())
    }

    pub fn notify_compartment_has_changed(
        &mut self,
        compartment: CompartmentKind,
        weak_session: WeakUnitModel,
    ) {
        use Affected::*;
        self.handle_affected(
            One(SessionProp::InCompartment(compartment, Multiple)),
            None,
            weak_session,
        );
    }

    pub fn notify_mapping_has_changed(
        &mut self,
        id: QualifiedMappingId,
        weak_session: WeakUnitModel,
    ) {
        use Affected::*;
        self.handle_affected(
            One(SessionProp::InCompartment(
                id.compartment,
                One(CompartmentProp::InMapping(id.id, Multiple)),
            )),
            None,
            weak_session,
        );
    }

    fn change_with_closure(
        &mut self,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
        f: impl FnOnce(&mut UnitModel) -> ChangeResult<SessionProp>,
    ) -> Result<(), String> {
        if let Some(affected) = f(self)? {
            self.handle_affected(affected, initiator, weak_session);
        }
        Ok(())
    }

    fn handle_affected(
        &self,
        affected: Affected<SessionProp>,
        initiator: Option<u32>,
        weak_session: WeakUnitModel,
    ) {
        // We react in the next main loop cycle. First, because otherwise we can easily run into
        // BorrowMut errors (because the handler might borrow the session but we still have it
        // borrowed at this point because this handler is called by the session). Second, because
        // deferring the reaction seems to result in a smoother user experience.
        //
        // Sending all affected properties to the next main loop cycle as one batch can improve
        // could make flickering less likely, so do it.
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(move || {
                // Internal reaction
                let Some(session) = weak_session.upgrade() else {
                    // We panicked here before and that was sometimes popping up as an error. But
                    // if the session doesn't exist anymore, then this is always a sign that the
                    // ReaLearn FX instance has been removed, which is fine. And whatever we
                    // want to do here then wouldn't matter anyway. So don't panic!
                    return;
                };
                {
                    use Affected::*;
                    use CompartmentProp::*;
                    use SessionProp::*;
                    let mut session = session.borrow_mut();
                    match &affected {
                        One(WantsKeyboardInput | StreamDeckDeviceId) => {
                            session.sync_settings();
                        }
                        One(InCompartment(compartment, One(Notes))) => {
                            session.mark_compartment_dirty(*compartment);
                        }
                        One(InCompartment(compartment, One(CommonLua))) => {
                            session.sync_compartment_settings(*compartment);
                            session.mark_compartment_dirty(*compartment);
                        }
                        One(InCompartment(compartment, One(InGroup(_, affected)))) => {
                            // Sync all mappings to processor if necessary (change of a single
                            // group can affect many mappings)
                            if affected.processing_relevance().is_some() {
                                session.sync_all_mappings_full(*compartment);
                            }
                            // Mark dirty
                            session.mark_compartment_dirty(*compartment);
                        }
                        One(InCompartment(compartment, One(InMapping(mapping_id, affected)))) => {
                            // Sync mapping to processors if necessary.
                            if let Some(relevance) = affected.processing_relevance() {
                                if let Some(mapping) =
                                    session.find_mapping_by_id(*compartment, *mapping_id)
                                {
                                    let mapping = mapping.borrow();
                                    use ProcessingRelevance::*;
                                    match relevance {
                                        PersistentProcessingRelevant => {
                                            // Keep syncing persistent mapping processing state only
                                            // (must be cheap because can be triggered by processing).
                                            session
                                                .sync_persistent_mapping_processing_state(&mapping);
                                        }
                                        ProcessingRelevant => {
                                            // Keep syncing complete mappings to processors.
                                            session.sync_single_mapping_to_processors(&mapping);
                                        }
                                    }
                                }
                            }
                            // Mark dirty
                            session.mark_compartment_dirty(*compartment);
                        }
                        _ => {}
                    }
                }
                // UI reaction
                {
                    // Borrowing the session while UI update shouldn't be an issue
                    // because we are just invalidating the UI. A UI reaction shouldn't
                    // need to borrow the session mutably. In case it's going to be an issue,
                    // we can also choose to clone the weak main panel instead.
                    let session = session.borrow();
                    session.ui().handle_affected(&session, affected, initiator);
                }
            })
            .unwrap();
    }

    pub fn ui(&self) -> &dyn SessionUi {
        &**(self.ui.get().expect("UI not yet set"))
    }

    fn change_compartment_internal(
        &mut self,
        compartment: CompartmentKind,
        cmd: CompartmentCommand,
    ) -> ChangeResult<CompartmentProp> {
        use CompartmentCommand as C;
        let affected = match cmd {
            C::ChangeMapping(mapping_id, cmd) => self.changing_mapping_by_id(
                QualifiedMappingId::new(compartment, mapping_id),
                move |ctx| Ok(ctx.mapping.change(*cmd)),
            )?,
            C::SetNotes(notes) => {
                self.compartment_notes[compartment] = notes;
                Some(Affected::One(CompartmentProp::Notes))
            }
            C::SetCommonLua(notes) => {
                self.compartment_common_lua[compartment] = notes;
                Some(Affected::One(CompartmentProp::CommonLua))
            }
        };
        Ok(affected)
    }

    fn changing_mapping_by_id(
        &mut self,
        id: QualifiedMappingId,
        f: impl FnOnce(MappingChangeContext) -> ChangeResult<MappingProp>,
    ) -> ChangeResult<CompartmentProp> {
        let mapping = self
            .find_mapping_by_id(id.compartment, id.id)
            .ok_or_else(|| String::from("mapping not found"))?
            .clone();
        let mut mapping = mapping.borrow_mut();
        self.changing_mapping(&mut mapping, f)
    }

    fn changing_mapping(
        &mut self,
        mapping: &mut MappingModel,
        f: impl FnOnce(MappingChangeContext) -> ChangeResult<MappingProp>,
    ) -> ChangeResult<CompartmentProp> {
        use Affected::*;
        let change_context = MappingChangeContext {
            mapping,
            extended_context: self.extended_context(),
        };
        Ok(f(change_context)?
            .map(|affected| One(CompartmentProp::InMapping(mapping.id(), affected))))
    }

    pub fn compartment_in_session(&self, compartment: CompartmentKind) -> CompartmentInSession {
        CompartmentInSession {
            session: self,
            compartment,
        }
    }

    pub fn add_default_mapping(
        &mut self,
        compartment: CompartmentKind,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_character: VirtualControlElementCharacter,
    ) -> SharedMapping {
        let mut mapping = MappingModel::new(
            compartment,
            initial_group_id,
            MappingKey::random(),
            MappingId::random(),
        );
        let new_name = self.generate_name_for_new_mapping(compartment);
        let _ = mapping.change(MappingCommand::SetName(new_name));
        if compartment == CompartmentKind::Controller {
            let next_control_element_index =
                self.get_next_control_element_index(control_element_character);
            mapping.target_model =
                TargetModel::virtual_default(control_element_character, next_control_element_index);
        }
        self.add_mapping(compartment, mapping)
    }

    /// Silently assigns random keys if given keys conflict with existing keys or are not unique.
    pub fn insert_mappings_at(
        &mut self,
        compartment: CompartmentKind,
        index: usize,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        let mut mapping_key_set = self.mapping_key_set(compartment);
        let mut index = index.min(self.mappings[compartment].len());
        let mut first_mapping_id = None;
        for mut m in mappings {
            if !mapping_key_set.insert(m.key().clone()) {
                m.reset_key();
            }
            if first_mapping_id.is_none() {
                first_mapping_id = Some(m.id());
            }
            let shared_mapping = share_mapping(m);
            self.mappings[compartment].insert(index, shared_mapping);
            index += 1;
        }
        self.notify_mapping_list_changed(compartment, first_mapping_id);
    }

    /// Silently assigns random keys if given keys conflict with existing keys or are not unique.
    pub fn replace_mappings_of_group(
        &mut self,
        compartment: CompartmentKind,
        group_id: GroupId,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        let mut mapping_key_set = self.mapping_key_set(compartment);
        self.mappings[compartment].retain(|m| m.borrow().group_id() != group_id);
        for mut m in mappings {
            if !mapping_key_set.insert(m.key().clone()) {
                m.reset_key();
            }
            let shared_mapping = share_mapping(m);
            self.mappings[compartment].push(shared_mapping);
        }
        self.notify_mapping_list_changed(compartment, None);
    }

    fn mapping_key_set(&self, compartment: CompartmentKind) -> NonCryptoHashSet<MappingKey> {
        self.mappings[compartment]
            .iter()
            .map(|m| m.borrow().key().clone())
            .collect()
    }

    fn get_next_control_element_index(&self, character: VirtualControlElementCharacter) -> u32 {
        let max_index_so_far = self
            .mappings(CompartmentKind::Controller)
            .filter_map(|m| {
                let m = m.borrow();
                let target = &m.target_model;
                if target.category() != TargetCategory::Virtual
                    || target.control_element_character() != character
                {
                    return None;
                }
                if let VirtualControlElementId::Indexed(i) = target.control_element_id() {
                    Some(i)
                } else {
                    None
                }
            })
            .max();
        if let Some(i) = max_index_so_far {
            i + 1
        } else {
            0
        }
    }

    pub fn start_learning_many_mappings(
        &mut self,
        session: &SharedUnitModel,
        compartment: CompartmentKind,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_character: VirtualControlElementCharacter,
    ) {
        // Prepare
        self.disable_control();
        self.stop_mapping_actions();
        // Add initial mapping and start learning its source
        self.add_and_learn_one_of_many_mappings(
            session,
            compartment,
            initial_group_id,
            control_element_character,
        );
        // After target learned, add new mapping and start learning its source
        let instance_state = self.unit.borrow();
        let prop_to_observe = match compartment {
            // For controller mappings we don't need to learn a target so we move on to the next
            // mapping as soon as the source has been learned.
            CompartmentKind::Controller => instance_state.mapping_which_learns_source(),
            // For main mappings we want to learn a target before moving on to the next mapping.
            CompartmentKind::Main => instance_state.mapping_which_learns_target(),
        };
        when(
            prop_to_observe
                .changed_to(None)
                .take_until(self.learn_many_state.changed_to(None)),
        )
        .with(Rc::downgrade(session))
        .do_async(move |session, _| {
            session.borrow_mut().add_and_learn_one_of_many_mappings(
                &session,
                compartment,
                initial_group_id,
                control_element_character,
            );
        });
    }

    fn add_and_learn_one_of_many_mappings(
        &mut self,
        session: &SharedUnitModel,
        compartment: CompartmentKind,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_character: VirtualControlElementCharacter,
    ) {
        let ignore_sources: Vec<_> = match compartment {
            // When batch-learning controller mappings, we just want to learn sources that have
            // not yet been learned. Otherwise when we move a fader, we create many mappings in
            // one go.
            CompartmentKind::Controller => self
                .mappings(compartment)
                .map(|m| m.borrow().source_model.create_source())
                .collect(),
            // When batch-learning main mappings, we always wait for a target touch between the
            // mappings, so this is not necessary.
            CompartmentKind::Main => vec![],
        };
        let mapping =
            self.add_default_mapping(compartment, initial_group_id, control_element_character);
        let qualified_mapping_id = mapping.borrow().qualified_id();
        self.learn_many_state
            .set(Some(LearnManyState::learning_source(
                compartment,
                qualified_mapping_id.id,
                control_element_character,
            )));
        self.start_learning_source_internal(
            Rc::downgrade(session),
            qualified_mapping_id,
            false,
            ignore_sources,
        )
        .expect("error during learn many");
        // If this is a main mapping, start learning target as soon as source learned. For
        // controller mappings we don't need to do this because adding the default mapping will
        // automatically increase the virtual target control element index (which is usually what
        // one wants when creating a controller mapping).
        if compartment == CompartmentKind::Main {
            when(
                self.unit()
                    .borrow()
                    .mapping_which_learns_source()
                    .changed_to(None)
                    .take_until(self.learn_many_state.changed_to(None))
                    .take(1),
            )
            .with(Rc::downgrade(session))
            .do_async(move |shared_session, _| {
                let mut session = shared_session.borrow_mut();
                session
                    .learn_many_state
                    .set(Some(LearnManyState::learning_target(
                        compartment,
                        qualified_mapping_id.id,
                    )));
                let filter = (ReaperTargetType::all(), TargetTouchCause::Reaper);
                session.start_learning_target_internal(
                    Rc::downgrade(&shared_session),
                    qualified_mapping_id,
                    false,
                    filter,
                );
            });
        }
    }

    pub fn stop_learning_many_mappings(&mut self) {
        self.learn_many_state.set(None);
        let source_learning_mapping_id = self.unit.borrow().mapping_which_learns_source().get();
        self.stop_mapping_actions();
        self.enable_control();
        // Remove last added mapping if source not learned already
        if let Some(id) = source_learning_mapping_id {
            self.remove_mapping(id);
        }
    }

    pub fn learn_many_state_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.learn_many_state.changed()
    }

    pub fn is_learning_many_mappings(&self) -> bool {
        self.learn_many_state.get_ref().is_some()
    }

    pub fn learn_many_state(&self) -> Option<&LearnManyState> {
        self.learn_many_state.get_ref().as_ref()
    }

    pub fn mapping_count(&self, compartment: CompartmentKind) -> usize {
        self.mappings[compartment].len()
    }

    pub fn find_mapping_by_qualified_id(&self, id: QualifiedMappingId) -> Option<&SharedMapping> {
        self.find_mapping_by_id(id.compartment, id.id)
    }

    pub fn find_mapping_and_index_by_qualified_id(
        &self,
        id: QualifiedMappingId,
    ) -> Option<(usize, &SharedMapping)> {
        self.find_mapping_and_index_by_id(id.compartment, id.id)
    }

    pub fn find_mapping_by_id(
        &self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
    ) -> Option<&SharedMapping> {
        Some(
            self.find_mapping_and_index_by_id(compartment, mapping_id)?
                .1,
        )
    }

    pub fn find_mapping_and_index_by_id(
        &self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
    ) -> Option<(usize, &SharedMapping)> {
        self.mappings(compartment)
            .enumerate()
            .find(|(_, m)| m.borrow().id() == mapping_id)
    }

    pub fn find_mapping_id_by_key(
        &self,
        compartment: CompartmentKind,
        key: &MappingKey,
    ) -> Option<MappingId> {
        self.mappings(compartment).find_map(|m| {
            let m = m.try_borrow().ok()?;
            if m.key() == key {
                Some(m.id())
            } else {
                None
            }
        })
    }

    pub fn find_mapping_by_key(
        &self,
        compartment: CompartmentKind,
        key: &MappingKey,
    ) -> Option<SharedMapping> {
        self.mappings(compartment)
            .find(|m| {
                let m = m.borrow();
                m.key() == key
            })
            .cloned()
    }

    pub fn mappings(&self, compartment: CompartmentKind) -> impl Iterator<Item = &SharedMapping> {
        self.mappings[compartment].iter()
    }

    pub fn default_group(&self, compartment: CompartmentKind) -> &SharedGroup {
        match compartment {
            CompartmentKind::Controller => &self.default_controller_group,
            CompartmentKind::Main => &self.default_main_group,
        }
    }

    pub fn groups(&self, compartment: CompartmentKind) -> impl Iterator<Item = &SharedGroup> {
        self.groups[compartment].iter()
    }

    fn groups_including_default_group(
        &self,
        compartment: CompartmentKind,
    ) -> impl Iterator<Item = &SharedGroup> {
        std::iter::once(self.default_group(compartment)).chain(self.groups[compartment].iter())
    }

    fn all_mappings(&self) -> impl Iterator<Item = &SharedMapping> {
        CompartmentKind::enum_iter().flat_map(move |compartment| self.mappings(compartment))
    }

    pub fn toggle_learning_source(
        &mut self,
        session: WeakUnitModel,
        mapping_id: QualifiedMappingId,
    ) -> Result<(), &'static str> {
        let currently_learning_mapping_id = self.unit.borrow().mapping_which_learns_source().get();
        if let Some(id) = currently_learning_mapping_id {
            if id == mapping_id {
                self.stop_learning_source();
                return Ok(());
            }
        }
        self.start_learning_source(session, mapping_id, vec![])
    }

    fn start_learning_source(
        &mut self,
        session: WeakUnitModel,
        mapping_id: QualifiedMappingId,
        ignore_sources: Vec<CompoundMappingSource>,
    ) -> Result<(), &'static str> {
        if self
            .unit
            .borrow()
            .mapping_which_learns_source()
            .get_ref()
            .is_some()
        {
            // Learning active already. Simply change the mapping that's going to be learned.
            self.unit
                .borrow_mut()
                .set_mapping_which_learns_source(Some(mapping_id));
            Ok(())
        } else {
            self.start_learning_source_internal(session, mapping_id, true, ignore_sources)
        }
    }

    fn start_learning_source_internal(
        &mut self,
        session: WeakUnitModel,
        mapping_id: QualifiedMappingId,
        reenable_control_after_touched: bool,
        ignore_sources: Vec<CompoundMappingSource>,
    ) -> Result<(), &'static str> {
        // Warn if settings are not good
        if self.control_input.get().is_midi_fx_input() {
            if let Some(track) = self.processor_context.track() {
                if !track
                    .recording_input()
                    .is_some_and(|i| matches!(i, RecordingInput::Midi { .. }))
                {
                    self.ui().handle_external_info_event(
                        InstanceInfoEvent::MidiLearnFromFxInputButTrackHasAudioInput,
                    );
                } else if !track.is_armed(false)
                    || track.input_monitoring_mode() != InputMonitoringMode::Normal
                {
                    self.ui().handle_external_info_event(
                        InstanceInfoEvent::MidiLearnFromFxInputButTrackNotArmed,
                    );
                }
            }
        }
        let allow_virtual_sources = mapping_id.compartment != CompartmentKind::Controller;
        let osc_arg_index_hint = {
            let mapping = self
                .find_mapping_by_qualified_id(mapping_id)
                .ok_or("mapping not found")?;
            let m = mapping.borrow();
            m.source_model.osc_arg_index()
        };
        self.unit
            .borrow_mut()
            .set_mapping_which_learns_source(Some(mapping_id));
        when(
            self.incoming_msg_captured(
                reenable_control_after_touched,
                allow_virtual_sources,
                osc_arg_index_hint,
            )
            .filter(move |capture_event: &MessageCaptureEvent| {
                !ignore_sources.iter().any(|is| {
                    is.reacts_to_source_value_with(capture_event.result.message())
                        .is_some()
                })
            })
            // We have this explicit stop criteria because we listen to global REAPER
            // events.
            .take_until(self.party_is_over())
            // If the user stops learning manually without ever touching the controller.
            .take_until(
                self.unit
                    .borrow()
                    .mapping_which_learns_source()
                    .changed_to(None),
            ),
        )
        .with(session)
        .do_async(|shared_session, event: MessageCaptureEvent| {
            let mut session = shared_session.borrow_mut();
            let qualified_id = session.unit.borrow().mapping_which_learns_source().get();
            if let Some(qualified_id) = qualified_id {
                if let Some(source) = session.create_compound_source_for_learning(event) {
                    // The learn process should stop when removing a mapping but just in case,
                    // let's react gracefully if the mapping doesn't exist anymore (do nothing).
                    let _ = session.change_mapping_by_id_with_closure(
                        qualified_id,
                        None,
                        Rc::downgrade(&shared_session),
                        |ctx| Ok(ctx.mapping.source_model.apply_from_source(&source)),
                    );
                    session
                        .unit
                        .borrow_mut()
                        .set_mapping_which_learns_source(None);
                }
            }
        });
        Ok(())
    }

    fn stop_learning_source(&self) {
        self.unit.borrow_mut().set_mapping_which_learns_source(None);
    }

    pub fn toggle_learning_target(
        &mut self,
        session: WeakUnitModel,
        mapping_id: QualifiedMappingId,
    ) {
        let currently_learning_mapping_id = self.unit.borrow().mapping_which_learns_target().get();
        if let Some(id) = currently_learning_mapping_id {
            if id == mapping_id {
                self.stop_learning_target();
                return;
            }
        }
        let filter = (ReaperTargetType::all(), TargetTouchCause::Reaper);
        self.start_learning_target_internal(session, mapping_id, true, filter);
    }

    /// Not setting `included_targets` means all targets are potentially included. This should
    /// not be used for presets / Lua code because the set of learnable targets will widen in
    /// future.
    fn start_learning_target_internal(
        &mut self,
        weak_session: WeakUnitModel,
        mapping_id: QualifiedMappingId,
        handle_control_disabling: bool,
        filter: (NonCryptoHashSet<ReaperTargetType>, TargetTouchCause),
    ) {
        let instance_id = self.unit_id;
        if handle_control_disabling {
            self.disable_control();
        }
        self.unit
            .borrow_mut()
            .set_mapping_which_learns_target(Some(mapping_id));
        Global::future_support().spawn_in_main_thread_from_main_thread(async move {
            let receiver = weak_session
                .upgrade()
                .ok_or(SESSION_GONE)?
                .borrow()
                .control_surface_main_task_sender
                .capture_targets(Some(instance_id));
            while let Ok(event) = receiver.recv().await {
                let filter = LastTouchedTargetFilter {
                    included_target_types: &filter.0,
                    touch_cause: filter.1,
                };
                if !filter.matches(&event) {
                    continue;
                }
                let session = weak_session.upgrade().ok_or(SESSION_GONE)?;
                let mut session = session.borrow_mut();
                session.learn_target(&event.target, weak_session.clone());
                session.stop_learning_target();
            }
            Ok(())
        });
    }

    fn disable_control(&self) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::DisableControl);
    }

    fn enable_control(&self) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::ReturnToControlMode);
    }

    fn stop_learning_target(&self) {
        self.control_surface_main_task_sender
            .stop_capturing_targets(Some(self.unit_id));
        self.unit.borrow_mut().set_mapping_which_learns_target(None);
        self.enable_control();
    }

    fn find_index_of_closest_mapping(
        &self,
        compartment: CompartmentKind,
        mapping: &SharedMapping,
        index: usize,
        within_same_group: bool,
        increment: isize,
    ) -> Option<usize> {
        let mappings = &self.mappings[compartment];
        let total_mapping_count = mappings.len();
        let result_index = if within_same_group {
            let group_id = mapping.borrow().group_id();
            let mut i = index as isize + increment;
            while i >= 0 && i < total_mapping_count as isize {
                let m = &mappings[i as usize];
                if m.borrow().group_id() == group_id {
                    break;
                }
                i += increment;
            }
            i
        } else {
            index as isize + increment
        };
        if result_index < 0 || result_index as usize >= total_mapping_count {
            return None;
        }
        Some(result_index as usize)
    }

    pub fn move_mapping_within_list(
        &mut self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
        within_same_group: bool,
        increment: isize,
    ) -> Result<(), &str> {
        let (current_index, mapping) = self
            .find_mapping_and_index_by_id(compartment, mapping_id)
            .ok_or("mapping not found")?;
        let dest_index = self
            .find_index_of_closest_mapping(
                compartment,
                mapping,
                current_index,
                within_same_group,
                increment,
            )
            .ok_or("move not possible because boundary reached")?;
        let pending_mapping = self.mappings[compartment].remove(current_index);
        self.mappings[compartment].insert(dest_index, pending_mapping);
        self.notify_mapping_list_changed(compartment, None);
        Ok(())
    }

    pub fn remove_mapping(&mut self, id: QualifiedMappingId) {
        self.remove_mapping_internal(id);
    }

    fn remove_mapping_internal(&mut self, id: QualifiedMappingId) {
        self.stop_mapping_actions();
        self.mappings[id.compartment].retain(|m| m.borrow().id() != id.id);
        self.notify_mapping_list_changed(id.compartment, None);
    }

    fn stop_mapping_actions(&mut self) {
        self.stop_learning_source();
        self.stop_learning_target();
    }

    pub fn duplicate_mapping(&mut self, id: QualifiedMappingId) -> Result<(), &str> {
        let (index, mapping) = self.mappings[id.compartment]
            .iter()
            .enumerate()
            .find(|(_i, m)| m.borrow().id() == id.id)
            .ok_or("mapping not found")?;
        let duplicate = mapping.borrow().duplicate();
        let duplicate_id = duplicate.id();
        self.mappings[id.compartment].insert(index + 1, share_mapping(duplicate));
        self.notify_mapping_list_changed(id.compartment, Some(duplicate_id));
        Ok(())
    }

    pub fn has_mapping(&self, mapping: *const MappingModel) -> bool {
        self.all_mappings().any(|m| m.as_ptr() == mapping as _)
    }

    pub fn index_of_mapping(
        &self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
    ) -> Option<usize> {
        self.mappings[compartment]
            .iter()
            .position(|m| m.borrow().id() == mapping_id)
    }

    pub fn show_in_floating_window(&self) {
        let _ = self
            .processor_context()
            .containing_fx()
            .show_in_floating_window();
    }

    pub fn containing_fx_is_in_input_fx_chain(&self) -> bool {
        self.processor_context.containing_fx().is_input_fx()
    }

    pub fn use_unit_preset_links_only(&self) -> bool {
        self.use_unit_preset_links_only
    }

    pub fn set_use_unit_preset_links_only(&mut self, value: bool) {
        self.use_unit_preset_links_only = value;
    }

    pub fn instance_preset_link_config(&self) -> &FxPresetLinkConfig {
        &self.unit_preset_link_config
    }

    pub fn instance_preset_link_config_mut(&mut self) -> &mut FxPresetLinkConfig {
        &mut self.unit_preset_link_config
    }

    pub fn set_instance_preset_link_config(&mut self, config: FxPresetLinkConfig) {
        self.unit_preset_link_config = config;
    }

    pub fn set_active_controller_id_without_notification(
        &mut self,
        active_controller_id: Option<String>,
    ) {
        self.active_controller_preset_id = active_controller_id;
    }

    pub fn set_active_main_preset_id_without_notification(
        &mut self,
        active_main_preset_id: Option<String>,
    ) {
        self.active_main_preset_id = active_main_preset_id;
    }

    pub fn active_controller_preset_id(&self) -> Option<&str> {
        self.active_controller_preset_id.as_deref()
    }

    pub fn active_preset_id(&self, compartment: CompartmentKind) -> Option<&str> {
        let id = match compartment {
            CompartmentKind::Controller => &self.active_controller_preset_id,
            CompartmentKind::Main => &self.active_main_preset_id,
        };
        id.as_deref()
    }

    pub fn compartment_common_lua(&self, compartment: CompartmentKind) -> &str {
        &self.compartment_common_lua[compartment]
    }

    pub fn compartment_notes(&self, compartment: CompartmentKind) -> &str {
        &self.compartment_notes[compartment]
    }

    pub fn active_main_preset(&self) -> Option<CompartmentPresetModel> {
        let id = self.active_preset_id(CompartmentKind::Main)?;
        self.main_preset_manager.find_by_id(id)
    }

    /// Returns `true` if the preset has unsaved changes (if a preset is active) or if at least one
    /// mapping or group exists (if no preset is active).
    pub fn compartment_or_preset_is_dirty(&self, compartment: CompartmentKind) -> bool {
        if self.active_preset_id(compartment).is_some() {
            // Preset active.
            self.compartment_is_dirty[compartment].get()
        } else {
            // No preset active.
            !self.mappings[compartment].is_empty() || !self.groups[compartment].is_empty()
        }
    }

    pub fn activate_controller_preset(&mut self, id: Option<String>) {
        let compartment = CompartmentKind::Controller;
        let model = if let Some(id) = id.as_ref() {
            self.controller_preset_manager
                .find_by_id(id)
                .map(|preset| preset.model().clone())
        } else {
            // <None> preset
            None
        };
        self.active_controller_preset_id = id;
        self.replace_compartment(compartment, model);
        self.compartment_is_dirty[compartment].set(false);
    }

    pub fn auto_load_fallback_compartment(&self) -> Option<&CompartmentModel> {
        self.auto_load_fallback_compartment.as_ref()
    }

    pub fn set_auto_load_fallback_compartment_model(
        &mut self,
        compartment: Option<CompartmentModel>,
    ) {
        self.auto_load_fallback_compartment = compartment;
    }

    pub fn auto_load_fallback_preset_id(&self) -> Option<&str> {
        self.auto_load_fallback_preset_id.as_deref()
    }

    pub fn set_auto_load_fallback_preset_id(&mut self, preset_id: Option<String>) {
        self.auto_load_fallback_preset_id = preset_id;
    }

    pub fn activate_preset(&mut self, compartment: CompartmentKind, id: Option<String>) {
        match compartment {
            CompartmentKind::Controller => self.activate_controller_preset(id),
            CompartmentKind::Main => self.activate_main_preset(id),
        }
    }

    pub fn activate_main_preset(&mut self, id: Option<String>) {
        let model = if let Some(id) = id.as_ref() {
            self.main_preset_manager
                .find_by_id(id)
                .map(|preset| preset.model().clone())
        } else {
            // <None> preset
            None
        };
        self.activate_main_preset_internal(id, model);
    }

    /// This returns an early `false` if the desired preset is already active.
    fn auto_load_preset(&mut self, preset_id: Option<String>) -> bool {
        if self.active_main_preset_id == preset_id {
            return false;
        }
        let instruction = self.take_auto_load_preset_load_instruction(preset_id);
        self.activate_main_preset_internal(instruction.preset_id, instruction.compartment_model);
        true
    }

    fn take_auto_load_preset_load_instruction(
        &mut self,
        preset_id: Option<String>,
    ) -> PresetLoadInstruction {
        if let Some(preset_id) = preset_id {
            // There's a linked preset. Try to load it.
            let compartment_model = self
                .main_preset_manager
                .find_by_id(&preset_id)
                .map(|p| p.model().clone());
            // Maybe save current mappings for later fallback
            if compartment_model.is_some()
                && self.active_main_preset_id.is_none()
                && self.auto_load_fallback_preset_id.is_none()
            {
                // Currently, <None> preset is active and there's no fallback preset.
                // Save current mappings, so that we can fall back to them later on.
                let compartment_model = self.extract_compartment_model(CompartmentKind::Main);
                self.auto_load_fallback_compartment = Some(compartment_model);
            }
            PresetLoadInstruction::new(Some(preset_id), compartment_model)
        } else if let Some(fallback_preset_id) = &self.auto_load_fallback_preset_id {
            // There's no linked preset. We have a fallback preset though.
            let compartment_model = self
                .main_preset_manager
                .find_by_id(fallback_preset_id)
                .map(|p| p.model().clone());
            PresetLoadInstruction::new(Some(fallback_preset_id.clone()), compartment_model)
        } else {
            // There's no linked preset and also no fallback preset. Fall back to ad-hoc memorized compartment.
            PresetLoadInstruction::new(None, self.auto_load_fallback_compartment.take())
        }
    }

    fn activate_main_preset_internal(
        &mut self,
        id: Option<String>,
        model: Option<CompartmentModel>,
    ) {
        let compartment = CompartmentKind::Main;
        self.active_main_preset_id = id;
        self.replace_compartment(compartment, model);
        self.compartment_is_dirty[compartment].set(false);
    }

    pub fn extract_compartment_model(&self, compartment: CompartmentKind) -> CompartmentModel {
        CompartmentModel {
            parameters: self
                .params
                .compartment_params(compartment)
                .non_default_settings(),
            default_group: self.default_group(compartment).borrow().clone(),
            groups: self
                .groups(compartment)
                .map(|ptr| ptr.borrow().clone())
                .collect(),
            mappings: self
                .mappings(compartment)
                .map(|ptr| ptr.borrow().clone())
                .collect(),
            custom_data: self
                .unit()
                .borrow()
                .custom_compartment_data(compartment)
                .clone(),
            common_lua: self.compartment_common_lua[compartment].clone(),
            notes: self.compartment_notes[compartment].clone(),
        }
    }

    /// Precondition: The given compartment model should be valid (e.g. no duplicate IDs)!
    pub fn import_compartment(
        &mut self,
        compartment: CompartmentKind,
        model: Option<CompartmentModel>,
    ) {
        self.replace_compartment(compartment, model);
        self.mark_compartment_dirty(compartment);
    }

    /// Precondition: The given compartment model should be valid (e.g. no duplicate IDs)!
    fn replace_compartment(
        &mut self,
        compartment: CompartmentKind,
        model: Option<CompartmentModel>,
    ) {
        self.stop_mapping_actions();
        if let Some(model) = model {
            let default_group = match compartment {
                CompartmentKind::Main => &mut self.default_main_group,
                CompartmentKind::Controller => &mut self.default_controller_group,
            };
            default_group.replace(model.default_group);
            self.set_groups_without_notification(compartment, model.groups);
            self.set_mappings_without_notification(compartment, model.mappings);
            let compartment_params = self.params.compartment_params_mut(compartment);
            compartment_params.reset_all();
            compartment_params.apply_given_settings(model.parameters);
            self.unit
                .borrow()
                .parameter_manager()
                .update_compartment_params(compartment, compartment_params.clone());
            self.unit
                .borrow_mut()
                .set_custom_compartment_data(compartment, model.custom_data);
            self.compartment_common_lua[compartment] = model.common_lua;
            self.compartment_notes[compartment] = model.notes;
        } else {
            self.clear_compartment_data(compartment);
        }
        self.reset_parameters(compartment);
        self.notify_everything_has_changed();
        self.notify_compartment_loaded(compartment);
    }

    pub fn notify_compartment_loaded(&self, compartment: CompartmentKind) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::NotifyRealearnCompartmentLoaded(compartment));
    }

    fn reset_parameters(&self, compartment: CompartmentKind) {
        let fx = self.processor_context.containing_fx().clone();
        let _ = Global::task_support().do_later_in_main_thread_from_main_thread_asap(move || {
            for i in convert_plugin_param_index_range_to_iter(&compartment.plugin_param_range()) {
                let _ = fx
                    .parameter_by_index(i.get())
                    .set_reaper_normalized_value(0.0);
            }
        });
    }

    fn clear_compartment_data(&mut self, compartment: CompartmentKind) {
        self.default_group(compartment)
            .replace(GroupModel::default_for_compartment(compartment));
        self.set_groups_without_notification(compartment, std::iter::empty());
        self.set_mappings_without_notification(compartment, std::iter::empty());
        self.params.compartment_params_mut(compartment).reset_all();
        self.unit
            .borrow()
            .parameter_manager()
            .update_compartment_params(compartment, Default::default());
        self.unit
            .borrow_mut()
            .set_custom_compartment_data(compartment, Default::default());
        self.compartment_common_lua[compartment] = Default::default();
        self.compartment_notes[compartment] = Default::default();
    }

    pub fn update_certain_param_settings(
        &mut self,
        compartment: CompartmentKind,
        settings: Vec<(CompartmentParamIndex, ParamSetting)>,
    ) {
        let compartment_params = self.params.compartment_params_mut(compartment);
        compartment_params.apply_given_settings(settings);
        self.unit
            .borrow()
            .parameter_manager()
            .update_compartment_params(compartment, compartment_params.clone());
        // We don't need to notify the UI because it will be done once the param container has
        // propagated the changes to the session again via event (uni-directional dataflow).
        self.mark_compartment_dirty(compartment);
    }

    /// Fires if everything has changed. Supposed to be used by UI, should rerender everything.
    ///
    /// The session itself shouldn't subscribe to this.
    pub fn everything_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.everything_changed_subject.clone()
    }

    /// Fires when a mapping has been added, removed or changed its position in the list.
    ///
    /// Doesn't fire if a mapping in the list or if the complete list has changed.
    pub fn mapping_list_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (CompartmentKind, Option<MappingId>), Err = ()> + 'static
    {
        self.mapping_list_changed_subject.clone()
    }

    /// Fires when a group has been added or removed.
    ///
    /// Doesn't fire if a group in the list or if the complete list has changed.
    pub fn group_list_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = CompartmentKind, Err = ()> + 'static {
        self.group_list_changed_subject.clone()
    }

    pub fn params(&self) -> &PluginParams {
        &self.params
    }

    pub fn set_mappings_without_notification(
        &mut self,
        compartment: CompartmentKind,
        mappings: impl IntoIterator<Item = MappingModel>,
    ) {
        self.mappings[compartment] = mappings.into_iter().map(share_mapping).collect();
    }

    pub fn set_groups_without_notification(
        &mut self,
        compartment: CompartmentKind,
        groups: impl IntoIterator<Item = GroupModel>,
    ) {
        self.groups[compartment] = groups.into_iter().map(share_group).collect();
    }

    fn add_mapping(
        &mut self,
        compartment: CompartmentKind,
        mapping: MappingModel,
    ) -> SharedMapping {
        let mapping_id = mapping.id();
        let shared_mapping = share_mapping(mapping);
        self.mappings[compartment].push(shared_mapping.clone());
        self.notify_mapping_list_changed(compartment, Some(mapping_id));
        shared_mapping
    }

    pub fn send_all_feedback(&self) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::SendAllFeedback);
    }

    pub fn log_debug_info(&self) {
        self.log_debug_info_internal();
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::LogDebugInfo);
    }

    pub fn log_mapping(
        &self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
    ) -> Result<(), &'static str> {
        let mapping = self
            .find_mapping_by_id(compartment, mapping_id)
            .ok_or("mapping not found")?;
        debug!(
            "MappingModel struct size: {}",
            std::mem::size_of::<MappingModel>()
        );
        debug!("{:?}", mapping);
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::LogMapping(compartment, mapping_id));
        Ok(())
    }

    pub fn mapping_is_on(&self, id: QualifiedMappingId) -> bool {
        self.unit.borrow().mapping_is_on(id)
    }

    fn log_debug_info_internal(&self) {
        // Summary
        let msg = format!(
            "\n\
            # Session\n\
            \n\
            - Instance ID (random): {}\n\
            - ID (persistent, maybe custom): {}\n\
            - Main mapping count: {}\n\
            - Main mapping subscription count: {}\n\
            - Group count: {}\n\
            - Group subscription count: {}\n\
            - Controller mapping model count: {}\n\
            - Controller mapping subscription count: {}\n\
            ",
            self.unit_id,
            &self.unit_key,
            self.mappings[CompartmentKind::Main].len(),
            self.mapping_subscriptions[CompartmentKind::Main].len(),
            self.groups.len(),
            self.group_subscriptions.len(),
            self.mappings[CompartmentKind::Controller].len(),
            self.mapping_subscriptions[CompartmentKind::Controller].len(),
        );
        Reaper::get().show_console_msg(msg);
        // Detailled
        trace!(
            "\n\
            # Session\n\
            \n\
            {:#?}
            ",
            self
        );
    }

    pub fn find_mapping_with_target(
        &self,
        compartment: CompartmentKind,
        target: &ReaperTarget,
    ) -> Option<&SharedMapping> {
        self.mappings(compartment).find(|m| {
            m.borrow()
                .with_context(self.extended_context())
                .has_target(target)
        })
    }

    pub fn toggle_learn_source_for_target(
        &mut self,
        session: &SharedUnitModel,
        compartment: CompartmentKind,
        target: &ReaperTarget,
        toggle: bool,
    ) -> SharedMapping {
        let mapping = match self.find_mapping_with_target(compartment, target) {
            None => {
                let m = self.add_default_mapping(
                    compartment,
                    GroupId::default(),
                    VirtualControlElementCharacter::Multi,
                );
                {
                    let mut mapping = m.borrow_mut();
                    self.change_target_with_closure(
                        &mut mapping,
                        None,
                        Rc::downgrade(session),
                        |ctx| {
                            if toggle {
                                ctx.mapping.mode_model.change(ModeCommand::SetAbsoluteMode(
                                    AbsoluteMode::ToggleButton,
                                ));
                            }
                            ctx.mapping.target_model.apply_from_target(
                                target,
                                ctx.extended_context,
                                compartment,
                            )
                        },
                    );
                }
                m
            }
            Some(m) => m.clone(),
        };
        self.toggle_learning_source(Rc::downgrade(session), mapping.borrow().qualified_id())
            .expect("error during toggle learn-source for target");
        mapping
    }

    pub fn remove_mapping_by_target(
        &mut self,
        compartment: CompartmentKind,
        target: &ReaperTarget,
    ) {
        if let Some(mapping) = self.find_mapping_with_target(compartment, target) {
            let id = mapping.borrow().qualified_id();
            self.remove_mapping_internal(id);
        }
    }

    pub fn show_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        self.ui().show_mapping(compartment, mapping_id);
    }

    /// Makes the main processor send feedback to the given sender instead of the configured
    /// feedback output.
    ///
    /// Good for checking produced feedback when doing integration testing.
    pub fn use_integration_test_feedback_sender(
        &self,
        sender: SenderToNormalThread<FinalSourceFeedbackValue>,
    ) {
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::UseIntegrationTestFeedbackSender(sender));
    }

    /// Notifies listeners async that something in a mapping list has changed.
    ///
    /// Shouldn't be used if the complete list has changed.
    fn notify_mapping_list_changed(
        &mut self,
        compartment: CompartmentKind,
        new_mapping_id: Option<MappingId>,
    ) {
        AsyncNotifier::notify(
            &mut self.mapping_list_changed_subject,
            &(compartment, new_mapping_id),
        );
    }

    /// Notifies listeners async that something in a group list has changed.
    ///
    /// Shouldn't be used if the complete list has changed.
    fn notify_group_list_changed(&mut self, compartment: CompartmentKind) {
        AsyncNotifier::notify(&mut self.group_list_changed_subject, &compartment);
    }

    fn sync_upper_floor_membership(&self) {
        let backbone_state = Backbone::get();
        if self.lives_on_upper_floor.get() {
            backbone_state.make_superior(self.unit_id);
        } else {
            backbone_state.make_inferior(&self.unit_id);
        }
    }

    pub fn control_input(&self) -> ControlInput {
        self.control_input.get()
    }

    pub fn feedback_output(&self) -> Option<FeedbackOutput> {
        self.feedback_output.get()
    }

    pub fn unit(&self) -> &SharedUnit {
        &self.unit
    }

    fn sync_compartment_settings(&self, compartment: CompartmentKind) {
        let common_lua_code = &self.compartment_common_lua[compartment];
        let common_lua_value = if common_lua_code.trim().is_empty() {
            None
        } else {
            match compile_common_lua(compartment, common_lua_code) {
                Ok(v) => Some(v),
                Err(e) => {
                    notification::warn_user_about_anyhow_error(e);
                    None
                }
            }
        };
        let settings = CompartmentSettings {
            common_lua: common_lua_value,
        };
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::UpdateCompartmentSettings(
                compartment,
                settings,
            ));
    }

    fn sync_settings(&self) {
        let settings = BasicSettings {
            control_input: self.control_input(),
            wants_keyboard_input: self.wants_keyboard_input,
            streamdeck_device_id: self.stream_deck_device_id,
            feedback_output: self.feedback_output(),
            real_input_logging_enabled: self.real_input_logging_enabled.get(),
            real_output_logging_enabled: self.real_output_logging_enabled.get(),
            virtual_input_logging_enabled: self.virtual_input_logging_enabled.get(),
            virtual_output_logging_enabled: self.virtual_output_logging_enabled.get(),
            target_control_logging_enabled: self.target_control_logging_enabled.get(),
            send_feedback_only_if_armed: self.send_feedback_only_if_armed.get(),
            reset_feedback_when_releasing_source: self.reset_feedback_when_releasing_source.get(),
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
            stay_active_when_project_in_background: self
                .stay_active_when_project_in_background
                .get(),
        };
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::UpdateSettings(settings));
    }

    fn sync_persistent_mapping_processing_state(&self, mapping: &MappingModel) {
        self.normal_main_task_sender.send_complaining(
            NormalMainTask::UpdatePersistentMappingProcessingState {
                id: mapping.qualified_id(),
                state: mapping.create_persistent_mapping_processing_state(),
            },
        );
    }

    fn sync_single_mapping_to_processors(&self, m: &MappingModel) {
        self.instance
            .borrow()
            .notify_mappings_in_unit_changed(self.unit_id);
        let group_data = self
            .find_group_of_mapping(m)
            .map(|g| g.borrow().create_data())
            .unwrap_or_default();
        let main_mapping = m.create_main_mapping(group_data);
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::UpdateSingleMapping(Box::new(main_mapping)));
    }

    pub fn instance(&self) -> &SharedInstance {
        &self.instance
    }

    fn find_group_of_mapping(&self, mapping: &MappingModel) -> Option<&SharedGroup> {
        let group_id = mapping.group_id();
        if group_id.is_default() {
            let group = match mapping.compartment() {
                CompartmentKind::Controller => &self.default_controller_group,
                CompartmentKind::Main => &self.default_main_group,
            };
            Some(group)
        } else {
            self.find_group_by_id(mapping.compartment(), group_id)
        }
    }

    /// Does a full mapping sync.
    fn sync_all_mappings_full(&self, compartment: CompartmentKind) {
        tracing::debug!(msg = "Sync all mappings full", %compartment);
        self.instance
            .borrow()
            .notify_mappings_in_unit_changed(self.unit_id);
        let main_mappings = self.create_main_mappings(compartment);
        self.normal_main_task_sender
            .send_complaining(NormalMainTask::UpdateAllMappings(
                compartment,
                main_mappings,
            ));
    }

    /// Creates mappings from mapping models so they can be distributed to different processors.
    fn create_main_mappings(&self, compartment: CompartmentKind) -> Vec<MainMapping> {
        let group_map: NonCryptoHashMap<GroupId, Ref<GroupModel>> = self
            .groups_including_default_group(compartment)
            .map(|group| {
                let group = group.borrow();
                (group.id(), group)
            })
            .collect();
        // TODO-medium This is non-optimal if we have a group that uses an EEL activation condition
        //  and has many mappings. Because of our strategy of groups being an application-layer
        //  concept only, we equip *all* n mappings in that group with the group activation
        //  condition. The EEL compilation is done n times, but maybe worse: There are n EEL VMs
        //  in the domain layer and all of them have to run on parameter changes - whereas 1 would
        //  be enough if the domain layer would know about groups.
        self.mappings(compartment)
            .map(|mapping| {
                let mapping = mapping.borrow();
                let group_data = group_map
                    .get(&mapping.group_id())
                    .map(|g| g.create_data())
                    .unwrap_or_default();
                mapping.create_main_mapping(group_data)
            })
            .collect()
    }

    fn generate_name_for_new_mapping(&self, compartment: CompartmentKind) -> String {
        format!("{}", self.mappings[compartment].len() + 1)
    }

    fn party_is_over(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.party_is_over_subject.clone()
    }

    /// Shouldn't be called on load (project load, undo, redo, preset change).
    pub fn mark_compartment_dirty(&mut self, compartment: CompartmentKind) {
        debug!("Marking compartment as dirty");
        self.compartment_is_dirty[compartment].set(true);
        self.mark_dirty();
    }

    /// Shouldn't be called on load (project load, undo, redo, preset change).
    pub fn mark_dirty(&self) {
        debug!("Marking unit as dirty");
        self.processor_context.notify_dirty();
    }

    /// Does a full resync and notifies the UI async.
    ///
    /// Explicitly doesn't mark the project as dirty - because this is also used when loading data
    /// (project load, undo, redo, preset change).
    pub fn notify_everything_has_changed(&mut self) {
        if let Some(ui) = self.ui.get() {
            ui.handle_everything_changed(self);
        }
        self.full_sync();
        // For UI
        AsyncNotifier::notify(&mut self.everything_changed_subject, &());
    }
}

impl Drop for UnitModel {
    fn drop(&mut self) {
        debug!("Dropping UnitModel...");
        self.party_is_over_subject.next(());
    }
}

impl Display for UnitModel {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let fx_pos = self.processor_context.containing_fx().index() + 1;
        let fx_name = self.processor_context.containing_fx().name();
        let session_id = self.unit_key();
        if let Some(track) = self.processor_context.track() {
            if track.is_master_track() {
                f.write_str(MASTER_TRACK_LABEL)?;
            } else {
                let track_label = get_track_label(track);
                let chain = if self.processor_context.containing_fx().chain().is_input_fx() {
                    "Input FX"
                } else {
                    "Normal FX"
                };
                write!(f, "Track \"{track_label}\" / {chain}")?;
            }
        } else {
            write!(f, "Monitoring FX chain")?;
        };
        write!(f, " / \"{fx_pos}. {fx_name}\" (ID \"{session_id}\")")?;
        Ok(())
    }
}

impl DomainEventHandler for WeakUnitModel {
    fn handle_event(&self, event: DomainEvent) -> Result<(), Box<dyn Error>> {
        let session = self.upgrade().ok_or("session not existing anymore")?;
        use DomainEvent::*;
        match event {
            Info(evt) => {
                let s = session.try_borrow()?;
                s.ui().handle_internal_info_event(evt);
            }
            ConditionsChanged => {
                let s = session.try_borrow()?;
                s.ui().conditions_changed()
            }
            TimeForCelebratingSuccess => {
                let s = session.try_borrow()?;
                s.ui().celebrate_success()
            }
            CapturedIncomingMessage(event) => {
                session.borrow_mut().captured_incoming_message(event);
            }
            UpdatedOnMappings(on_mappings) => {
                session
                    .try_borrow()?
                    .unit
                    .borrow_mut()
                    .set_on_mappings(on_mappings);
            }
            GlobalControlAndFeedbackStateChanged(state) => {
                let session = session.borrow();
                session
                    .unit
                    .borrow_mut()
                    .set_global_control_and_feedback_state(state);
                session
                    .ui()
                    .handle_global_control_and_feedback_state_changed();
            }
            UpdatedSingleMappingOnState(event) => {
                session
                    .borrow()
                    .unit
                    .borrow_mut()
                    .set_mapping_on(event.id, event.is_on);
            }
            TargetValueChanged(e) => {
                // If the session is borrowed already, just let it be. It happens only in a very
                // particular case of reentrancy (because of a quirk in REAPER related to master
                // tempo notification, https://github.com/helgoboss/helgobox/issues/199). If the
                // target value slider is not updated then ... so what.
                session.try_borrow()?.ui().target_value_changed(e);
            }
            UpdatedSingleParameterValue { index, value } => {
                let mut session = session.borrow_mut();
                session.params.at_mut(index).set_raw_value(value);
                session.ui().parameters_changed(&session);
            }
            UpdatedAllParameters(params) => {
                let mut session = session.borrow_mut();
                session.params = params;
                session.ui().parameters_changed(&session);
            }
            FullResyncRequested => {
                debug!("FullResyncRequested received");
                session.borrow_mut().full_sync();
            }
            MidiDevicesChanged => {
                session.try_borrow()?.ui().midi_devices_changed();
            }
            ProjectionFeedback(value) => {
                let s = session.try_borrow()?;
                s.ui().send_projection_feedback(&s, value);
            }
            MappingMatched(event) => {
                let s = session.try_borrow()?;
                s.ui().mapping_matched(event);
            }
            HandleTargetControl(event) => {
                let s = session.try_borrow()?;
                s.ui().handle_target_control(event);
            }
            HandleSourceFeedback(event) => {
                let s = session.try_borrow()?;
                s.ui().handle_source_feedback(event);
            }
            MappingEnabledChangeRequested(event) => {
                let mut s = session.try_borrow_mut()?;
                let id = QualifiedMappingId::new(event.compartment, event.mapping_id);
                s.change_mapping_from_session(
                    id,
                    MappingCommand::SetIsEnabled(event.is_enabled),
                    self.clone(),
                );
            }
            MappingModificationRequested(event) => {
                let mut s = session.try_borrow_mut()?;
                let id = QualifiedMappingId::new(event.compartment, event.mapping_id);
                match event.modification {
                    MappingModification::LearnTarget(m) => {
                        if event.value.is_on() {
                            let included_targets = m
                                .included_targets
                                .map(ReaperTargetType::from_learnable_target_kinds)
                                .unwrap_or_else(ReaperTargetType::all);
                            let filter = (included_targets, m.touch_cause.unwrap_or_default());
                            s.start_learning_target_internal(self.clone(), id, false, filter);
                        } else {
                            s.stop_learning_target();
                        }
                    }
                    MappingModification::SetTargetToLastTouched(m) => {
                        let included_targets = m
                            .included_targets
                            .map(ReaperTargetType::from_learnable_target_kinds)
                            .unwrap_or_else(ReaperTargetType::all);
                        let filter = LastTouchedTargetFilter {
                            included_target_types: &included_targets,
                            touch_cause: m.touch_cause.unwrap_or_default(),
                        };
                        let Some(target) = Backbone::get().find_last_touched_target(filter) else {
                            return Ok(());
                        };
                        let Some(m) = s.find_mapping_by_qualified_id(id).cloned() else {
                            return Ok(());
                        };
                        let mut m = m.borrow_mut();
                        s.change_target_with_closure(&mut m, None, self.clone(), |ctx| {
                            ctx.mapping.target_model.apply_from_target(
                                &target,
                                ctx.extended_context,
                                ctx.mapping.compartment(),
                            )
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn auto_load_different_preset_if_necessary(&self) -> anyhow::Result<bool> {
        let unit_model = self.upgrade().context("unit model not existing anymore")?;
        let unit_fx = {
            let unit_model = unit_model
                .try_borrow_mut()
                .context("unit model already borrowed")?;
            if unit_model.auto_load_mode.get() != AutoLoadMode::UnitFx {
                return Ok(false);
            }
            let unit = unit_model.unit.borrow();
            let unit_fx_descriptor = unit.instance_fx_descriptor();
            let unit_fx = unit_fx_descriptor
                .resolve(unit_model.extended_context(), CompartmentKind::Main)
                .unwrap_or_default()
                .into_iter()
                .next();
            unit_fx.filter(|unit_fx| {
                if matches!(&unit_fx_descriptor.fx, VirtualFx::LastFocused) {
                    // Unit FX refers to the last-focused FX. We need to make a few more checks.
                    unit_fx.window_is_open() && unit_fx.window_has_focus()
                } else {
                    // Unit FX is something else than the last-focused FX. Load linked preset!
                    true
                }
            })
        };
        let fx_id = unit_fx.and_then(|fx| FxId::from_fx(&fx, false).ok());
        let loaded = unit_model.borrow_mut().auto_load_preset_linked_to_fx(fx_id);
        Ok(loaded)
    }
}

/// Never store the strong reference to a session (except in the main owner RealearnPlugin)!
///
/// # Design
///
/// ## Why `Rc<RefCell<Session>>`?
///
/// `Plugin#get_editor()` must return a Box of something 'static, so it's impossible to take a
/// reference here. Why? Because a reference needs a lifetime. Any non-static lifetime would
/// not satisfy the 'static requirement. Why not require a 'static reference then? Simply
/// because we don't have a session object with static lifetime. The session object is
/// owned by the `Plugin` object, which itself doesn't have a static lifetime. The only way
/// to get a 'static session would be to not let the plugin object own the session but to
/// define a static global. This, however, would be a far worse design than just using a
/// smart pointer here. So using a smart pointer is the best we can do really.
///
/// This is not the only reason why taking a reference here is not feasible. During the
/// lifecycle of a ReaLearn session we need mutable access to the session both from the
/// editor (of course) and from the plugin (e.g. when REAPER wants us to load some data).
/// When using references, Rust's borrow checker wouldn't let that happen. We can't do anything
/// about this multiple-access requirement, it's just how the VST plugin API works (and
/// many other similar plugin interfaces as well - for good reasons). And being a plugin we
/// have to conform.
///
/// Fortunately, we know that actually both DAW-plugin interaction (such as loading data) and
/// UI interaction happens in the main thread, in the so called main loop. So there's no
/// need for using a thread-safe smart pointer here. We even can and also should satisfy
/// the borrow checker, meaning that if the session is mutably accessed at a given point in
/// time, it is not accessed from another point as well. This can happen even in a
/// single-threaded environment because functions can call other functions and thereby
/// accessing the same data - just in different stack positions. Just think of reentrancy.
/// Fortunately this is something we can control. And we should, because when this kind of
/// parallel access happens, this can lead to strange bugs which are particularly hard to
/// find.
///
/// Unfortunately we can't make use of Rust's compile time borrow checker because there's no
/// way that the compiler understands what's going on here. Why? For one thing, because of
/// the VST plugin API design. But first and foremost because we use the FFI, which means
/// we interface with non-Rust code, so Rust couldn't get the complete picture even if the
/// plugin system would be designed in a different way. However, we *can* use Rust's
/// runtime borrow checker `RefCell`. And we should, because it gives us fail-fast
/// behavior. It will let us know immediately when we violated that safety rule.
/// TODO-low We must take care, however, that REAPER will not crash as a result, that would be
/// very  bad.  See https://github.com/RustAudio/vst-rs/issues/122
pub type SharedUnitModel = Rc<RefCell<UnitModel>>;

/// Always use this when storing a reference to a session. This avoids memory leaks and ghost
/// sessions.
pub type WeakUnitModel = Weak<RefCell<UnitModel>>;

fn mappings_have_project_references<'a>(
    mut mappings: impl Iterator<Item = &'a SharedMapping>,
) -> bool {
    mappings.any(mapping_has_project_references)
}

/// Checks if the given mapping has references to a project, e.g. refers to track or FX by ID.
fn mapping_has_project_references(mapping: &SharedMapping) -> bool {
    let mapping = mapping.borrow();
    let target = &mapping.target_model;
    match target.category() {
        TargetCategory::Reaper => {
            if target.target_type().supports_track() && target.track_type().refers_to_project() {
                return true;
            }
            target.supports_fx() && target.fx_type().refers_to_project()
        }
        TargetCategory::Virtual => false,
    }
}

pub fn reaper_supports_global_midi_filter() -> bool {
    Reaper::get().version().revision() >= "6.35+dev0831"
}

#[allow(dead_code)]
pub enum SessionCommand {
    SetUnitName(Option<String>),
    SetUnitKey(String),
    SetInstanceTrack(TrackDescriptor),
    SetInstanceFx(FxDescriptor),
    SetWantsKeyboardInput(bool),
    SetStreamDeckDevice(Option<StreamDeckDeviceId>),
    ChangeCompartment(CompartmentKind, CompartmentCommand),
    AdjustMappingModeIfNecessary(QualifiedMappingId),
}

pub enum SessionProp {
    UnitName,
    UnitKey,
    InstanceTrack,
    InstanceFx,
    WantsKeyboardInput,
    StreamDeckDeviceId,
    InCompartment(CompartmentKind, Affected<CompartmentProp>),
}

#[derive(Copy, Clone)]
pub struct CompartmentInSession<'a> {
    pub session: &'a UnitModel,
    pub compartment: CompartmentKind,
}

impl<'a> CompartmentInSession<'a> {
    pub fn new(session: &'a UnitModel, compartment: CompartmentKind) -> Self {
        Self {
            session,
            compartment,
        }
    }
}

pub struct MappingChangeContext<'a> {
    pub mapping: &'a mut MappingModel,
    pub extended_context: ExtendedProcessorContext<'a>,
}

#[derive(Debug)]
pub struct RealearnControlSurfaceMainTaskSender(
    pub SenderToNormalThread<RealearnControlSurfaceMainTask<WeakUnitModel>>,
);

impl RealearnControlSurfaceMainTaskSender {
    pub fn capture_targets(
        &self,
        instance_id: Option<UnitId>,
    ) -> async_channel::Receiver<TargetTouchEvent> {
        let (sender, receiver) = async_channel::bounded(500);
        self.0
            .send_complaining(RealearnControlSurfaceMainTask::StartCapturingTargets(
                instance_id,
                sender,
            ));
        receiver
    }

    pub fn stop_capturing_targets(&self, instance_id: Option<UnitId>) {
        self.0
            .send_complaining(RealearnControlSurfaceMainTask::StopCapturingTargets(
                instance_id,
            ));
    }
}

const SESSION_GONE: &str = "session gone";

fn compile_common_lua(compartment: CompartmentKind, code: &str) -> anyhow::Result<mlua::Value> {
    let lua = unsafe { Backbone::main_thread_lua() };
    let env = lua.create_fresh_environment(false)?;
    let require = lua.as_ref().create_function(move |lua, path: String| {
        let val = match lua_module_path_without_ext(&path) {
            LUA_MIDI_SCRIPT_SOURCE_RUNTIME_NAME => create_lua_midi_script_source_runtime(lua),
            LUA_FEEDBACK_SCRIPT_RUNTIME_NAME => create_lua_feedback_script_runtime(lua),
            _ => return Err(mlua::Error::runtime("Common compartment Lua scripts don't support the usage of 'require' for anything else than runtimes (at the moment)!"))
        };
        Ok(val)
    })?;
    env.set("require", require)?;
    lua.compile_and_execute(compartment.to_string(), code, env)
}

struct PresetLoadInstruction {
    preset_id: Option<String>,
    compartment_model: Option<CompartmentModel>,
}

impl PresetLoadInstruction {
    pub fn new(preset_id: Option<String>, compartment_model: Option<CompartmentModel>) -> Self {
        Self {
            preset_id,
            compartment_model,
        }
    }
}

pub struct SourceValueVirtualization {
    pub virtual_source_value: VirtualSourceValue,
    pub learnable: bool,
}

pub struct FindMappingOutcome {
    pub mapping: SharedMapping,
    pub source_is_learnable: bool,
}

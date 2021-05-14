use crate::application::{
    share_group, share_mapping, ControllerPreset, FxId, GroupId, GroupModel, MainPreset,
    MainPresetAutoLoadMode, MappingModel, Preset, PresetLinkManager, PresetManager, SharedGroup,
    SharedMapping, TargetCategory, TargetModel, VirtualControlElementType,
};
use crate::base::default_util::is_default;
use crate::base::{prop, when, AsyncNotifier, Global, Prop};
use crate::domain::{
    BackboneState, CompoundMappingSource, ControlInput, DomainEvent, DomainEventHandler,
    ExtendedProcessorContext, FeedbackOutput, InstanceId, MainMapping, MappingCompartment,
    MappingId, MidiControlInput, MidiDestination, NormalMainTask, NormalRealTimeTask, OscDeviceId,
    ParameterArray, ProcessorContext, ProjectionFeedbackValue, QualifiedMappingId, RealSource,
    RealTimeSender, RealearnTarget, ReaperTarget, SharedInstanceState, TargetValueChangedEvent,
    VirtualControlElementId, VirtualSource, COMPARTMENT_PARAMETER_COUNT, ZEROED_PLUGIN_PARAMETERS,
};
use derivative::Derivative;
use enum_map::{enum_map, EnumMap};
use serde::{Deserialize, Serialize};

use reaper_high::Reaper;
use rx_util::Notifier;
use rxrust::prelude::ops::box_it::LocalBoxOp;
use rxrust::prelude::*;
use slog::debug;
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use helgoboss_midi::Channel;
use itertools::Itertools;
use reaper_medium::{MidiInputDeviceId, RecordingInput};
use std::rc::{Rc, Weak};

pub trait SessionUi {
    fn show_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId);
    fn target_value_changed(&self, event: TargetValueChangedEvent);
    fn parameters_changed(&self, session: &Session);
    fn send_projection_feedback(&self, session: &Session, value: ProjectionFeedbackValue);
}

/// This represents the user session with one ReaLearn instance.
///
/// It's ReaLearn's main object which keeps everything together.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct Session {
    instance_id: InstanceId,
    /// Initially corresponds to instance ID but is persisted and can be user-customized. Should be
    /// unique but if not it's not a big deal, then it won't crash but the user can't be sure which
    /// session will be picked. Most relevant for HTTP/WS API.
    pub id: Prop<String>,
    logger: slog::Logger,
    pub let_matched_events_through: Prop<bool>,
    pub let_unmatched_events_through: Prop<bool>,
    pub auto_correct_settings: Prop<bool>,
    pub send_feedback_only_if_armed: Prop<bool>,
    pub midi_control_input: Prop<MidiControlInput>,
    pub midi_feedback_output: Prop<Option<MidiDestination>>,
    pub osc_input_device_id: Prop<Option<OscDeviceId>>,
    pub osc_output_device_id: Prop<Option<OscDeviceId>>,
    pub main_preset_auto_load_mode: Prop<MainPresetAutoLoadMode>,
    pub lives_on_upper_floor: Prop<bool>,
    // Is set when in the state of learning multiple mappings ("batch learn")
    learn_many_state: Prop<Option<LearnManyState>>,
    // We want that learn works independently of the UI, so they are session properties.
    mapping_which_learns_source: Prop<Option<QualifiedMappingId>>,
    mapping_which_learns_target: Prop<Option<QualifiedMappingId>>,
    active_controller_preset_id: Option<String>,
    active_main_preset_id: Option<String>,
    context: ProcessorContext,
    mappings: EnumMap<MappingCompartment, Vec<SharedMapping>>,
    default_main_group: SharedGroup,
    default_controller_group: SharedGroup,
    groups: EnumMap<MappingCompartment, Vec<SharedGroup>>,
    everything_changed_subject: LocalSubject<'static, (), ()>,
    mapping_list_changed_subject:
        LocalSubject<'static, (MappingCompartment, Option<MappingId>), ()>,
    group_list_changed_subject: LocalSubject<'static, MappingCompartment, ()>,
    parameter_settings_changed_subject: LocalSubject<'static, MappingCompartment, ()>,
    mapping_changed_subject: LocalSubject<'static, MappingCompartment, ()>,
    group_changed_subject: LocalSubject<'static, MappingCompartment, ()>,
    source_touched_subject: LocalSubject<'static, CompoundMappingSource, ()>,
    mapping_subscriptions: EnumMap<MappingCompartment, Vec<SubscriptionGuard<LocalSubscription>>>,
    group_subscriptions: EnumMap<MappingCompartment, Vec<SubscriptionGuard<LocalSubscription>>>,
    normal_main_task_sender: crossbeam_channel::Sender<NormalMainTask>,
    normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    #[derivative(Debug = "ignore")]
    ui: Box<dyn SessionUi>,
    parameters: ParameterArray,
    parameter_settings: EnumMap<MappingCompartment, Vec<ParameterSetting>>,
    controller_preset_manager: Box<dyn PresetManager<PresetType = ControllerPreset>>,
    main_preset_manager: Box<dyn PresetManager<PresetType = MainPreset>>,
    main_preset_link_manager: Box<dyn PresetLinkManager>,
    /// The mappings which are on (control or feedback enabled + mapping active + target active)
    on_mappings: Prop<HashSet<MappingId>>,
    instance_state: SharedInstanceState,
}

#[derive(Clone, PartialEq, Debug)]
pub struct LearnManyState {
    pub compartment: MappingCompartment,
    pub current_mapping_id: MappingId,
    pub sub_state: LearnManySubState,
}

#[derive(Clone, PartialEq, Debug)]
pub enum LearnManySubState {
    LearningSource {
        // Only relevant in controller compartment
        control_element_type: VirtualControlElementType,
    },
    LearningTarget,
}

impl LearnManyState {
    pub fn learning_source(
        compartment: MappingCompartment,
        current_mapping_id: MappingId,
        control_element_type: VirtualControlElementType,
    ) -> LearnManyState {
        LearnManyState {
            compartment,
            current_mapping_id,
            sub_state: LearnManySubState::LearningSource {
                control_element_type,
            },
        }
    }

    pub fn learning_target(
        compartment: MappingCompartment,
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
    use crate::application::MainPresetAutoLoadMode;

    pub const LET_MATCHED_EVENTS_THROUGH: bool = false;
    pub const LET_UNMATCHED_EVENTS_THROUGH: bool = true;
    pub const AUTO_CORRECT_SETTINGS: bool = true;
    pub const LIVES_ON_UPPER_FLOOR: bool = false;
    pub const SEND_FEEDBACK_ONLY_IF_ARMED: bool = true;
    pub const MAIN_PRESET_AUTO_LOAD_MODE: MainPresetAutoLoadMode = MainPresetAutoLoadMode::Off;
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: InstanceId,
        parent_logger: &slog::Logger,
        context: ProcessorContext,
        normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
        normal_main_task_sender: crossbeam_channel::Sender<NormalMainTask>,
        ui: impl SessionUi + 'static,
        controller_manager: impl PresetManager<PresetType = ControllerPreset> + 'static,
        main_preset_manager: impl PresetManager<PresetType = MainPreset> + 'static,
        preset_link_manager: impl PresetLinkManager + 'static,
        instance_state: SharedInstanceState,
    ) -> Session {
        Self {
            // As long not changed (by loading a preset or manually changing session ID), the
            // session ID is equal to the instance ID.
            id: prop(instance_id.to_string()),
            instance_id,
            logger: parent_logger.clone(),
            let_matched_events_through: prop(session_defaults::LET_MATCHED_EVENTS_THROUGH),
            let_unmatched_events_through: prop(session_defaults::LET_UNMATCHED_EVENTS_THROUGH),
            auto_correct_settings: prop(session_defaults::AUTO_CORRECT_SETTINGS),
            send_feedback_only_if_armed: prop(session_defaults::SEND_FEEDBACK_ONLY_IF_ARMED),
            midi_control_input: prop(MidiControlInput::FxInput),
            midi_feedback_output: prop(None),
            osc_input_device_id: prop(None),
            osc_output_device_id: prop(None),
            main_preset_auto_load_mode: prop(session_defaults::MAIN_PRESET_AUTO_LOAD_MODE),
            lives_on_upper_floor: prop(false),
            learn_many_state: prop(None),
            mapping_which_learns_source: prop(None),
            mapping_which_learns_target: prop(None),
            active_controller_preset_id: None,
            active_main_preset_id: None,
            context,
            mappings: Default::default(),
            default_main_group: Rc::new(RefCell::new(GroupModel::default_for_compartment(
                MappingCompartment::MainMappings,
            ))),
            default_controller_group: Rc::new(RefCell::new(GroupModel::default_for_compartment(
                MappingCompartment::ControllerMappings,
            ))),
            groups: Default::default(),
            everything_changed_subject: Default::default(),
            mapping_list_changed_subject: Default::default(),
            group_list_changed_subject: Default::default(),
            parameter_settings_changed_subject: Default::default(),
            mapping_changed_subject: Default::default(),
            group_changed_subject: Default::default(),
            source_touched_subject: Default::default(),
            mapping_subscriptions: Default::default(),
            group_subscriptions: Default::default(),
            normal_main_task_sender,
            normal_real_time_task_sender,
            party_is_over_subject: Default::default(),
            ui: Box::new(ui),
            parameters: ZEROED_PLUGIN_PARAMETERS,
            parameter_settings: enum_map! {
                MappingCompartment::ControllerMappings => vec![Default::default(); COMPARTMENT_PARAMETER_COUNT as usize],
                MappingCompartment::MainMappings => vec![Default::default(); COMPARTMENT_PARAMETER_COUNT as usize],
            },
            controller_preset_manager: Box::new(controller_manager),
            main_preset_manager: Box::new(main_preset_manager),
            main_preset_link_manager: Box::new(preset_link_manager),
            on_mappings: Default::default(),
            instance_state,
        }
    }

    pub fn id(&self) -> &str {
        self.id.get_ref()
    }

    pub fn receives_input_from(&self, input_descriptor: &InputDescriptor) -> bool {
        match input_descriptor {
            InputDescriptor::Midi { device_id, channel } => match self.midi_control_input.get() {
                MidiControlInput::FxInput => {
                    if let Some(track) = self.context().track() {
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
                MidiControlInput::Device(dev_id) => dev_id == *device_id,
            },
            InputDescriptor::Osc { device_id } => {
                self.osc_input_device_id.get_ref().as_ref() == Some(device_id)
            }
        }
    }

    pub fn find_mapping_with_source(
        &self,
        compartment: MappingCompartment,
        actual_real_source: &RealSource,
    ) -> Option<&SharedMapping> {
        let actual_virt_source = self.virtualize_if_possible(actual_real_source);
        use CompoundMappingSource::*;
        self.mappings(compartment).find(|m| {
            let m = m.borrow();
            if !self.on_mappings.get_ref().contains(&m.id()) {
                return false;
            }
            let mapping_source = m.source_model.create_source();
            match (mapping_source, actual_virt_source, actual_real_source) {
                (Virtual(map_source), Some(act_source), _) => map_source == act_source,
                (Midi(map_source), _, RealSource::Midi(act_source)) => map_source == *act_source,
                (Osc(map_source), _, RealSource::Osc(act_source)) => map_source == *act_source,
                _ => false,
            }
        })
    }

    pub fn get_parameter_settings(
        &self,
        compartment: MappingCompartment,
        index: u32,
    ) -> &ParameterSetting {
        &self.parameter_settings[compartment][index as usize]
    }

    pub fn non_default_parameter_settings_by_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> HashMap<u32, ParameterSetting> {
        self.parameter_settings[compartment]
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_default())
            .map(|(i, s)| (i as u32, s.clone()))
            .collect()
    }

    pub fn get_qualified_parameter_name(
        &self,
        compartment: MappingCompartment,
        rel_index: u32,
    ) -> String {
        let name = self.get_parameter_name(compartment, rel_index);
        let compartment_label = match compartment {
            MappingCompartment::ControllerMappings => "Ctrl",
            MappingCompartment::MainMappings => "Main",
        };
        format!("{} p{}: {}", compartment_label, rel_index + 1, name)
    }

    pub fn mappings_are_read_only(&self, compartment: MappingCompartment) -> bool {
        self.is_learning_many_mappings()
            || (compartment == MappingCompartment::MainMappings
                && self.main_preset_auto_load_is_active())
    }

    pub fn get_parameter_name(&self, compartment: MappingCompartment, rel_index: u32) -> String {
        let setting = &self.parameter_settings[compartment][rel_index as usize];
        if setting.name.is_empty() {
            format!("Param {}", rel_index + 1)
        } else {
            setting.name.clone()
        }
    }

    pub fn set_parameter_settings(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = (u32, ParameterSetting)>,
    ) {
        for (i, s) in settings {
            self.parameter_settings[compartment][i as usize] = s;
        }
        self.notify_parameter_settings_changed(compartment);
    }

    pub fn set_parameter_settings_without_notification(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: Vec<ParameterSetting>,
    ) {
        self.parameter_settings[compartment] = parameter_settings;
    }

    pub fn set_parameter_settings_from_non_default(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: &HashMap<u32, ParameterSetting>,
    ) {
        let mut settings = empty_parameter_settings();
        for (i, s) in parameter_settings {
            settings[*i as usize] = s.clone();
        }
        self.parameter_settings[compartment] = settings;
    }

    fn full_sync(&mut self, weak_session: WeakSession) {
        for compartment in MappingCompartment::enum_iter() {
            self.resubscribe_to_groups(weak_session.clone(), compartment);
        }
        // It's important to sync feedback device first, otherwise the initial feedback messages
        // won't arrive!
        self.sync_settings();
        self.sync_upper_floor_membership();
        self.sync_control_is_globally_enabled();
        self.sync_feedback_is_globally_enabled();
        // Now sync mappings - which includes initial feedback.
        for compartment in MappingCompartment::enum_iter() {
            self.resubscribe_to_mappings(compartment, weak_session.clone());
            self.sync_all_mappings_full(compartment);
        }
    }

    /// Connects the dots.
    // TODO-low Too large. Split this into several methods.
    pub fn activate(&mut self, weak_session: WeakSession) {
        // Initial sync
        self.full_sync(weak_session.clone());
        // Whenever auto-correct setting changes, resubscribe to all mappings because
        // that saves us some mapping subscriptions.
        when(self.auto_correct_settings.changed())
            .with(weak_session.clone())
            .do_async(|shared_session, _| {
                for compartment in MappingCompartment::enum_iter() {
                    shared_session
                        .borrow_mut()
                        .resubscribe_to_mappings(compartment, Rc::downgrade(&shared_session));
                }
            });
        // Whenever something in a specific mapping list changes, resubscribe to those mappings.
        when(self.mapping_list_changed())
            .with(weak_session.clone())
            .do_async(|shared_session, (compartment, _)| {
                shared_session
                    .borrow_mut()
                    .resubscribe_to_mappings(compartment, Rc::downgrade(&shared_session));
            });
        // Whenever something in the group list changes, resubscribe to those groups and sync
        // (because a mapping could have changed its group).
        when(self.group_list_changed())
            .with(weak_session.clone())
            .do_async(|shared_session, compartment| {
                let mut session = shared_session.borrow_mut();
                session.resubscribe_to_groups(Rc::downgrade(&shared_session), compartment);
                session.sync_all_mappings_full(compartment);
            });
        // Whenever anything in a mapping list changes and other things which affect all
        // processors (including the real-time processor which takes care of sources only), resync
        // all mappings to *all* processors.
        when(self.mapping_list_changed())
            .with(weak_session.clone())
            .do_async(move |session, (compartment, _)| {
                session.borrow_mut().sync_all_mappings_full(compartment);
            });
        // Whenever something changes that determines if feedback is enabled in general, let the
        // processors know.
        when(
            // There are several global conditions which affect whether feedback will be enabled in
            // general.
            self.midi_feedback_output
                .changed()
                .merge(self.osc_output_device_id.changed())
                .merge(self.containing_track_armed_or_disarmed())
                .merge(self.send_feedback_only_if_armed.changed())
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session.clone())
        .do_async(move |session, _| {
            session.borrow_mut().sync_feedback_is_globally_enabled();
        });
        // Whenever containing FX is disabled or enabled, we need to completely disable/enable
        // control/feedback.
        when(
            self.containing_fx_enabled_or_disabled()
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session.clone())
        .do_async(move |session, _| {
            let session = session.borrow_mut();
            session.sync_control_is_globally_enabled();
            session.sync_feedback_is_globally_enabled();
        });
        // Marking project as dirty if certain things are changed. Should only contain events that
        // are triggered by the user.
        when(
            self.settings_changed()
                .merge(self.mapping_list_changed().map_to(())),
        )
        .with(weak_session.clone())
        .do_sync(move |s, _| {
            s.borrow().mark_project_as_dirty();
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
        .with(weak_session.clone())
        .do_sync(|s, _| {
            s.borrow().invalidate_fx_indexes_of_mapping_targets();
        });
        // When FX focus changes, maybe trigger main preset change
        when(
            Global::control_surface_rx()
                // We need this event primarily to get informed of focus changes (because a
                // change in focus can happen without an FX to be opened or closed).
                .fx_focused()
                .map_to(())
                // For unloading preset when FX closed (we want that for now, it's clean!)
                .merge(Global::control_surface_rx().fx_closed().map_to(()))
                // For loading preset when FX opened (even if last focused FX is the same).
                .merge(Global::control_surface_rx().fx_opened().map_to(()))
                // When preset changed (for links that also have preset name as criteria)
                .merge(Global::control_surface_rx().fx_preset_changed().map_to(()))
                .take_until(self.party_is_over()),
        )
        .with(weak_session)
        // Doing this async is important to let REAPER digest the info about "Is the window open?"
        // and "What FX is focused?"
        .do_async(move |s, _| {
            if s.borrow().main_preset_auto_load_mode.get() == MainPresetAutoLoadMode::FocusedFx {
                let currently_focused_fx = if let Some(fx) = Reaper::get().focused_fx() {
                    if fx.window_is_open() {
                        Some(fx)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let fx_id = currently_focused_fx
                    .as_ref()
                    .and_then(|f| FxId::from_fx(f, true).ok());
                s.borrow_mut()
                    .auto_load_preset_linked_to_fx(fx_id, Rc::downgrade(&s));
            }
        });
    }

    pub fn activate_main_preset_auto_load_mode(
        &mut self,
        mode: MainPresetAutoLoadMode,
        session: WeakSession,
    ) {
        if mode != MainPresetAutoLoadMode::Off {
            self.activate_main_preset(None, session).unwrap();
        }
        self.main_preset_auto_load_mode.set(mode);
    }

    pub fn main_preset_auto_load_is_active(&self) -> bool {
        self.main_preset_auto_load_mode.get() != MainPresetAutoLoadMode::Off
    }

    fn auto_load_preset_linked_to_fx(&mut self, fx_id: Option<FxId>, weak_session: WeakSession) {
        let preset_id =
            fx_id.and_then(|id| self.main_preset_link_manager.find_preset_linked_to_fx(&id));
        if self.active_main_preset_id != preset_id {
            let _ = self.activate_main_preset(preset_id, weak_session);
        }
    }

    fn invalidate_fx_indexes_of_mapping_targets(&self) {
        for m in self.all_mappings() {
            let mut m = m.borrow_mut();
            let compartment = m.compartment();
            m.target_model
                .invalidate_fx_index(self.extended_context(), compartment);
        }
    }

    /// Settings are all the things displayed in the ReaLearn header panel.
    fn settings_changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.let_matched_events_through
            .changed()
            .merge(self.let_unmatched_events_through.changed())
            .merge(self.midi_control_input.changed())
            .merge(self.midi_feedback_output.changed())
            .merge(self.osc_input_device_id.changed())
            .merge(self.osc_output_device_id.changed())
            .merge(self.auto_correct_settings.changed())
            .merge(self.send_feedback_only_if_armed.changed())
            .merge(self.main_preset_auto_load_mode.changed())
    }

    pub fn learn_source(&mut self, source: RealSource, allow_virtual_sources: bool) {
        self.source_touched_subject
            .next(self.create_compound_source(source, allow_virtual_sources));
    }

    pub fn create_compound_source(
        &self,
        source: RealSource,
        allow_virtual_sources: bool,
    ) -> CompoundMappingSource {
        if allow_virtual_sources {
            if let Some(virt_source) = self.virtualize_if_possible(&source) {
                CompoundMappingSource::Virtual(virt_source)
            } else {
                source.into_compound_source()
            }
        } else {
            source.into_compound_source()
        }
    }

    fn virtualize_if_possible(&self, source: &RealSource) -> Option<VirtualSource> {
        for m in self.mappings(MappingCompartment::ControllerMappings) {
            let m = m.borrow();
            if !m.control_is_enabled.get() {
                continue;
            }
            if m.target_model.category.get() != TargetCategory::Virtual {
                continue;
            }
            if !self.on_mappings.get_ref().contains(&m.id()) {
                // Since virtual mappings support conditional activation, too!
                continue;
            }
            if let Some(s) = RealSource::from_compound_source(m.source_model.create_source()) {
                if s == *source {
                    let virtual_source =
                        VirtualSource::new(m.target_model.create_control_element());
                    return Some(virtual_source);
                }
            }
        }
        None
    }

    pub fn source_touched(
        &self,
        reenable_control_after_touched: bool,
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    ) -> impl LocalObservable<'static, Item = CompoundMappingSource, Err = ()> + 'static {
        // TODO-low We should migrate this to the nice async-await mechanism that we use for global
        //  learning (via REAPER action). That way we don't need the subject and also don't need
        //  to pass the information through multiple processors whether we allow virtual sources.
        // TODO-low Would be nicer to do this on subscription instead of immediately. from_fn()?
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::StartLearnSource {
                allow_virtual_sources,
            })
            .unwrap();
        self.normal_main_task_sender
            .try_send(NormalMainTask::StartLearnSource {
                allow_virtual_sources,
                osc_arg_index_hint,
            })
            .unwrap();
        let rt_sender = self.normal_real_time_task_sender.clone();
        let main_sender = self.normal_main_task_sender.clone();
        self.source_touched_subject.clone().finalize(move || {
            if reenable_control_after_touched {
                rt_sender
                    .send(NormalRealTimeTask::ReturnToControlMode)
                    .unwrap();
                main_sender
                    .try_send(NormalMainTask::ReturnToControlMode)
                    .unwrap();
            }
        })
    }

    fn resubscribe_to_mappings(
        &mut self,
        compartment: MappingCompartment,
        weak_session: WeakSession,
    ) {
        self.mapping_subscriptions[compartment] = self.mappings[compartment]
            .iter()
            .map(|shared_mapping| {
                // We don't need to take until "party is over" because if the session disappears,
                // we know the mappings disappear as well.
                let mapping = shared_mapping.borrow();
                let shared_mapping_clone = shared_mapping.clone();
                let all_subscriptions = LocalSubscription::default();
                // Keep syncing to processors
                {
                    let subscription = when(mapping.changed_processing_relevant())
                        .with(weak_session.clone())
                        .do_sync(move |session, _| {
                            let mut session = session.borrow_mut();
                            session.sync_single_mapping_to_processors(
                                compartment,
                                &shared_mapping_clone.borrow(),
                            );
                            session.mark_project_as_dirty();
                            session.notify_mapping_changed(compartment);
                        });
                    all_subscriptions.add(subscription);
                }
                // Keep marking project as dirty
                {
                    let subscription = when(mapping.changed_non_processing_relevant())
                        .with(weak_session.clone())
                        .do_sync(move |session, _| {
                            let mut session = session.borrow_mut();
                            session.mark_project_as_dirty();
                            session.notify_mapping_changed(compartment);
                        });
                    all_subscriptions.add(subscription);
                }
                // Keep auto-correcting mode settings
                if self.auto_correct_settings.get() {
                    let processor_context = self.context().clone();
                    let subscription = when(
                        mapping
                            .source_model
                            .changed()
                            .merge(mapping.target_model.changed()),
                    )
                    .with(Rc::downgrade(&shared_mapping))
                    .do_sync(move |mapping, _| {
                        // Parameter values are not important for mode auto correction because
                        // dynamic targets don't really profit from it anyway. Therefore just
                        // use zero parameters.
                        let extended_context = ExtendedProcessorContext::new(
                            &processor_context,
                            &ZEROED_PLUGIN_PARAMETERS,
                        );
                        mapping
                            .borrow_mut()
                            .adjust_mode_if_necessary(extended_context);
                    });
                    all_subscriptions.add(subscription);
                }
                SubscriptionGuard::new(all_subscriptions)
            })
            .collect();
    }

    fn resubscribe_to_groups(
        &mut self,
        weak_session: WeakSession,
        compartment: MappingCompartment,
    ) {
        self.group_subscriptions[compartment] = self
            .groups_including_default_group(compartment)
            .map(|shared_group| {
                // We don't need to take until "party is over" because if the session disappears,
                // we know the groups disappear as well.
                let group = shared_group.borrow();
                let all_subscriptions = LocalSubscription::default();
                // Keep syncing to processors
                {
                    let subscription = when(group.changed_processing_relevant())
                        .with(weak_session.clone())
                        .do_sync(move |session, _| {
                            let mut session = session.borrow_mut();
                            // Change of a single group can affect many mappings
                            session.sync_all_mappings_full(compartment);
                            session.mark_project_as_dirty();
                            session.notify_group_changed(compartment);
                        });
                    all_subscriptions.add(subscription);
                }
                // Keep marking project as dirty
                {
                    let subscription = when(group.changed_non_processing_relevant())
                        .with(weak_session.clone())
                        .do_sync(move |session, _| {
                            let mut session = session.borrow_mut();
                            session.mark_project_as_dirty();
                            session.notify_group_changed(compartment);
                        });
                    all_subscriptions.add(subscription);
                }
                SubscriptionGuard::new(all_subscriptions)
            })
            .collect();
    }

    fn learn_target(&mut self, target: &ReaperTarget) {
        // Prevent learning targets from in other project tabs (leads to weird effects, just think
        // about it)
        if let Some(p) = target.project() {
            if p != self.context.project_or_current_project() {
                return;
            }
        }
        if let Some(qualified_id) = self.mapping_which_learns_target.replace(None) {
            if let Some((_, mapping)) = self.find_mapping_and_index_by_qualified_id(qualified_id) {
                mapping
                    .borrow_mut()
                    .target_model
                    .apply_from_target(target, &self.context);
            }
        }
    }

    pub fn context(&self) -> &ProcessorContext {
        &self.context
    }

    pub fn extended_context(&self) -> ExtendedProcessorContext {
        ExtendedProcessorContext::new(&self.context, &self.parameters)
    }

    pub fn add_default_group(&mut self, compartment: MappingCompartment, name: String) -> GroupId {
        let group = GroupModel::new_from_ui(compartment, name);
        self.add_group(compartment, group)
    }

    fn add_group(&mut self, compartment: MappingCompartment, group: GroupModel) -> GroupId {
        let id = group.id();
        let shared_group = Rc::new(RefCell::new(group));
        self.groups[compartment].push(shared_group);
        self.notify_group_list_changed(compartment);
        id
    }

    pub fn find_group_index_by_id_sorted(
        &self,
        compartment: MappingCompartment,
        id: GroupId,
    ) -> Option<usize> {
        self.groups_sorted(compartment)
            .position(|g| g.borrow().id() == id)
    }

    pub fn group_contains_mappings(&self, compartment: MappingCompartment, id: GroupId) -> bool {
        self.mappings(compartment)
            .filter(|m| m.borrow().group_id.get() == id)
            .count()
            > 0
    }

    pub fn find_group_by_id(
        &self,
        compartment: MappingCompartment,
        id: GroupId,
    ) -> Option<&SharedGroup> {
        self.groups[compartment]
            .iter()
            .find(|g| g.borrow().id() == id)
    }

    pub fn find_group_by_index_sorted(
        &self,
        compartment: MappingCompartment,
        index: usize,
    ) -> Option<&SharedGroup> {
        self.groups_sorted(compartment).nth(index)
    }

    pub fn groups_sorted(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &SharedGroup> {
        self.groups[compartment]
            .iter()
            .sorted_by_key(|g| g.borrow().name.get_ref().clone())
    }

    pub fn move_mappings_to_group(
        &mut self,
        compartment: MappingCompartment,
        mapping_ids: &[MappingId],
        group_id: GroupId,
    ) -> Result<(), &'static str> {
        for mapping_id in mapping_ids.iter() {
            let (_, mapping) = self
                .find_mapping_and_index_by_id(compartment, *mapping_id)
                .ok_or("no such mapping")?;
            mapping.borrow_mut().group_id.set(group_id);
        }
        self.notify_group_list_changed(compartment);
        Ok(())
    }

    pub fn remove_group(
        &mut self,
        compartment: MappingCompartment,
        id: GroupId,
        delete_mappings: bool,
    ) {
        self.groups[compartment].retain(|g| g.borrow().id() != id);
        if delete_mappings {
            self.mappings[compartment].retain(|m| m.borrow().group_id.get() != id);
        } else {
            for m in self.mappings(compartment) {
                let mut m = m.borrow_mut();
                if m.group_id.get() == id {
                    m.group_id.set_without_notification(GroupId::default());
                }
            }
        }
        self.notify_group_list_changed(compartment);
    }

    pub fn add_default_mapping(
        &mut self,
        compartment: MappingCompartment,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_type: VirtualControlElementType,
    ) -> SharedMapping {
        let mut mapping = MappingModel::new(compartment, initial_group_id);
        mapping
            .name
            .set_without_notification(self.generate_name_for_new_mapping(compartment));
        if compartment == MappingCompartment::ControllerMappings {
            let next_control_element_index =
                self.get_next_control_element_index(control_element_type);
            let target_model = TargetModel {
                category: prop(TargetCategory::Virtual),
                control_element_type: prop(control_element_type),
                control_element_id: prop(VirtualControlElementId::Indexed(
                    next_control_element_index,
                )),
                ..Default::default()
            };
            mapping.target_model = target_model;
        }
        self.add_mapping(compartment, mapping)
    }

    pub fn insert_mappings_at(
        &mut self,
        compartment: MappingCompartment,
        index: usize,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        let mut index = index.min(self.mappings[compartment].len());
        let mut first_mapping_id = None;
        for mut m in mappings {
            m.set_id_without_notification(MappingId::random());
            if first_mapping_id.is_none() {
                first_mapping_id = Some(m.id());
            }
            let shared_mapping = share_mapping(m);
            self.mappings[compartment].insert(index, shared_mapping);
            index += 1;
        }
        self.notify_mapping_list_changed(compartment, first_mapping_id);
    }

    pub fn replace_mappings_of_group(
        &mut self,
        compartment: MappingCompartment,
        group_id: GroupId,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        self.mappings[compartment].retain(|m| m.borrow().group_id.get() != group_id);
        for mut m in mappings {
            m.set_id_without_notification(MappingId::random());
            let shared_mapping = share_mapping(m);
            self.mappings[compartment].push(shared_mapping);
        }
        self.notify_mapping_list_changed(compartment, None);
    }

    fn get_next_control_element_index(&self, element_type: VirtualControlElementType) -> u32 {
        let max_index_so_far = self
            .mappings(MappingCompartment::ControllerMappings)
            .filter_map(|m| {
                let m = m.borrow();
                let target = &m.target_model;
                if target.category.get() != TargetCategory::Virtual
                    || target.control_element_type.get() != element_type
                {
                    return None;
                }
                if let VirtualControlElementId::Indexed(i) = target.control_element_id.get() {
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
        session: &SharedSession,
        compartment: MappingCompartment,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_type: VirtualControlElementType,
    ) {
        // Prepare
        self.disable_control();
        self.stop_learning_source();
        self.stop_learning_target();
        // Add initial mapping and start learning its source
        self.add_and_learn_one_of_many_mappings(
            session,
            compartment,
            initial_group_id,
            control_element_type,
        );
        // After target learned, add new mapping and start learning its source
        let prop_to_observe = match compartment {
            // For controller mappings we don't need to learn a target so we move on to the next
            // mapping as soon as the source has been learned.
            MappingCompartment::ControllerMappings => &self.mapping_which_learns_source,
            // For main mappings we want to learn a target before moving on to the next mapping.
            MappingCompartment::MainMappings => &self.mapping_which_learns_target,
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
                control_element_type,
            );
        });
    }

    fn add_and_learn_one_of_many_mappings(
        &mut self,
        session: &SharedSession,
        compartment: MappingCompartment,
        // Only relevant for main mapping compartment
        initial_group_id: GroupId,
        // Only relevant for controller mapping compartment
        control_element_type: VirtualControlElementType,
    ) {
        let ignore_sources = match compartment {
            MappingCompartment::ControllerMappings => {
                // When batch-learning controller mappings, we just want to learn sources that have
                // not yet been learned. Otherwise when we move a fader, we create many mappings in
                // one go.
                self.mappings(compartment)
                    .map(|m| m.borrow().source_model.create_source())
                    .collect()
            }
            MappingCompartment::MainMappings => HashSet::new(),
        };
        let mapping = self.add_default_mapping(compartment, initial_group_id, control_element_type);
        let qualified_mapping_id = mapping.borrow().qualified_id();
        self.learn_many_state
            .set(Some(LearnManyState::learning_source(
                compartment,
                qualified_mapping_id.id,
                control_element_type,
            )));
        self.start_learning_source(
            Rc::downgrade(session),
            mapping,
            false,
            ignore_sources,
            compartment != MappingCompartment::ControllerMappings,
        );
        // If this is a main mapping, start learning target as soon as source learned. For
        // controller mappings we don't need to do this because adding the default mapping will
        // automatically increase the virtual target control element index (which is usually what
        // one wants when creating a controller mapping).
        if compartment == MappingCompartment::MainMappings {
            when(
                self.mapping_which_learns_source
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
                session.start_learning_target(
                    Rc::downgrade(&shared_session),
                    qualified_mapping_id,
                    false,
                );
            });
        }
    }

    pub fn stop_learning_many_mappings(&mut self) {
        self.learn_many_state.set(None);
        let source_learning_mapping_id = self.mapping_which_learns_source.get();
        self.stop_learning_source();
        self.stop_learning_target();
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

    pub fn mapping_count(&self, compartment: MappingCompartment) -> usize {
        self.mappings[compartment].len()
    }

    pub fn find_mapping_and_index_by_qualified_id(
        &self,
        id: QualifiedMappingId,
    ) -> Option<(usize, &SharedMapping)> {
        self.find_mapping_and_index_by_id(id.compartment, id.id)
    }

    pub fn find_mapping_and_index_by_id(
        &self,
        compartment: MappingCompartment,
        mapping_id: MappingId,
    ) -> Option<(usize, &SharedMapping)> {
        self.mappings(compartment)
            .enumerate()
            .find(|(_, m)| m.borrow().id() == mapping_id)
    }

    pub fn mappings(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &SharedMapping> {
        self.mappings[compartment].iter()
    }

    pub fn default_group(&self, compartment: MappingCompartment) -> &SharedGroup {
        match compartment {
            MappingCompartment::ControllerMappings => &self.default_controller_group,
            MappingCompartment::MainMappings => &self.default_main_group,
        }
    }

    pub fn groups(&self, compartment: MappingCompartment) -> impl Iterator<Item = &SharedGroup> {
        self.groups[compartment].iter()
    }

    fn groups_including_default_group(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &SharedGroup> {
        std::iter::once(self.default_group(compartment)).chain(self.groups[compartment].iter())
    }

    fn all_mappings(&self) -> impl Iterator<Item = &SharedMapping> {
        MappingCompartment::enum_iter()
            .map(move |compartment| self.mappings(compartment))
            .flatten()
    }

    pub fn mapping_is_learning_source(&self, id: QualifiedMappingId) -> bool {
        match self.mapping_which_learns_source.get_ref() {
            None => false,
            Some(i) => *i == id,
        }
    }

    pub fn mapping_is_learning_target(&self, id: QualifiedMappingId) -> bool {
        match self.mapping_which_learns_target.get_ref() {
            None => false,
            Some(i) => *i == id,
        }
    }

    /// Resets the session ID to the (hopefully) always unique instance ID.
    pub fn reset_id(&mut self) {
        self.id.set(self.instance_id.to_string());
    }

    pub fn mapping_which_learns_source_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.mapping_which_learns_source.changed()
    }

    pub fn mapping_which_learns_target_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.mapping_which_learns_target.changed()
    }

    pub fn toggle_learning_source(&mut self, session: &SharedSession, mapping: &SharedMapping) {
        if self.mapping_which_learns_source.get_ref().is_none() {
            self.start_learning_source(
                Rc::downgrade(session),
                mapping.clone(),
                true,
                HashSet::new(),
                mapping.borrow().compartment() != MappingCompartment::ControllerMappings,
            );
        } else {
            self.stop_learning_source();
        }
    }

    fn start_learning_source(
        &mut self,
        session: WeakSession,
        mapping: SharedMapping,
        reenable_control_after_touched: bool,
        ignore_sources: HashSet<CompoundMappingSource>,
        allow_virtual_sources: bool,
    ) {
        let (mapping_id, osc_arg_index_hint) = {
            let m = mapping.borrow();
            (m.qualified_id(), m.source_model.osc_arg_index.get())
        };
        self.mapping_which_learns_source.set(Some(mapping_id));
        when(
            self.source_touched(
                reenable_control_after_touched,
                allow_virtual_sources,
                osc_arg_index_hint,
            )
            .filter(move |s| !ignore_sources.contains(s))
            // We have this explicit stop criteria because we listen to global REAPER
            // events.
            .take_until(self.party_is_over())
            .take_until(self.mapping_which_learns_source.changed_to(None))
            .take(1),
        )
        .with(session)
        .finally(|session| session.borrow_mut().mapping_which_learns_source.set(None))
        .do_async(|session, source| {
            let session = session.borrow();
            if let Some(qualified_id) = session.mapping_which_learns_source.get_ref() {
                if let Some((_, m)) =
                    session.find_mapping_and_index_by_id(qualified_id.compartment, qualified_id.id)
                {
                    m.borrow_mut().source_model.apply_from_source(&source);
                }
            }
        });
    }

    fn stop_learning_source(&mut self) {
        self.mapping_which_learns_source.set(None);
    }

    pub fn toggle_learning_target(
        &mut self,
        session: &SharedSession,
        mapping_id: QualifiedMappingId,
    ) {
        if self.mapping_which_learns_target.get_ref().is_none() {
            self.start_learning_target(Rc::downgrade(session), mapping_id, true);
        } else {
            self.stop_learning_target();
        }
    }

    fn start_learning_target(
        &mut self,
        session: WeakSession,
        mapping_id: QualifiedMappingId,
        handle_control_disabling: bool,
    ) {
        self.mapping_which_learns_target.set(Some(mapping_id));
        if handle_control_disabling {
            self.disable_control();
        }
        when(
            ReaperTarget::touched()
                // We have this explicit stop criteria because we listen to global REAPER
                // events.
                .take_until(self.party_is_over())
                .take_until(self.mapping_which_learns_target.changed_to(None))
                .take(1),
        )
        .with(session)
        .finally(move |session| {
            let mut session = session.borrow_mut();
            if handle_control_disabling {
                session.enable_control();
            }
            session.mapping_which_learns_target.set(None);
        })
        .do_async(|session, target| {
            session.borrow_mut().learn_target(target.as_ref());
        });
    }

    fn disable_control(&self) {
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::DisableControl)
            .unwrap();
        self.normal_main_task_sender
            .try_send(NormalMainTask::DisableControl)
            .unwrap();
    }

    fn enable_control(&self) {
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::ReturnToControlMode)
            .unwrap();
        self.normal_main_task_sender
            .try_send(NormalMainTask::ReturnToControlMode)
            .unwrap();
    }

    fn stop_learning_target(&mut self) {
        self.mapping_which_learns_target.set(None);
    }

    fn find_index_of_closest_mapping(
        &self,
        compartment: MappingCompartment,
        mapping: &SharedMapping,
        index: usize,
        within_same_group: bool,
        increment: isize,
    ) -> Option<usize> {
        let mappings = &self.mappings[compartment];
        let total_mapping_count = mappings.len();
        let result_index = if within_same_group {
            let group_id = mapping.borrow().group_id.get();
            let mut i = index as isize + increment;
            while i >= 0 && i < total_mapping_count as isize {
                let m = &mappings[i as usize];
                if m.borrow().group_id.get() == group_id {
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
        compartment: MappingCompartment,
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
        self.mappings[id.compartment].retain(|m| m.borrow().id() != id.id);
        self.notify_mapping_list_changed(id.compartment, None);
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

    fn index_of_mapping(
        &self,
        compartment: MappingCompartment,
        mapping_id: MappingId,
    ) -> Option<usize> {
        self.mappings[compartment]
            .iter()
            .position(|m| m.borrow().id() == mapping_id)
    }

    pub fn location_of_mapping(
        &self,
        mapping_id: MappingId,
    ) -> Option<(MappingCompartment, usize)> {
        MappingCompartment::enum_iter().find_map(|compartment| {
            let index = self.index_of_mapping(compartment, mapping_id)?;
            Some((compartment, index))
        })
    }

    pub fn show_in_floating_window(&self) {
        self.context().containing_fx().show_in_floating_window();
    }

    pub fn containing_fx_is_in_input_fx_chain(&self) -> bool {
        self.context.containing_fx().is_input_fx()
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

    pub fn active_main_preset_id(&self) -> Option<&str> {
        self.active_main_preset_id.as_deref()
    }

    pub fn active_controller(&self) -> Option<ControllerPreset> {
        let id = self.active_controller_preset_id()?;
        self.controller_preset_manager.find_by_id(id)
    }

    pub fn active_main_preset(&self) -> Option<MainPreset> {
        let id = self.active_main_preset_id()?;
        self.main_preset_manager.find_by_id(id)
    }

    pub fn controller_preset_is_out_of_date(&self) -> bool {
        let compartment = MappingCompartment::ControllerMappings;
        let id = match &self.active_controller_preset_id {
            None => return self.mapping_count(compartment) > 0,
            Some(id) => id,
        };
        self.controller_preset_manager
            .mappings_are_dirty(id, &self.mappings[compartment])
            || self.controller_preset_manager.groups_are_dirty(
                id,
                &self.default_group(compartment),
                &self.groups[compartment],
            )
            || self.controller_preset_manager.parameter_settings_are_dirty(
                id,
                &self.non_default_parameter_settings_by_compartment(compartment),
            )
    }

    pub fn main_preset_is_out_of_date(&self) -> bool {
        let compartment = MappingCompartment::MainMappings;
        let id = match &self.active_main_preset_id {
            None => {
                return self.mapping_count(compartment) > 0 || !self.groups.is_empty();
            }
            Some(id) => id,
        };
        self.main_preset_manager
            .mappings_are_dirty(id, &self.mappings[compartment])
            || self.main_preset_manager.groups_are_dirty(
                id,
                &self.default_main_group,
                &self.groups[compartment],
            )
            || self.main_preset_manager.parameter_settings_are_dirty(
                id,
                &self.non_default_parameter_settings_by_compartment(compartment),
            )
    }

    pub fn activate_controller_preset(
        &mut self,
        id: Option<String>,
        weak_session: WeakSession,
    ) -> Result<(), &'static str> {
        // TODO-medium The code duplication with main mappings is terrible.
        let compartment = MappingCompartment::ControllerMappings;
        self.active_controller_preset_id = id.clone();
        if let Some(id) = id.as_ref() {
            let preset = self
                .controller_preset_manager
                .find_by_id(id)
                .ok_or("controller preset not found")?;
            self.default_controller_group
                .replace(preset.default_group().clone());
            self.set_groups_without_notification(compartment, preset.groups().iter().cloned());
            self.set_mappings_without_notification(compartment, preset.mappings().iter().cloned());
            self.set_parameter_settings_from_non_default(compartment, preset.parameters());
        } else {
            // <None> preset
            self.clear_compartment_data(compartment);
        };
        self.reset_parameters(compartment);
        self.notify_everything_has_changed(weak_session);
        Ok(())
    }

    pub fn activate_main_preset(
        &mut self,
        id: Option<String>,
        weak_session: WeakSession,
    ) -> Result<(), &'static str> {
        let compartment = MappingCompartment::MainMappings;
        self.active_main_preset_id = id.clone();
        if let Some(id) = id.as_ref() {
            let preset = self
                .main_preset_manager
                .find_by_id(id)
                .ok_or("main preset not found")?;
            self.default_main_group
                .replace(preset.default_group().clone());
            self.set_groups_without_notification(compartment, preset.groups().iter().cloned());
            self.set_mappings_without_notification(compartment, preset.mappings().iter().cloned());
            self.set_parameter_settings_from_non_default(compartment, preset.parameters());
        } else {
            // <None> preset
            self.clear_compartment_data(compartment);
        }
        self.reset_parameters(compartment);
        self.notify_everything_has_changed(weak_session);
        Ok(())
    }

    fn reset_parameters(&self, compartment: MappingCompartment) {
        let fx = self.context.containing_fx().clone();
        let _ = Global::task_support().do_later_in_main_thread_from_main_thread_asap(move || {
            for i in compartment.param_range() {
                let _ = fx.parameter_by_index(i).set_reaper_normalized_value(0.0);
            }
        });
    }

    fn clear_compartment_data(&mut self, compartment: MappingCompartment) {
        self.default_group(compartment)
            .replace(GroupModel::default_for_compartment(compartment));
        self.set_groups_without_notification(compartment, std::iter::empty());
        self.set_mappings_without_notification(compartment, std::iter::empty());
        self.set_parameter_settings_without_notification(compartment, empty_parameter_settings());
    }

    fn containing_fx_enabled_or_disabled(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        let containing_fx = self.context.containing_fx().clone();
        Global::control_surface_rx()
            .fx_enabled_changed()
            .filter(move |fx| *fx == containing_fx)
            .map_to(())
    }

    fn containing_track_armed_or_disarmed(&self) -> LocalBoxOp<'static, (), ()> {
        if let Some(track) = self.context.containing_fx().track().cloned() {
            Global::control_surface_rx()
                .track_arm_changed()
                .filter(move |t| *t == track)
                .map_to(())
                .box_it()
        } else {
            observable::never().box_it()
        }
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
    ) -> impl LocalObservable<'static, Item = (MappingCompartment, Option<MappingId>), Err = ()> + 'static
    {
        self.mapping_list_changed_subject.clone()
    }

    /// Fires when a group has been added or removed.
    ///
    /// Doesn't fire if a group in the list or if the complete list has changed.
    pub fn group_list_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = MappingCompartment, Err = ()> + 'static {
        self.group_list_changed_subject.clone()
    }

    pub fn parameter_settings_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = MappingCompartment, Err = ()> + 'static {
        self.parameter_settings_changed_subject.clone()
    }

    /// Fires if a group itself has been changed.
    pub fn group_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = MappingCompartment, Err = ()> + 'static {
        self.group_changed_subject.clone()
    }

    /// Fires if a mapping itself has been changed.
    pub fn mapping_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = MappingCompartment, Err = ()> + 'static {
        self.mapping_changed_subject.clone()
    }

    pub fn set_mappings_without_notification(
        &mut self,
        compartment: MappingCompartment,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        // If we import JSON from clipboard, we might stumble upon duplicate mapping IDs. Fix those!
        // This is a feature for power users.
        let mut used_ids = HashSet::new();
        let fixed_mappings: Vec<_> = mappings
            .map(|mut m| {
                if used_ids.contains(&m.id()) {
                    m.set_id_without_notification(MappingId::random());
                } else {
                    used_ids.insert(m.id());
                }
                m
            })
            .collect();
        self.mappings[compartment] = fixed_mappings.into_iter().map(share_mapping).collect();
    }

    pub fn set_groups_without_notification(
        &mut self,
        compartment: MappingCompartment,
        groups: impl Iterator<Item = GroupModel>,
    ) {
        self.groups[compartment] = groups.into_iter().map(share_group).collect();
    }

    fn add_mapping(
        &mut self,
        compartment: MappingCompartment,
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
            .try_send(NormalMainTask::SendAllFeedback)
            .unwrap();
    }

    pub fn log_debug_info(&self) {
        self.log_debug_info_internal();
        self.normal_main_task_sender
            .try_send(NormalMainTask::LogDebugInfo)
            .unwrap();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::LogDebugInfo)
            .unwrap();
    }

    pub fn mapping_is_on(&self, id: MappingId) -> bool {
        self.on_mappings.get_ref().contains(&id)
    }

    pub fn on_mappings_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.on_mappings.changed()
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
            self.instance_id,
            self.id.get_ref(),
            self.mappings[MappingCompartment::MainMappings].len(),
            self.mapping_subscriptions[MappingCompartment::MainMappings].len(),
            self.groups.len(),
            self.group_subscriptions.len(),
            self.mappings[MappingCompartment::ControllerMappings].len(),
            self.mapping_subscriptions[MappingCompartment::ControllerMappings].len(),
        );
        Reaper::get().show_console_msg(msg);
        // Detailled
        println!(
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
        compartment: MappingCompartment,
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
        session: &SharedSession,
        compartment: MappingCompartment,
        target: &ReaperTarget,
    ) -> SharedMapping {
        let mapping = match self.find_mapping_with_target(compartment, target) {
            None => {
                let m = self.add_default_mapping(
                    compartment,
                    GroupId::default(),
                    VirtualControlElementType::Multi,
                );
                m.borrow_mut()
                    .target_model
                    .apply_from_target(target, &self.context);
                m
            }
            Some(m) => m.clone(),
        };
        self.toggle_learning_source(session, &mapping);
        mapping
    }

    pub fn show_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        self.ui.show_mapping(compartment, mapping_id);
    }

    /// Notifies listeners async that something in a mapping list has changed.
    ///
    /// Shouldn't be used if the complete list has changed.
    fn notify_mapping_list_changed(
        &mut self,
        compartment: MappingCompartment,
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
    fn notify_group_list_changed(&mut self, compartment: MappingCompartment) {
        AsyncNotifier::notify(&mut self.group_list_changed_subject, &compartment);
    }

    fn notify_parameter_settings_changed(&mut self, compartment: MappingCompartment) {
        AsyncNotifier::notify(&mut self.parameter_settings_changed_subject, &compartment);
    }

    /// Notifies listeners async a group in the group list has changed.
    fn notify_group_changed(&mut self, compartment: MappingCompartment) {
        AsyncNotifier::notify(&mut self.group_changed_subject, &compartment);
    }

    /// Notifies listeners async a mapping in a mapping list has changed.
    fn notify_mapping_changed(&mut self, compartment: MappingCompartment) {
        AsyncNotifier::notify(&mut self.mapping_changed_subject, &compartment);
    }

    fn sync_upper_floor_membership(&self) {
        let backbone_state = BackboneState::get();
        if self.lives_on_upper_floor.get() {
            backbone_state.add_to_upper_floor(self.instance_id);
        } else {
            backbone_state.remove_from_upper_floor(&self.instance_id);
        }
    }

    pub fn control_input(&self) -> ControlInput {
        if let Some(osc_dev_id) = self.osc_input_device_id.get() {
            ControlInput::Osc(osc_dev_id)
        } else {
            ControlInput::Midi(self.midi_control_input.get())
        }
    }

    pub fn feedback_output(&self) -> Option<FeedbackOutput> {
        if let Some(osc_dev_id) = self.osc_output_device_id.get() {
            Some(FeedbackOutput::Osc(osc_dev_id))
        } else {
            self.midi_feedback_output.get().map(FeedbackOutput::Midi)
        }
    }

    pub fn instance_state(&self) -> &SharedInstanceState {
        &self.instance_state
    }

    fn sync_settings(&self) {
        let task = NormalMainTask::UpdateSettings {
            control_input: self.control_input(),
            feedback_output: self.feedback_output(),
        };
        self.normal_main_task_sender.try_send(task).unwrap();
        let task = NormalRealTimeTask::UpdateSettings {
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
            midi_control_input: self.midi_control_input.get(),
            midi_feedback_output: self.midi_feedback_output.get(),
        };
        self.normal_real_time_task_sender.send(task).unwrap();
    }

    fn sync_single_mapping_to_processors(&self, compartment: MappingCompartment, m: &MappingModel) {
        let group_data = self
            .find_group_of_mapping(m)
            .map(|g| g.borrow().create_data())
            .unwrap_or_default();
        let main_mapping = m.create_main_mapping(group_data);
        self.normal_main_task_sender
            .try_send(NormalMainTask::UpdateSingleMapping(
                compartment,
                Box::new(main_mapping),
            ))
            .unwrap();
    }

    fn find_group_of_mapping(&self, mapping: &MappingModel) -> Option<&SharedGroup> {
        let group_id = mapping.group_id.get();
        if group_id.is_default() {
            let group = match mapping.compartment() {
                MappingCompartment::ControllerMappings => &self.default_controller_group,
                MappingCompartment::MainMappings => &self.default_main_group,
            };
            Some(group)
        } else {
            self.find_group_by_id(mapping.compartment(), group_id)
        }
    }

    fn control_is_globally_enabled(&self) -> bool {
        self.context.containing_fx().is_enabled()
    }

    fn feedback_is_globally_enabled(&self) -> bool {
        (self.midi_feedback_output.get().is_some() || self.osc_output_device_id.get_ref().is_some())
            && self.context.containing_fx().is_enabled()
            && self.track_arm_conditions_are_met()
    }

    fn track_arm_conditions_are_met(&self) -> bool {
        if !self.containing_fx_is_in_input_fx_chain() && !self.send_feedback_only_if_armed.get() {
            return true;
        }
        match self.context.track() {
            None => true,
            Some(t) => t.is_available() && t.is_armed(false),
        }
    }

    pub fn parameters(&self) -> &ParameterArray {
        &self.parameters
    }

    /// Just syncs whether control globally enabled or not.
    fn sync_control_is_globally_enabled(&self) {
        let enabled = self.control_is_globally_enabled();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateControlIsGloballyEnabled(enabled))
            .unwrap();
        self.normal_main_task_sender
            .try_send(NormalMainTask::UpdateControlIsGloballyEnabled(enabled))
            .unwrap();
    }

    /// Just syncs whether feedback globally enabled or not.
    fn sync_feedback_is_globally_enabled(&self) {
        let enabled = self.feedback_is_globally_enabled();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateFeedbackIsGloballyEnabled(enabled))
            .unwrap();
        self.normal_main_task_sender
            .try_send(NormalMainTask::UpdateFeedbackIsGloballyEnabled(enabled))
            .unwrap();
    }

    /// Does a full mapping sync.
    fn sync_all_mappings_full(&self, compartment: MappingCompartment) {
        let main_mappings = self.create_main_mappings(compartment);
        self.normal_main_task_sender
            .try_send(NormalMainTask::UpdateAllMappings(
                compartment,
                main_mappings,
            ))
            .unwrap();
    }

    /// Creates mappings from mapping models so they can be distributed to different processors.
    fn create_main_mappings(&self, compartment: MappingCompartment) -> Vec<MainMapping> {
        let group_map: HashMap<GroupId, Ref<GroupModel>> = self
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
                    .get(mapping.group_id.get_ref())
                    .map(|g| g.create_data())
                    .unwrap_or_default();
                mapping.create_main_mapping(group_data)
            })
            .collect()
    }

    fn generate_name_for_new_mapping(&self, compartment: MappingCompartment) -> String {
        format!("{}", self.mappings[compartment].len() + 1)
    }

    fn party_is_over(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.party_is_over_subject.clone()
    }

    /// Shouldn't be called on load (project load, undo, redo, preset change).
    pub fn mark_project_as_dirty(&self) {
        debug!(self.logger, "Marking project as dirty");
        self.context.project_or_current_project().mark_as_dirty();
    }

    pub fn logger(&self) -> &slog::Logger {
        &self.logger
    }

    /// Does a full resync and notifies the UI async.
    ///
    /// Explicitly doesn't mark the project as dirty - because this is also used when loading data
    /// (project load, undo, redo, preset change).
    pub fn notify_everything_has_changed(&mut self, weak_session: WeakSession) {
        self.full_sync(weak_session);
        // For UI
        AsyncNotifier::notify(&mut self.everything_changed_subject, &());
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterSetting {
    #[serde(rename = "name", default, skip_serializing_if = "is_default")]
    pub name: String,
}

impl ParameterSetting {
    pub fn is_default(&self) -> bool {
        self.name.is_empty()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        debug!(self.logger(), "Dropping session...");
        self.party_is_over_subject.next(());
    }
}

impl DomainEventHandler for WeakSession {
    fn handle_event(&self, event: DomainEvent) {
        let session = self.upgrade().expect("session not existing anymore");
        use DomainEvent::*;
        match event {
            LearnedSource {
                source,
                allow_virtual_sources,
            } => {
                session
                    .borrow_mut()
                    .learn_source(source, allow_virtual_sources);
            }
            UpdatedOnMappings(on_mappings) => {
                session.borrow_mut().on_mappings.set(on_mappings);
            }
            TargetValueChanged(e) => {
                // If the session is borrowed already, just let it be. It happens only in a very
                // particular case of reentrancy (because of a quirk in REAPER related to master
                // tempo notification, https://github.com/helgoboss/realearn/issues/199). If the
                // target value slider is not updated then ... so what.
                if let Ok(s) = session.try_borrow_mut() {
                    s.ui.target_value_changed(e);
                }
            }
            UpdatedParameter { index, value } => {
                let mut session = session.borrow_mut();
                session.parameters[index as usize] = value;
                session.ui.parameters_changed(&session);
            }
            UpdatedAllParameters(params) => {
                let mut session = session.borrow_mut();
                session.parameters = *params;
                session.ui.parameters_changed(&session);
            }
            FullResyncRequested => {
                session.borrow_mut().full_sync(self.clone());
            }
            ProjectionFeedback(value) => {
                if let Ok(s) = session.try_borrow() {
                    s.ui.send_projection_feedback(&s, value);
                }
            }
        }
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
pub type SharedSession = Rc<RefCell<Session>>;

/// Always use this when storing a reference to a session. This avoids memory leaks and ghost
/// sessions.
pub type WeakSession = Weak<RefCell<Session>>;

pub enum InputDescriptor {
    Midi {
        device_id: MidiInputDeviceId,
        channel: Option<Channel>,
    },
    Osc {
        device_id: OscDeviceId,
    },
}

pub fn empty_parameter_settings() -> Vec<ParameterSetting> {
    vec![Default::default(); COMPARTMENT_PARAMETER_COUNT as usize]
}

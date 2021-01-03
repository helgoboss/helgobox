use crate::application::{
    share_mapping, ControllerPreset, FxId, MainPreset, MainPresetAutoLoadMode, MappingModel,
    Preset, PresetLinkManager, PresetManager, SharedMapping, TargetCategory, TargetModel,
    VirtualControlElementType,
};
use crate::core::{prop, when, AsyncNotifier, Prop};
use crate::domain::{
    CompoundMappingSource, ControlMainTask, DomainEvent, DomainEventHandler, FeedbackRealTimeTask,
    Global, MainMapping, MainProcessor, MappingCompartment, MappingId, MidiControlInput,
    MidiFeedbackOutput, NormalMainTask, NormalRealTimeTask, ParameterMainTask, ProcessorContext,
    ReaperTarget, PLUGIN_PARAMETER_COUNT,
};
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;

use reaper_high::Reaper;
use reaper_medium::RegistrationHandle;
use rx_util::{BoxedUnitEvent, Event, Notifier, SharedItemEvent, SharedPayload, UnitEvent};
use rxrust::prelude::*;
use slog::debug;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Debug;

use std::rc::{Rc, Weak};
use wrap_debug::WrapDebug;

pub trait SessionUi {
    fn show_mapping(&self, mapping: *const MappingModel);
}

/// This represents the user session with one ReaLearn instance.
///
/// It's ReaLearn's main object which keeps everything together.
#[derive(Debug)]
pub struct Session {
    instance_id: String,
    /// Initially corresponds to instance ID but is persisted and can be user-customized. Should be
    /// unique but if not it's not a big deal, then it won't crash but the user can't be sure which
    /// session will be picked. Most relevant for HTTP/WS API.
    pub id: Prop<String>,
    logger: slog::Logger,
    pub let_matched_events_through: Prop<bool>,
    pub let_unmatched_events_through: Prop<bool>,
    pub always_auto_detect: Prop<bool>,
    pub send_feedback_only_if_armed: Prop<bool>,
    pub midi_control_input: Prop<MidiControlInput>,
    pub midi_feedback_output: Prop<Option<MidiFeedbackOutput>>,
    pub main_preset_auto_load_mode: Prop<MainPresetAutoLoadMode>,
    // Is set when in the state of learning multiple mappings ("batch learn")
    learn_many_state: Prop<Option<LearnManyState>>,
    // We want that learn works independently of the UI, so they are session properties.
    mapping_which_learns_source: Prop<Option<SharedMapping>>,
    mapping_which_learns_target: Prop<Option<SharedMapping>>,
    active_controller_id: Option<String>,
    active_main_preset_id: Option<String>,
    context: ProcessorContext,
    mappings: EnumMap<MappingCompartment, Vec<SharedMapping>>,
    everything_changed_subject: LocalSubject<'static, (), ()>,
    mapping_list_changed_subject:
        LocalSubject<'static, (MappingCompartment, Option<MappingId>), ()>,
    mapping_changed_subject: LocalSubject<'static, MappingCompartment, ()>,
    source_touched_subject: LocalSubject<'static, CompoundMappingSource, ()>,
    mapping_subscriptions: EnumMap<MappingCompartment, Vec<SubscriptionGuard<LocalSubscription>>>,
    // It's super important to unregister this when the session is dropped. Otherwise ReaLearn
    // will stay around as a ghost after the plug-in is removed.
    main_processor_registration: Option<RegistrationHandle<MainProcessor<WeakSession>>>,
    normal_main_task_channel: (
        crossbeam_channel::Sender<NormalMainTask>,
        crossbeam_channel::Receiver<NormalMainTask>,
    ),
    parameter_main_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    ui: WrapDebug<Box<dyn SessionUi>>,
    parameter_settings: Vec<ParameterSetting>,
    controller_preset_manager: Box<dyn PresetManager<PresetType = ControllerPreset>>,
    main_preset_manager: Box<dyn PresetManager<PresetType = MainPreset>>,
    main_preset_link_manager: Box<dyn PresetLinkManager>,
    /// The mappings which are on (control or feedback enabled + mapping active + target active)
    on_mappings: Prop<HashSet<MappingId>>,
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

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: String,
        parent_logger: &slog::Logger,
        context: ProcessorContext,
        normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        normal_main_task_channel: (
            crossbeam_channel::Sender<NormalMainTask>,
            crossbeam_channel::Receiver<NormalMainTask>,
        ),
        control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        parameter_main_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
        ui: impl SessionUi + 'static,
        controller_manager: impl PresetManager<PresetType = ControllerPreset> + 'static,
        main_preset_manager: impl PresetManager<PresetType = MainPreset> + 'static,
        preset_link_manager: impl PresetLinkManager + 'static,
    ) -> Session {
        Self {
            // As long not changed (by loading a preset or manually changing session ID), the
            // session ID is equal to the instance ID.
            id: prop(instance_id.clone()),
            instance_id,
            logger: parent_logger.clone(),
            let_matched_events_through: prop(false),
            let_unmatched_events_through: prop(true),
            always_auto_detect: prop(true),
            send_feedback_only_if_armed: prop(true),
            midi_control_input: prop(MidiControlInput::FxInput),
            midi_feedback_output: prop(None),
            main_preset_auto_load_mode: Default::default(),
            learn_many_state: prop(None),
            mapping_which_learns_source: prop(None),
            mapping_which_learns_target: prop(None),
            active_controller_id: None,
            active_main_preset_id: None,
            context,
            mappings: Default::default(),
            everything_changed_subject: Default::default(),
            mapping_list_changed_subject: Default::default(),
            mapping_changed_subject: Default::default(),
            source_touched_subject: Default::default(),
            mapping_subscriptions: Default::default(),
            main_processor_registration: None,
            normal_main_task_channel,
            parameter_main_task_receiver,
            control_main_task_receiver,
            normal_real_time_task_sender,
            feedback_real_time_task_sender,
            party_is_over_subject: Default::default(),
            ui: WrapDebug(Box::new(ui)),
            parameter_settings: vec![Default::default(); PLUGIN_PARAMETER_COUNT as usize],
            controller_preset_manager: Box::new(controller_manager),
            main_preset_manager: Box::new(main_preset_manager),
            main_preset_link_manager: Box::new(preset_link_manager),
            on_mappings: Default::default(),
        }
    }

    pub fn id(&self) -> &str {
        self.id.get_ref()
    }

    pub fn get_parameter_settings(&self, index: u32) -> &ParameterSetting {
        &self.parameter_settings[index as usize]
    }

    pub fn get_parameter_name(&self, index: u32) -> String {
        let setting = &self.parameter_settings[index as usize];
        match &setting.custom_name {
            None => format!("Parameter {}", index + 1),
            Some(n) => n.clone(),
        }
    }

    pub fn set_parameter_settings_without_notification(
        &mut self,
        parameter_settings: Vec<ParameterSetting>,
    ) {
        self.parameter_settings = parameter_settings;
    }

    fn initial_sync(&mut self, weak_session: WeakSession) {
        for compartment in MappingCompartment::into_enum_iter() {
            self.resubscribe_to_mappings(compartment, weak_session.clone());
            self.sync_all_mappings_full(compartment);
        }
        self.sync_settings_to_real_time_processor();
    }

    /// Connects the dots.
    // TODO-low Too large. Split this into several methods.
    pub fn activate(&mut self, weak_session: WeakSession) {
        // Register the main processor. We instantiate it as control surface because it must be
        // called regularly, even when the ReaLearn UI is closed. That means, the VST GUI idle
        // callback is not suited.
        let main_processor = MainProcessor::new(
            &self.logger,
            self.normal_main_task_channel.0.clone(),
            self.normal_main_task_channel.1.clone(),
            self.parameter_main_task_receiver.clone(),
            self.control_main_task_receiver.clone(),
            self.normal_real_time_task_sender.clone(),
            self.feedback_real_time_task_sender.clone(),
            weak_session.clone(),
            self.context.clone(),
        );
        main_processor.activate();
        let reg = Reaper::get()
            .medium_session()
            .plugin_register_add_csurf_inst(Box::new(main_processor))
            .expect("couldn't register local control surface");
        self.main_processor_registration = Some(reg);
        // Initial sync
        self.initial_sync(weak_session.clone());
        // Whenever auto-correct setting changes, resubscribe to all mappings because
        // that saves us some mapping subscriptions.
        when(self.always_auto_detect.changed())
            .with(weak_session.clone())
            .do_async(|shared_session, _| {
                for compartment in MappingCompartment::into_enum_iter() {
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
                .merge(self.containing_fx_enabled_or_disabled())
                .merge(self.containing_track_armed_or_disarmed())
                .merge(self.send_feedback_only_if_armed.changed())
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session.clone())
        .do_async(move |session, _| {
            session.borrow_mut().sync_feedback_is_globally_enabled();
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
        // Keep syncing some general settings to real-time processor.
        when(self.settings_changed())
            .with(weak_session.clone())
            .do_async(move |s, _| {
                s.borrow().sync_settings_to_real_time_processor();
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
                .fx_focused()
                .take_until(self.party_is_over()),
        )
        .with(weak_session)
        .do_sync(|s, fx| {
            if s.borrow().main_preset_auto_load_mode.get() == MainPresetAutoLoadMode::FocusedFx {
                let fx_id = fx.as_ref().map(FxId::from_fx);
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
            self.activate_main_preset(None, session);
        }
        self.main_preset_auto_load_mode.set(mode);
    }

    pub fn main_preset_auto_load_is_active(&self) -> bool {
        self.main_preset_auto_load_mode.get() != MainPresetAutoLoadMode::Off
    }

    fn auto_load_preset_linked_to_fx(&mut self, fx_id: Option<FxId>, weak_session: WeakSession) {
        if let Some(fx_id) = fx_id {
            let preset_id = self
                .main_preset_link_manager
                .find_preset_linked_to_fx(&fx_id);
            self.activate_main_preset(preset_id, weak_session);
        } else {
            self.activate_main_preset(None, weak_session);
        }
    }

    fn invalidate_fx_indexes_of_mapping_targets(&self) {
        for m in self.all_mappings() {
            m.borrow_mut()
                .target_model
                .invalidate_fx_index(&self.context);
        }
    }

    /// Settings are all the things displayed in the ReaLearn header panel.
    fn settings_changed(&self) -> impl UnitEvent {
        self.let_matched_events_through
            .changed()
            .merge(self.let_unmatched_events_through.changed())
            .merge(self.midi_control_input.changed())
            .merge(self.midi_feedback_output.changed())
            .merge(self.always_auto_detect.changed())
            .merge(self.send_feedback_only_if_armed.changed())
            .merge(self.main_preset_auto_load_mode.changed())
    }

    pub fn learn_source(&mut self, source: CompoundMappingSource) {
        self.source_touched_subject.next(source);
    }

    pub fn source_touched(
        &self,
        compartment: MappingCompartment,
        reenable_control_after_touched: bool,
    ) -> impl Event<CompoundMappingSource> {
        // TODO-low Would be nicer to do this on subscription instead of immediately. from_fn()?
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::StartLearnSource(compartment))
            .unwrap();
        let rt_sender = self.normal_real_time_task_sender.clone();
        self.source_touched_subject.clone().finalize(move || {
            if reenable_control_after_touched {
                rt_sender
                    .send(NormalRealTimeTask::ReturnToControlMode)
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
                let mut all_subscriptions = LocalSubscription::default();
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
                // Keep auto-detecting mode settings
                if self.always_auto_detect.get() {
                    let processor_context = self.context().clone();
                    let subscription = when(
                        mapping
                            .source_model
                            .changed()
                            .merge(mapping.target_model.changed()),
                    )
                    .with(Rc::downgrade(&shared_mapping))
                    .do_sync(move |mapping, _| {
                        mapping
                            .borrow_mut()
                            .adjust_mode_if_necessary(&processor_context);
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
            if p != self.context.project() {
                return;
            }
        }
        if let Some(mapping) = self.mapping_which_learns_target.replace(None) {
            mapping
                .borrow_mut()
                .target_model
                .apply_from_target(target, &self.context);
        }
    }

    pub fn context(&self) -> &ProcessorContext {
        &self.context
    }

    pub fn add_default_mapping(
        &mut self,
        compartment: MappingCompartment,
        // Only relevant for controller mapping compartment
        control_element_type: VirtualControlElementType,
    ) -> SharedMapping {
        let mut mapping = MappingModel::new(compartment);
        mapping
            .name
            .set_without_notification(self.generate_name_for_new_mapping(compartment));
        if compartment == MappingCompartment::ControllerMappings {
            let next_control_element_index =
                self.get_next_control_element_index(control_element_type);
            let target_model = TargetModel {
                category: prop(TargetCategory::Virtual),
                control_element_type: prop(control_element_type),
                control_element_index: prop(next_control_element_index),
                ..Default::default()
            };
            mapping.target_model = target_model;
        }
        self.add_mapping(compartment, mapping)
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
                Some(target.control_element_index.get())
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
        // Only relevant for controller mapping compartment
        control_element_type: VirtualControlElementType,
    ) {
        // Prepare
        self.disable_control();
        self.stop_learning_source();
        self.stop_learning_target();
        // Add initial mapping and start learning its source
        self.add_and_learn_one_of_many_mappings(session, compartment, control_element_type);
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
                control_element_type,
            );
        });
    }

    fn add_and_learn_one_of_many_mappings(
        &mut self,
        session: &SharedSession,
        compartment: MappingCompartment,
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
        let mapping = self.add_default_mapping(compartment, control_element_type);
        let mapping_id = mapping.borrow().id();
        self.learn_many_state
            .set(Some(LearnManyState::learning_source(
                compartment,
                mapping_id,
                control_element_type,
            )));
        self.start_learning_source(
            Rc::downgrade(session),
            mapping.clone(),
            false,
            ignore_sources,
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
                        mapping_id,
                    )));
                session.start_learning_target(
                    Rc::downgrade(&shared_session),
                    mapping.clone(),
                    false,
                );
            });
        }
    }

    pub fn stop_learning_many_mappings(&mut self) {
        self.learn_many_state.set(None);
        let source_learning_mapping = self.mapping_which_learns_source.get_ref().clone();
        self.stop_learning_source();
        self.stop_learning_target();
        self.enable_control();
        // Remove last added mapping if source not learned already
        if let Some(mapping) = source_learning_mapping {
            self.remove_mapping(mapping.borrow().compartment(), mapping.as_ptr());
        }
    }

    pub fn learn_many_state_changed(&self) -> impl UnitEvent {
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

    pub fn find_mapping_by_address(
        &self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
    ) -> Option<&SharedMapping> {
        self.mappings(compartment)
            .find(|m| m.as_ptr() == mapping as _)
    }

    pub fn find_mapping_by_id(
        &self,
        compartment: MappingCompartment,
        mapping_id: MappingId,
    ) -> Option<&SharedMapping> {
        self.mappings(compartment)
            .find(|m| m.borrow().id() == mapping_id)
    }

    pub fn mappings(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &SharedMapping> {
        self.mappings[compartment].iter()
    }

    fn all_mappings(&self) -> impl Iterator<Item = &SharedMapping> {
        MappingCompartment::into_enum_iter()
            .map(move |compartment| self.mappings(compartment))
            .flatten()
    }

    pub fn mapping_is_learning_source(&self, mapping: *const MappingModel) -> bool {
        match self.mapping_which_learns_source.get_ref() {
            None => false,
            Some(m) => m.as_ptr() == mapping as _,
        }
    }

    pub fn mapping_is_learning_target(&self, mapping: *const MappingModel) -> bool {
        match self.mapping_which_learns_target.get_ref() {
            None => false,
            Some(m) => m.as_ptr() == mapping as _,
        }
    }

    /// Resets the session ID to the (hopefully) always unique instance ID.
    pub fn reset_id(&mut self) {
        self.id.set(self.instance_id.clone());
    }

    pub fn mapping_which_learns_source_changed(&self) -> impl UnitEvent {
        self.mapping_which_learns_source.changed()
    }

    pub fn mapping_which_learns_target_changed(&self) -> impl UnitEvent {
        self.mapping_which_learns_target.changed()
    }

    pub fn toggle_learning_source(&mut self, session: &SharedSession, mapping: &SharedMapping) {
        if self.mapping_which_learns_source.get_ref().is_none() {
            self.start_learning_source(
                Rc::downgrade(session),
                mapping.clone(),
                true,
                HashSet::new(),
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
    ) {
        self.mapping_which_learns_source.set(Some(mapping.clone()));
        when(
            self.source_touched(
                mapping.borrow().compartment(),
                reenable_control_after_touched,
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
            if let Some(m) = session.borrow().mapping_which_learns_source.get_ref() {
                m.borrow_mut().source_model.apply_from_source(&source);
            }
        });
    }

    fn stop_learning_source(&mut self) {
        self.mapping_which_learns_source.set(None);
    }

    pub fn toggle_learning_target(&mut self, session: &SharedSession, mapping: &SharedMapping) {
        if self.mapping_which_learns_target.get_ref().is_none() {
            self.start_learning_target(Rc::downgrade(session), mapping.clone(), true);
        } else {
            self.stop_learning_target();
        }
    }

    fn start_learning_target(
        &mut self,
        session: WeakSession,
        mapping: SharedMapping,
        handle_control_disabling: bool,
    ) {
        self.mapping_which_learns_target.set(Some(mapping));
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
    }

    fn enable_control(&self) {
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::ReturnToControlMode)
            .unwrap();
    }

    fn stop_learning_target(&mut self) {
        self.mapping_which_learns_target.set(None);
    }

    pub fn move_mapping_up(
        &mut self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
    ) {
        // No problem if it doesn't work
        let _ = self.swap_mappings(compartment, mapping, -1);
    }

    pub fn move_mapping_down(
        &mut self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
    ) {
        // No problem if it doesn't work
        let _ = self.swap_mappings(compartment, mapping, 1);
    }

    fn swap_mappings(
        &mut self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
        increment: isize,
    ) -> Result<(), &str> {
        let current_index = self.mappings[compartment]
            .iter()
            .position(|m| m.as_ptr() == mapping as _)
            .ok_or("mapping not found")?;
        let new_index = current_index as isize + increment;
        if new_index < 0 {
            return Err("too far up");
        }
        let new_index = new_index as usize;
        if new_index >= self.mappings[compartment].len() {
            return Err("too far down");
        }
        self.mappings[compartment].swap(current_index, new_index);
        self.notify_mapping_list_changed(compartment, None);
        Ok(())
    }

    pub fn remove_mapping(
        &mut self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
    ) {
        self.mappings[compartment].retain(|m| m.as_ptr() != mapping as _);
        self.notify_mapping_list_changed(compartment, None);
    }

    pub fn duplicate_mapping(
        &mut self,
        compartment: MappingCompartment,
        mapping: *const MappingModel,
    ) -> Result<(), &str> {
        let (index, mapping) = self.mappings[compartment]
            .iter()
            .enumerate()
            .find(|(_i, m)| m.as_ptr() == mapping as _)
            .ok_or("mapping not found")?;
        let duplicate = {
            let mapping = mapping.borrow();
            let mut duplicate = mapping.duplicate();
            duplicate
                .name
                .set(format!("Copy of {}", mapping.name.get_ref()));
            duplicate
        };
        let duplicate_id = duplicate.id();
        self.mappings[compartment].insert(index + 1, share_mapping(duplicate));
        self.notify_mapping_list_changed(compartment, Some(duplicate_id));
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
        MappingCompartment::into_enum_iter().find_map(|compartment| {
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
        self.active_controller_id = active_controller_id;
    }

    pub fn set_active_main_preset_id_without_notification(
        &mut self,
        active_main_preset_id: Option<String>,
    ) {
        self.active_main_preset_id = active_main_preset_id;
    }

    pub fn active_controller_id(&self) -> Option<&str> {
        self.active_controller_id.as_deref()
    }

    pub fn active_main_preset_id(&self) -> Option<&str> {
        self.active_main_preset_id.as_deref()
    }

    pub fn active_controller(&self) -> Option<ControllerPreset> {
        let id = self.active_controller_id()?;
        self.controller_preset_manager.find_by_id(id)
    }

    pub fn active_main_preset(&self) -> Option<MainPreset> {
        let id = self.active_main_preset_id()?;
        self.main_preset_manager.find_by_id(id)
    }

    pub fn controller_mappings_are_dirty(&self) -> bool {
        let id = match &self.active_controller_id {
            None => return self.mapping_count(MappingCompartment::ControllerMappings) > 0,
            Some(id) => id,
        };
        self.controller_preset_manager
            .mappings_are_dirty(id, &self.mappings[MappingCompartment::ControllerMappings])
    }

    pub fn main_mappings_are_dirty(&self) -> bool {
        let id = match &self.active_main_preset_id {
            None => return self.mapping_count(MappingCompartment::MainMappings) > 0,
            Some(id) => id,
        };
        self.main_preset_manager
            .mappings_are_dirty(id, &self.mappings[MappingCompartment::MainMappings])
    }

    pub fn activate_controller(
        &mut self,
        id: Option<String>,
        weak_session: WeakSession,
    ) -> Result<(), &'static str> {
        self.active_controller_id = id.clone();
        self.activate_preset(
            MappingCompartment::ControllerMappings,
            id,
            weak_session,
            |session, id| {
                let controller = session
                    .controller_preset_manager
                    .find_by_id(id)
                    .ok_or("controller not found")?;
                Ok(controller.mappings().clone())
            },
        )
    }

    pub fn activate_main_preset(
        &mut self,
        id: Option<String>,
        weak_session: WeakSession,
    ) -> Result<(), &'static str> {
        self.active_main_preset_id = id.clone();
        self.activate_preset(
            MappingCompartment::MainMappings,
            id,
            weak_session,
            |session, id| {
                let main_preset = session
                    .main_preset_manager
                    .find_by_id(id)
                    .ok_or("main preset not found")?;
                Ok(main_preset.mappings().clone())
            },
        )
    }

    fn activate_preset(
        &mut self,
        compartment: MappingCompartment,
        id: Option<String>,
        weak_session: WeakSession,
        get_mappings_by_preset_id: impl FnOnce(
            &Session,
            &str,
        ) -> Result<Vec<MappingModel>, &'static str>,
    ) -> Result<(), &'static str> {
        match id.as_ref() {
            None => {
                self.set_mappings_without_notification(compartment, std::iter::empty());
            }
            Some(id) => {
                let mappings = get_mappings_by_preset_id(self, id)?;
                self.set_mappings_without_notification(compartment, mappings.into_iter());
            }
        };
        self.notify_everything_has_changed(weak_session);
        Ok(())
    }

    fn containing_fx_enabled_or_disabled(&self) -> impl UnitEvent {
        let containing_fx = self.context.containing_fx().clone();
        Global::control_surface_rx()
            .fx_enabled_changed()
            .filter(move |fx| *fx == containing_fx)
            .map_to(())
    }

    fn containing_track_armed_or_disarmed(&self) -> BoxedUnitEvent {
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
    pub fn everything_changed(&self) -> impl UnitEvent {
        self.everything_changed_subject.clone()
    }

    /// Fires if a mapping has been added, removed or changed its position in the list.
    ///
    /// Doesn't fire if a mapping in the list or if the complete list has changed.
    pub fn mapping_list_changed(
        &self,
    ) -> impl SharedItemEvent<(MappingCompartment, Option<MappingId>)> {
        self.mapping_list_changed_subject.clone()
    }

    /// Fires if a mapping itself has been changed.
    pub fn mapping_changed(&self) -> impl SharedItemEvent<MappingCompartment> {
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

    pub fn send_feedback(&self) {
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::FeedbackAll)
            .unwrap();
    }

    pub fn log_debug_info(&self) {
        self.log_debug_info_internal();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::LogDebugInfo)
            .unwrap();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::LogDebugInfo)
            .unwrap();
    }

    pub fn mapping_is_on(&self, id: MappingId) -> bool {
        self.on_mappings.get_ref().contains(&id)
    }

    pub fn on_mappings_changed(&self) -> impl UnitEvent {
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
            - Main mapping model count: {}\n\
            - Main mapping subscription count: {}\n\
            - Controller mapping model count: {}\n\
            - Controller mapping subscription count: {}\n\
            ",
            self.instance_id,
            self.id.get_ref(),
            self.mappings[MappingCompartment::MainMappings].len(),
            self.mapping_subscriptions[MappingCompartment::MainMappings].len(),
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
        self.mappings(compartment)
            .find(|m| m.borrow().with_context(&self.context).has_target(target))
    }

    pub fn toggle_learn_source_for_target(
        &mut self,
        session: &SharedSession,
        compartment: MappingCompartment,
        target: &ReaperTarget,
    ) -> SharedMapping {
        let mapping = match self.find_mapping_with_target(compartment, target) {
            None => {
                let m = self.add_default_mapping(compartment, VirtualControlElementType::Multi);
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

    pub fn show_mapping(&self, mapping: *const MappingModel) {
        self.ui.show_mapping(mapping);
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

    /// Notifies listeners async a mapping in a mapping list has changed.
    fn notify_mapping_changed(&mut self, compartment: MappingCompartment) {
        AsyncNotifier::notify(&mut self.mapping_changed_subject, &compartment);
    }

    fn sync_settings_to_real_time_processor(&self) {
        let task = NormalRealTimeTask::UpdateSettings {
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
            midi_control_input: self.midi_control_input.get(),
            midi_feedback_output: self.midi_feedback_output.get(),
        };
        self.normal_real_time_task_sender.send(task).unwrap();
    }

    fn sync_single_mapping_to_processors(&self, compartment: MappingCompartment, m: &MappingModel) {
        let main_mapping = m.create_main_mapping();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateSingleMapping(
                compartment,
                Box::new(main_mapping),
            ))
            .unwrap();
    }

    fn feedback_is_globally_enabled(&self) -> bool {
        self.midi_feedback_output.get().is_some()
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

    /// Just syncs whether feedback globally enabled or not.
    fn sync_feedback_is_globally_enabled(&self) {
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateFeedbackIsGloballyEnabled(
                self.feedback_is_globally_enabled(),
            ))
            .unwrap();
    }

    /// Does a full mapping sync.
    fn sync_all_mappings_full(&self, compartment: MappingCompartment) {
        let main_mappings = self.create_main_mappings(compartment);
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateAllMappings(
                compartment,
                main_mappings,
            ))
            .unwrap();
    }

    /// Creates mappings from mapping models so they can be distributed to different processors.
    fn create_main_mappings(&self, compartment: MappingCompartment) -> Vec<MainMapping> {
        self.mappings(compartment)
            .map(|m| m.borrow().create_main_mapping())
            .collect()
    }

    fn generate_name_for_new_mapping(&self, compartment: MappingCompartment) -> String {
        format!("{}", self.mappings[compartment].len() + 1)
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }

    /// Shouldn't be called on load (project load, undo, redo, preset change).
    pub fn mark_project_as_dirty(&self) {
        debug!(self.logger, "Marking project as dirty");
        self.context.project().mark_as_dirty();
    }

    pub fn logger(&self) -> &slog::Logger {
        &self.logger
    }

    /// Does a full resync and notifies the UI async.
    ///
    /// Explicitly doesn't mark the project as dirty - because this is also used when loading data
    /// (project load, undo, redo, preset change).
    pub fn notify_everything_has_changed(&mut self, weak_session: WeakSession) {
        self.initial_sync(weak_session);
        // Not sure why this is not included in initial sync
        self.sync_feedback_is_globally_enabled();
        // For UI
        AsyncNotifier::notify(&mut self.everything_changed_subject, &());
    }
}

#[derive(Clone, Debug, Default)]
pub struct ParameterSetting {
    pub custom_name: Option<String>,
}

impl Drop for Session {
    fn drop(&mut self) {
        debug!(self.logger(), "Dropping session...");
        if let Some(reg) = self.main_processor_registration {
            unsafe {
                // We can throw the unregistered control surface immediately because we are sure
                // that we are currently not in a control surface call.
                let _ = Reaper::get()
                    .medium_session()
                    .plugin_register_remove_csurf_inst(reg);
            }
        }
        self.party_is_over_subject.next(());
    }
}

impl DomainEventHandler for WeakSession {
    fn handle_event(&self, event: DomainEvent) {
        let session = self.upgrade().expect("session not existing anymore");
        let mut session = session.borrow_mut();
        use DomainEvent::*;
        match event {
            LearnedSource(source) => {
                session.learn_source(source);
            }
            UpdatedOnMappings(on_mappings) => {
                session.on_mappings.set(on_mappings);
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

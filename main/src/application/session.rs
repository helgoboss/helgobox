use crate::core::{prop, when, AsyncNotifier, Prop};
use crate::domain::{
    ControlMainTask, DomainEvent, DomainEventHandler, FeedbackRealTimeTask, MainProcessor,
    MidiControlInput, MidiFeedbackOutput, NormalMainMapping, NormalMainTask, NormalMappingSource,
    NormalRealTimeMapping, NormalRealTimeTask, ReaperTarget, PLUGIN_PARAMETER_COUNT,
};
use helgoboss_learn::MidiSource;

use crate::application::{
    session_manager, share_mapping, MappingModel, SessionContext, SharedMapping, TargetModel,
};
use reaper_high::Reaper;
use reaper_medium::RegistrationHandle;
use rx_util::{BoxedUnitEvent, Event, Notifier, SharedPayload, UnitEvent};
use rxrust::prelude::ops::box_it::LocalBoxOp;
use rxrust::prelude::*;
use slog::debug;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::{Rc, Weak};
use wrap_debug::WrapDebug;

pub trait SessionUi {
    fn show_mapping(&self, mapping: *const MappingModel);
}

/// This represents the user session with one ReaLearn instance.
///
/// It's ReaLearn's main object which keeps everything together.
// TODO-low Probably belongs in application layer.
#[derive(Debug)]
pub struct Session {
    pub let_matched_events_through: Prop<bool>,
    pub let_unmatched_events_through: Prop<bool>,
    pub always_auto_detect: Prop<bool>,
    pub send_feedback_only_if_armed: Prop<bool>,
    pub midi_control_input: Prop<MidiControlInput>,
    pub midi_feedback_output: Prop<Option<MidiFeedbackOutput>>,
    // We want that learn works independently of the UI, so they are session properties.
    pub mapping_which_learns_source: Prop<Option<SharedMapping>>,
    pub mapping_which_learns_target: Prop<Option<SharedMapping>>,
    context: SessionContext,
    mapping_models: Vec<SharedMapping>,
    everything_changed_subject: LocalSubject<'static, (), ()>,
    mapping_list_changed_subject: LocalSubject<'static, (), ()>,
    source_touched_subject: LocalSubject<'static, NormalMappingSource, ()>,
    mapping_subscriptions: Vec<SubscriptionGuard<LocalSubscription>>,
    // It's super important to unregister this when the session is dropped. Otherwise ReaLearn
    // will stay around as a ghost after the plug-in is removed.
    main_processor_registration: Option<RegistrationHandle<MainProcessor<WeakSession>>>,
    normal_main_task_channel: (
        crossbeam_channel::Sender<NormalMainTask>,
        crossbeam_channel::Receiver<NormalMainTask>,
    ),
    control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    ui: WrapDebug<Box<dyn SessionUi>>,
    parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
    parameter_settings: Vec<ParameterSetting>,
}

impl Session {
    pub fn new(
        context: SessionContext,
        normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        normal_main_task_channel: (
            crossbeam_channel::Sender<NormalMainTask>,
            crossbeam_channel::Receiver<NormalMainTask>,
        ),
        control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        ui: impl SessionUi + 'static,
    ) -> Session {
        Self {
            let_matched_events_through: prop(false),
            let_unmatched_events_through: prop(true),
            always_auto_detect: prop(true),
            send_feedback_only_if_armed: prop(true),
            midi_control_input: prop(MidiControlInput::FxInput),
            midi_feedback_output: prop(None),
            mapping_which_learns_source: prop(None),
            mapping_which_learns_target: prop(None),
            context,
            mapping_models: vec![],
            everything_changed_subject: Default::default(),
            mapping_list_changed_subject: Default::default(),
            source_touched_subject: Default::default(),
            mapping_subscriptions: vec![],
            main_processor_registration: None,
            normal_main_task_channel,
            control_main_task_receiver,
            normal_real_time_task_sender,
            feedback_real_time_task_sender,
            party_is_over_subject: Default::default(),
            ui: WrapDebug(Box::new(ui)),
            parameters: [0.0; PLUGIN_PARAMETER_COUNT as usize],
            parameter_settings: vec![Default::default(); PLUGIN_PARAMETER_COUNT as usize],
        }
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

    pub fn get_parameter(&self, index: u32) -> f32 {
        self.parameters[index as usize]
    }

    pub fn set_parameter(&mut self, index: u32, value: f32) {
        self.parameters[index as usize] = value;
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateParameter { index, value })
            .unwrap();
    }

    pub fn set_parameter_settings_without_notification(
        &mut self,
        parameter_settings: Vec<ParameterSetting>,
    ) {
        self.parameter_settings = parameter_settings;
    }

    pub fn set_parameters_without_notification(
        &mut self,
        parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
    ) {
        self.parameters = parameters;
    }

    /// Connects the dots.
    // TODO-low Too large. Split this into several methods.
    pub fn activate(&mut self, weak_session: WeakSession) {
        // Register the main processor. We instantiate it as control surface because it must be
        // called regularly, even when the ReaLearn UI is closed. That means, the VST GUI idle
        // callback is not suited.
        let reg = Reaper::get()
            .medium_session()
            .plugin_register_add_csurf_inst(Box::new(MainProcessor::new(
                self.normal_main_task_channel.1.clone(),
                self.control_main_task_receiver.clone(),
                self.normal_real_time_task_sender.clone(),
                self.feedback_real_time_task_sender.clone(),
                self.parameters,
                weak_session.clone(),
            )))
            .expect("couldn't register local control surface");
        self.main_processor_registration = Some(reg);
        // Whenever something in the mapping list changes, resubscribe to mappings themselves.
        when(
            // Initial sync
            observable::of(())
                // Future syncs
                // When the mapping list changes.
                .merge(self.mapping_list_changed())
                // When auto-detect is off, we can save some mapping descriptions
                .merge(self.always_auto_detect.changed()),
        )
        .with(weak_session.clone())
        .do_async(|shared_session, _| {
            shared_session
                .borrow_mut()
                .resubscribe_to_mappings_in_current_list(Rc::downgrade(&shared_session));
        });
        // Whenever anything in the mapping list changes and other things which affect all
        // processor (including the real-time processor which takes care of sources only), resync
        // all mappings to *all* processors.
        when(
            // Initial sync
            observable::of(())
                // Future syncs
                // When the mapping list changes.
                .merge(self.mapping_list_changed()),
        )
        .with(weak_session.clone())
        .do_async(move |session, _| {
            session.borrow_mut().sync_all_mappings_full();
        });
        // Whenever anything changes that just affects the main processor targets, resync all
        // targets to the main processor. We don't want to resync to the real-time processor
        // just because another track has been selected. First, it would reset any source state
        // (e.g. short/long press timers). Second, it wouldn't change anything about the sources.
        // We also don't want to resync modes to the main processor. First, it would reset any
        // mode state (e.g. throttling data). Second, it would - again - not result in any change.
        when(
            // There are several global conditions which affect whether feedback will be sent
            // from a target or not. Similar global conditions decide what exactly produces the
            // feedback values (e.g. when there's a target which uses <Selected track>,
            // then a track selection change changes the feedback value producer ... so
            // the main processor needs to unsubscribe from the old producer and
            // subscribe to the new one).
            TargetModel::potential_static_change_events()
                .merge(TargetModel::potential_dynamic_change_events())
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session.clone())
        .do_async(move |session, _| {
            session.borrow_mut().sync_all_mappings_light();
        });
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
        when(self.settings_changed().merge(self.mapping_list_changed()))
            .with(weak_session.clone())
            .do_sync(move |s, _| {
                s.borrow().mark_project_as_dirty();
            });
        // Keep syncing some general settings to real-time processor.
        when(
            // Initial sync
            observable::of(())
                // Future syncs
                .merge(self.settings_changed()),
        )
        .with(weak_session.clone())
        .do_async(move |s, _| {
            s.borrow().sync_settings_to_real_time_processor();
        });
        // When FX is reordered, invalidate FX indexes. This is primarily for the GUI.
        // Existing GUID-tracked `Fx` instances will detect wrong index automatically.
        when(
            Reaper::get()
                .fx_reordered()
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session.clone())
        .do_sync(move |s, _| {
            s.borrow().invalidate_fx_indexes_of_mapping_targets();
        });
        // Enable source learning
        // TODO-low This could be handled by normal methods instead of observables, like source
        //  filter learning.
        when(
            // TODO-low Listen to values instead of change event only. Filter Some only and
            // flatten.
            self.mapping_which_learns_source.changed(),
        )
        .with(weak_session.clone())
        .do_async(move |shared_session, _| {
            let session = shared_session.borrow();
            if session.mapping_which_learns_source.get_ref().is_none() {
                return;
            }
            when(
                session
                    .source_touched()
                    // We have this explicit stop criteria because we listen to global REAPER
                    // events.
                    .take_until(session.party_is_over())
                    .take_until(session.mapping_which_learns_source.changed_to(None))
                    .take(1),
            )
            .with(Rc::downgrade(&shared_session))
            .finally(|session| session.borrow_mut().mapping_which_learns_source.set(None))
            .do_async(|session, source| {
                if let Some(m) = session.borrow().mapping_which_learns_source.get_ref() {
                    m.borrow_mut().source_model.apply_from_source(&source);
                }
            });
        });
        // Enable target learning
        when(
            self.target_touched_observables(weak_session.clone())
                .switch_on_next()
                // We have this explicit stop criteria because we listen to global REAPER events.
                .take_until(self.party_is_over()),
        )
        .with(weak_session)
        .do_async(|session, target| {
            session.borrow_mut().learn_target(target.as_ref());
        });
    }

    fn invalidate_fx_indexes_of_mapping_targets(&self) {
        for m in self.mappings() {
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
    }

    pub fn learn_source(&mut self, source: NormalMappingSource) {
        self.source_touched_subject.next(source);
    }

    pub fn source_touched(&self) -> impl Event<NormalMappingSource> {
        // TODO-low Would be nicer to do this on subscription instead of immediately. from_fn()?
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::StartLearnSource)
            .unwrap();
        let rt_sender = self.normal_real_time_task_sender.clone();
        self.source_touched_subject.clone().finalize(move || {
            rt_sender.send(NormalRealTimeTask::StopLearnSource).unwrap();
        })
    }

    fn resubscribe_to_mappings_in_current_list(&mut self, weak_session: WeakSession) {
        self.mapping_subscriptions = self
            .mapping_models
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
                            let session = session.borrow();
                            session
                                .sync_single_mapping_to_processors(&shared_mapping_clone.borrow());
                            session.mark_project_as_dirty();
                        });
                    all_subscriptions.add(subscription);
                }
                // Keep marking project as dirty
                {
                    let subscription = when(mapping.changed_non_processing_relevant())
                        .with(weak_session.clone())
                        .do_sync(|session, _| {
                            session.borrow().mark_project_as_dirty();
                        });
                    all_subscriptions.add(subscription);
                }
                // Keep auto-detecting mode settings
                if self.always_auto_detect.get() {
                    let session_context = self.context().clone();
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
                            .adjust_mode_if_necessary(&session_context);
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

    pub fn context(&self) -> &SessionContext {
        &self.context
    }

    pub fn add_default_mapping(&mut self) -> SharedMapping {
        let mut mapping = MappingModel::default();
        mapping.name.set(self.generate_name_for_new_mapping());
        self.add_mapping(mapping)
    }

    pub fn mapping_count(&self) -> usize {
        self.mapping_models.len()
    }

    pub fn find_mapping_by_index(&self, index: usize) -> Option<&SharedMapping> {
        self.mapping_models.get(index)
    }

    pub fn find_mapping_by_address(&self, mapping: *const MappingModel) -> Option<&SharedMapping> {
        self.mappings().find(|m| m.as_ptr() == mapping as _)
    }

    pub fn mappings(&self) -> impl Iterator<Item = &SharedMapping> {
        self.mapping_models.iter()
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

    pub fn toggle_learn_source(&mut self, mapping: &SharedMapping) {
        toggle_learn(&mut self.mapping_which_learns_source, mapping);
    }

    pub fn toggle_learn_target(&mut self, mapping: &SharedMapping) {
        toggle_learn(&mut self.mapping_which_learns_target, mapping);
    }

    pub fn move_mapping_up(&mut self, mapping: *const MappingModel) {
        // No problem if it doesn't work
        let _ = self.swap_mappings(mapping, -1);
    }

    pub fn move_mapping_down(&mut self, mapping: *const MappingModel) {
        // No problem if it doesn't work
        let _ = self.swap_mappings(mapping, 1);
    }

    fn swap_mappings(
        &mut self,
        mapping: *const MappingModel,
        increment: isize,
    ) -> Result<(), &str> {
        let current_index = self
            .mapping_models
            .iter()
            .position(|m| m.as_ptr() == mapping as _)
            .ok_or("mapping not found")?;
        let new_index = current_index as isize + increment;
        if new_index < 0 {
            return Err("too far up");
        }
        let new_index = new_index as usize;
        if new_index >= self.mapping_models.len() {
            return Err("too far down");
        }
        self.mapping_models.swap(current_index, new_index);
        self.notify_mapping_list_changed();
        Ok(())
    }

    pub fn remove_mapping(&mut self, mapping: *const MappingModel) {
        self.mapping_models.retain(|m| m.as_ptr() != mapping as _);
        self.notify_mapping_list_changed();
    }

    pub fn duplicate_mapping(&mut self, mapping: *const MappingModel) -> Result<(), &str> {
        let (index, mapping) = self
            .mapping_models
            .iter()
            .enumerate()
            .find(|(_i, m)| m.as_ptr() == mapping as _)
            .ok_or("mapping not found")?;
        let duplicate = {
            let mapping = mapping.borrow();
            let mut duplicate = mapping.clone();
            duplicate
                .name
                .set(format!("Copy of {}", mapping.name.get_ref()));
            duplicate
        };
        self.mapping_models
            .insert(index + 1, share_mapping(duplicate));
        self.notify_mapping_list_changed();
        Ok(())
    }

    pub fn has_mapping(&self, mapping: *const MappingModel) -> bool {
        self.mapping_models
            .iter()
            .any(|m| m.as_ptr() == mapping as _)
    }

    pub fn index_of_mapping(&self, mapping: *const MappingModel) -> Option<usize> {
        self.mapping_models
            .iter()
            .position(|m| m.as_ptr() == mapping as _)
    }

    pub fn show_in_floating_window(&self) {
        self.context().containing_fx().show_in_floating_window();
    }

    pub fn containing_fx_is_in_input_fx_chain(&self) -> bool {
        self.context.containing_fx().is_input_fx()
    }

    fn containing_fx_enabled_or_disabled(&self) -> impl UnitEvent {
        let containing_fx = self.context.containing_fx().clone();
        Reaper::get()
            .fx_enabled_changed()
            .filter(move |fx| *fx == containing_fx)
            .map_to(())
    }

    fn containing_track_armed_or_disarmed(&self) -> BoxedUnitEvent {
        if let Some(track) = self.context.containing_fx().track().cloned() {
            Reaper::get()
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
    pub fn mapping_list_changed(&self) -> impl UnitEvent {
        self.mapping_list_changed_subject.clone()
    }

    /// Omits observables that omit touched targets as long as target learn is enabled.
    // TODO-low Why not handle this in a more simple way? Like learning target filter.
    //  That way we get rid of the switch_on_next() which is not part of the main rxRust
    //  distribution  because we haven't fully implemented it yet.
    fn target_touched_observables(
        &self,
        weak_session: WeakSession,
    ) -> impl Event<LocalBoxOp<'static, Rc<ReaperTarget>, ()>> {
        self.mapping_which_learns_target.changed().map(move |_| {
            let shared_session = weak_session
                .upgrade()
                .expect("session not existing anymore");
            let session = shared_session.borrow();
            match session.mapping_which_learns_target.get_ref() {
                None => observable::empty().box_it(),
                Some(_) => ReaperTarget::touched().box_it(),
            }
        })
    }

    pub fn set_mappings_without_notification(
        &mut self,
        mappings: impl Iterator<Item = MappingModel>,
    ) {
        self.mapping_models = mappings.map(share_mapping).collect();
    }

    fn add_mapping(&mut self, mapping: MappingModel) -> SharedMapping {
        let shared_mapping = share_mapping(mapping);
        self.mapping_models.push(shared_mapping.clone());
        self.notify_mapping_list_changed();
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
        session_manager::log_debug_info();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::LogDebugInfo)
            .unwrap();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::LogDebugInfo)
            .unwrap();
    }

    fn log_debug_info_internal(&self) {
        let msg = format!(
            "\n\
            # Session\n\
            \n\
            - Mapping model count: {}\n\
            - Mapping subscription count: {}\n\
            ",
            self.mapping_models.len(),
            self.mapping_subscriptions.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    pub fn find_mapping_with_target(&self, target: &ReaperTarget) -> Option<&SharedMapping> {
        self.mappings()
            .find(|m| m.borrow().with_context(&self.context).has_target(target))
    }

    pub fn toggle_learn_source_for_target(&mut self, target: &ReaperTarget) -> SharedMapping {
        let mapping = match self.find_mapping_with_target(target) {
            None => {
                let m = self.add_default_mapping();
                m.borrow_mut()
                    .target_model
                    .apply_from_target(target, &self.context);
                m
            }
            Some(m) => m.clone(),
        };
        self.toggle_learn_source(&mapping);
        mapping
    }

    pub fn show_mapping(&self, mapping: *const MappingModel) {
        self.ui.show_mapping(mapping);
    }

    /// Notifies listeners async that something in the mapping list has changed.
    ///
    /// Shouldn't be used if the complete list has changed.
    fn notify_mapping_list_changed(&mut self) {
        AsyncNotifier::notify(&mut self.mapping_list_changed_subject, &());
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

    fn sync_single_mapping_to_processors(&self, m: &MappingModel) {
        let processor_mapping = m
            .with_context(&self.context)
            .create_processor_mapping(&self.parameters);
        let splintered = processor_mapping.splinter();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateSingleMapping(Box::new(splintered.1)))
            .unwrap();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateSingleNormalMapping(splintered.0))
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

    /// Just syncs mapping enabled/disabled states and targets.
    ///
    /// Usually invoked whenever target conditions have changed, e.g. track selection.
    fn sync_all_mappings_light(&self) {
        let splintered = self.create_and_splinter_mappings();
        let main_target_updates = splintered
            .main
            .into_iter()
            .map(|m| m.into_main_processor_target_update())
            .collect();
        let mappings_with_active_targets = splintered
            .real_time
            .into_iter()
            .filter(|m| m.target_is_active())
            .map(|m| m.id())
            .collect();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::EnableMappingsExclusively(
                mappings_with_active_targets,
            ))
            .unwrap();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateAllTargets(main_target_updates))
            .unwrap();
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

    fn sync_all_parameters(&self) {
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateAllParameters(self.parameters))
            .unwrap();
    }

    /// Does a full mapping sync.
    fn sync_all_mappings_full(&self) {
        let splintered = self.create_and_splinter_mappings();
        self.normal_main_task_channel
            .0
            .send(NormalMainTask::UpdateAllMappings(splintered.main))
            .unwrap();
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateAllNormalMappings(
                splintered.real_time,
            ))
            .unwrap();
    }

    /// Creates mappings from mapping models and splits them into different lists so they can be
    /// distributed to different processors.
    fn create_and_splinter_mappings(&self) -> SplinteredProcessorMappings {
        // At first we want a clean representation of each relevant mapping, without all the
        // property stuff and so on.
        let mappings: Vec<_> = self
            .mappings()
            .map(|m| {
                m.borrow()
                    .with_context(&self.context)
                    .create_processor_mapping(&self.parameters)
            })
            .collect();
        // Then we need to splinter each of it.
        let (real_time, main): (Vec<_>, Vec<_>) =
            mappings.into_iter().map(|m| m.splinter()).unzip();
        SplinteredProcessorMappings { real_time, main }
    }

    fn generate_name_for_new_mapping(&self) -> String {
        format!("{}", self.mapping_models.len() + 1)
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }

    /// Shouldn't be called on load (project load, undo, redo, preset change).
    pub fn mark_project_as_dirty(&self) {
        debug!(Reaper::get().logger(), "Marking project as dirty");
        self.context.project().mark_as_dirty();
    }

    /// Does a full resync and notifies the UI async.
    ///
    /// Explicitly doesn't mark the project as dirty - because this is also used when loading data
    /// (project load, undo, redo, preset change).
    pub fn notify_everything_has_changed(&mut self, weak_session: WeakSession) {
        self.resubscribe_to_mappings_in_current_list(weak_session);
        self.sync_settings_to_real_time_processor();
        self.sync_feedback_is_globally_enabled();
        self.sync_all_mappings_full();
        self.sync_all_parameters();
        // For UI
        AsyncNotifier::notify(&mut self.everything_changed_subject, &());
    }
}

#[derive(Clone, Debug, Default)]
pub struct ParameterSetting {
    pub custom_name: Option<String>,
}

struct SplinteredProcessorMappings {
    real_time: Vec<NormalRealTimeMapping>,
    main: Vec<NormalMainMapping>,
}

impl Drop for Session {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping session...");
        if let Some(reg) = self.main_processor_registration {
            unsafe {
                // We can throw the unregistered control surface immediately because we are sure
                // that we are currently not in a control surface call.
                let _ = Reaper::get()
                    .medium_session()
                    .plugin_register_remove_csurf_inst(reg);
            }
        }
        self.party_is_over_subject.next(())
    }
}

fn toggle_learn(prop: &mut Prop<Option<SharedMapping>>, mapping: &SharedMapping) {
    match prop.get_ref() {
        Some(m) if m.as_ptr() == mapping.as_ptr() => prop.set(None),
        _ => prop.set(Some(mapping.clone())),
    };
}

impl DomainEventHandler for WeakSession {
    fn handle_event(&self, event: DomainEvent) {
        use DomainEvent::*;
        match event {
            LearnedSource(source) => {
                self.upgrade()
                    .expect("session not existing anymore")
                    .borrow_mut()
                    .learn_source(source);
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

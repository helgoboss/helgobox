use super::MidiSourceModel;
use crate::core::{
    prop, when_async, when_async_with_item, when_sync, when_sync_with_item, AsyncNotifier, Prop,
};
use crate::domain::{
    share_mapping, MainProcessor, MainProcessorControlMapping, MainProcessorFeedbackMapping,
    MainProcessorTask, MappingId, MappingModel, ProcessorMapping, RealTimeProcessorControlMapping,
    RealTimeProcessorTask, ReaperTarget, SessionContext, SharedMapping, TargetModel,
};
use helgoboss_learn::MidiSource;
use helgoboss_midi::ShortMessage;
use lazycell::LazyCell;
use reaper_high::{Fx, MidiInputDevice, MidiOutputDevice, Reaper, Track};
use reaper_medium::MidiInputDeviceId;
use rx_util::{
    BoxedUnitEvent, Event, Notifier, SharedEvent, SharedItemEvent, SharedPayload, SyncNotifier,
    UnitEvent,
};
use rxrust::prelude::ops::box_it::LocalBoxOp;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

/// MIDI source which provides ReaLearn control data.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiControlInput {
    /// Processes MIDI messages which are fed into ReaLearn FX.
    FxInput,
    /// Processes MIDI messages coming directly from a MIDI input device.
    Device(MidiInputDevice),
}

/// MIDI destination to which ReaLearn's feedback data is sent.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiFeedbackOutput {
    /// Routes feedback messages to the ReaLearn FX output.
    FxOutput,
    /// Routes feedback messages directly to a MIDI output device.
    Device(MidiOutputDevice),
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
    pub mapping_which_learns_source: Prop<Option<SharedMapping>>,
    pub mapping_which_learns_target: Prop<Option<SharedMapping>>,
    context: SessionContext,
    mapping_models: Vec<SharedMapping>,
    mapping_list_changed_subject: LocalSubject<'static, (), ()>,
    mapping_subscriptions: Vec<SubscriptionGuard<Box<dyn SubscriptionLike>>>,
    main_processor_channel: (
        crossbeam_channel::Sender<MainProcessorTask>,
        crossbeam_channel::Receiver<MainProcessorTask>,
    ),
    real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
}

impl Session {
    pub fn new(
        context: SessionContext,
        real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
        main_processor_channel: (
            crossbeam_channel::Sender<MainProcessorTask>,
            crossbeam_channel::Receiver<MainProcessorTask>,
        ),
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
            mapping_list_changed_subject: Default::default(),
            mapping_subscriptions: vec![],
            main_processor_channel,
            real_time_processor_sender,
            party_is_over_subject: Default::default(),
        }
    }

    /// Connects the dots.
    pub fn activate(shared_session: SharedSession) {
        let session = shared_session.borrow();
        // Register the main processor. We instantiate it as control surface because it must be
        // called regularly, even when the ReaLearn UI is closed. That means, the VST GUI idle
        // callback is not suited.
        Reaper::get()
            .medium_session()
            .plugin_register_add_csurf_inst(MainProcessor::new(
                session.main_processor_channel.0.clone(),
                session.main_processor_channel.1.clone(),
                session.real_time_processor_sender.clone(),
                shared_session.clone(),
            ));
        // Whenever anything in the mapping list changes, resync all mappings to processors.
        // When one of the mapping changes, sync just that.
        Session::when_async(
            // Initial sync
            observable::of(())
                // Future syncs
                // When the mapping list changes.
                .merge(session.mapping_list_changed())
                // There are several global conditions which affect whether feedback will be sent
                // or not. If not, the main processor will not get any mappings from us, because
                // it's unnecessary. It only ever gets the mappings it needs, that's the principle.
                // However, when anything about those conditions changes, we need to sync again.
                .merge(session.midi_feedback_output.changed())
                .merge(session.containing_fx_enabled_or_disabled())
                .merge(session.containing_track_armed_or_disarmed())
                .merge(session.send_feedback_only_if_armed.changed())
                // When target conditions change.
                .merge(TargetModel::potential_global_change_events()),
            &shared_session,
            move |s| {
                Session::resubscribe_to_mappings_in_current_list(&s);
                s.borrow_mut().sync_all_mappings_to_processors();
            },
        );
        // Keep syncing some general settings to real-time processor.
        Session::when_sync(
            // Initial sync
            observable::of(())
                // Future syncs
                .merge(session.let_matched_events_through.changed())
                .merge(session.let_unmatched_events_through.changed())
                .merge(session.midi_control_input.changed())
                .merge(session.midi_feedback_output.changed()),
            &shared_session,
            move |s| {
                s.borrow().sync_settings_to_real_time_processor();
            },
        );
        // Enable source learning
        Session::when_async(
            session.mapping_which_learns_source.changed(),
            &shared_session,
            move |s| {
                let session = s.borrow();
                let task = match session.mapping_which_learns_source.get_ref() {
                    None => RealTimeProcessorTask::StopLearnSource,
                    Some(_) => RealTimeProcessorTask::StartLearnSource,
                };
                session.real_time_processor_sender.send(task);
            },
        );
        // Enable target learning
        Session::when_async_with_item(
            Session::target_touched_observables(shared_session.clone()).switch_on_next(),
            &shared_session,
            move |s, t| {
                s.borrow_mut().learn_target(t.as_ref());
            },
        );
    }

    pub fn learn_source(&mut self, source: &MidiSource) {
        if let Some(mapping) = self.mapping_which_learns_source.replace(None) {
            mapping.borrow_mut().source_model.apply_from_source(source);
        }
    }

    fn resubscribe_to_mappings_in_current_list(shared_session: &SharedSession) {
        let mut s = shared_session.borrow_mut();
        s.mapping_subscriptions = s
            .mapping_models
            .iter()
            .map(move |shared_mapping| {
                let m = shared_mapping.borrow();
                let trigger: BoxedUnitEvent = m.changed_processing_relevant().box_it();
                let shared_session = shared_session.clone();
                let shared_mapping = shared_mapping.clone();
                trigger
                    .subscribe(move |_| {
                        let m = shared_mapping.borrow();
                        shared_session.borrow().sync_mapping_to_processors(&m)
                    })
                    .unsubscribe_when_dropped()
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

    pub fn add_default_mapping(&mut self) {
        let mut mapping = MappingModel::default();
        mapping.name.set(self.generate_name_for_new_mapping());
        self.add_mapping(mapping);
    }

    pub fn mapping_count(&self) -> usize {
        self.mapping_models.len()
    }

    pub fn mapping_by_index(&self, index: usize) -> Option<SharedMapping> {
        self.mapping_models.get(index).map(|m| m.clone())
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
        self.swap_mappings(mapping, -1);
    }

    pub fn move_mapping_down(&mut self, mapping: *const MappingModel) {
        self.swap_mappings(mapping, 1);
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
            .find(|(i, m)| m.as_ptr() == mapping as _)
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

    fn containing_track_armed_or_disarmed(&self) -> impl UnitEvent {
        let containing_track = self.context.containing_fx().track().clone();
        Reaper::get()
            .track_arm_changed()
            .filter(move |t| *t == containing_track)
            .map_to(())
    }

    /// Fires if a mapping has been added, removed or changed its position in the list.
    ///
    /// Doesn't fire if a mapping in the list has changed.
    pub fn mapping_list_changed(&self) -> impl UnitEvent {
        self.mapping_list_changed_subject.clone()
    }

    /// Omits observables that omit touched targets as long as target learn is enabled.
    fn target_touched_observables(
        shared_session: SharedSession,
    ) -> impl Event<LocalBoxOp<'static, Rc<ReaperTarget>, ()>> {
        let trigger = {
            let session = shared_session.borrow();
            session.mapping_which_learns_target.changed()
        };
        trigger.map(move |_| {
            let session = shared_session.borrow();
            match session.mapping_which_learns_target.get_ref() {
                None => observable::empty().box_it(),
                Some(_) => ReaperTarget::touched().box_it(),
            }
        })
    }

    pub fn set_mappings(&mut self, mappings: impl Iterator<Item = MappingModel>) {
        self.mapping_models = mappings.map(share_mapping).collect();
        self.notify_mapping_list_changed();
    }

    fn add_mapping(&mut self, mapping: MappingModel) {
        self.mapping_models.push(share_mapping(mapping));
        self.notify_mapping_list_changed();
    }

    pub fn send_feedback(&self) {
        self.main_processor_channel
            .0
            .send(MainProcessorTask::FeedbackAll);
    }

    fn notify_mapping_list_changed(&mut self) {
        AsyncNotifier::notify(&mut self.mapping_list_changed_subject);
    }

    fn sync_settings_to_real_time_processor(&self) {
        let task = RealTimeProcessorTask::UpdateSettings {
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
            midi_control_input: self.midi_control_input.get(),
            midi_feedback_output: self.midi_feedback_output.get(),
        };
        self.real_time_processor_sender.send(task);
    }

    fn sync_mapping_to_processors(&self, m: &MappingModel) {
        let processor_mapping = m.with_context(&self.context).create_processor_mapping();
        let control_mapping = processor_mapping.as_ref().and_then(|m| m.for_control());
        let (real_time_control_mapping, main_control_mapping) = match control_mapping {
            None => (None, None),
            Some((r, m)) => (Some(r), Some(m)),
        };
        let feedback_mapping = if self.feedback_is_effectively_enabled() {
            processor_mapping.and_then(|m| m.for_feedback())
        } else {
            None
        };
        self.main_processor_channel
            .0
            .send(MainProcessorTask::UpdateMapping {
                id: *m.id(),
                control_mapping: main_control_mapping,
                feedback_mapping,
            });
        self.real_time_processor_sender
            .send(RealTimeProcessorTask::UpdateMapping {
                id: *m.id(),
                mapping: real_time_control_mapping,
            });
    }

    fn feedback_is_effectively_enabled(&self) -> bool {
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

    fn sync_all_mappings_to_processors(&self) {
        let processor_mappings: Vec<_> = self
            .mappings()
            .filter_map(|m| {
                m.borrow()
                    .with_context(&self.context)
                    .create_processor_mapping()
            })
            .collect();
        let (real_time_control_mappings, main_control_mappings) = processor_mappings
            .iter()
            .filter_map(|m| m.for_control())
            .unzip();
        let feedback_mappings = if self.feedback_is_effectively_enabled() {
            processor_mappings
                .into_iter()
                .filter_map(|m| m.for_feedback())
                .collect()
        } else {
            Vec::new()
        };
        self.main_processor_channel
            .0
            .send(MainProcessorTask::UpdateAllMappings {
                control_mappings: main_control_mappings,
                feedback_mappings,
            });
        self.real_time_processor_sender
            .send(RealTimeProcessorTask::UpdateAllMappings(
                real_time_control_mappings,
            ));
    }

    fn generate_name_for_new_mapping(&self) -> String {
        format!("{}", self.mapping_models.len() + 1)
    }

    fn when_async_with_item<I: 'static>(
        event: impl Event<I>,
        shared_session: &SharedSession,
        reaction: impl Fn(SharedSession, I) + 'static + Copy,
    ) {
        when_async_with_item(
            event,
            shared_session.borrow().party_is_over(),
            shared_session,
            reaction,
        );
    }

    fn when_sync(
        event: impl UnitEvent,
        shared_session: &SharedSession,
        reaction: impl Fn(SharedSession) + 'static + Copy,
    ) {
        when_sync(
            event,
            shared_session.borrow().party_is_over(),
            shared_session,
            reaction,
        );
    }

    fn when_async(
        event: impl UnitEvent,
        shared_session: &SharedSession,
        reaction: impl Fn(SharedSession) + 'static + Copy,
    ) {
        when_async(
            event,
            shared_session.borrow().party_is_over(),
            shared_session,
            reaction,
        );
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.party_is_over_subject.next(())
    }
}

fn toggle_learn(prop: &mut Prop<Option<SharedMapping>>, mapping: &SharedMapping) {
    match prop.get_ref() {
        Some(m) if m.as_ptr() == mapping.as_ptr() => prop.set(None),
        _ => prop.set(Some(mapping.clone())),
    };
}

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

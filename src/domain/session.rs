use super::MidiSourceModel;
use crate::core::{prop, when_async, when_sync, AsyncNotifier, Prop};
use crate::domain::{
    share_mapping, MainProcessor, MainProcessorMapping, MainProcessorTask, MappingId, MappingModel,
    ProcessorMapping, RealTimeProcessorMapping, RealTimeProcessorTask, SessionContext,
    SharedMapping, TargetModel,
};
use helgoboss_midi::ShortMessage;
use lazycell::LazyCell;
use reaper_high::{Fx, MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::MidiInputDeviceId;
use rx_util::{BoxedUnitEvent, Notifier, SharedEvent, SyncNotifier, UnitEvent};
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
// TODO Probably belongs in application layer.
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
    // TODO-high Solve this with switch_next
    mapping_list_or_any_mapping_changed_subject: LocalSubject<'static, (), ()>,
    main_processor: MainProcessor,
    real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
}

impl Session {
    pub fn new(
        context: SessionContext,
        real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
        main_processor_receiver: crossbeam_channel::Receiver<MainProcessorTask>,
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
            mapping_list_or_any_mapping_changed_subject: Default::default(),
            real_time_processor_sender,
            main_processor: MainProcessor::new(main_processor_receiver),
        }
    }

    /// Connects the dots.
    pub fn activate(shared_session: SharedSession) {
        {
            let session = shared_session.borrow();
            // Whenever the mapping list changes, notify listeners and resubscribe to all mappings.
            Session::when_async(session.mapping_list_changed(), &shared_session, move |s| {
                s.borrow_mut().notify_mapping_list_or_any_mapping_changed();
                Session::resubscribe_to_all_mappings(s.clone());
            });
            let reaper = Reaper::get();
            // Whenever anything in the mapping changes, including the mappings itself, resync
            // mappings to processors.
            Session::when_async(
                session
                    .mapping_list_or_any_mapping_changed()
                    .merge(TargetModel::potential_global_change_events()),
                &shared_session,
                move |s| {
                    // TODO-medium This is pretty much stuff to do when doing slider changes.
                    //  A debounce is in order!
                    s.borrow_mut().sync_mappings_to_processors();
                },
            );
            // Whenever additional settings are changed, resync them to the processors.
            Session::when_sync(
                session
                    .let_matched_events_through
                    .changed()
                    .merge(session.let_unmatched_events_through.changed()),
                &shared_session,
                move |s| {
                    s.borrow().sync_flags_to_real_time_processor();
                },
            );
        }
        // Call main processor regularly so that it can process control tasks.
        Reaper::get().main_thread_idle().subscribe(move |_| {
            shared_session.borrow().main_processor.idle();
        });
    }

    fn resubscribe_to_all_mappings(shared_session: SharedSession) {
        let session = shared_session.borrow();
        Session::when_async(session.any_mapping_changed(), &shared_session, move |s| {
            s.borrow_mut().notify_mapping_list_or_any_mapping_changed();
        });
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

    pub fn is_in_input_fx_chain(&self) -> bool {
        self.context.containing_fx().is_input_fx()
    }

    /// Fires if a mapping has been added, removed or changed its position in the list.
    ///
    /// Doesn't fire if a mapping in the list has changed.
    pub fn mapping_list_changed(&self) -> impl UnitEvent {
        self.mapping_list_changed_subject.clone()
    }

    /// Fires whenever any mapping in the list has changed, until the list itself changes.
    fn any_mapping_changed(&self) -> impl UnitEvent {
        self.mapping_models
            .iter()
            .map(|m| m.borrow().control_relevant_prop_changed())
            .fold(
                observable::never().box_it(),
                |prev: BoxedUnitEvent, current| prev.merge(current).box_it(),
            )
            .take_until(self.mapping_list_changed())
    }

    fn mapping_list_or_any_mapping_changed(&self) -> impl UnitEvent {
        // TODO Maybe we can just merge mapping_list_changed_subject and
        //  any_mapping_changed_subject. But it might cause some order issues.
        self.mapping_list_or_any_mapping_changed_subject.clone()
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
        todo!()
    }

    fn notify_mapping_list_changed(&mut self) {
        AsyncNotifier::notify(&mut self.mapping_list_changed_subject);
    }

    fn notify_mapping_list_or_any_mapping_changed(&mut self) {
        AsyncNotifier::notify(&mut self.mapping_list_or_any_mapping_changed_subject);
    }

    fn sync_flags_to_real_time_processor(&self) {
        let task = RealTimeProcessorTask::UpdateFlags {
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
        };
        self.real_time_processor_sender.send(task);
    }

    fn sync_mappings_to_processors(&mut self) {
        let processor_mappings = self.mappings().filter_map(|m| {
            m.borrow()
                .with_context(&self.context)
                .create_processor_mapping()
        });
        let (real_time_mappings, main_mappings): (Vec<_>, Vec<_>) = processor_mappings
            .enumerate()
            .map(|(i, m)| m.splinter(MappingId::new(i as _)))
            .unzip();
        self.main_processor.update_mappings(main_mappings);
        self.real_time_processor_sender
            .send(RealTimeProcessorTask::UpdateMappings(real_time_mappings));
    }

    fn generate_name_for_new_mapping(&self) -> String {
        format!("{}", self.mapping_models.len() + 1)
    }

    fn when_sync(
        event: impl UnitEvent,
        shared_session: &SharedSession,
        reaction: impl Fn(SharedSession) + 'static + Copy,
    ) {
        // TODO-medium Maybe observable::empty() is better here because it completes and frees
        // resources?
        when_sync(event, observable::never(), shared_session, reaction);
    }

    fn when_async(
        event: impl UnitEvent,
        shared_session: &SharedSession,
        reaction: impl Fn(SharedSession) + 'static + Copy,
    ) {
        // TODO-medium Maybe observable::empty() is better here because it completes and frees
        // resources?
        when_async(event, observable::never(), shared_session, reaction);
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
pub type SharedSession = Rc<debug_cell::RefCell<Session>>;

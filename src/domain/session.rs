use super::MidiSourceModel;
use crate::core::when_async;
use crate::domain::{share_mapping, MappingModel, RealTimeTask, SessionContext, SharedMapping};
use helgoboss_midi::ShortMessage;
use lazycell::LazyCell;
use reaper_high::{Fx, MidiInputDevice, MidiOutputDevice};
use reaper_medium::MidiInputDeviceId;
use rx_util::{
    create_local_prop as p, BoxedUnitEvent, LocalProp, LocalStaticProp, SharedEvent, SharedProp,
    UnitEvent,
};
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
    pub let_matched_events_through: LocalStaticProp<bool>,
    pub let_unmatched_events_through: LocalStaticProp<bool>,
    pub always_auto_detect: LocalStaticProp<bool>,
    pub send_feedback_only_if_armed: LocalStaticProp<bool>,
    pub midi_control_input: LocalStaticProp<MidiControlInput>,
    pub midi_feedback_output: LocalStaticProp<Option<MidiFeedbackOutput>>,
    pub mapping_which_learns_source: LocalStaticProp<Option<SharedMapping>>,
    pub mapping_which_learns_target: LocalStaticProp<Option<SharedMapping>>,
    context: SessionContext,
    mapping_models: Vec<SharedMapping>,
    mapping_list_changed_subject: LocalSubject<'static, (), ()>,
    mapping_list_or_any_mapping_changed_subject: LocalSubject<'static, (), ()>,
    real_time_sender: crossbeam_channel::Sender<RealTimeTask>,
}

impl Session {
    pub fn new(
        context: SessionContext,
        real_time_sender: crossbeam_channel::Sender<RealTimeTask>,
    ) -> Session {
        Self {
            let_matched_events_through: p(false),
            let_unmatched_events_through: p(true),
            always_auto_detect: p(true),
            send_feedback_only_if_armed: p(true),
            midi_control_input: p(MidiControlInput::FxInput),
            midi_feedback_output: p(None),
            mapping_which_learns_source: p(None),
            mapping_which_learns_target: p(None),
            context,
            mapping_models: vec![],
            mapping_list_changed_subject: Default::default(),
            mapping_list_or_any_mapping_changed_subject: Default::default(),
            real_time_sender,
        }
    }

    /// Connects the dots by keeping settings of real-time processor in sync with the session.
    // TODO Maybe we should put SharedSession type in this file
    pub fn activate(shared_session: Rc<debug_cell::RefCell<Session>>) {
        let session = shared_session.borrow();
        // Whenever the mapping list changes, notify listeners and resubscribe to all mappings.
        Session::when(&shared_session, session.mapping_list_changed(), move |s| {
            s.borrow_mut()
                .mapping_list_or_any_mapping_changed_subject
                .next(());
            Session::resubscribe_to_all_mappings(s.clone());
        });
        Session::when(
            &shared_session,
            session.mapping_list_or_any_mapping_changed(),
            move |s| {
                s.borrow().sync_mappings_to_real_time_processor();
            },
        );
        Session::when(
            &shared_session,
            session
                .let_matched_events_through
                .changed()
                .merge(session.let_unmatched_events_through.changed()),
            move |s| {
                s.borrow().sync_flags_to_real_time_processor();
            },
        );
    }

    fn when(
        shared_session: &Rc<debug_cell::RefCell<Session>>,
        event: impl UnitEvent,
        reaction: impl Fn(Rc<debug_cell::RefCell<Session>>) + 'static + Copy,
    ) {
        when_async(event, observable::never(), shared_session, reaction);
    }

    fn resubscribe_to_all_mappings(shared_session: Rc<debug_cell::RefCell<Session>>) {
        let session = shared_session.borrow();
        Session::when(&shared_session, session.any_mapping_changed(), move |s| {
            s.borrow_mut()
                .mapping_list_or_any_mapping_changed_subject
                .next(());
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
        self.mapping_list_changed_subject.next(());
        Ok(())
    }

    pub fn remove_mapping(&mut self, mapping: *const MappingModel) {
        self.mapping_models.retain(|m| m.as_ptr() != mapping as _);
        self.mapping_list_changed_subject.next(());
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
        self.mapping_list_changed_subject.next(());
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
        //  any_mapping_changed_subject
        self.mapping_list_or_any_mapping_changed_subject.clone()
    }

    pub fn set_mappings(&mut self, mappings: impl Iterator<Item = MappingModel>) {
        self.mapping_models = mappings.map(share_mapping).collect();
        self.mapping_list_changed_subject.next(());
    }

    fn add_mapping(&mut self, mapping: MappingModel) {
        self.mapping_models.push(share_mapping(mapping));
        self.mapping_list_changed_subject.next(());
    }

    pub fn send_feedback(&self) {
        todo!()
    }

    fn sync_flags_to_real_time_processor(&self) {
        let task = RealTimeTask::UpdateFlags {
            let_matched_events_through: self.let_matched_events_through.get(),
            let_unmatched_events_through: self.let_unmatched_events_through.get(),
        };
        self.real_time_sender.send(task);
    }

    fn sync_mappings_to_real_time_processor(&self) {
        println!("Something changed");
    }

    fn generate_name_for_new_mapping(&self) -> String {
        format!("{}", self.mapping_models.len() + 1)
    }
}

fn toggle_learn(prop: &mut LocalStaticProp<Option<SharedMapping>>, mapping: &SharedMapping) {
    match prop.get_ref() {
        Some(m) if m.as_ptr() == mapping.as_ptr() => prop.set(None),
        _ => prop.set(Some(mapping.clone())),
    };
}

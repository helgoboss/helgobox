use super::MidiSourceModel;
use crate::domain::{share_mapping, MappingModel, SessionContext, SharedMapping};
use lazycell::LazyCell;
use reaper_high::{Fx, MidiInputDevice, MidiOutputDevice};
use reaper_medium::MidiInputDeviceId;
use rx_util::{
    create_local_prop as p, LocalProp, LocalStaticProp, SharedEvent, SharedProp, UnitEvent,
};
use rxrust::prelude::*;
use std::borrow::BorrowMut;
use std::cell::RefCell;
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
    mappings_changed_subject: LocalSubject<'static, (), ()>,
}

impl Session {
    pub fn new(context: SessionContext) -> Session {
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
            mapping_models: example_data::create_example_mappings()
                .into_iter()
                .map(share_mapping)
                .collect(),
            mappings_changed_subject: Default::default(),
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
        self.mappings_changed_subject.next(());
        Ok(())
    }

    pub fn remove_mapping(&mut self, mapping: *const MappingModel) {
        self.mapping_models.retain(|m| m.as_ptr() != mapping as _);
        self.mappings_changed_subject.next(());
    }

    pub fn duplicate_mapping(&mut self, mapping: *const MappingModel) -> Result<(), &str> {
        let (index, mapping) = self
            .mapping_models
            .iter()
            .enumerate()
            .find(|(i, m)| m.as_ptr() == mapping as _)
            .ok_or("mapping not found")?;
        let mut duplicate = mapping.borrow().clone();
        duplicate.name.set(self.generate_name_for_new_mapping());
        self.mapping_models
            .insert(index + 1, share_mapping(duplicate));
        self.mappings_changed_subject.next(());
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

    pub fn mappings_changed(&self) -> impl UnitEvent {
        self.mappings_changed_subject.clone()
    }

    pub fn set_mappings(&mut self, mappings: impl Iterator<Item = MappingModel>) {
        self.mapping_models = mappings.map(share_mapping).collect();
        self.mappings_changed_subject.next(());
    }

    fn add_mapping(&mut self, mapping: MappingModel) {
        self.mapping_models.push(share_mapping(mapping));
        self.mappings_changed_subject.next(());
    }

    pub fn send_feedback(&self) {
        todo!()
    }

    fn generate_name_for_new_mapping(&self) -> String {
        format!("{}", self.mapping_models.len() + 1)
    }
}

// TODO remove
mod example_data {
    use crate::domain::{
        ActionInvocationType, MappingModel, MidiSourceModel, MidiSourceType, ModeModel, ModeType,
        TargetModel, TargetType, VirtualTrack,
    };
    use helgoboss_learn::{
        DiscreteValue, Interval, MidiClockTransportMessage, SourceCharacter, UnitValue,
    };
    use helgoboss_midi::Channel;
    use reaper_medium::CommandId;
    use rx_util::{create_local_prop as p, SharedProp};

    pub fn create_example_mappings() -> Vec<MappingModel> {
        vec![
            MappingModel {
                name: p(String::from("Mapping A")),
                control_is_enabled: p(true),
                feedback_is_enabled: p(false),
                source_model: MidiSourceModel {
                    r#type: p(MidiSourceType::PolyphonicKeyPressureAmount),
                    channel: p(Some(Channel::new(5))),
                    midi_message_number: p(None),
                    parameter_number_message_number: p(None),
                    custom_character: p(SourceCharacter::Encoder2),
                    midi_clock_transport_message: p(MidiClockTransportMessage::Start),
                    is_registered: p(Some(true)),
                    is_14_bit: p(Some(false)),
                },
                mode_model: Default::default(),
                target_model: Default::default(),
            },
            MappingModel {
                name: p(String::from("Mapping B")),
                control_is_enabled: p(false),
                feedback_is_enabled: p(true),
                source_model: Default::default(),
                mode_model: ModeModel {
                    r#type: p(ModeType::Relative),
                    target_value_interval: p(Interval::new(UnitValue::new(0.5), UnitValue::MAX)),
                    source_value_interval: p(Interval::new(UnitValue::MIN, UnitValue::MAX)),
                    reverse: p(true),
                    jump_interval: p(Interval::new(UnitValue::MIN, UnitValue::MAX)),
                    ignore_out_of_range_source_values: p(false),
                    round_target_value: p(false),
                    approach_target_value: p(false),
                    eel_control_transformation: p(String::new()),
                    eel_feedback_transformation: p(String::new()),
                    step_size_interval: p(Interval::new(
                        UnitValue::new(0.01),
                        UnitValue::new(0.01),
                    )),
                    rotate: p(true),
                },
                target_model: TargetModel {
                    r#type: p(TargetType::TrackSelection),
                    command_id: p(Some(CommandId::new(3500))),
                    action_invocation_type: p(ActionInvocationType::Absolute),
                    track: p(VirtualTrack::Selected),
                    enable_only_if_track_selected: p(true),
                    fx_index: p(Some(5)),
                    is_input_fx: p(true),
                    enable_only_if_fx_has_focus: p(true),
                    param_index: p(20),
                    send_index: p(Some(2)),
                    select_exclusively: p(true),
                },
            },
        ]
    }
}

fn toggle_learn(prop: &mut LocalStaticProp<Option<SharedMapping>>, mapping: &SharedMapping) {
    match prop.get_ref() {
        Some(m) if m.as_ptr() == mapping.as_ptr() => prop.set(None),
        _ => prop.set(Some(mapping.clone())),
    };
}

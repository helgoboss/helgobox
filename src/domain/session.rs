use super::MidiSourceModel;
use crate::domain::MappingModel;
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::MidiInputDeviceId;
use rx_util::{create_local_prop as p, LocalProp};
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
pub struct Session<'a> {
    pub let_matched_events_through: LocalProp<'a, bool>,
    pub let_unmatched_events_through: LocalProp<'a, bool>,
    pub always_auto_detect: LocalProp<'a, bool>,
    pub send_feedback_only_if_armed: LocalProp<'a, bool>,
    pub midi_control_input: LocalProp<'a, MidiControlInput>,
    pub midi_feedback_output: LocalProp<'a, Option<MidiFeedbackOutput>>,
    mapping_models: Vec<Rc<RefCell<MappingModel<'a>>>>,
    mappings_changed_subject: LocalSubject<'a, (), ()>,
}

impl<'a> Default for Session<'a> {
    fn default() -> Self {
        Self {
            let_matched_events_through: p(false),
            let_unmatched_events_through: p(true),
            always_auto_detect: p(true),
            send_feedback_only_if_armed: p(true),
            midi_control_input: p(MidiControlInput::Device(MidiInputDevice::new(
                MidiInputDeviceId::new(47),
            ))),
            midi_feedback_output: p(None),
            mapping_models: example_data::create_example_mappings()
                .into_iter()
                .map(|m| Rc::new(RefCell::new(m)))
                .collect(),
            mappings_changed_subject: Default::default(),
        }
    }
}

impl<'a> Session<'a> {
    pub fn new() -> Session<'a> {
        Session::default()
    }

    pub fn add_default_mapping(&mut self) {
        let mut mapping = MappingModel::default();
        mapping.name.set(self.generate_name_for_new_mapping());
        self.add_mapping(mapping);
    }

    pub fn is_in_input_fx_chain(&self) -> bool {
        // TODO
        false
    }

    pub fn mappings_changed(&self) -> impl LocalObservable<'a, Item = (), Err = ()> {
        self.mappings_changed_subject.clone()
    }

    fn add_mapping(&mut self, mapping: MappingModel<'a>) {
        self.mapping_models.push(Rc::new(RefCell::new(mapping)));
        self.mappings_changed_subject.next(());
    }

    pub fn import_from_clipboard(&mut self) {
        todo!()
    }

    pub fn export_to_clipboard(&self) {
        todo!()
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
    use helgoboss_learn::{MidiClockTransportMessage, SourceCharacter, UnitValue};
    use helgoboss_midi::Channel;
    use reaper_medium::CommandId;
    use rx_util::{create_local_prop as p, LocalProp};

    pub fn create_example_mappings<'a>() -> Vec<MappingModel<'a>> {
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
                    is_14_bit: p(None),
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
                    min_target_value: p(UnitValue::new(0.5)),
                    max_target_value: p(UnitValue::MAX),
                    min_source_value: p(UnitValue::MIN),
                    max_source_value: p(UnitValue::MAX),
                    reverse: p(true),
                    min_jump: p(UnitValue::MIN),
                    max_jump: p(UnitValue::MAX),
                    ignore_out_of_range_source_values: p(false),
                    round_target_value: p(false),
                    approach_target_value: p(false),
                    eel_control_transformation: p(String::new()),
                    eel_feedback_transformation: p(String::new()),
                    min_step_size: p(UnitValue::new(0.01)),
                    max_step_size: p(UnitValue::new(0.01)),
                    rotate: p(true),
                },
                target_model: TargetModel {
                    r#type: p(TargetType::TrackSelection),
                    command_id: p(CommandId::new(3500)),
                    action_invocation_type: p(ActionInvocationType::Absolute),
                    track: p(VirtualTrack::Selected),
                    enable_only_if_track_selected: p(true),
                    fx_index: p(Some(5)),
                    is_input_fx: p(true),
                    enable_only_if_fx_has_focus: p(true),
                    parameter_index: p(20),
                    send_index: p(Some(2)),
                    select_exclusively: p(true),
                },
            },
        ]
    }
}

use super::MidiSourceModel;
use crate::domain::MappingModel;
use std::cell::RefCell;
use std::rc::Rc;

/// This represents the user session with one ReaLearn instance.
///
/// It's ReaLearn's main object which keeps everything together.
// TODO Probably belongs in application layer.
#[derive(Default, Debug)]
pub struct Session<'a> {
    mapping_models: Vec<Rc<RefCell<MappingModel<'a>>>>,
    // TODO remove
    dummy_source_model: MidiSourceModel<'a>,
}

impl<'a> Session<'a> {
    pub fn new() -> Session<'a> {
        Session {
            mapping_models: example_data::create_example_mappings()
                .into_iter()
                .map(|m| Rc::new(RefCell::new(m)))
                .collect(),
            dummy_source_model: Default::default(),
        }
    }

    // TODO remove
    pub fn get_dummy_source_model(&mut self) -> &mut MidiSourceModel<'a> {
        &mut self.dummy_source_model
    }
}

// TODO remove
mod example_data {
    use crate::domain::{
        create_property as p, ActionInvocationType, MappingModel, MidiSourceModel, MidiSourceType,
        ModeModel, ModeType, TargetModel, TargetType, VirtualTrack,
    };
    use helgoboss_learn::{MidiClockTransportMessage, SourceCharacter, UnitValue};
    use helgoboss_midi::Channel;
    use reaper_medium::CommandId;

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

use crate::domain::create_property as p;
use crate::domain::Property;
use reaper_high::Track;
use reaper_medium::CommandId;
use serde_repr::*;

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel<'a> {
    // For all targets
    pub r#type: Property<'a, TargetType>,
    // For action targets only
    pub command_id: Property<'a, CommandId>,
    pub action_invocation_type: Property<'a, ActionInvocationType>,
    // For track targets
    pub track: Property<'a, VirtualTrack>,
    pub enable_only_if_track_selected: Property<'a, bool>,
    // For track FX targets
    pub fx_index: Property<'a, Option<u32>>,
    pub is_input_fx: Property<'a, bool>,
    pub enable_only_if_fx_has_focus: Property<'a, bool>,
    // For track FX parameter targets
    pub parameter_index: Property<'a, u32>,
    // For track send targets
    pub send_index: Property<'a, Option<u32>>,
    // For track selection targets
    pub select_exclusively: Property<'a, bool>,
}

impl<'a> Default for TargetModel<'a> {
    fn default() -> Self {
        Self {
            r#type: p(TargetType::FxParameter),
            command_id: p(CommandId::new(1)),
            action_invocation_type: p(ActionInvocationType::Trigger),
            track: p(VirtualTrack::This),
            enable_only_if_track_selected: p(false),
            fx_index: p(None),
            is_input_fx: p(false),
            enable_only_if_fx_has_focus: p(false),
            parameter_index: p(0),
            send_index: p(None),
            select_exclusively: p(false),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualTrack {
    /// A particular track.
    Particular(Track),
    /// Current track (the one which contains the ReaLearn instance).
    This,
    /// Currently selected track.
    Selected,
}

/// Type of a target
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum TargetType {
    Action = 0,
    FxParameter = 1,
    TrackVolume = 2,
    TrackSendVolume = 3,
    TrackPan = 4,
    TrackArm = 5,
    TrackSelection = 6,
    TrackMute = 7,
    TrackSolo = 8,
    TrackSendPan = 9,
    Tempo = 10,
    Playrate = 11,
    FxEnable = 12,
    FxPreset = 13,
}

/// How to invoke an action target
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ActionInvocationType {
    Trigger = 0,
    Absolute = 1,
    Relative = 2,
}

use reaper_high::{Action, Reaper, Track};
use reaper_medium::CommandId;
use rx_util::{create_local_prop as p, LocalProp, LocalStaticProp};
use serde_repr::*;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // For all targets
    pub r#type: LocalStaticProp<TargetType>,
    // For action targets only
    pub command_id: LocalStaticProp<Option<CommandId>>,
    pub action_invocation_type: LocalStaticProp<ActionInvocationType>,
    // For track targets
    pub track: LocalStaticProp<VirtualTrack>,
    pub enable_only_if_track_selected: LocalStaticProp<bool>,
    // For track FX targets
    pub fx_index: LocalStaticProp<Option<u32>>,
    pub is_input_fx: LocalStaticProp<bool>,
    pub enable_only_if_fx_has_focus: LocalStaticProp<bool>,
    // For track FX parameter targets
    pub parameter_index: LocalStaticProp<u32>,
    // For track send targets
    pub send_index: LocalStaticProp<Option<u32>>,
    // For track selection targets
    pub select_exclusively: LocalStaticProp<bool>,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            r#type: p(TargetType::FxParameter),
            command_id: p(None),
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

impl TargetModel {
    fn command_id_label(&self) -> Cow<str> {
        match self.command_id.get() {
            None => "-".into(),
            Some(id) => id.to_string().into(),
        }
    }

    fn action(&self) -> Option<Action> {
        self.command_id
            .get()
            .map(|id| Reaper::get().main_section().action_by_command_id(id))
    }

    fn action_name_label(&self) -> Cow<str> {
        match self.action() {
            None => "-".into(),
            Some(a) => a.name().into_string().expect("not UTF-8").into(),
        }
    }
}

impl Display for TargetModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetType::*;
        match self.r#type.get() {
            Action => write!(
                f,
                "Action {}\n{}",
                self.command_id_label(),
                self.action_name_label()
            ),
            FxParameter => write!(f, "{}", ""),
            TrackVolume => write!(f, "{}", ""),
            TrackSendVolume => write!(f, "{}", ""),
            TrackPan => write!(f, "{}", ""),
            TrackArm => write!(f, "{}", ""),
            TrackSelection => write!(f, "{}", ""),
            TrackMute => write!(f, "{}", ""),
            TrackSolo => write!(f, "{}", ""),
            TrackSendPan => write!(f, "{}", ""),
            Tempo => write!(f, "{}", ""),
            Playrate => write!(f, "{}", ""),
            FxEnable => write!(f, "{}", ""),
            FxPreset => write!(f, "{}", ""),
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

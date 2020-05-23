use reaper_high::{Action, Fx, FxParameter, Reaper, Track, TrackSend};
use reaper_medium::{CommandId, TrackLocation};
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
    pub param_index: LocalStaticProp<u32>,
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
            param_index: p(0),
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

    fn fx(&self) -> Option<Fx> {
        let fx_index = self.fx_index.get()?;
        let track = self.effective_track()?;
        let fx_chain = if self.is_input_fx.get() {
            track.normal_fx_chain()
        } else {
            track.input_fx_chain()
        };
        if *self.track.get_ref() == VirtualTrack::Selected {
            Some(fx_chain.fx_by_index_untracked(fx_index))
        } else {
            fx_chain.fx_by_index(fx_index)
        }
    }

    fn effective_track(&self) -> Option<Track> {
        todo!()
    }

    fn track_send(&self) -> Option<TrackSend> {
        let send_index = self.send_index.get()?;
        let track = self.effective_track()?;
        let send = track.index_based_send_by_index(send_index);
        if !send.is_available() {
            return None;
        }
        Some(send)
    }

    fn fx_param(&self) -> Option<FxParameter> {
        let fx = self.fx()?;
        if !fx.is_available() {
            return None;
        }
        Some(fx.parameter_by_index(self.param_index.get()))
    }

    fn action_name_label(&self) -> Cow<str> {
        match self.action() {
            None => "-".into(),
            Some(a) => a.name().into_string().into(),
        }
    }

    fn track_label(&self) -> Cow<str> {
        use VirtualTrack::*;
        match self.track.get_ref() {
            This => "<This>".into(),
            Selected => "<Selected>".into(),
            Particular(t) => get_track_label(t).into(),
        }
    }

    fn track_send_label(&self) -> Cow<str> {
        match self.track_send() {
            None => "-".into(),
            Some(s) => s.name().into_string().into(),
        }
    }

    fn fx_label(&self) -> Cow<str> {
        get_fx_label(&self.fx(), self.fx_index.get())
    }

    fn fx_param_label(&self) -> Cow<str> {
        get_fx_param_label(&self.fx_param(), self.param_index.get())
    }
}

pub fn get_fx_param_label(fx_param: &Option<FxParameter>, index: u32) -> Cow<'static, str> {
    let position = index + 1;
    match fx_param {
        None => format!("{}. <Not present>", position).into(),
        Some(p) => {
            let name = p.name();
            let name = name.to_str();
            if name.is_empty() {
                position.to_string().into()
            } else {
                format!("{}. {}", position, name).into()
            }
        }
    }
}

pub fn get_fx_label(fx: &Option<Fx>, index: Option<u32>) -> Cow<'static, str> {
    let index = match index {
        None => return "<None>".into(),
        Some(i) => i,
    };
    let position = index + 1;
    match fx {
        None => format!("{}. <Not present>", position).into(),
        Some(fx) => format!("{}. {}", position, fx.name().to_str()).into(),
    }
}

pub fn get_track_label(track: &Track) -> String {
    use TrackLocation::*;
    match track.location() {
        TrackLocation::MasterTrack => "<Master track>".into(),
        TrackLocation::NormalTrack(i) => {
            let position = i + 1;
            let name = track.name().expect("non-master track must have name");
            let name = name.to_str();
            if name.is_empty() {
                position.to_string()
            } else {
                format!("{}. {}", position, name)
            }
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
            FxParameter => write!(
                f,
                "Track FX parameter\nTrack {}\nFX {}\nParam {}",
                self.track_label(),
                self.fx_label(),
                self.fx_param_label()
            ),
            TrackVolume => write!(f, "Track volume\nTrack {}", self.track_label()),
            TrackSendVolume => write!(
                f,
                "Track send volume\nTrack {}\nSend {}",
                self.track_label(),
                self.track_send_label()
            ),
            TrackPan => write!(f, "Track pan\nTrack {}", self.track_label()),
            TrackArm => write!(f, "Track arm\nTrack {}", self.track_label()),
            TrackSelection => write!(f, "Track selection\nTrack {}", self.track_label()),
            TrackMute => write!(f, "Track mute\nTrack {}", self.track_label()),
            TrackSolo => write!(f, "Track solo\nTrack {}", self.track_label()),
            TrackSendPan => write!(
                f,
                "Track send pan\nTrack {}\nSend {}",
                self.track_label(),
                self.track_send_label()
            ),
            Tempo => write!(f, "Master tempo"),
            Playrate => write!(f, "Master playrate"),
            FxEnable => write!(
                f,
                "Track FX enable\nTrack {}\nFX {}",
                self.track_label(),
                self.fx_label(),
            ),
            FxPreset => write!(
                f,
                "Track FX preset\nTrack {}\nFX {}",
                self.track_label(),
                self.fx_label(),
            ),
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

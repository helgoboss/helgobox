use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use reaper_high::{Action, Fx, FxParameter, Reaper, Track, TrackSend};
use reaper_medium::MasterTrackBehavior::IncludeMasterTrack;
use reaper_medium::{CommandId, MasterTrackBehavior, TrackLocation};
use rx_util::{create_local_prop as p, LocalProp, LocalStaticProp, UnitEvent};
use rxrust::prelude::*;
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
    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
        self.r#type
            .changed()
            .merge(self.command_id.changed())
            .merge(self.action_invocation_type.changed())
            .merge(self.track.changed())
            .merge(self.enable_only_if_track_selected.changed())
            .merge(self.fx_index.changed())
            .merge(self.is_input_fx.changed())
            .merge(self.enable_only_if_fx_has_focus.changed())
            .merge(self.param_index.changed())
            .merge(self.send_index.changed())
            .merge(self.select_exclusively.changed())
    }

    pub fn with_context<'a>(&'a self, containing_fx: &'a Fx) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            containing_fx,
        }
    }
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

    fn fx(&self, containing_fx: &Fx) -> Option<Fx> {
        let fx_index = self.fx_index.get()?;
        let track = self.effective_track(containing_fx)?;
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

    // TODO-low Consider returning a Cow
    fn effective_track(&self, containing_fx: &Fx) -> Option<Track> {
        use VirtualTrack::*;
        match self.track.get_ref() {
            Particular(track) => Some(track.clone()),
            This => Some(containing_fx.track().clone()),
            Selected => containing_fx
                .project()
                .unwrap_or(Reaper::get().current_project())
                .first_selected_track(IncludeMasterTrack),
        }
    }

    fn track_send(&self, containing_fx: &Fx) -> Option<TrackSend> {
        let send_index = self.send_index.get()?;
        let track = self.effective_track(containing_fx)?;
        let send = track.index_based_send_by_index(send_index);
        if !send.is_available() {
            return None;
        }
        Some(send)
    }

    fn fx_param(&self, containing_fx: &Fx) -> Option<FxParameter> {
        let fx = self.fx(containing_fx)?;
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

    fn track_send_label(&self, containing_fx: &Fx) -> Cow<str> {
        match self.track_send(containing_fx) {
            None => "-".into(),
            Some(s) => s.name().into_string().into(),
        }
    }

    fn fx_label(&self, containing_fx: &Fx) -> Cow<str> {
        get_fx_label(&self.fx(containing_fx), self.fx_index.get())
    }

    fn fx_param_label(&self, containing_fx: &Fx) -> Cow<str> {
        get_fx_param_label(&self.fx_param(containing_fx), self.param_index.get())
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

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    containing_fx: &'a Fx,
}

impl<'a> Display for TargetModelWithContext<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetType::*;

        match self.target.r#type.get() {
            Action => write!(
                f,
                "Action {}\n{}",
                self.target.command_id_label(),
                self.target.action_name_label()
            ),
            FxParameter => write!(
                f,
                "Track FX parameter\nTrack {}\nFX {}\nParam {}",
                self.target.track_label(),
                self.target.fx_label(self.containing_fx),
                self.target.fx_param_label(self.containing_fx)
            ),
            TrackVolume => write!(f, "Track volume\nTrack {}", self.target.track_label()),
            TrackSendVolume => write!(
                f,
                "Track send volume\nTrack {}\nSend {}",
                self.target.track_label(),
                self.target.track_send_label(self.containing_fx)
            ),
            TrackPan => write!(f, "Track pan\nTrack {}", self.target.track_label()),
            TrackArm => write!(f, "Track arm\nTrack {}", self.target.track_label()),
            TrackSelection => write!(f, "Track selection\nTrack {}", self.target.track_label()),
            TrackMute => write!(f, "Track mute\nTrack {}", self.target.track_label()),
            TrackSolo => write!(f, "Track solo\nTrack {}", self.target.track_label()),
            TrackSendPan => write!(
                f,
                "Track send pan\nTrack {}\nSend {}",
                self.target.track_label(),
                self.target.track_send_label(self.containing_fx)
            ),
            Tempo => write!(f, "Master tempo"),
            Playrate => write!(f, "Master playrate"),
            FxEnable => write!(
                f,
                "Track FX enable\nTrack {}\nFX {}",
                self.target.track_label(),
                self.target.fx_label(self.containing_fx),
            ),
            FxPreset => write!(
                f,
                "Track FX preset\nTrack {}\nFX {}",
                self.target.track_label(),
                self.target.fx_label(self.containing_fx),
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
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr, IntoEnumIterator, Display,
)]
#[repr(u8)]
pub enum TargetType {
    #[display(fmt = "Action (limited feedback)")]
    Action = 0,
    #[display(fmt = "Track FX parameter")]
    FxParameter = 1,
    #[display(fmt = "Track volume")]
    TrackVolume = 2,
    #[display(fmt = "Track send volume")]
    TrackSendVolume = 3,
    #[display(fmt = "Track pan")]
    TrackPan = 4,
    #[display(fmt = "Track arm")]
    TrackArm = 5,
    #[display(fmt = "Track selection")]
    TrackSelection = 6,
    #[display(fmt = "Track mute (no feedback from automation)")]
    TrackMute = 7,
    #[display(fmt = "Track solo")]
    TrackSolo = 8,
    #[display(fmt = "Track send pan")]
    TrackSendPan = 9,
    #[display(fmt = "Master tempo")]
    Tempo = 10,
    #[display(fmt = "Master playrate")]
    Playrate = 11,
    #[display(fmt = "Track FX enable (no feedback from automation)")]
    FxEnable = 12,
    #[display(fmt = "Track FX preset (no feedback)")]
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

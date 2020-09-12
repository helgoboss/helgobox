use crate::core::{prop, Prop};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::Target;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{Action, Fx, FxChain, FxParameter, Guid, Project, Reaper, Track, TrackSend};
use reaper_medium::MasterTrackBehavior::IncludeMasterTrack;
use reaper_medium::TrackLocation;
use rx_util::{Event, UnitEvent};

use crate::application::SessionContext;
use crate::domain::{ActionInvocationType, ReaperTarget, TargetCharacter, TransportAction};
use serde_repr::*;
use std::borrow::Cow;
use std::fmt;
use std::fmt::{Display, Formatter};

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // # For all targets
    pub r#type: Prop<TargetType>,
    // # For action targets only
    // TODO-low Maybe replace Action with just command ID and/or command name
    pub action: Prop<Option<Action>>,
    pub action_invocation_type: Prop<ActionInvocationType>,
    // # For track targets
    // TODO-low Maybe replace VirtualTrack::Particular(track) with Particular(GUID)
    pub track: Prop<VirtualTrack>,
    pub enable_only_if_track_selected: Prop<bool>,
    // # For track FX targets
    // Used for getting FX by index, e.g. when track is "<Selected>"
    pub fx_index: Prop<Option<u32>>,
    // Used for tracking particular FX by GUID
    pub fx_guid: Prop<Option<Guid>>,
    pub is_input_fx: Prop<bool>,
    pub enable_only_if_fx_has_focus: Prop<bool>,
    // # For track FX parameter targets
    pub param_index: Prop<u32>,
    // # For track send targets
    pub send_index: Prop<Option<u32>>,
    // # For track selection targets
    pub select_exclusively: Prop<bool>,
    // # For transport target
    pub transport_action: Prop<TransportAction>,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            r#type: prop(TargetType::FxParameter),
            action: prop(None),
            action_invocation_type: prop(ActionInvocationType::Trigger),
            track: prop(VirtualTrack::This),
            enable_only_if_track_selected: prop(false),
            fx_index: prop(None),
            is_input_fx: prop(false),
            fx_guid: prop(None),
            enable_only_if_fx_has_focus: prop(false),
            param_index: prop(0),
            send_index: prop(None),
            select_exclusively: prop(false),
            transport_action: prop(TransportAction::PlayStop),
        }
    }
}

impl TargetModel {
    pub fn set_fx_index_and_memorize_guid(
        &mut self,
        context: &SessionContext,
        fx_index: Option<u32>,
    ) {
        self.fx_index.set(fx_index);
        let fx_guid = fx_index.and_then(|fx_index| {
            let track = self.track.get_ref();
            let is_input_fx = self.is_input_fx.get();
            let fx = get_guid_based_fx_at_index(context, track, is_input_fx, fx_index).ok()?;
            fx.guid()
        });
        self.fx_guid.set(fx_guid);
    }

    pub fn invalidate_fx_index(&mut self, context: &SessionContext) {
        if !self.supports_fx() {
            return;
        }
        if let Ok(fx) = self.with_context(context).fx() {
            self.fx_index.set(Some(fx.index()));
        }
    }

    pub fn apply_from_target(&mut self, target: &ReaperTarget, context: &SessionContext) {
        use ReaperTarget::*;
        self.r#type.set(TargetType::from_target(target));
        if let Some(track) = target.track() {
            self.track.set(virtualize_track(track.clone(), context));
        }
        if let Some(fx) = target.fx() {
            self.fx_index.set(Some(fx.index()));
            self.fx_guid.set(fx.guid());
            self.is_input_fx.set(fx.is_input_fx());
        }
        if let Some(send) = target.send() {
            self.send_index.set(Some(send.index()));
        }
        match target {
            Action {
                action,
                invocation_type,
                ..
            } => {
                self.action.set(Some(action.clone()));
                self.action_invocation_type.set(*invocation_type);
            }
            FxParameter { param } => {
                self.param_index.set(param.index());
            }
            _ => {}
        };
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
        self.r#type
            .changed()
            .merge(self.action.changed())
            .merge(self.action_invocation_type.changed())
            .merge(self.track.changed())
            .merge(self.enable_only_if_track_selected.changed())
            .merge(self.fx_index.changed())
            .merge(self.is_input_fx.changed())
            .merge(self.enable_only_if_fx_has_focus.changed())
            .merge(self.param_index.changed())
            .merge(self.send_index.changed())
            .merge(self.select_exclusively.changed())
            .merge(self.transport_action.changed())
    }

    pub fn with_context<'a>(&'a self, context: &'a SessionContext) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
        }
    }

    pub fn supports_track(&self) -> bool {
        use TargetType::*;
        match self.r#type.get() {
            FxParameter | TrackVolume | TrackSendVolume | TrackPan | TrackArm | TrackSelection
            | TrackMute | TrackSolo | TrackSendPan | FxEnable | FxPreset | AllTrackFxEnable => true,
            Action | Tempo | Playrate | SelectedTrack | Transport => false,
        }
    }

    pub fn supports_send(&self) -> bool {
        use TargetType::*;
        match self.r#type.get() {
            TrackSendVolume | TrackSendPan => true,
            FxParameter | TrackVolume | TrackPan | TrackArm | TrackSelection | TrackMute
            | TrackSolo | FxEnable | FxPreset | Action | Tempo | Playrate | SelectedTrack
            | AllTrackFxEnable | Transport => false,
        }
    }

    pub fn supports_fx(&self) -> bool {
        use TargetType::*;
        match self.r#type.get() {
            FxParameter | FxEnable | FxPreset => true,
            TrackSendVolume | TrackSendPan | TrackVolume | TrackPan | TrackArm | TrackSelection
            | TrackMute | TrackSolo | Action | Tempo | Playrate | SelectedTrack
            | AllTrackFxEnable | Transport => false,
        }
    }

    /// Returns whether all conditions for this target to be active are met.
    ///
    /// Targets conditions are for example "track selected" or "FX focused".
    pub fn conditions_are_met(&self, target: &ReaperTarget) -> bool {
        if self.enable_only_if_track_selected.get() {
            if let Some(track) = target.track() {
                if !track.is_selected() {
                    return false;
                }
            }
        }
        if self.enable_only_if_fx_has_focus.get() {
            if let Some(fx) = target.fx() {
                if !fx.window_has_focus() {
                    return false;
                }
            }
        }
        true
    }

    fn command_id_label(&self) -> Cow<str> {
        match self.action.get_ref() {
            None => "-".into(),
            Some(action) => {
                if action.is_available() {
                    action.command_id().to_string().into()
                } else {
                    "<Not present>".into()
                }
            }
        }
    }

    pub fn action(&self) -> Result<Action, &'static str> {
        let action = self.action.get_ref().as_ref().ok_or("action not set")?;
        if !action.is_available() {
            return Err("action not available");
        }
        Ok(action.clone())
    }

    fn track_label(&self) -> String {
        self.track.get_ref().to_string()
    }

    pub fn action_name_label(&self) -> Cow<str> {
        match self.action().ok() {
            None => "-".into(),
            Some(a) => a.name().into_string().into(),
        }
    }
}

pub fn get_fx_param_label(fx_param: Option<&FxParameter>, index: u32) -> Cow<'static, str> {
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

pub fn get_fx_label(fx: Option<&Fx>, index: Option<u32>) -> Cow<'static, str> {
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
    context: &'a SessionContext,
}

impl<'a> TargetModelWithContext<'a> {
    /// Creates a target based on this model's properties and the current REAPER state.
    ///
    /// This returns a target regardless of the activation conditions of the target. Example:
    /// If `enable_only_if_track_selected` is `true` and the track is _not_ selected when calling
    /// this function, the target will still be created!
    ///
    /// # Errors
    ///
    /// Returns an error if not enough information is provided by the model or if something (e.g.
    /// track/FX/parameter) is not available.
    pub fn create_target(&self) -> Result<ReaperTarget, &'static str> {
        use TargetType::*;
        let target = match self.target.r#type.get() {
            Action => ReaperTarget::Action {
                action: self.target.action()?,
                invocation_type: self.target.action_invocation_type.get(),
                project: self.project(),
            },
            FxParameter => ReaperTarget::FxParameter {
                param: self.fx_param()?,
            },
            TrackVolume => ReaperTarget::TrackVolume {
                track: self.effective_track()?,
            },
            TrackSendVolume => ReaperTarget::TrackSendVolume {
                send: self.track_send()?,
            },
            TrackPan => ReaperTarget::TrackPan {
                track: self.effective_track()?,
            },
            TrackArm => ReaperTarget::TrackArm {
                track: self.effective_track()?,
            },
            TrackSelection => ReaperTarget::TrackSelection {
                track: self.effective_track()?,
                select_exclusively: self.target.select_exclusively.get(),
            },
            TrackMute => ReaperTarget::TrackMute {
                track: self.effective_track()?,
            },
            TrackSolo => ReaperTarget::TrackSolo {
                track: self.effective_track()?,
            },
            TrackSendPan => ReaperTarget::TrackSendPan {
                send: self.track_send()?,
            },
            Tempo => ReaperTarget::Tempo {
                project: self.project(),
            },
            Playrate => ReaperTarget::Playrate {
                project: self.project(),
            },
            FxEnable => ReaperTarget::FxEnable { fx: self.fx()? },
            FxPreset => ReaperTarget::FxPreset { fx: self.fx()? },
            SelectedTrack => ReaperTarget::SelectedTrack {
                project: self.project(),
            },
            AllTrackFxEnable => ReaperTarget::AllTrackFxEnable {
                track: self.effective_track()?,
            },
            Transport => ReaperTarget::Transport {
                project: self.project(),
                action: self.target.transport_action.get(),
            },
        };
        Ok(target)
    }

    pub fn is_known_to_be_discrete(&self) -> bool {
        // TODO-low use cached
        self.create_target()
            .map(|t| t.character() == TargetCharacter::Discrete)
            .unwrap_or(false)
    }

    pub fn is_known_to_be_relative(&self) -> bool {
        // TODO-low use cached
        self.create_target()
            .map(|t| t.control_type().is_relative())
            .unwrap_or(false)
    }

    pub fn is_known_to_be_roundable(&self) -> bool {
        // TODO-low use cached
        self.create_target()
            .map(|t| t.is_roundable())
            .unwrap_or(false)
    }
    // Returns an error if the FX doesn't exist.
    pub fn fx(&self) -> Result<Fx, &'static str> {
        // Actually it's not that important whether we create an index-based or GUID-based FX.
        // The session listeners will recreate and resync the FX whenever something has
        // changed anyway. But for monitoring FX it could still be good (which we don't get notified
        // about unfortunately).
        let track = self.target.track.get_ref();
        let is_input_fx = self.target.is_input_fx.get();
        let fx_index = self.target.fx_index.get().ok_or("FX index not set")?;
        if *track == VirtualTrack::Selected {
            // When the target relates to the selected track, GUID-based FX doesn't make sense.
            get_index_based_fx(&self.context, track, is_input_fx, fx_index)
        } else {
            let guid = self.target.fx_guid.get_ref().as_ref();
            match guid {
                None => get_index_based_fx(&self.context, track, is_input_fx, fx_index),
                Some(guid) => {
                    // Track by GUID because target relates to a very particular FX
                    get_guid_based_fx_by_guid_with_index_hint(
                        &self.context,
                        track,
                        is_input_fx,
                        guid,
                        fx_index,
                    )
                    // Fall back to index-based (otherwise this could have the unpleasant effect
                    // that mapping panel FX menu doesn't find any FX anymore.
                    .or_else(|_| get_index_based_fx(&self.context, track, is_input_fx, fx_index))
                }
            }
        }
    }

    pub fn project(&self) -> Project {
        self.context.project()
    }

    // TODO-low Consider returning a Cow
    pub fn effective_track(&self) -> Result<Track, &'static str> {
        get_effective_track(&self.context, self.target.track.get_ref())
    }

    // Returns an error if that send (or track) doesn't exist.
    fn track_send(&self) -> Result<TrackSend, &'static str> {
        let send_index = self.target.send_index.get().ok_or("send index not set")?;
        let track = self.effective_track()?;
        let send = track.index_based_send_by_index(send_index);
        if !send.is_available() {
            return Err("send doesn't exist");
        }
        Ok(send)
    }

    // Returns an error if that param (or FX) doesn't exist.
    fn fx_param(&self) -> Result<FxParameter, &'static str> {
        let fx = self.fx()?;
        let param = fx.parameter_by_index(self.target.param_index.get());
        if !param.is_available() {
            return Err("parameter doesn't exist");
        }
        Ok(param)
    }

    fn track_send_label(&self) -> Cow<str> {
        match self.track_send().ok() {
            None => "-".into(),
            Some(s) => s.name().into_string().into(),
        }
    }

    fn fx_label(&self) -> Cow<str> {
        get_fx_label(self.fx().ok().as_ref(), self.target.fx_index.get())
    }

    fn fx_param_label(&self) -> Cow<str> {
        get_fx_param_label(self.fx_param().ok().as_ref(), self.target.param_index.get())
    }
}

pub fn get_fx_chain(
    context: &SessionContext,
    track: &VirtualTrack,
    is_input_fx: bool,
) -> Result<FxChain, &'static str> {
    let track = get_effective_track(context, track)?;
    let result = if is_input_fx {
        track.input_fx_chain()
    } else {
        track.normal_fx_chain()
    };
    Ok(result)
}

pub fn get_index_based_fx(
    context: &SessionContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    let fx = fx_chain.fx_by_index_untracked(fx_index);
    if !fx.is_available() {
        return Err("no FX at that index");
    }
    Ok(fx)
}

pub fn get_guid_based_fx_at_index(
    context: &SessionContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    fx_chain.fx_by_index(fx_index).ok_or("no FX at that index")
}

pub fn get_guid_based_fx_by_guid_with_index_hint(
    context: &SessionContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    guid: &Guid,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    let fx = fx_chain.fx_by_guid_and_index(guid, fx_index);
    // is_available() also invalidates the index if necessary
    // TODO-low This is too implicit.
    if !fx.is_available() {
        return Err("no FX with that GUID");
    }
    Ok(fx)
}

pub fn get_effective_track(
    context: &SessionContext,
    track: &VirtualTrack,
) -> Result<Track, &'static str> {
    use VirtualTrack::*;
    let track = match track {
        This => context
            .containing_fx()
            .track()
            .cloned()
            // If this is monitoring FX, we want this to resolve to the master track since
            // in most functions, monitoring FX chain is the "input FX chain" of the master track.
            .unwrap_or_else(|| context.project().master_track()),
        Selected => context
            .project()
            .first_selected_track(IncludeMasterTrack)
            .ok_or("no track selected")?,
        Master => context.project().master_track(),
        Particular(track) => track.clone(),
    };
    Ok(track)
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
                self.fx_label(),
                self.fx_param_label()
            ),
            TrackVolume => write!(f, "Track volume\nTrack {}", self.target.track_label()),
            TrackSendVolume => write!(
                f,
                "Track send volume\nTrack {}\nSend {}",
                self.target.track_label(),
                self.track_send_label()
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
                self.track_send_label()
            ),
            Tempo => write!(f, "Master tempo"),
            Playrate => write!(f, "Master playrate"),
            FxEnable => write!(
                f,
                "Track FX enable\nTrack {}\nFX {}",
                self.target.track_label(),
                self.fx_label(),
            ),
            FxPreset => write!(
                f,
                "Track FX preset\nTrack {}\nFX {}",
                self.target.track_label(),
                self.fx_label(),
            ),
            SelectedTrack => write!(f, "Selected track",),
            AllTrackFxEnable => write!(
                f,
                "Track FX all enable\nTrack {}",
                self.target.track_label()
            ),
            Transport => write!(f, "Transport\n{}", self.target.transport_action.get()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualTrack {
    /// Current track (the one which contains the ReaLearn instance).
    This,
    /// Currently selected track.
    Selected,
    /// Master track.
    Master,
    /// A particular track.
    Particular(Track),
}

impl fmt::Display for VirtualTrack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VirtualTrack::*;
        match self {
            This => write!(f, "<This>"),
            Selected => write!(f, "<Selected>"),
            Master => write!(f, "<Master>"),
            Particular(t) => write!(f, "{}", get_track_label(t)),
        }
    }
}

/// Type of a target
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
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
    #[display(fmt = "Track FX preset (feedback since REAPER v6.13)")]
    FxPreset = 13,
    #[display(fmt = "Selected track")]
    SelectedTrack = 14,
    #[display(fmt = "Track FX all enable (no feedback)")]
    AllTrackFxEnable = 15,
    #[display(fmt = "Transport")]
    Transport = 16,
}

impl TargetType {
    pub fn from_target(target: &ReaperTarget) -> TargetType {
        use ReaperTarget::*;
        match target {
            Action { .. } => TargetType::Action,
            FxParameter { .. } => TargetType::FxParameter,
            TrackVolume { .. } => TargetType::TrackVolume,
            TrackSendVolume { .. } => TargetType::TrackSendVolume,
            TrackPan { .. } => TargetType::TrackPan,
            TrackArm { .. } => TargetType::TrackArm,
            TrackSelection { .. } => TargetType::TrackSelection,
            TrackMute { .. } => TargetType::TrackMute,
            TrackSolo { .. } => TargetType::TrackSolo,
            TrackSendPan { .. } => TargetType::TrackSendPan,
            Tempo { .. } => TargetType::Tempo,
            Playrate { .. } => TargetType::Playrate,
            FxEnable { .. } => TargetType::FxEnable,
            FxPreset { .. } => TargetType::FxPreset,
            SelectedTrack { .. } => TargetType::SelectedTrack,
            AllTrackFxEnable { .. } => TargetType::AllTrackFxEnable,
            Transport { .. } => TargetType::Transport,
        }
    }
}

fn virtualize_track(track: Track, context: &SessionContext) -> VirtualTrack {
    match context.track() {
        Some(t) if *t == track => VirtualTrack::This,
        _ => {
            if track.is_master_track() {
                VirtualTrack::Master
            } else {
                VirtualTrack::Particular(track)
            }
        }
    }
}

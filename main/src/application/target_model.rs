use crate::core::{prop, Prop};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlType, Target};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{Action, Fx, FxParameter, Project, Track, TrackSend};

use rx_util::{Event, UnitEvent};
use serde::{Deserialize, Serialize};

use crate::application::VirtualControlElementType;
use crate::domain::{
    get_effective_track, get_fx, get_fx_chain, get_fx_param, get_track_send, ActionInvocationType,
    CompoundMappingTarget, FxAnchor, FxDescriptor, ProcessorContext, ReaperTarget, TrackAnchor,
    TrackDescriptor, TransportAction, UnresolvedCompoundMappingTarget, UnresolvedReaperTarget,
    VirtualControlElement, VirtualFx, VirtualTarget, VirtualTrack,
};
use serde_repr::*;
use std::borrow::Cow;

use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // # For all targets
    pub category: Prop<TargetCategory>,
    // # For virtual targets
    pub control_element_type: Prop<VirtualControlElementType>,
    pub control_element_index: Prop<u32>,
    // # For REAPER targets
    // TODO-low Rename this to reaper_target_type
    pub r#type: Prop<ReaperTargetType>,
    // # For action targets only
    // TODO-low Maybe replace Action with just command ID and/or command name
    pub action: Prop<Option<Action>>,
    pub action_invocation_type: Prop<ActionInvocationType>,
    // # For track targets
    // TODO-low Maybe replace VirtualTrack::Particular(track) with Particular(GUID)
    pub track: Prop<VirtualTrack>,
    pub enable_only_if_track_selected: Prop<bool>,
    // # For track FX targets
    pub fx: Prop<Option<VirtualFx>>,
    pub enable_only_if_fx_has_focus: Prop<bool>,
    // # For track FX parameter targets
    pub param_index: Prop<u32>,
    // # For track send targets
    pub send_index: Prop<Option<u32>>,
    // # For track selection targets
    pub select_exclusively: Prop<bool>,
    // # For transport target
    pub transport_action: Prop<TransportAction>,
    // # For "Load FX snapshot" target
    pub fx_snapshot: Prop<Option<FxSnapshot>>,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            category: prop(TargetCategory::default()),
            control_element_type: prop(VirtualControlElementType::default()),
            control_element_index: prop(0),
            r#type: prop(ReaperTargetType::FxParameter),
            action: prop(None),
            action_invocation_type: prop(ActionInvocationType::default()),
            track: prop(VirtualTrack::This),
            enable_only_if_track_selected: prop(false),
            fx: prop(None),
            enable_only_if_fx_has_focus: prop(false),
            param_index: prop(0),
            send_index: prop(None),
            select_exclusively: prop(false),
            transport_action: prop(TransportAction::default()),
            fx_snapshot: prop(None),
        }
    }
}

impl TargetModel {
    pub fn take_fx_snapshot(&self, context: &ProcessorContext) -> Result<FxSnapshot, &'static str> {
        let fx = self.with_context(context).fx()?;
        let fx_info = fx.info();
        let fx_snapshot = FxSnapshot {
            effect_type: if fx_info.sub_type_expression.is_empty() {
                fx_info.type_expression
            } else {
                fx_info.sub_type_expression
            },
            effect_name: fx_info.effect_name,
            preset_name: fx.preset_name().map(|n| n.into_string()),
            chunk: Rc::new(fx.tag_chunk().content().to_owned()),
        };
        Ok(fx_snapshot)
    }

    pub fn invalidate_fx_index(&mut self, context: &ProcessorContext) {
        if !self.supports_fx() {
            return;
        }
        if let Ok(actual_fx) = self.with_context(context).fx() {
            let new_virtual_fx = match self.fx.get_ref() {
                Some(virtual_fx) => {
                    match virtual_fx {
                        VirtualFx::Particular {
                            is_input_fx,
                            anchor,
                        } => match anchor {
                            FxAnchor::IdOrIndex(guid, _) => Some(VirtualFx::Particular {
                                is_input_fx: *is_input_fx,
                                anchor: FxAnchor::IdOrIndex(*guid, actual_fx.index()),
                            }),
                            _ => None,
                        },
                        // No update necessary
                        VirtualFx::Focused => None,
                    }
                }
                // Shouldn't happen
                None => None,
            };
            if let Some(virtual_fx) = new_virtual_fx {
                self.fx.set(Some(virtual_fx));
            }
        }
    }

    pub fn apply_from_target(&mut self, target: &ReaperTarget, context: &ProcessorContext) {
        use ReaperTarget::*;
        self.category.set(TargetCategory::Reaper);
        self.r#type.set(ReaperTargetType::from_target(target));
        if let Some(actual_fx) = target.fx() {
            let virtual_fx = virtualize_fx(actual_fx, context);
            self.fx.set(Some(virtual_fx));
            let track = if let Some(track) = actual_fx.track() {
                track.clone()
            } else {
                // Must be monitoring FX. In this case we want the master track (it's REAPER's
                // convention and ours).
                context.project_or_current_project().master_track()
            };
            self.track.set(virtualize_track(track, context));
        } else if let Some(track) = target.track() {
            self.track.set(virtualize_track(track.clone(), context));
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
            Transport { action, .. } => {
                self.transport_action.set(*action);
            }
            _ => {}
        };
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl UnitEvent {
        self.category
            .changed()
            .merge(self.r#type.changed())
            .merge(self.action.changed())
            .merge(self.action_invocation_type.changed())
            .merge(self.track.changed())
            .merge(self.enable_only_if_track_selected.changed())
            .merge(self.fx.changed())
            .merge(self.enable_only_if_fx_has_focus.changed())
            .merge(self.param_index.changed())
            .merge(self.send_index.changed())
            .merge(self.select_exclusively.changed())
            .merge(self.transport_action.changed())
            .merge(self.control_element_type.changed())
            .merge(self.control_element_index.changed())
            .merge(self.fx_snapshot.changed())
    }

    fn track_descriptor(&self) -> TrackDescriptor {
        TrackDescriptor {
            track: self.track.get_ref().clone(),
            enable_only_if_track_selected: self.enable_only_if_track_selected.get(),
        }
    }

    fn fx_descriptor(&self) -> Result<FxDescriptor, &'static str> {
        let desc = FxDescriptor {
            track_descriptor: self.track_descriptor(),
            enable_only_if_fx_has_focus: self.enable_only_if_fx_has_focus.get(),
            fx: self.fx.get_ref().clone().ok_or("FX not set")?,
        };
        Ok(desc)
    }

    pub fn create_target(&self) -> Result<UnresolvedCompoundMappingTarget, &'static str> {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => {
                use ReaperTargetType::*;
                let target = match self.r#type.get() {
                    Action => UnresolvedReaperTarget::Action {
                        action: self.action()?,
                        invocation_type: self.action_invocation_type.get(),
                    },
                    FxParameter => UnresolvedReaperTarget::FxParameter {
                        fx_descriptor: self.fx_descriptor()?,
                        fx_param_index: self.param_index.get(),
                    },
                    TrackVolume => UnresolvedReaperTarget::TrackVolume {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackSendVolume => UnresolvedReaperTarget::TrackSendVolume {
                        track_descriptor: self.track_descriptor(),
                        send_index: self.send_index.get().ok_or("send index not set")?,
                    },
                    TrackPan => UnresolvedReaperTarget::TrackPan {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackWidth => UnresolvedReaperTarget::TrackWidth {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackArm => UnresolvedReaperTarget::TrackArm {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackSelection => UnresolvedReaperTarget::TrackSelection {
                        track_descriptor: self.track_descriptor(),
                        select_exclusively: self.select_exclusively.get(),
                    },
                    TrackMute => UnresolvedReaperTarget::TrackMute {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackSolo => UnresolvedReaperTarget::TrackSolo {
                        track_descriptor: self.track_descriptor(),
                    },
                    TrackSendPan => UnresolvedReaperTarget::TrackSendPan {
                        track_descriptor: self.track_descriptor(),
                        send_index: self.send_index.get().ok_or("send index not set")?,
                    },
                    TrackSendMute => UnresolvedReaperTarget::TrackSendMute {
                        track_descriptor: self.track_descriptor(),
                        send_index: self.send_index.get().ok_or("send index not set")?,
                    },
                    Tempo => UnresolvedReaperTarget::Tempo,
                    Playrate => UnresolvedReaperTarget::Playrate,
                    FxEnable => UnresolvedReaperTarget::FxEnable {
                        fx_descriptor: self.fx_descriptor()?,
                    },
                    FxPreset => UnresolvedReaperTarget::FxPreset {
                        fx_descriptor: self.fx_descriptor()?,
                    },
                    SelectedTrack => UnresolvedReaperTarget::SelectedTrack,
                    AllTrackFxEnable => UnresolvedReaperTarget::AllTrackFxEnable {
                        track_descriptor: self.track_descriptor(),
                    },
                    Transport => UnresolvedReaperTarget::Transport {
                        action: self.transport_action.get(),
                    },
                    LoadFxSnapshot => UnresolvedReaperTarget::LoadFxPreset {
                        fx_descriptor: self.fx_descriptor()?,
                        chunk: self
                            .fx_snapshot
                            .get_ref()
                            .as_ref()
                            .ok_or("FX chunk not set")?
                            .chunk
                            .clone(),
                    },
                };
                Ok(UnresolvedCompoundMappingTarget::Reaper(target))
            }
            Virtual => {
                let virtual_target = VirtualTarget::new(self.create_control_element());
                Ok(UnresolvedCompoundMappingTarget::Virtual(virtual_target))
            }
        }
    }

    pub fn with_context<'a>(&'a self, context: &'a ProcessorContext) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
        }
    }

    pub fn supports_track(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_track()
    }

    pub fn supports_send(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_send()
    }

    pub fn supports_fx(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_fx()
    }

    pub fn create_control_element(&self) -> VirtualControlElement {
        self.control_element_type
            .get()
            .create_control_element(self.control_element_index.get())
    }

    fn is_reaper(&self) -> bool {
        self.category.get() == TargetCategory::Reaper
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

pub fn get_virtual_fx_label(fx: Option<&Fx>, virtual_fx: Option<&VirtualFx>) -> Cow<'static, str> {
    let virtual_fx = match virtual_fx {
        None => return "<None>".into(),
        Some(f) => f,
    };
    match virtual_fx {
        VirtualFx::Focused => "<Focused>".into(),
        VirtualFx::Particular { anchor, .. } => get_optional_fx_label(anchor, fx).into(),
    }
}

pub fn get_optional_fx_label(anchor: &FxAnchor, fx: Option<&Fx>) -> String {
    match fx {
        None => format!("<Not present> ({})", anchor),
        Some(fx) => get_fx_label(fx.index(), fx),
    }
}

pub fn get_fx_label(index: u32, fx: &Fx) -> String {
    format!("{}. {}", index + 1, fx.name().to_str())
}

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    context: &'a ProcessorContext,
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
    pub fn create_target(&self) -> Result<CompoundMappingTarget, &'static str> {
        let unresolved = self.target.create_target()?;
        unresolved.resolve(&self.context)
    }

    pub fn is_known_to_be_roundable(&self) -> bool {
        // TODO-low use cached
        self.create_target()
            .map(|t| {
                matches!(
                    t.control_type(),
                    ControlType::AbsoluteContinuousRoundable { .. }
                )
            })
            .unwrap_or(false)
    }
    // Returns an error if the FX doesn't exist.
    pub fn fx(&self) -> Result<Fx, &'static str> {
        get_fx(&self.context, &self.target.fx_descriptor()?)
    }

    pub fn project(&self) -> Project {
        self.context.project_or_current_project()
    }

    // TODO-low Consider returning a Cow
    pub fn effective_track(&self) -> Result<Track, &'static str> {
        get_effective_track(&self.context, self.target.track.get_ref())
    }

    // Returns an error if that send (or track) doesn't exist.
    fn track_send(&self) -> Result<TrackSend, &'static str> {
        get_track_send(
            &self.context,
            self.target.track.get_ref(),
            self.target.send_index.get().ok_or("send index not set")?,
        )
    }

    // Returns an error if that param (or FX) doesn't exist.
    fn fx_param(&self) -> Result<FxParameter, &'static str> {
        get_fx_param(
            &self.context,
            &self.target.fx_descriptor()?,
            self.target.param_index.get(),
        )
    }

    fn track_send_label(&self) -> Cow<str> {
        match self.track_send().ok() {
            None => "-".into(),
            Some(s) => s.name().into_string().into(),
        }
    }

    fn fx_label(&self) -> Cow<str> {
        get_virtual_fx_label(self.fx().ok().as_ref(), self.target.fx.get_ref().as_ref())
    }

    fn fx_param_label(&self) -> Cow<str> {
        get_fx_param_label(self.fx_param().ok().as_ref(), self.target.param_index.get())
    }

    fn track_label(&self) -> String {
        self.target
            .track
            .get_ref()
            .with_context(self.context)
            .to_string()
    }
}

pub fn get_guid_based_fx_at_index(
    context: &ProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx)?;
    fx_chain.fx_by_index(fx_index).ok_or("no FX at that index")
}

impl<'a> Display for TargetModelWithContext<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                use ReaperTargetType::*;
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
                    TrackWidth => write!(f, "Track width\nTrack {}", self.track_label()),
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
                    TrackSendMute => write!(
                        f,
                        "Track send mute\nTrack {}\nSend {}",
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
                    SelectedTrack => write!(f, "Selected track",),
                    AllTrackFxEnable => {
                        write!(f, "Track FX all enable\nTrack {}", self.track_label())
                    }
                    Transport => write!(f, "Transport\n{}", self.target.transport_action.get()),
                    LoadFxSnapshot => write!(
                        f,
                        "Load FX snapshot\n{}",
                        self.target
                            .fx_snapshot
                            .get_ref()
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ),
                }
            }
            Virtual => write!(f, "Virtual\n{}", self.target.create_control_element()),
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
pub enum ReaperTargetType {
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
    #[display(fmt = "Track width")]
    TrackWidth = 17,
    #[display(fmt = "Track send mute (no feedback)")]
    TrackSendMute = 18,
    #[display(fmt = "Load FX snapshot (experimental)")]
    LoadFxSnapshot = 19,
}

impl Default for ReaperTargetType {
    fn default() -> Self {
        ReaperTargetType::FxParameter
    }
}

impl ReaperTargetType {
    pub fn from_target(target: &ReaperTarget) -> ReaperTargetType {
        use ReaperTarget::*;
        match target {
            Action { .. } => ReaperTargetType::Action,
            FxParameter { .. } => ReaperTargetType::FxParameter,
            TrackVolume { .. } => ReaperTargetType::TrackVolume,
            TrackSendVolume { .. } => ReaperTargetType::TrackSendVolume,
            TrackPan { .. } => ReaperTargetType::TrackPan,
            TrackWidth { .. } => ReaperTargetType::TrackWidth,
            TrackArm { .. } => ReaperTargetType::TrackArm,
            TrackSelection { .. } => ReaperTargetType::TrackSelection,
            TrackMute { .. } => ReaperTargetType::TrackMute,
            TrackSolo { .. } => ReaperTargetType::TrackSolo,
            TrackSendPan { .. } => ReaperTargetType::TrackSendPan,
            TrackSendMute { .. } => ReaperTargetType::TrackSendMute,
            Tempo { .. } => ReaperTargetType::Tempo,
            Playrate { .. } => ReaperTargetType::Playrate,
            FxEnable { .. } => ReaperTargetType::FxEnable,
            FxPreset { .. } => ReaperTargetType::FxPreset,
            SelectedTrack { .. } => ReaperTargetType::SelectedTrack,
            AllTrackFxEnable { .. } => ReaperTargetType::AllTrackFxEnable,
            Transport { .. } => ReaperTargetType::Transport,
            LoadFxSnapshot { .. } => ReaperTargetType::LoadFxSnapshot,
        }
    }

    pub fn supports_track(self) -> bool {
        use ReaperTargetType::*;
        match self {
            FxParameter | TrackVolume | TrackSendVolume | TrackPan | TrackWidth | TrackArm
            | TrackSelection | TrackMute | TrackSolo | TrackSendPan | TrackSendMute | FxEnable
            | FxPreset | AllTrackFxEnable | LoadFxSnapshot => true,
            Action | Tempo | Playrate | SelectedTrack | Transport => false,
        }
    }

    pub fn supports_fx(self) -> bool {
        use ReaperTargetType::*;
        match self {
            FxParameter | FxEnable | FxPreset | LoadFxSnapshot => true,
            TrackSendVolume | TrackSendPan | TrackSendMute | TrackVolume | TrackPan
            | TrackWidth | TrackArm | TrackSelection | TrackMute | TrackSolo | Action | Tempo
            | Playrate | SelectedTrack | AllTrackFxEnable | Transport => false,
        }
    }

    pub fn supports_send(self) -> bool {
        use ReaperTargetType::*;
        match self {
            TrackSendVolume | TrackSendPan | TrackSendMute => true,
            FxParameter | TrackVolume | TrackPan | TrackWidth | TrackArm | TrackSelection
            | TrackMute | TrackSolo | FxEnable | FxPreset | Action | Tempo | Playrate
            | SelectedTrack | AllTrackFxEnable | Transport | LoadFxSnapshot => false,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetCategory {
    #[serde(rename = "reaper")]
    #[display(fmt = "REAPER")]
    Reaper,
    #[serde(rename = "virtual")]
    #[display(fmt = "Virtual")]
    Virtual,
}

impl Default for TargetCategory {
    fn default() -> Self {
        TargetCategory::Reaper
    }
}

fn virtualize_track(track: Track, context: &ProcessorContext) -> VirtualTrack {
    let own_track = context
        .track()
        .cloned()
        .unwrap_or_else(|| context.project_or_current_project().master_track());
    if own_track == track {
        VirtualTrack::This
    } else if track.is_master_track() {
        VirtualTrack::Master
    } else if context.is_on_monitoring_fx_chain() {
        // Doesn't make sense to refer to tracks via ID if we are on monitoring FX chain.
        VirtualTrack::Particular(TrackAnchor::Index(track.index().expect("impossible")))
    } else {
        VirtualTrack::Particular(TrackAnchor::Id(*track.guid()))
    }
}

fn virtualize_fx(fx: &Fx, context: &ProcessorContext) -> VirtualFx {
    VirtualFx::Particular {
        is_input_fx: fx.is_input_fx(),
        anchor: if context.is_on_monitoring_fx_chain() {
            // Doesn't make sense to refer to FX via UUID if we are on monitoring FX chain.
            FxAnchor::Index(fx.index())
        } else if let Some(guid) = fx.guid() {
            FxAnchor::Id(guid, Some(fx.index()))
        } else {
            // Don't know how that can happen but let's handle it gracefully.
            FxAnchor::IdOrIndex(None, fx.index())
        },
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, IntoEnumIterator, TryFromPrimitive, IntoPrimitive, Display,
)]
#[repr(usize)]
pub enum TrackAnchorType {
    #[display(fmt = "By ID")]
    Id,
    #[display(fmt = "By name")]
    Name,
    #[display(fmt = "By position")]
    Index,
    #[display(fmt = "By ID or name")]
    IdOrName,
}

impl TrackAnchorType {
    pub fn from_anchor(anchor: &TrackAnchor) -> Self {
        use TrackAnchor::*;
        match anchor {
            IdOrName(_, _) => TrackAnchorType::IdOrName,
            Id(_) => TrackAnchorType::Id,
            Name(_) => TrackAnchorType::Name,
            Index(_) => TrackAnchorType::Index,
        }
    }

    pub fn to_anchor(self, track: Track) -> Result<TrackAnchor, &'static str> {
        use TrackAnchorType::*;
        let get_name = || {
            track
                .name()
                .map(|n| n.into_string())
                .ok_or("track must have name")
        };
        let anchor = match self {
            Id => TrackAnchor::Id(*track.guid()),
            Name => TrackAnchor::Name(get_name()?),
            Index => TrackAnchor::Index(track.index().ok_or("track must have index")?),
            IdOrName => TrackAnchor::IdOrName(*track.guid(), get_name()?),
        };
        Ok(anchor)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum FxAnchorType {
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    Id,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    Name,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    Index,
    #[display(fmt = "By ID or pos")]
    #[serde(rename = "id-or-index")]
    IdOrIndex,
}

impl FxAnchorType {
    pub fn from_anchor(anchor: &FxAnchor) -> Self {
        use FxAnchor::*;
        match anchor {
            Id(_, _) => FxAnchorType::Id,
            Name(_) => FxAnchorType::Name,
            Index(_) => FxAnchorType::Index,
            IdOrIndex(_, _) => FxAnchorType::IdOrIndex,
        }
    }

    pub fn to_anchor(self, fx: &Fx) -> Result<FxAnchor, &'static str> {
        use FxAnchorType::*;
        let anchor = match self {
            Id => FxAnchor::Id(fx.guid().ok_or("FX not GUID-based")?, Some(fx.index())),
            Name => FxAnchor::Name(fx.name().into_string()),
            Index => FxAnchor::Index(fx.index()),
            IdOrIndex => FxAnchor::IdOrIndex(fx.guid(), fx.index()),
        };
        Ok(anchor)
    }
}

#[derive(PartialEq, Debug)]
pub struct FxSnapshot {
    pub effect_type: String,
    pub effect_name: String,
    pub preset_name: Option<String>,
    pub chunk: Rc<String>,
}

impl Clone for FxSnapshot {
    fn clone(&self) -> Self {
        Self {
            effect_type: self.effect_type.clone(),
            effect_name: self.effect_name.clone(),
            preset_name: self.preset_name.clone(),
            // We want a totally detached duplicate.
            chunk: Rc::new((*self.chunk).clone()),
        }
    }
}

impl Display for FxSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let fmt_size = bytesize::ByteSize(self.chunk.len() as _);
        write!(
            f,
            "{} | {} | {}",
            self.preset_name.as_ref().map(|s| s.as_str()).unwrap_or("-"),
            fmt_size,
            self.effect_name,
        )
    }
}

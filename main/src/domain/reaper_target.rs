use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, ActionCharacter, Fx, FxParameter, FxParameterCharacter, Pan, PlayRate, Project, Reaper,
    Tempo, Track, TrackSend, Volume,
};
use reaper_medium::{
    Bpm, CommandId, Db, FxPresetRef, MasterTrackBehavior, NormalizedPlayRate, PlaybackSpeedFactor,
    ReaperNormalizedFxParamValue, UndoBehavior,
};
use rx_util::{BoxedUnitEvent, Event};
use rxrust::prelude::*;

use serde_repr::{Deserialize_repr, Serialize_repr};
use slog::warn;

use std::convert::TryInto;
use std::rc::Rc;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TargetCharacter {
    Trigger,
    Switch,
    Discrete,
    Continuous,
}

/// This is a ReaLearn target.
///
/// Unlike TargetModel, the real target has everything resolved already (e.g. track and FX) and
/// is immutable.
#[derive(Clone, Debug, PartialEq)]
pub enum ReaperTarget {
    Action {
        action: Action,
        invocation_type: ActionInvocationType,
        project: Project,
    },
    FxParameter {
        param: FxParameter,
    },
    TrackVolume {
        track: Track,
    },
    TrackSendVolume {
        send: TrackSend,
    },
    TrackPan {
        track: Track,
    },
    TrackArm {
        track: Track,
    },
    TrackSelection {
        track: Track,
        select_exclusively: bool,
    },
    TrackMute {
        track: Track,
    },
    TrackSolo {
        track: Track,
    },
    TrackSendPan {
        send: TrackSend,
    },
    Tempo {
        project: Project,
    },
    Playrate {
        project: Project,
    },
    FxEnable {
        fx: Fx,
    },
    FxPreset {
        fx: Fx,
    },
    SelectedTrack {
        project: Project,
    },
    AllTrackFxEnable {
        track: Track,
    },
}

impl ReaperTarget {
    pub fn touched() -> impl Event<Rc<ReaperTarget>> {
        use ReaperTarget::*;
        let reaper = Reaper::get();
        observable::empty()
            .merge(
                reaper
                    .fx_parameter_touched()
                    .map(move |param| FxParameter { param }.into()),
            )
            .merge(
                reaper
                    .fx_enabled_changed()
                    .map(move |fx| FxEnable { fx }.into()),
            )
            .merge(
                reaper
                    .fx_preset_changed()
                    .map(move |fx| FxPreset { fx }.into()),
            )
            .merge(
                reaper
                    .track_volume_touched()
                    .map(move |track| TrackVolume { track }.into()),
            )
            .merge(
                reaper
                    .track_pan_touched()
                    .map(move |track| TrackPan { track }.into()),
            )
            .merge(
                reaper
                    .track_arm_changed()
                    .map(move |track| TrackArm { track }.into()),
            )
            .merge(reaper.track_selected_changed().map(move |track| {
                TrackSelection {
                    track,
                    select_exclusively: false,
                }
                .into()
            }))
            .merge(
                reaper
                    .track_mute_touched()
                    .map(move |track| TrackMute { track }.into()),
            )
            .merge(
                reaper
                    .track_solo_changed()
                    .map(move |track| TrackSolo { track }.into()),
            )
            .merge(
                reaper
                    .track_send_volume_touched()
                    .map(move |send| TrackSendVolume { send }.into()),
            )
            .merge(
                reaper
                    .track_send_pan_touched()
                    .map(move |send| TrackSendPan { send }.into()),
            )
            .merge(reaper.action_invoked().map(move |action| {
                Action {
                    action: (*action).clone(),
                    invocation_type: ActionInvocationType::Trigger,
                    project: reaper.current_project(),
                }
                .into()
            }))
            .merge(
                reaper
                    .master_tempo_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Tempo {
                            project: reaper.current_project(),
                        }
                        .into()
                    }),
            )
            .merge(
                reaper
                    .master_playrate_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Playrate {
                            project: reaper.current_project(),
                        }
                        .into()
                    }),
            )
    }

    pub fn open(&self) {
        if let ReaperTarget::Action {
            action: _, project, ..
        } = self
        {
            // Just open action window
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40605))
                .invoke_as_trigger(Some(*project));
            return;
        }
        if let Some(fx) = self.fx() {
            fx.show_in_floating_window();
            return;
        }
        if let Some(track) = self.track() {
            track.select_exclusively();
            // Scroll to track
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()));
        }
    }

    pub fn character(&self) -> TargetCharacter {
        use ReaperTarget::*;
        use TargetCharacter::*;
        match self {
            Action {
                action,
                invocation_type: _,
                ..
            } => match action.character() {
                ActionCharacter::Toggle => Trigger,
                ActionCharacter::Trigger => Switch,
            },
            FxParameter { param } => match param.character() {
                FxParameterCharacter::Toggle => Switch,
                FxParameterCharacter::Discrete => Discrete,
                FxParameterCharacter::Continuous => Continuous,
            },
            TrackVolume { .. } => Continuous,
            TrackSendVolume { .. } => Continuous,
            TrackPan { .. } => Continuous,
            TrackArm { .. } => Switch,
            TrackSelection { .. } => Switch,
            TrackMute { .. } => Switch,
            TrackSolo { .. } => Switch,
            TrackSendPan { .. } => Continuous,
            Tempo { .. } => Continuous,
            Playrate { .. } => Continuous,
            FxEnable { .. } => Switch,
            FxPreset { .. } => Discrete,
            SelectedTrack { .. } => Discrete,
            AllTrackFxEnable { .. } => Switch,
        }
    }

    pub fn is_roundable(&self) -> bool {
        matches!(self.control_type(), ControlType::AbsoluteContinuousRoundable { .. })
    }

    /// Formats the value completely (including a possible unit).
    pub fn format_value(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            Action { .. } => "".to_string(),
            FxParameter { param } => param
                // Even if a REAPER-normalized value can take numbers > 1.0, the usual value range
                // is in fact normalized in the classical sense (unit interval).
                .format_reaper_normalized_value(ReaperNormalizedFxParamValue::new(value.get()))
                .map(|s| s.into_string())
                .unwrap_or_else(|_| self.format_value_generic(value)),
            TrackVolume { .. } | TrackSendVolume { .. } => format_value_as_db(value),
            TrackPan { .. } | TrackSendPan { .. } => format_value_as_pan(value),
            FxEnable { .. }
            | TrackArm { .. }
            | TrackMute { .. }
            | TrackSelection { .. }
            | TrackSolo { .. } => format_value_as_on_off(value).to_string(),
            FxPreset { fx } => match convert_unit_value_to_preset_index(fx, value) {
                None => "<No preset>".to_string(),
                Some(i) => (i + 1).to_string(),
            },
            SelectedTrack { project } => match convert_unit_value_to_track_index(*project, value) {
                None => "<Master track>".to_string(),
                Some(i) => (i + 1).to_string(),
            },
            _ => self.format_value_generic(value),
        }
    }

    fn format_value_generic(&self, value: UnitValue) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value),
            self.value_unit()
        )
    }

    /// Formats the given value without unit.
    pub fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => format_value_as_db_without_unit(value),
            TrackPan { .. } | TrackSendPan { .. } => format_value_as_pan(value),
            Tempo { .. } => format_value_as_bpm_without_unit(value),
            Playrate { .. } => format_value_as_playback_speed_factor_without_unit(value),
            _ => format_as_percentage_without_unit(value),
        }
    }

    /// Formats the given step size without unit.
    pub fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            Tempo { .. } => format_step_size_as_bpm_without_unit(step_size),
            Playrate { .. } => format_step_size_as_playback_speed_factor_without_unit(step_size),
            _ => format_as_percentage_without_unit(step_size),
        }
    }

    /// This converts the given normalized value to a discrete value.
    ///
    /// Used for displaying discrete target values in edit fields.
    /// Must be implemented for discrete targets only which don't support parsing according to
    /// `can_parse_values()`, e.g. FX preset. This target reports a step size. If we want to
    /// display an increment or a particular value in an edit field, we don't show normalized
    /// values of course but a discrete number, by using this function. Should be the reverse of
    /// `convert_discrete_value_to_unit_value()` because latter is used for parsing.
    ///
    /// In case the target wants increments, this takes 63 as the highest possible value.
    ///
    /// # Errors
    ///
    /// Returns an error if this target doesn't report a step size.
    pub fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
    ) -> Result<u32, &'static str> {
        if self.control_type().is_relative() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        use ReaperTarget::*;
        let result = match self {
            FxPreset { fx } => convert_unit_value_to_preset_index(fx, input)
                .map(|i| i + 1)
                .unwrap_or(0),
            SelectedTrack { project } => convert_unit_value_to_track_index(*project, input)
                .map(|i| i + 1)
                .unwrap_or(0),
            FxParameter { param } => {
                // Example (target step size = 0.10):
                // - 0    => 0
                // - 0.05 => 1
                // - 0.10 => 1
                // - 0.15 => 2
                // - 0.20 => 2
                let step_size = param.step_size().ok_or("not supported")?;
                (input.get() / step_size).round() as _
            }
            _ => return Err("not supported"),
        };
        Ok(result)
    }

    /// Like `convert_unit_value_to_discrete_value()` but in the other direction.
    ///
    /// Used for parsing discrete values of discrete targets that can't do real parsing according to
    /// `can_parse_values()`.
    pub fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
    ) -> Result<UnitValue, &'static str> {
        if self.control_type().is_relative() {
            return (value as f64 / 63.0).try_into();
        }
        use ReaperTarget::*;
        let result = match self {
            FxPreset { fx } => {
                let index = if value == 0 { None } else { Some(value - 1) };
                convert_preset_index_to_unit_value(fx, index)
            }
            SelectedTrack { project } => {
                let index = if value == 0 { None } else { Some(value - 1) };
                convert_track_index_to_unit_value(*project, index)
            }
            FxParameter { param } => {
                let step_size = param.step_size().ok_or("not supported")?;
                (value as f64 * step_size).try_into()?
            }
            _ => return Err("not supported"),
        };
        Ok(result)
    }

    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    pub fn hide_formatted_value(&self) -> bool {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. }
            | TrackSendVolume { .. }
            | TrackPan { .. }
            | TrackSendPan { .. }
            | Playrate { .. }
            | Tempo { .. } => true,
            _ => false,
        }
    }

    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    pub fn hide_formatted_step_size(&self) -> bool {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. }
            | TrackSendVolume { .. }
            | TrackPan { .. }
            | TrackSendPan { .. }
            | Playrate { .. }
            | Tempo { .. } => true,
            _ => false,
        }
    }

    /// Parses the given text as a target value and returns it as unit value.
    pub fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => parse_value_from_db(text),
            TrackPan { .. } | TrackSendPan { .. } => parse_value_from_pan(text),
            Playrate { .. } => parse_value_from_playback_speed_factor(text),
            Tempo { .. } => parse_value_from_bpm(text),
            FxPreset { .. } | SelectedTrack { .. } => self.parse_value_from_discrete_value(text),
            FxParameter { param } if param.character() == FxParameterCharacter::Discrete => {
                self.parse_value_from_discrete_value(text)
            }
            _ => parse_from_percentage(text),
        }
    }

    /// Parses the given text as a target step size and returns it as unit value.
    pub fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            Playrate { .. } => parse_step_size_from_playback_speed_factor(text),
            Tempo { .. } => parse_step_size_from_bpm(text),
            FxPreset { .. } | SelectedTrack { .. } => self.parse_value_from_discrete_value(text),
            FxParameter { param } if param.character() == FxParameterCharacter::Discrete => {
                self.parse_value_from_discrete_value(text)
            }
            _ => parse_from_percentage(text),
        }
    }

    fn parse_value_from_discrete_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(text.parse().map_err(|_| "not a discrete value")?)
    }

    pub fn value_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => "dB",
            TrackPan { .. } | TrackSendPan { .. } => "",
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            _ => "%",
        }
    }

    pub fn step_size_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackPan { .. } | TrackSendPan { .. } => "",
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            _ => "%",
        }
    }

    pub fn project(&self) -> Option<Project> {
        use ReaperTarget::*;
        let project = match self {
            Action { .. } => return None,
            FxParameter { param } => param.fx().project()?,
            TrackVolume { track }
            | TrackPan { track }
            | TrackArm { track }
            | TrackSelection { track, .. }
            | TrackMute { track }
            | TrackSolo { track }
            | AllTrackFxEnable { track } => track.project(),
            TrackSendPan { send } | TrackSendVolume { send } => send.source_track().project(),
            Tempo { project } | Playrate { project } | SelectedTrack { project } => *project,
            FxEnable { fx } | FxPreset { fx } => fx.project()?,
        };
        Some(project)
    }

    pub fn track(&self) -> Option<&Track> {
        use ReaperTarget::*;
        let track = match self {
            FxParameter { param } => param.fx().track()?,
            TrackVolume { track } => track,
            TrackSendVolume { send } => send.source_track(),
            TrackPan { track } => track,
            TrackArm { track } => track,
            TrackSelection { track, .. } => track,
            TrackMute { track } => track,
            TrackSolo { track } => track,
            TrackSendPan { send } => send.source_track(),
            FxEnable { fx } => fx.track()?,
            FxPreset { fx } => fx.track()?,
            AllTrackFxEnable { track } => track,
            Action { .. } | Tempo { .. } | Playrate { .. } | SelectedTrack { .. } => return None,
        };
        Some(track)
    }

    pub fn fx(&self) -> Option<&Fx> {
        use ReaperTarget::*;
        let fx = match self {
            FxParameter { param } => param.fx(),
            FxEnable { fx } => fx,
            FxPreset { fx } => fx,
            Action { .. }
            | TrackVolume { .. }
            | TrackSendVolume { .. }
            | TrackPan { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackSolo { .. }
            | TrackSendPan { .. }
            | Tempo { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. } => return None,
        };
        Some(fx)
    }

    pub fn send(&self) -> Option<&TrackSend> {
        use ReaperTarget::*;
        let send = match self {
            TrackSendPan { send } | TrackSendVolume { send } => send,
            FxParameter { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | Action { .. }
            | TrackVolume { .. }
            | TrackPan { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackSolo { .. }
            | Tempo { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. } => return None,
        };
        Some(send)
    }

    pub fn control(&self, value: ControlValue) -> Result<(), &'static str> {
        use ControlValue::*;
        use ReaperTarget::*;
        match self {
            Action {
                action,
                invocation_type,
                project,
            } => match value {
                Absolute(v) => match invocation_type {
                    ActionInvocationType::Trigger => {
                        if !v.is_zero() {
                            action.invoke(v.get(), false, Some(*project));
                        }
                    }
                    ActionInvocationType::Absolute => action.invoke(v.get(), false, Some(*project)),
                    ActionInvocationType::Relative => {
                        return Err("relative invocation type can't take absolute values");
                    }
                },
                Relative(i) => {
                    if let ActionInvocationType::Relative = invocation_type {
                        action.invoke(i.get() as f64, true, Some(*project));
                    } else {
                        return Err("relative values need relative invocation type");
                    }
                }
            },
            FxParameter { param } => {
                // It's okay to just convert this to a REAPER-normalized value. We don't support
                // values above the maximum (or buggy plug-ins).
                let v = ReaperNormalizedFxParamValue::new(value.as_absolute()?.get());
                param.set_reaper_normalized_value(v).unwrap();
            }
            TrackVolume { track } => {
                let volume = Volume::from_soft_normalized_value(value.as_absolute()?.get());
                track.set_volume(volume);
            }
            TrackSendVolume { send } => {
                let volume = Volume::from_soft_normalized_value(value.as_absolute()?.get());
                send.set_volume(volume);
            }
            TrackPan { track } => {
                let pan = Pan::from_normalized_value(value.as_absolute()?.get());
                track.set_pan(pan);
            }
            TrackArm { track } => {
                if value.as_absolute()?.is_zero() {
                    track.disarm(false);
                } else {
                    track.arm(false);
                }
            }
            TrackSelection {
                track,
                select_exclusively,
            } => {
                if value.as_absolute()?.is_zero() {
                    track.unselect();
                } else if *select_exclusively {
                    track.select_exclusively();
                } else {
                    track.select();
                }
                track.scroll_mixer();
            }
            TrackMute { track } => {
                if value.as_absolute()?.is_zero() {
                    track.unmute();
                } else {
                    track.mute();
                }
            }
            TrackSolo { track } => {
                if value.as_absolute()?.is_zero() {
                    track.unsolo();
                } else {
                    track.solo();
                }
            }
            TrackSendPan { send } => {
                let pan = Pan::from_normalized_value(value.as_absolute()?.get());
                send.set_pan(pan);
            }
            Tempo { project } => {
                let tempo = reaper_high::Tempo::from_normalized_value(value.as_absolute()?.get());
                project.set_tempo(tempo, UndoBehavior::OmitUndoPoint);
            }
            Playrate { project } => {
                let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(
                    value.as_absolute()?.get(),
                ));
                project.set_play_rate(play_rate);
            }
            FxEnable { fx } => {
                if value.as_absolute()?.is_zero() {
                    fx.disable();
                } else {
                    fx.enable();
                }
            }
            FxPreset { fx } => {
                let preset_index = convert_unit_value_to_preset_index(fx, value.as_absolute()?);
                let preset_ref = match preset_index {
                    None => FxPresetRef::FactoryPreset,
                    Some(i) => FxPresetRef::Preset(i),
                };
                fx.activate_preset(preset_ref);
            }
            SelectedTrack { project } => {
                let track_index = convert_unit_value_to_track_index(*project, value.as_absolute()?);
                let track = match track_index {
                    None => project.master_track(),
                    Some(i) => project.track_by_index(i).ok_or("track not available")?,
                };
                track.select_exclusively();
            }
            ReaperTarget::AllTrackFxEnable { track } => {
                if value.as_absolute()?.is_zero() {
                    track.disable_fx();
                } else {
                    track.enable_fx();
                }
            }
        };
        Ok(())
    }

    pub fn value_changed(&self) -> BoxedUnitEvent {
        use ReaperTarget::*;
        match self {
            Action {
                action,
                invocation_type: _,
                ..
            } => {
                let action = action.clone();
                // TODO-medium It's not cool that reaper-rs exposes some events as Rc<T>
                //  and some not
                Reaper::get()
                    .action_invoked()
                    .filter(move |a| a.as_ref() == &action)
                    .map_to(())
                    .box_it()
            }
            FxParameter { param } => {
                let param = param.clone();
                Reaper::get()
                    .fx_parameter_value_changed()
                    .filter(move |p| p == &param)
                    .map_to(())
                    .box_it()
            }
            TrackVolume { track } => {
                let track = track.clone();
                Reaper::get()
                    .track_volume_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackSendVolume { send } => {
                let send = send.clone();
                Reaper::get()
                    .track_send_volume_changed()
                    .filter(move |s| s == &send)
                    .map_to(())
                    .box_it()
            }
            TrackPan { track } => {
                let track = track.clone();
                Reaper::get()
                    .track_pan_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackArm { track } => {
                let track = track.clone();
                Reaper::get()
                    .track_arm_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackSelection { track, .. } => {
                let track = track.clone();
                Reaper::get()
                    .track_selected_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackMute { track } => {
                let track = track.clone();
                Reaper::get()
                    .track_mute_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackSolo { track } => {
                let track = track.clone();
                Reaper::get()
                    .track_solo_changed()
                    .filter(move |t| t == &track)
                    .map_to(())
                    .box_it()
            }
            TrackSendPan { send } => {
                let send = send.clone();
                Reaper::get()
                    .track_send_pan_changed()
                    .filter(move |s| s == &send)
                    .map_to(())
                    .box_it()
            }
            Tempo { .. } => Reaper::get().master_tempo_changed().map_to(()).box_it(),
            Playrate { .. } => Reaper::get().master_playrate_changed().map_to(()).box_it(),
            FxEnable { fx } => {
                let fx = fx.clone();
                Reaper::get()
                    .fx_enabled_changed()
                    .filter(move |f| f == &fx)
                    .map_to(())
                    .box_it()
            }
            FxPreset { fx } => {
                let fx = fx.clone();
                Reaper::get()
                    .fx_preset_changed()
                    .filter(move |f| f == &fx)
                    .map_to(())
                    .box_it()
            }
            SelectedTrack { project } => {
                let project = *project;
                Reaper::get()
                    .track_selected_changed()
                    .filter(move |t| t.project() == project)
                    .map_to(())
                    .box_it()
            }
            AllTrackFxEnable { .. } => observable::never().box_it(),
        }
    }
}

impl Target for ReaperTarget {
    fn current_value(&self) -> UnitValue {
        use ReaperTarget::*;
        match self {
            Action { action, .. } => convert_bool_to_unit_value(action.is_on()),
            FxParameter { param } => {
                let v = param
                    .reaper_normalized_value()
                    .expect("couldn't get FX param value")
                    .get();
                if !UnitValue::is_valid(v) {
                    // Either the FX reports a wrong value range (e.g. TAL Flanger Sync Speed)
                    // or the value range exceeded a "normal" range (e.g. ReaPitch Wet). We can't
                    // know. In future, we might offer further customization possibilities here.
                    // For now, we just report it as 1.0 and log a warning.
                    warn!(
                        Reaper::get().logger(),
                        "FX parameter reported normalized value {:?}, which is > 1.0: {:?}",
                        v,
                        param
                    );
                    return UnitValue::MAX;
                }
                UnitValue::new(v)
            }
            // TODO-medium This will panic if the "soft" normalized value is > 1
            TrackVolume { track } => UnitValue::new(track.volume().soft_normalized_value()),
            // TODO-medium This will panic if the "soft" normalized value is > 1
            TrackSendVolume { send } => UnitValue::new(send.volume().soft_normalized_value()),
            TrackPan { track } => UnitValue::new(track.pan().normalized_value()),
            TrackArm { track } => convert_bool_to_unit_value(track.is_armed(false)),
            TrackSelection { track, .. } => convert_bool_to_unit_value(track.is_selected()),
            TrackMute { track } => convert_bool_to_unit_value(track.is_muted()),
            TrackSolo { track } => convert_bool_to_unit_value(track.is_solo()),
            TrackSendPan { send } => UnitValue::new(send.pan().normalized_value()),
            Tempo { project } => UnitValue::new(project.tempo().normalized_value()),
            Playrate { project } => UnitValue::new(project.play_rate().normalized_value().get()),
            FxEnable { fx } => convert_bool_to_unit_value(fx.is_enabled()),
            FxPreset { fx } => convert_preset_index_to_unit_value(fx, fx.preset_index()),
            SelectedTrack { project } => convert_track_index_to_unit_value(
                *project,
                project
                    .first_selected_track(MasterTrackBehavior::ExcludeMasterTrack)
                    .and_then(|t| t.index()),
            ),
            AllTrackFxEnable { track } => convert_bool_to_unit_value(track.fx_is_enabled()),
        }
    }

    fn control_type(&self) -> ControlType {
        use ReaperTarget::*;
        match self {
            Action {
                invocation_type, ..
            } if *invocation_type == ActionInvocationType::Relative => ControlType::Relative,
            FxParameter { param } => match param.step_size() {
                None => ControlType::AbsoluteContinuous,
                Some(step_size) => ControlType::AbsoluteDiscrete {
                    atomic_step_size: UnitValue::new(step_size),
                },
            },
            Tempo { .. } => ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / bpm_span()),
            },
            Playrate { .. } => ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / (playback_speed_factor_span() * 100.0)),
            },
            // `+ 1` because "<no preset>" is also a possible value.
            FxPreset { fx } => ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(fx.preset_count() + 1),
            },
            // `+ 1` because "<Master track>" is also a possible value.
            SelectedTrack { project } => ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(project.track_count() + 1),
            },
            _ => ControlType::AbsoluteContinuous,
        }
    }
}

/// Converts a number of possible values to a step size.
fn convert_count_to_step_size(n: u32) -> UnitValue {
    // Dividing 1.0 by n would divide the unit interval (0..=1) into n same-sized
    // sub intervals, which means we would have n + 1 possible values. We want to
    // represent just n values, so we need n - 1 same-sized sub intervals.
    if n == 1 {
        return UnitValue::MAX;
    }
    UnitValue::new(1.0 / (n - 1) as f64)
}

fn format_value_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(value.get()));
    format_playback_speed(play_rate.playback_speed_factor().get())
}

fn format_playback_speed(speed: f64) -> String {
    format!("{:.4}", speed)
}

fn format_step_size_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0x
    // 1.0 => 3.75x
    let speed_increment = value.get() * playback_speed_factor_span();
    format_playback_speed(speed_increment)
}

fn format_value_as_bpm_without_unit(value: UnitValue) -> String {
    let tempo = Tempo::from_normalized_value(value.get());
    format_bpm(tempo.bpm().get())
}

fn format_step_size_as_bpm_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0 bpm
    // 1.0 => 959.0 bpm
    let bpm_increment = value.get() * bpm_span();
    format_bpm(bpm_increment)
}

// Should be 959.0
fn bpm_span() -> f64 {
    Bpm::MAX.get() - Bpm::MIN.get()
}

fn format_bpm(bpm: f64) -> String {
    format!("{:.4}", bpm)
}

fn format_as_percentage_without_unit(value: UnitValue) -> String {
    let percent = value.get() * 100.0;
    if (percent - percent.round()).abs() < 0.0000_0001 {
        // No fraction. Omit zeros after dot.
        format!("{:.0}", percent)
    } else {
        // Has fraction. We want to display these.
        format!("{:.8}", percent)
    }
}

fn format_value_as_db_without_unit(value: UnitValue) -> String {
    let db = Volume::from_soft_normalized_value(value.get()).db();
    if db == Db::MINUS_INF {
        "-inf".to_string()
    } else {
        format!("{:.2}", db.get())
    }
}

fn format_value_as_db(value: UnitValue) -> String {
    Volume::from_soft_normalized_value(value.get()).to_string()
}

fn format_value_as_pan(value: UnitValue) -> String {
    Pan::from_normalized_value(value.get()).to_string()
}

fn format_value_as_on_off(value: UnitValue) -> &'static str {
    if value.is_zero() { "Off" } else { "On" }
}

fn convert_bool_to_unit_value(on: bool) -> UnitValue {
    if on { UnitValue::MAX } else { UnitValue::MIN }
}

fn convert_unit_value_to_preset_index(fx: &Fx, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, fx.preset_count())
}

fn convert_unit_value_to_track_index(project: Project, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, project.track_count())
}

fn convert_unit_to_discrete_value_with_none(value: UnitValue, count: u32) -> Option<u32> {
    // Example: <no preset> + 4 presets
    if value.is_zero() {
        // 0.00 => <no preset>
        None
    } else {
        // 0.25 => 0
        // 0.50 => 1
        // 0.75 => 2
        // 1.00 => 3

        // Example: value = 0.75
        let step_size = 1.0 / count as f64; // 0.25
        let zero_based_value = (value.get() - step_size).max(0.0); // 0.5
        Some((zero_based_value * count as f64).round() as u32) // 2
    }
}

fn convert_track_index_to_unit_value(project: Project, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, project.track_count())
}

fn convert_preset_index_to_unit_value(fx: &Fx, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx.preset_count())
}

fn convert_discrete_to_unit_value_with_none(value: Option<u32>, count: u32) -> UnitValue {
    // Example: <no preset> + 4 presets
    match value {
        // <no preset> => 0.00
        None => UnitValue::MIN,
        // 0 => 0.25
        // 1 => 0.50
        // 2 => 0.75
        // 3 => 1.00
        Some(i) => {
            // Example: i = 2
            let zero_based_value = i as f64 / count as f64; // 0.5
            let step_size = 1.0 / count as f64; // 0.25
            let value = (zero_based_value + step_size).min(1.0); // 0.75
            UnitValue::new(value)
        }
    }
}

fn parse_from_percentage(text: &str) -> Result<UnitValue, &'static str> {
    let percentage: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
    (percentage / 100.0).try_into()
}

fn parse_value_from_db(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let db: Db = decimal.try_into().map_err(|_| "not in dB range")?;
    Volume::from_db(db).soft_normalized_value().try_into()
}

fn parse_value_from_pan(text: &str) -> Result<UnitValue, &'static str> {
    let pan: Pan = text.parse()?;
    pan.normalized_value().try_into()
}

fn parse_value_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let factor: PlaybackSpeedFactor = decimal.try_into().map_err(|_| "not in play rate range")?;
    PlayRate::from_playback_speed_factor(factor)
        .normalized_value()
        .get()
        .try_into()
}

fn parse_step_size_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    // 0.0x => 0.0
    // 3.75x => 1.0
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let span = playback_speed_factor_span();
    if decimal < 0.0 || decimal > span {
        return Err("not in playback speed factor increment range");
    }
    Ok(UnitValue::new(decimal / span))
}

/// Should be 3.75
fn playback_speed_factor_span() -> f64 {
    PlaybackSpeedFactor::MAX.get() - PlaybackSpeedFactor::MIN.get()
}

fn parse_value_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let bpm: Bpm = decimal.try_into().map_err(|_| "not in BPM range")?;
    Tempo::from_bpm(bpm).normalized_value().try_into()
}

fn parse_step_size_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    // 0.0 bpm => 0.0
    // 959.0 bpm => 1.0
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let span = bpm_span();
    if decimal < 0.0 || decimal > span {
        return Err("not in BPM increment range");
    }
    Ok(UnitValue::new(decimal / span))
}

/// How to invoke an action target
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
pub enum ActionInvocationType {
    #[display(fmt = "Trigger")]
    Trigger = 0,
    #[display(fmt = "Absolute")]
    Absolute = 1,
    #[display(fmt = "Relative")]
    Relative = 2,
}

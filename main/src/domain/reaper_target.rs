use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, ActionCharacter, Fx, FxParameter, FxParameterCharacter, Pan, PlayRate, Project, Reaper,
    Tempo, Track, TrackSend, Volume,
};
use reaper_medium::{
    Bpm, CommandId, Db, FxPresetRef, GetParameterStepSizesResult, MasterTrackBehavior,
    NormalizedPlayRate, PlaybackSpeedFactor, ReaperNormalizedFxParamValue, UndoBehavior,
};
use rx_util::{BoxedUnitEvent, Event, UnitEvent};
use rxrust::prelude::*;

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use slog::warn;

use crate::domain::ui_util::{format_as_percentage_without_unit, parse_from_percentage};
use crate::domain::RealearnTarget;
use std::convert::TryInto;
use std::rc::Rc;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TargetCharacter {
    Trigger,
    Switch,
    Discrete,
    Continuous,
    VirtualMulti,
    VirtualButton,
}

impl TargetCharacter {
    pub fn from_control_type(control_type: ControlType) -> TargetCharacter {
        use ControlType::*;
        match control_type {
            AbsoluteTrigger => TargetCharacter::Trigger,
            AbsoluteSwitch => TargetCharacter::Switch,
            AbsoluteContinuous | AbsoluteContinuousRoundable { .. } => TargetCharacter::Continuous,
            AbsoluteDiscrete { .. } | Relative => TargetCharacter::Discrete,
            VirtualMulti => TargetCharacter::VirtualMulti,
            VirtualButton => TargetCharacter::VirtualButton,
        }
    }
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
    Transport {
        project: Project,
        action: TransportAction,
    },
}

impl RealearnTarget for ReaperTarget {
    fn character(&self) -> TargetCharacter {
        TargetCharacter::from_control_type(self.control_type())
    }

    fn open(&self) {
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
    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
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

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
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

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
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

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => format_value_as_db_without_unit(value),
            TrackPan { .. } | TrackSendPan { .. } => format_value_as_pan(value),
            Tempo { .. } => format_value_as_bpm_without_unit(value),
            Playrate { .. } => format_value_as_playback_speed_factor_without_unit(value),
            _ => format_as_percentage_without_unit(value),
        }
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            Tempo { .. } => format_step_size_as_bpm_without_unit(step_size),
            Playrate { .. } => format_step_size_as_playback_speed_factor_without_unit(step_size),
            _ => format_as_percentage_without_unit(step_size),
        }
    }

    fn hide_formatted_value(&self) -> bool {
        use ReaperTarget::*;
        matches!(
            self,
            TrackVolume { .. }
                | TrackSendVolume { .. }
                | TrackPan { .. }
                | TrackSendPan { .. }
                | Playrate { .. }
                | Tempo { .. }
        )
    }

    fn hide_formatted_step_size(&self) -> bool {
        use ReaperTarget::*;
        matches!(
            self,
            TrackVolume { .. }
                | TrackSendVolume { .. }
                | TrackPan { .. }
                | TrackSendPan { .. }
                | Playrate { .. }
                | Tempo { .. }
        )
    }

    fn value_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => "dB",
            TrackPan { .. } | TrackSendPan { .. } => "",
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            _ => "%",
        }
    }

    fn step_size_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackPan { .. } | TrackSendPan { .. } => "",
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            _ => "%",
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
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

    fn control(&self, value: ControlValue) -> Result<(), &'static str> {
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
            Transport { project, action } => {
                use TransportAction::*;
                let off = value.as_absolute()?.is_zero();
                match action {
                    PlayStop => {
                        if off {
                            project.stop();
                        } else {
                            project.play();
                        }
                    }
                    PlayPause => {
                        if off {
                            project.pause();
                        } else {
                            project.play();
                        }
                    }
                    Record => {
                        if off {
                            Reaper::get().disable_record_in_current_project();
                        } else {
                            Reaper::get().enable_record_in_current_project();
                        }
                    }
                    Repeat => {
                        if off {
                            project.disable_repeat();
                        } else {
                            project.enable_repeat();
                        }
                    }
                };
            }
        };
        Ok(())
    }

    fn can_report_current_value(&self) -> bool {
        true
    }
}

impl ReaperTarget {
    /// Notifies about other events which can affect the resulting `ReaperTarget`.
    ///
    /// The resulting `ReaperTarget` doesn't change only if one of our the model properties changes.
    /// It can also change if a track is removed or FX focus changes. We don't include
    /// those in `changed()` because they are global in nature. If we listen to n targets,
    /// we don't want to listen to those global events n times. Just 1 time is enough!
    pub fn potential_static_change_events() -> impl UnitEvent {
        let reaper = Reaper::get();
        reaper
            // Considering fx_focused() as static event is okay as long as we don't have a target
            // which switches focus between different FX. As soon as we have that, we must treat
            // fx_focused() as a dynamic event, like track_selection_changed().
            .fx_focused()
            .map_to(())
            .merge(reaper.track_added().map_to(()))
            .merge(reaper.track_removed().map_to(()))
            .merge(reaper.fx_reordered().map_to(()))
            .merge(reaper.fx_removed().map_to(()))
    }

    /// This contains all potential target-changing events which could also be fired by targets
    /// themselves. Be careful with those. Reentrancy very likely.
    ///
    /// Previously we always reacted on selection changes. But this naturally causes issues,
    /// which become most obvious with the "Selected track" target. If we resync all mappings
    /// whenever another track is selected, this happens very often while turning an encoder
    /// that navigates between tracks. This in turn renders throttling functionality
    /// useless (because with a resync all runtime mode state is gone). Plus, reentrancy
    /// issues will arise.
    pub fn potential_dynamic_change_events() -> impl UnitEvent {
        let reaper = Reaper::get();
        reaper.track_selected_changed().map_to(())
    }

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

    fn format_value_generic(&self, value: UnitValue) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value),
            self.value_unit()
        )
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

    fn parse_value_from_discrete_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(text.parse().map_err(|_| "not a discrete value")?)
    }

    pub fn project(&self) -> Option<Project> {
        use ReaperTarget::*;
        let project = match self {
            Action { .. } | Transport { .. } => return None,
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
            Action { .. }
            | Tempo { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | Transport { .. } => return None,
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
            | AllTrackFxEnable { .. }
            | Transport { .. } => return None,
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
            | AllTrackFxEnable { .. }
            | Transport { .. } => return None,
        };
        Some(send)
    }

    pub fn supports_feedback(&self) -> bool {
        use ReaperTarget::*;
        match self {
            Action { .. } => true,
            FxParameter { .. } => true,
            TrackVolume { .. } => true,
            TrackSendVolume { .. } => true,
            TrackPan { .. } => true,
            TrackArm { .. } => true,
            TrackSelection { .. } => true,
            TrackMute { .. } => true,
            TrackSolo { .. } => true,
            TrackSendPan { .. } => true,
            Tempo { .. } => true,
            Playrate { .. } => true,
            FxEnable { .. } => true,
            FxPreset { .. } => true,
            SelectedTrack { .. } => true,
            AllTrackFxEnable { .. } => false,
            Transport { .. } => true,
        }
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
            Transport { action, .. } => {
                let reaper = Reaper::get();
                if *action == TransportAction::Repeat {
                    reaper.repeat_state_changed().box_it()
                } else {
                    reaper.play_state_changed().box_it()
                }
            }
        }
    }
}

impl Target for ReaperTarget {
    fn current_value(&self) -> Option<UnitValue> {
        use ReaperTarget::*;
        let result = match self {
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
                    // For now, we just report it as 0.0 or 1.0 and log a warning.
                    warn!(
                        Reaper::get().logger(),
                        "FX parameter reported normalized value {:?} which is not in unit interval: {:?}",
                        v,
                        param
                    );
                    return Some(UnitValue::new_clamped(v));
                }
                UnitValue::new(v)
            }
            // The soft-normalized value can be > 1.0, e.g. when we have a volume of 12 dB and then
            // lower the volume fader limit to a lower value. In that case we just report the
            // highest possible value ... not much else we can do.
            TrackVolume { track } => UnitValue::new_clamped(track.volume().soft_normalized_value()),
            TrackSendVolume { send } => {
                UnitValue::new_clamped(send.volume().soft_normalized_value())
            }
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
            Transport { project, action } => {
                use TransportAction::*;
                match action {
                    PlayStop | PlayPause => convert_bool_to_unit_value(project.is_playing()),
                    Record => convert_bool_to_unit_value(project.is_recording()),
                    Repeat => convert_bool_to_unit_value(project.repeat_is_enabled()),
                }
            }
        };
        Some(result)
    }

    fn control_type(&self) -> ControlType {
        use ReaperTarget::*;
        match self {
            Action {
                invocation_type,
                action,
                ..
            } => {
                use ActionInvocationType::*;
                match *invocation_type {
                    Trigger => ControlType::AbsoluteTrigger,
                    Absolute => match action.character() {
                        ActionCharacter::Toggle => ControlType::AbsoluteSwitch,
                        ActionCharacter::Trigger => ControlType::AbsoluteContinuous,
                    },
                    Relative => ControlType::Relative,
                }
            }
            FxParameter { param } => {
                use GetParameterStepSizesResult::*;
                match param.step_sizes() {
                    None => ControlType::AbsoluteContinuous,
                    Some(Normal {
                        normal_step,
                        small_step,
                        ..
                    }) => {
                        // The reported step sizes relate to the reported value range, which is not
                        // always the unit interval! Easy to test with JS
                        // FX.
                        let range = param.value_range();
                        // We are primarily interested in the smallest step size that makes sense.
                        // We can always create multiples of it.
                        let span = (range.max_val - range.min_val).abs();
                        if span == 0.0 {
                            return ControlType::AbsoluteContinuous;
                        }
                        let pref_step_size = small_step.unwrap_or(normal_step);
                        let step_size = pref_step_size / span;
                        ControlType::AbsoluteDiscrete {
                            atomic_step_size: UnitValue::new(step_size),
                        }
                    }
                    Some(Toggle) => ControlType::AbsoluteSwitch,
                }
            }
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
            TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | FxEnable { .. }
            | AllTrackFxEnable { .. }
            | Transport { .. }
            | TrackSolo { .. } => ControlType::AbsoluteSwitch,
            TrackVolume { .. } | TrackSendVolume { .. } | TrackPan { .. } | TrackSendPan { .. } => {
                ControlType::AbsoluteContinuous
            }
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

impl Default for ActionInvocationType {
    fn default() -> Self {
        ActionInvocationType::Trigger
    }
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TransportAction {
    #[serde(rename = "playStop")]
    #[display(fmt = "Play/stop")]
    PlayStop,
    #[serde(rename = "playPause")]
    #[display(fmt = "Play/pause")]
    PlayPause,
    #[serde(rename = "record")]
    #[display(fmt = "Record")]
    Record,
    #[serde(rename = "repeat")]
    #[display(fmt = "Repeat")]
    Repeat,
}

impl Default for TransportAction {
    fn default() -> Self {
        TransportAction::PlayStop
    }
}

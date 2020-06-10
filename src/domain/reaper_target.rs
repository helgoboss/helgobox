use crate::domain::ActionInvocationType;
use helgoboss_learn::{ControlValue, Target, UnitValue};
use reaper_high::{
    Action, ActionCharacter, Fx, FxParameter, FxParameterCharacter, Pan, PlayRate, Project, Reaper,
    Tempo, Track, TrackSend, Volume,
};
use reaper_medium::{
    Bpm, CommandId, Db, FxPresetRef, NormalizedPlayRate, PlaybackSpeedFactor,
    ReaperNormalizedFxParamValue, UndoBehavior,
};
use rx_util::{BoxedUnitEvent, Event, UnitEvent};
use rxrust::prelude::*;
use std::cmp;
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
#[derive(Clone, Debug)]
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
            action, project, ..
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
                invocation_type,
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
        }
    }

    pub fn can_be_discrete(&self) -> bool {
        self.character() != TargetCharacter::Discrete && self.step_size().is_some()
    }

    /// Formats the value completely (including a possible unit).
    pub fn format_value(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            Action { .. } => "".to_string(),
            FxParameter { param } => {
                // TODO This doesn't take into account that ReaperNormalizedFxParamValue can be > 1.
                param
                    .format_normalized_value(ReaperNormalizedFxParamValue::new(value.get()))
                    .into_string()
            }
            TrackVolume { .. } | TrackSendVolume { .. } => format_as_db(value),
            TrackPan { .. } | TrackSendPan { .. } => format_as_pan(value),
            FxEnable { .. }
            | TrackArm { .. }
            | TrackMute { .. }
            | TrackSelection { .. }
            | TrackSolo { .. } => format_as_on_off(value).to_string(),
            FxPreset { fx } => match unit_value_to_preset_index(fx, value) {
                None => "<No preset>".to_string(),
                Some(i) => (i + 1).to_string(),
            },
            _ => format!("{} {}", self.format_value_without_unit(value), self.unit()),
        }
    }

    /// Formats the value without unit.
    pub fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => format_as_db_without_unit(value),
            TrackPan { .. } | TrackSendPan { .. } => format_as_pan(value),
            Tempo { .. } => format_as_bpm_without_unit(value),
            Playrate { .. } => format_as_playback_speed_factor_without_unit(value),
            _ => format_as_percentage_without_unit(value),
        }
    }

    /// This converts the given normalized value to a discrete value.
    ///
    /// Should be used for discrete targets only, e.g. FX preset. This target reports a step size
    /// which is 1.0 divided by the number of presets (because step sizes are reported as normalized
    /// values). If we want to display an increment or a particular value for this target, we
    /// don't show normalized values of course but a discrete number of presets, by using this
    /// function.
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
        if self.wants_increments() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        // Example (target step size = 0.10):
        // - 0    => 0
        // - 0.05 => 1
        // - 0.10 => 1
        // - 0.15 => 2
        // - 0.20 => 2
        // TODO This is maybe wrong for preset target because it has 0 as special value (no preset)
        //  and is otherwise shifted
        let target_step_size = self.step_size().ok_or("target doesn't report step size")?;
        Ok((input.get() / target_step_size.get()).round() as _)
    }

    /// Meaning: not just percentages.
    pub fn can_parse_values(&self) -> bool {
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
    pub fn parse_unit_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => parse_from_db(text),
            TrackPan { .. } | TrackSendPan { .. } => parse_from_pan(text),
            Playrate { .. } => parse_from_playback_speed_factor(text),
            Tempo { .. } => parse_from_bpm(text),
            _ => parse_from_percentage(text),
        }
    }

    /// Like `convert_ùnit_value_to_discrete_value()` but in the other direction.
    pub fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
    ) -> Result<UnitValue, &'static str> {
        if self.wants_increments() {
            return (value as f64 / 63.0).try_into();
        }
        // Example (target step size = 0.10):
        // - 0    => 0
        // - 0.05 => 1
        // - 0.10 => 1
        // - 0.15 => 2
        // - 0.20 => 2
        // TODO This is maybe wrong for preset target because it has 0 as special value (no preset)
        //  and is otherwise shifted
        let target_step_size = self.step_size().ok_or("target doesn't report step size")?;
        (value as f64 * target_step_size.get()).try_into()
    }

    pub fn unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => "dB",
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
            | TrackSolo { track } => track.project(),
            TrackSendPan { send } | TrackSendVolume { send } => send.source_track().project(),
            Tempo { project } | Playrate { project } => *project,
            FxEnable { fx } | FxPreset { fx } => fx.project()?,
        };
        Some(project)
    }

    pub fn track(&self) -> Option<&Track> {
        use ReaperTarget::*;
        let track = match self {
            FxParameter { param } => param.fx().track(),
            TrackVolume { track } => track,
            TrackSendVolume { send } => send.source_track(),
            TrackPan { track } => track,
            TrackArm { track } => track,
            TrackSelection { track, .. } => track,
            TrackMute { track } => track,
            TrackSolo { track } => track,
            TrackSendPan { send } => send.source_track(),
            FxEnable { fx } => fx.track(),
            FxPreset { fx } => fx.track(),
            _ => return None,
        };
        Some(track)
    }

    pub fn fx(&self) -> Option<&Fx> {
        use ReaperTarget::*;
        let fx = match self {
            FxParameter { param } => param.fx(),
            FxEnable { fx } => fx,
            FxPreset { fx } => fx,
            _ => return None,
        };
        Some(fx)
    }

    pub fn send(&self) -> Option<&TrackSend> {
        use ReaperTarget::*;
        let send = match self {
            TrackSendPan { send } | TrackSendVolume { send } => send,
            _ => return None,
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
                            action.invoke_as_trigger(Some(*project));
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
                // TODO-high How about values > 1.0?
                let fx_value = ReaperNormalizedFxParamValue::new(value.as_absolute()?.get());
                param.set_normalized_value(fx_value);
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
                } else {
                    if *select_exclusively {
                        track.select_exclusively();
                    } else {
                        track.select();
                    }
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
                let preset_index = unit_value_to_preset_index(fx, value.as_absolute()?);
                let preset_ref = match preset_index {
                    None => FxPresetRef::FactoryPreset,
                    Some(i) => FxPresetRef::Preset(i),
                };
                fx.activate_preset(preset_ref);
            }
        };
        Ok(())
    }

    pub fn value_changed(&self) -> BoxedUnitEvent {
        use ReaperTarget::*;
        match self {
            Action {
                action,
                invocation_type,
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
            FxPreset { .. } => {
                // REAPER doesn't notify us when a preset is changed.
                observable::empty().box_it()
            }
        }
    }
}

impl Target for ReaperTarget {
    fn current_value(&self) -> UnitValue {
        use ReaperTarget::*;
        match self {
            Action { action, .. } => bool_to_unit_value(action.is_on()),
            // TODO This will panic if the "soft" normalized value is > 1
            FxParameter { param } => UnitValue::new(param.normalized_value().get()),
            // TODO This will panic if the "soft" normalized value is > 1
            TrackVolume { track } => UnitValue::new(track.volume().soft_normalized_value()),
            // TODO This will panic if the "soft" normalized value is > 1
            TrackSendVolume { send } => UnitValue::new(send.volume().soft_normalized_value()),
            TrackPan { track } => UnitValue::new(track.pan().normalized_value()),
            TrackArm { track } => bool_to_unit_value(track.is_armed(false)),
            TrackSelection { track, .. } => bool_to_unit_value(track.is_selected()),
            TrackMute { track } => bool_to_unit_value(track.is_muted()),
            TrackSolo { track } => bool_to_unit_value(track.is_solo()),
            TrackSendPan { send } => UnitValue::new(send.pan().normalized_value()),
            Tempo { project } => UnitValue::new(project.tempo().normalized_value()),
            Playrate { project } => UnitValue::new(project.play_rate().normalized_value().get()),
            FxEnable { fx } => bool_to_unit_value(fx.is_enabled()),
            FxPreset { fx } => preset_index_to_unit_value(fx, fx.preset_index()),
        }
    }

    fn step_size(&self) -> Option<UnitValue> {
        use ReaperTarget::*;
        match self {
            FxParameter { param } => param.step_size().map(UnitValue::new),
            Tempo { .. } => Some(UnitValue::MIN),
            FxPreset { fx } => {
                // `+ 1` because "no preset" is also a possible value
                Some(UnitValue::new(1.0 / (fx.preset_count() + 1) as f64))
            }
            _ => None,
        }
    }

    fn wants_increments(&self) -> bool {
        match self {
            ReaperTarget::Action {
                invocation_type, ..
            } if *invocation_type == ActionInvocationType::Relative => true,
            _ => false,
        }
    }
}

fn format_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(value.get()));
    format!("{:.2}", play_rate.playback_speed_factor().get())
}

fn format_as_bpm_without_unit(value: UnitValue) -> String {
    let tempo = Tempo::from_normalized_value(value.get());
    format!("{:.4}", tempo.bpm().get())
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

fn format_as_db_without_unit(value: UnitValue) -> String {
    let db = Volume::from_soft_normalized_value(value.get()).db();
    if db == Db::MINUS_INF {
        "-inf".to_string()
    } else {
        format!("{:.2}", db.get())
    }
}

fn format_as_db(value: UnitValue) -> String {
    Volume::from_soft_normalized_value(value.get()).to_string()
}

fn format_as_pan(value: UnitValue) -> String {
    Pan::from_normalized_value(value.get()).to_string()
}

fn format_as_on_off(value: UnitValue) -> &'static str {
    if value.is_one() { "On" } else { "Off" }
}

fn bool_to_unit_value(on: bool) -> UnitValue {
    if on { UnitValue::MAX } else { UnitValue::MIN }
}

fn unit_value_to_preset_index(fx: &Fx, value: UnitValue) -> Option<u32> {
    // 0 corresponds to "no preset"
    if value.is_zero() {
        None
    } else {
        let preset_count = fx.preset_count();
        let step_size = 1.0 / preset_count as f64;
        let shifted_value = (value.get() - step_size).max(0.0);
        Some((shifted_value * preset_count as f64).round() as u32)
    }
}

fn preset_index_to_unit_value(fx: &Fx, index: Option<u32>) -> UnitValue {
    // 0 corresponds to "no preset"
    match index {
        None => UnitValue::MIN,
        Some(i) => {
            let preset_count = fx.preset_count();
            let shifted_value = i as f64 / preset_count as f64;
            let step_size = 1.0 / preset_count as f64;
            let value = (shifted_value + step_size).min(1.0);
            UnitValue::new(value)
        }
    }
}

fn parse_from_percentage(text: &str) -> Result<UnitValue, &'static str> {
    let percentage: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
    (percentage / 100.0).try_into()
}

fn parse_from_db(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let db: Db = decimal.try_into().map_err(|_| "not in dB range")?;
    Volume::from_db(db).soft_normalized_value().try_into()
}

fn parse_from_pan(text: &str) -> Result<UnitValue, &'static str> {
    let pan: Pan = text.parse()?;
    pan.normalized_value().try_into()
}

fn parse_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let factor: PlaybackSpeedFactor = decimal.try_into().map_err(|_| "not in play rate range")?;
    PlayRate::from_playback_speed_factor(factor)
        .normalized_value()
        .get()
        .try_into()
}

fn parse_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let bpm: Bpm = decimal.try_into().map_err(|_| "not in BPM range")?;
    Tempo::from_bpm(bpm).normalized_value().try_into()
}

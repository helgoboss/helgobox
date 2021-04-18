use crate::core::default_util::is_default;
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    ControlType, ControlValue, OscArgDescriptor, OscTypeTag, RawMidiPattern, Target, UnitValue,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, ActionCharacter, AvailablePanValue, BookmarkType, ChangeEvent, Fx, FxChain,
    FxParameter, FxParameterCharacter, Pan, PlayRate, Project, Reaper, Tempo, Track, TrackRoute,
    Volume, Width,
};
use reaper_medium::{
    AutoSeekBehavior, AutomationMode, BookmarkRef, Bpm, CommandId, Db, FxChainVisibility,
    FxPresetRef, GetLoopTimeRange2Result, GetParameterStepSizesResult,
    GlobalAutomationModeOverride, MasterTrackBehavior, NormalizedPlayRate, PlaybackSpeedFactor,
    PositionInSeconds, ReaperNormalizedFxParamValue, ReaperPanValue, ReaperWidthValue,
    SetEditCurPosOptions, SoloMode, TrackArea, UndoBehavior,
};
use rx_util::{Event, UnitEvent};
use rxrust::prelude::*;

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use slog::warn;

use crate::core::Global;
use crate::domain::ui_util::{
    format_as_double_percentage_without_unit, format_as_percentage_without_unit,
    format_as_symmetric_percentage_without_unit, parse_from_double_percentage,
    parse_from_symmetric_percentage, parse_unit_value_from_percentage,
};
use crate::domain::{
    handle_exclusivity, AdditionalFeedbackEvent, BackboneState, ClipPlayState, ControlContext,
    FeedbackAudioHookTask, FeedbackOutput, HierarchyEntry, HierarchyEntryProvider,
    InstanceFeedbackEvent, InstanceState, MidiDestination, OscDeviceId, OscFeedbackTask,
    RealearnTarget, SharedInstanceState, SlotPlayOptions,
};
use reaper_low::raw;
use rosc::OscMessage;
use std::cell::RefCell;
use std::convert::TryInto;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::rc::Rc;

/// This target character is just used for auto-correct settings! It doesn't have influence
/// on control/feedback.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TargetCharacter {
    Trigger,
    Switch,
    Discrete,
    Continuous,
    VirtualMulti,
    VirtualButton,
}

/// This is a ReaLearn target.
///
/// Unlike TargetModel, the real target has everything resolved already (e.g. track and FX) and
/// is immutable.
//
// When adding a new target type, please proceed like this:
//
// 1. Recompile and see what fails.
//      - Yes, we basically let the compiler write our to-do list :)
//      - For this to work, we must take care not to use `_` when doing pattern matching on
//        `ReaperTarget`, but instead mention each variant explicitly.
// 2. One situation where this doesn't work is when we use `matches!`. So after that, just search
//    for occurrences of `matches!` in this file and do what needs to be done!
// 3. To not miss anything, look for occurrences of `TrackVolume` (as a good example).
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
    TrackRouteVolume {
        route: TrackRoute,
    },
    TrackPan {
        track: Track,
    },
    TrackWidth {
        track: Track,
    },
    TrackArm {
        track: Track,
        exclusivity: TrackExclusivity,
    },
    TrackSelection {
        track: Track,
        exclusivity: TrackExclusivity,
        scroll_arrange_view: bool,
        scroll_mixer: bool,
    },
    TrackMute {
        track: Track,
        exclusivity: TrackExclusivity,
    },
    TrackShow {
        track: Track,
        exclusivity: TrackExclusivity,
        area: TrackArea,
    },
    TrackSolo {
        track: Track,
        behavior: SoloBehavior,
        exclusivity: TrackExclusivity,
    },
    TrackAutomationMode {
        track: Track,
        exclusivity: TrackExclusivity,
        mode: AutomationMode,
    },
    TrackRoutePan {
        route: TrackRoute,
    },
    TrackRouteMute {
        route: TrackRoute,
    },
    Tempo {
        project: Project,
    },
    Playrate {
        project: Project,
    },
    AutomationModeOverride {
        mode_override: Option<GlobalAutomationModeOverride>,
    },
    FxEnable {
        fx: Fx,
    },
    FxOpen {
        fx: Fx,
        display_type: FxDisplayType,
    },
    FxPreset {
        fx: Fx,
    },
    SelectedTrack {
        project: Project,
        scroll_arrange_view: bool,
        scroll_mixer: bool,
    },
    FxNavigate {
        fx_chain: FxChain,
        display_type: FxDisplayType,
    },
    AllTrackFxEnable {
        track: Track,
        exclusivity: TrackExclusivity,
    },
    Transport {
        project: Project,
        action: TransportAction,
    },
    LoadFxSnapshot {
        fx: Fx,
        chunk: Rc<String>,
        chunk_hash: u64,
    },
    AutomationTouchState {
        track: Track,
        parameter_type: TouchedParameterType,
        exclusivity: TrackExclusivity,
    },
    GoToBookmark {
        project: Project,
        bookmark_type: BookmarkType,
        // This counts both markers and regions. We need it for getting the current value.
        index: u32,
        // This counts either only markers or only regions. We need it for control. The alternative
        // would be an ID but unfortunately, marker IDs are not unique which means we would
        // unnecessarily lack reliability to go to markers in a position-based way.
        position: NonZeroU32,
        set_time_selection: bool,
        set_loop_points: bool,
    },
    Seek {
        project: Project,
        options: SeekOptions,
    },
    SendMidi {
        pattern: RawMidiPattern,
        destination: SendMidiDestination,
    },
    SendOsc {
        address_pattern: String,
        arg_descriptor: Option<OscArgDescriptor>,
        device_id: Option<OscDeviceId>,
    },
    ClipTransport {
        track: Option<Track>,
        slot_index: usize,
        action: TransportAction,
        play_options: SlotPlayOptions,
    },
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
pub enum SendMidiDestination {
    #[serde(rename = "fx-output")]
    #[display(fmt = "FX output (with FX input only)")]
    FxOutput,
    #[serde(rename = "feedback-output")]
    #[display(fmt = "Feedback output")]
    FeedbackOutput,
}

impl Default for SendMidiDestination {
    fn default() -> Self {
        Self::FxOutput
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeekOptions {
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_time_selection: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_loop_points: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_regions: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_project: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub move_view: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub seek_play: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub feedback_resolution: PlayPosFeedbackResolution,
}

/// Determines in which granularity the play position influences feedback of a target.
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
pub enum PlayPosFeedbackResolution {
    /// It's enough to ask every beat.
    #[serde(rename = "beat")]
    #[display(fmt = "Beat")]
    Beat,
    /// It should be asked as frequently as possible (main loop).
    #[serde(rename = "high")]
    #[display(fmt = "Fast")]
    High,
}

impl Default for PlayPosFeedbackResolution {
    fn default() -> Self {
        Self::Beat
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
pub enum FxDisplayType {
    #[serde(rename = "floating")]
    #[display(fmt = "Floating window")]
    FloatingWindow,
    #[serde(rename = "chain")]
    #[display(fmt = "FX chain (limited feedback)")]
    Chain,
}

impl Default for FxDisplayType {
    fn default() -> Self {
        Self::FloatingWindow
    }
}

impl RealearnTarget for ReaperTarget {
    fn character(&self) -> TargetCharacter {
        self.control_type_and_character().1
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
        // TODO-medium Have a look which other targets could profit from this!
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackRouteVolume { .. } => parse_value_from_db(text),
            TrackPan { .. } | TrackRoutePan { .. } => parse_value_from_pan(text),
            Playrate { .. } => parse_value_from_playback_speed_factor(text),
            Tempo { .. } => parse_value_from_bpm(text),
            FxPreset { .. } | FxNavigate { .. } | SelectedTrack { .. } | SendMidi { .. } => {
                self.parse_value_from_discrete_value(text)
            }
            FxParameter { param } if param.character() == FxParameterCharacter::Discrete => {
                self.parse_value_from_discrete_value(text)
            }
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | TrackSolo { .. }
            | TrackRouteMute { .. }
            | GoToBookmark { .. }
            | FxEnable { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Transport { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Seek { .. } => parse_unit_value_from_percentage(text),
            TrackWidth { .. } => parse_from_symmetric_percentage(text),
        }
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            Playrate { .. } => parse_step_size_from_playback_speed_factor(text),
            Tempo { .. } => parse_step_size_from_bpm(text),
            FxPreset { .. } | FxNavigate { .. } | SelectedTrack { .. } | SendMidi { .. } => {
                self.parse_value_from_discrete_value(text)
            }
            FxParameter { param } if param.character() == FxParameterCharacter::Discrete => {
                self.parse_value_from_discrete_value(text)
            }
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | FxEnable { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Transport { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Seek { .. } => parse_unit_value_from_percentage(text),
            TrackWidth { .. } => parse_from_double_percentage(text),
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
            SelectedTrack { project, .. } => convert_unit_value_to_track_index(*project, input)
                .map(|i| i + 1)
                .unwrap_or(0),
            FxNavigate { fx_chain, .. } => convert_unit_value_to_fx_index(fx_chain, input)
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
            SendMidi { pattern, .. } => {
                let step_size = pattern.step_size().ok_or("not supported")?;
                (input.get() / step_size.get()).round() as _
            }
            Action { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | Tempo { .. }
            | Playrate { .. }
            | FxEnable { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | LoadFxSnapshot { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => return Err("not supported"),
        };
        Ok(result)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackRouteVolume { .. } => format_value_as_db_without_unit(value),
            TrackPan { .. } | TrackRoutePan { .. } => format_value_as_pan(value),
            Tempo { .. } => format_value_as_bpm_without_unit(value),
            Playrate { .. } => format_value_as_playback_speed_factor_without_unit(value),
            SendMidi { .. } => {
                if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(value) {
                    discrete_value.to_string()
                } else {
                    "0".to_owned()
                }
            }
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRouteMute { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => format_as_percentage_without_unit(value),
            TrackWidth { .. } => format_as_symmetric_percentage_without_unit(value),
        }
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            Tempo { .. } => format_step_size_as_bpm_without_unit(step_size),
            Playrate { .. } => format_step_size_as_playback_speed_factor_without_unit(step_size),
            SendMidi { .. } => {
                if let Ok(discrete_value) = self.convert_unit_value_to_discrete_value(step_size) {
                    discrete_value.to_string()
                } else {
                    "0".to_owned()
                }
            }
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => format_as_percentage_without_unit(step_size),
            TrackWidth { .. } => format_as_double_percentage_without_unit(step_size),
        }
    }

    /// Usually `true` for targets that have special value parsing support, so the edit control can
    /// contain the unit so an additional label is superfluous.
    fn hide_formatted_value(&self) -> bool {
        use ReaperTarget::*;
        matches!(
            self,
            TrackVolume { .. }
                | TrackRouteVolume { .. }
                | TrackPan { .. }
                | TrackWidth { .. }
                | TrackRoutePan { .. }
                | Playrate { .. }
                | Tempo { .. }
        )
    }

    /// Usually `true` for targets that have special step size parsing support, so the edit control
    /// can contain the unit so an additional label is superfluous.
    fn hide_formatted_step_size(&self) -> bool {
        use ReaperTarget::*;
        matches!(
            self,
            TrackVolume { .. }
                | TrackRouteVolume { .. }
                | TrackPan { .. }
                | TrackWidth { .. }
                | TrackRoutePan { .. }
                | Playrate { .. }
                | Tempo { .. }
        )
    }

    fn value_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackRouteVolume { .. } => "dB",
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackArm { .. }
            | TrackWidth { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRouteMute { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => "%",
            TrackPan { .. } | TrackRoutePan { .. } | SendMidi { .. } => "",
        }
    }

    fn step_size_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            Tempo { .. } => "bpm",
            Playrate { .. } => "x",
            Action { .. }
            | LoadFxSnapshot { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackWidth { .. }
            | TrackRouteVolume { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRouteMute { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => "%",
            TrackPan { .. } | TrackRoutePan { .. } | SendMidi { .. } => "",
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            FxParameter { param } => param
                // Even if a REAPER-normalized value can take numbers > 1.0, the usual value range
                // is in fact normalized in the classical sense (unit interval).
                .format_reaper_normalized_value(ReaperNormalizedFxParamValue::new(value.get()))
                .map(|s| s.into_string())
                .unwrap_or_else(|_| self.format_value_generic(value)),
            TrackVolume { .. } | TrackRouteVolume { .. } => format_value_as_db(value),
            TrackPan { .. } | TrackRoutePan { .. } => format_value_as_pan(value),
            FxEnable { .. }
            | TrackArm { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackRouteMute { .. }
            | TrackSelection { .. }
            | TrackSolo { .. } => format_value_as_on_off(value).to_string(),
            FxPreset { fx } => match convert_unit_value_to_preset_index(fx, value) {
                None => "<No preset>".to_string(),
                Some(i) => (i + 1).to_string(),
            },
            SelectedTrack { project, .. } => {
                match convert_unit_value_to_track_index(*project, value) {
                    None => "<Master track>".to_string(),
                    Some(i) => (i + 1).to_string(),
                }
            }
            FxNavigate { fx_chain, .. } => match convert_unit_value_to_fx_index(fx_chain, value) {
                None => "<No FX>".to_string(),
                Some(i) => (i + 1).to_string(),
            },
            Tempo { .. }
            | Playrate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Transport { .. }
            | Seek { .. }
            | SendMidi { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | TrackWidth { .. } => self.format_value_generic(value),
            Action { .. } | LoadFxSnapshot { .. } => "".to_owned(),
        }
    }

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
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
                param
                    .set_reaper_normalized_value(v)
                    .map_err(|_| "couldn't set FX parameter value")?;
            }
            TrackVolume { track } => {
                let volume = Volume::try_from_soft_normalized_value(value.as_absolute()?.get());
                track.set_volume(volume.unwrap_or(Volume::MIN));
            }
            TrackRouteVolume { route } => {
                let volume = Volume::try_from_soft_normalized_value(value.as_absolute()?.get());
                route
                    .set_volume(volume.unwrap_or(Volume::MIN))
                    .map_err(|_| "couldn't set route volume")?;
            }
            TrackPan { track } => {
                let pan = Pan::from_normalized_value(value.as_absolute()?.get());
                track.set_pan(pan);
            }
            TrackWidth { track } => {
                let width = Width::from_normalized_value(value.as_absolute()?.get());
                track.set_width(width);
            }
            TrackArm { track, exclusivity } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.arm(false));
                    track.disarm(false);
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.disarm(false));
                    track.arm(false);
                }
            }
            TrackSelection {
                track,
                exclusivity,
                scroll_arrange_view,
                scroll_mixer,
            } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.select());
                    track.unselect();
                } else if *exclusivity == TrackExclusivity::ExclusiveAll {
                    // We have a dedicated REAPER function to select the track exclusively.
                    track.select_exclusively();
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.unselect());
                    track.select();
                }
                if *scroll_arrange_view {
                    Reaper::get()
                        .main_section()
                        .action_by_command_id(CommandId::new(40913))
                        .invoke_as_trigger(Some(track.project()));
                }
                if *scroll_mixer {
                    track.scroll_mixer();
                }
            }
            TrackMute { track, exclusivity } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.mute());
                    track.unmute();
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.unmute());
                    track.mute();
                }
            }
            TrackShow {
                track,
                exclusivity,
                area,
            } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.set_shown(*area, true));
                    track.set_shown(*area, false);
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.set_shown(*area, false));
                    track.set_shown(*area, true);
                }
            }
            TrackAutomationMode {
                track,
                mode,
                exclusivity,
            } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.set_automation_mode(*mode));
                    track.set_automation_mode(AutomationMode::TrimRead);
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| {
                        t.set_automation_mode(AutomationMode::TrimRead)
                    });
                    track.set_automation_mode(*mode);
                }
            }
            TrackSolo {
                track,
                behavior,
                exclusivity,
            } => {
                let solo_track = |t: &Track| {
                    use SoloBehavior::*;
                    match *behavior {
                        InPlace => t.set_solo_mode(SoloMode::SoloInPlace),
                        IgnoreRouting => t.set_solo_mode(SoloMode::SoloIgnoreRouting),
                        ReaperPreference => t.solo(),
                    }
                };
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, solo_track);
                    track.unsolo();
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.unsolo());
                    solo_track(track);
                }
            }
            TrackRoutePan { route } => {
                let pan = Pan::from_normalized_value(value.as_absolute()?.get());
                route.set_pan(pan).map_err(|_| "couldn't set route pan")?;
            }
            TrackRouteMute { route } => {
                if value.as_absolute()?.is_zero() {
                    route.unmute();
                } else {
                    route.mute();
                }
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
            AutomationModeOverride { mode_override } => {
                if value.as_absolute()?.is_zero() {
                    Reaper::get().set_global_automation_override(None);
                } else {
                    Reaper::get().set_global_automation_override(*mode_override);
                }
            }
            FxEnable { fx } => {
                if value.as_absolute()?.is_zero() {
                    fx.disable();
                } else {
                    fx.enable();
                }
            }
            FxOpen { fx, display_type } => {
                use FxDisplayType::*;
                if value.as_absolute()?.is_zero() {
                    match *display_type {
                        FloatingWindow => {
                            fx.hide_floating_window();
                        }
                        Chain => {
                            fx.chain().hide();
                        }
                    }
                } else {
                    match display_type {
                        FloatingWindow => {
                            fx.show_in_floating_window();
                        }
                        Chain => {
                            fx.show_in_chain();
                        }
                    }
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
            SelectedTrack {
                project,
                scroll_arrange_view,
                scroll_mixer,
            } => {
                let track_index = convert_unit_value_to_track_index(*project, value.as_absolute()?);
                let track = match track_index {
                    None => project.master_track(),
                    Some(i) => project.track_by_index(i).ok_or("track not available")?,
                };
                track.select_exclusively();
                if *scroll_arrange_view {
                    Reaper::get()
                        .main_section()
                        .action_by_command_id(CommandId::new(40913))
                        .invoke_as_trigger(Some(track.project()));
                }
                if *scroll_mixer {
                    track.scroll_mixer();
                }
            }
            FxNavigate {
                fx_chain,
                display_type,
            } => {
                let fx_index = convert_unit_value_to_fx_index(fx_chain, value.as_absolute()?);
                use FxDisplayType::*;
                match fx_index {
                    None => match *display_type {
                        FloatingWindow => {
                            fx_chain.hide_all_floating_windows();
                        }
                        Chain => {
                            fx_chain.hide();
                        }
                    },
                    Some(fx_index) => match *display_type {
                        FloatingWindow => {
                            for (i, fx) in fx_chain.index_based_fxs().enumerate() {
                                if i == fx_index as usize {
                                    fx.show_in_floating_window();
                                } else {
                                    fx.hide_floating_window();
                                }
                            }
                        }
                        Chain => {
                            let fx = fx_chain
                                .index_based_fx_by_index(fx_index)
                                .ok_or("FX not available")?;
                            fx.show_in_chain();
                        }
                    },
                }
            }
            AllTrackFxEnable { track, exclusivity } => {
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| t.enable_fx());
                    track.disable_fx();
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| t.disable_fx());
                    track.enable_fx();
                }
            }
            Transport { project, action } => {
                use TransportAction::*;
                let on = !value.as_absolute()?.is_zero();
                match action {
                    PlayStop => {
                        if on {
                            project.play();
                        } else {
                            project.stop();
                        }
                    }
                    PlayPause => {
                        if on {
                            project.play();
                        } else {
                            project.pause();
                        }
                    }
                    Stop => {
                        if on {
                            project.stop();
                        }
                    }
                    Pause => {
                        if on {
                            project.pause();
                        }
                    }
                    Record => {
                        if on {
                            Reaper::get().enable_record_in_current_project();
                        } else {
                            Reaper::get().disable_record_in_current_project();
                        }
                    }
                    Repeat => {
                        if on {
                            project.enable_repeat();
                        } else {
                            project.disable_repeat();
                        }
                    }
                };
            }
            LoadFxSnapshot {
                fx,
                chunk,
                chunk_hash,
            } => {
                if !value.as_absolute()?.is_zero() {
                    BackboneState::target_context()
                        .borrow_mut()
                        .load_fx_snapshot(fx.clone(), chunk, *chunk_hash)?
                }
            }
            AutomationTouchState {
                track,
                parameter_type,
                exclusivity,
            } => {
                let mut ctx = BackboneState::target_context().borrow_mut();
                if value.as_absolute()?.is_zero() {
                    handle_track_exclusivity(track, *exclusivity, |t| {
                        ctx.touch_automation_parameter(t.raw(), *parameter_type)
                    });
                    ctx.untouch_automation_parameter(track.raw(), *parameter_type);
                } else {
                    handle_track_exclusivity(track, *exclusivity, |t| {
                        ctx.untouch_automation_parameter(t.raw(), *parameter_type)
                    });
                    ctx.touch_automation_parameter(track.raw(), *parameter_type);
                }
            }
            GoToBookmark {
                project,
                bookmark_type,
                position,
                set_loop_points,
                set_time_selection,
                ..
            } => {
                if !value.as_absolute()?.is_zero() {
                    match *bookmark_type {
                        BookmarkType::Marker => {
                            project.go_to_marker(BookmarkRef::Position(*position))
                        }
                        BookmarkType::Region => {
                            project.go_to_region_with_smooth_seek(BookmarkRef::Position(*position));
                            if *set_loop_points || *set_time_selection {
                                if let Some(bookmark) = project.find_bookmark_by_type_and_index(
                                    BookmarkType::Region,
                                    position.get() - 1,
                                ) {
                                    if let Some(end_pos) = bookmark.basic_info.region_end_position {
                                        if *set_loop_points {
                                            project.set_loop_points(
                                                bookmark.basic_info.position,
                                                end_pos,
                                                AutoSeekBehavior::DenyAutoSeek,
                                            );
                                        }
                                        if *set_time_selection {
                                            project.set_time_selection(
                                                bookmark.basic_info.position,
                                                end_pos,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Seek { project, options } => {
                let value = value.as_absolute()?;
                let info = get_seek_info(*project, *options).ok_or("nothing to seek")?;
                let desired_pos_within_range = value.get() * info.length();
                let desired_pos = info.start_pos.get() + desired_pos_within_range;
                project.set_edit_cursor_position(
                    PositionInSeconds::new(desired_pos),
                    SetEditCurPosOptions {
                        move_view: options.move_view,
                        seek_play: options.seek_play,
                    },
                );
            }
            SendMidi {
                pattern,
                destination,
            } => {
                // We arrive here only if controlled via OSC. Sending MIDI in response to incoming
                // MIDI messages is handled directly in the real-time processor.
                let raw_midi_event = pattern.to_concrete_midi_event(value.as_absolute()?);
                match *destination {
                    SendMidiDestination::FxOutput => {
                        return Err("OSC => MIDI FX output not supported");
                    }
                    SendMidiDestination::FeedbackOutput => {
                        let feedback_output =
                            context.feedback_output.ok_or("no feedback output set")?;
                        if let FeedbackOutput::Midi(MidiDestination::Device(dev_id)) =
                            feedback_output
                        {
                            let _ = context
                                .feedback_audio_hook_task_sender
                                .send(FeedbackAudioHookTask::SendMidi(
                                    dev_id,
                                    Box::new(raw_midi_event),
                                ))
                                .unwrap();
                        } else {
                            return Err("feedback output is not a MIDI device");
                        }
                    }
                }
            }
            SendOsc {
                address_pattern,
                arg_descriptor,
                device_id,
            } => {
                let msg = OscMessage {
                    addr: address_pattern.clone(),
                    args: if let Some(desc) = *arg_descriptor {
                        desc.to_concrete_args(value.as_absolute()?)
                            .ok_or("sending of this OSC type not supported")?
                    } else {
                        vec![]
                    },
                };
                let effective_dev_id = device_id
                    .or_else(|| {
                        if let FeedbackOutput::Osc(dev_id) = context.feedback_output? {
                            Some(dev_id)
                        } else {
                            None
                        }
                    })
                    .ok_or("no destination device for sending OSC")?;
                context
                    .osc_feedback_task_sender
                    .send(OscFeedbackTask::new(effective_dev_id, msg))
                    .unwrap();
            }
            ClipTransport {
                track,
                slot_index,
                action,
                play_options,
            } => {
                use TransportAction::*;
                let on = !value.as_absolute()?.is_zero();
                let mut instance_state = context.instance_state.borrow_mut();
                match action {
                    PlayStop => {
                        if on {
                            instance_state.play(*slot_index, track.as_ref(), *play_options)?;
                        } else {
                            instance_state.stop(*slot_index)?;
                        }
                    }
                    PlayPause => {
                        if on {
                            instance_state.play(*slot_index, track.as_ref(), *play_options)?;
                        } else {
                            instance_state.pause(*slot_index)?;
                        }
                    }
                    Stop => {
                        if on {
                            instance_state.stop(*slot_index)?;
                        }
                    }
                    Pause => {
                        if on {
                            instance_state.pause(*slot_index)?;
                        }
                    }
                    Record => {
                        return Err("not supported at the moment");
                    }
                    Repeat => {
                        instance_state.toggle_looped(*slot_index)?;
                    }
                };
            }
        };
        Ok(())
    }

    fn can_report_current_value(&self) -> bool {
        use ReaperTarget::*;
        !matches!(self, SendMidi { .. } | SendOsc { .. })
    }
}

struct SeekInfo {
    pub start_pos: PositionInSeconds,
    pub end_pos: PositionInSeconds,
}

impl SeekInfo {
    pub fn new(start_pos: PositionInSeconds, end_pos: PositionInSeconds) -> Self {
        Self { start_pos, end_pos }
    }

    fn from_time_range(range: GetLoopTimeRange2Result) -> Self {
        Self::new(range.start, range.end)
    }

    pub fn length(&self) -> f64 {
        self.end_pos.get() - self.start_pos.get()
    }
}

fn get_seek_info(project: Project, options: SeekOptions) -> Option<SeekInfo> {
    if options.use_time_selection {
        if let Some(r) = project.time_selection() {
            return Some(SeekInfo::from_time_range(r));
        }
    }
    if options.use_loop_points {
        if let Some(r) = project.loop_points() {
            return Some(SeekInfo::from_time_range(r));
        }
    }
    if options.use_regions {
        let bm = project.current_bookmark();
        if let Some(i) = bm.region_index {
            if let Some(bm) = project.find_bookmark_by_index(i) {
                let info = bm.basic_info();
                if let Some(end_pos) = info.region_end_position {
                    return Some(SeekInfo::new(info.position, end_pos));
                }
            }
        }
    }
    if options.use_project {
        let length = project.length();
        if length.get() > 0.0 {
            return Some(SeekInfo::new(
                PositionInSeconds::new(0.0),
                PositionInSeconds::new(length.get()),
            ));
        }
    }
    None
}

impl ReaperTarget {
    pub fn is_available(&self) -> bool {
        use ReaperTarget::*;
        match self {
            Action { action, .. } => action.is_available(),
            FxParameter { param } => param.is_available(),
            TrackArm { track, .. }
            | TrackSelection { track, .. }
            | TrackMute { track, .. }
            | TrackShow { track, .. }
            | TrackAutomationMode { track, .. }
            | TrackSolo { track, .. }
            | AllTrackFxEnable { track, .. }
            | AutomationTouchState { track, .. }
            | TrackVolume { track }
            | TrackPan { track }
            | TrackWidth { track } => track.is_available(),
            TrackRoutePan { route } | TrackRouteMute { route } | TrackRouteVolume { route } => {
                route.is_available()
            }
            Tempo { project }
            | Playrate { project }
            | Transport { project, .. }
            | SelectedTrack { project, .. }
            | GoToBookmark { project, .. }
            | Seek { project, .. } => project.is_available(),
            FxNavigate { fx_chain, .. } => fx_chain.is_available(),
            FxOpen { fx, .. } | FxEnable { fx } | FxPreset { fx } | LoadFxSnapshot { fx, .. } => {
                fx.is_available()
            }
            ClipTransport {
                track, slot_index, ..
            } => {
                if let Some(t) = track {
                    if !t.is_available() {
                        return false;
                    }
                }
                // TODO-medium We should check the control context (instance state) if slot filled.
                // BackboneState::get().preview_slot_is_filled(*slot_index)
                true
            }
            AutomationModeOverride { .. } | SendMidi { .. } | SendOsc { .. } => true,
        }
    }

    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        use ReaperTarget::*;
        use TargetCharacter::*;
        match self {
            Action {
                invocation_type,
                action,
                ..
            } => match *invocation_type {
                ActionInvocationType::Trigger => {
                    (ControlType::AbsoluteContinuousRetriggerable, Trigger)
                }
                ActionInvocationType::Absolute => match action.character() {
                    ActionCharacter::Toggle => (ControlType::AbsoluteContinuous, Switch),
                    ActionCharacter::Trigger => (ControlType::AbsoluteContinuous, Continuous),
                },
                ActionInvocationType::Relative => (ControlType::Relative, Discrete),
            },
            FxParameter { param } => {
                use GetParameterStepSizesResult::*;
                match param.step_sizes() {
                    None => (ControlType::AbsoluteContinuous, Continuous),
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
                            return (ControlType::AbsoluteContinuous, Continuous);
                        }
                        let pref_step_size = small_step.unwrap_or(normal_step);
                        let step_size = pref_step_size / span;
                        (
                            ControlType::AbsoluteDiscrete {
                                atomic_step_size: UnitValue::new(step_size),
                            },
                            Discrete,
                        )
                    }
                    Some(Toggle) => (ControlType::AbsoluteContinuous, Switch),
                }
            }
            Tempo { .. } => (
                ControlType::AbsoluteContinuousRoundable {
                    rounding_step_size: UnitValue::new(1.0 / bpm_span()),
                },
                Continuous,
            ),
            Playrate { .. } => (
                ControlType::AbsoluteContinuousRoundable {
                    rounding_step_size: UnitValue::new(
                        1.0 / (playback_speed_factor_span() * 100.0),
                    ),
                },
                Continuous,
            ),
            // `+ 1` because "<no preset>" is also a possible value.
            FxPreset { fx } => {
                let preset_count = fx.preset_count().unwrap_or(0);
                (
                    ControlType::AbsoluteDiscrete {
                        atomic_step_size: convert_count_to_step_size(preset_count + 1),
                    },
                    Discrete,
                )
            }
            // `+ 1` because "<Master track>" is also a possible value.
            SelectedTrack { project, .. } => (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(project.track_count() + 1),
                },
                Discrete,
            ),
            // `+ 1` because "<No FX>" is also a possible value.
            FxNavigate { fx_chain, .. } => (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(fx_chain.fx_count() + 1),
                },
                Discrete,
            ),
            TrackRouteMute { .. } | FxEnable { .. }
            | FxOpen { .. } => {
                (ControlType::AbsoluteContinuous, Switch)
            }
            // Retriggerable because of #277
            AutomationModeOverride { .. } => (ControlType::AbsoluteContinuousRetriggerable, Switch),
            // Retriggerable because of #277
            TrackAutomationMode { exclusivity, ..} => {
                if *exclusivity == TrackExclusivity::NonExclusive {
                    (ControlType::AbsoluteContinuousRetriggerable, Switch)
                } else {
                    (ControlType::AbsoluteContinuousRetriggerable, Trigger)
                }
            }
            Transport { action, .. } => {
                use TransportAction::*;
                match action {
                    // Retriggerable because we want to be able to retrigger play!
                    PlayStop|
                    PlayPause => (ControlType::AbsoluteContinuousRetriggerable, Switch),
                    Stop |
                    Pause |
                    Record |
                    Repeat => {
                        (ControlType::AbsoluteContinuous, Switch)
                    }
                }
            }
                TrackSolo { exclusivity, .. }
            | AllTrackFxEnable { exclusivity, .. }
            | AutomationTouchState { exclusivity, .. }
            | TrackArm { exclusivity, .. }
            | TrackSelection { exclusivity, .. }
            | TrackShow { exclusivity, .. }
            | TrackMute { exclusivity, .. } => {
                if *exclusivity == TrackExclusivity::NonExclusive {
                    (ControlType::AbsoluteContinuous, Switch)
                } else {
                    (ControlType::AbsoluteContinuousRetriggerable, Trigger)
                }
            }
            TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            // TODO-low "Seek" could support rounding/discrete (beats, measures, seconds, ...)
            | Seek { .. }
            | TrackRoutePan { .. } => (ControlType::AbsoluteContinuous, Continuous),
            LoadFxSnapshot { .. } | GoToBookmark { .. } => {
                (ControlType::AbsoluteContinuousRetriggerable, Trigger)
            }
            SendMidi { pattern, .. } => match pattern.step_size() {
                None => (ControlType::AbsoluteContinuousRetriggerable, Trigger),
                Some(step_size) => if pattern.resolution() == 1 {
                    (ControlType::AbsoluteContinuousRetriggerable, Switch)
                } else {
                    (ControlType::AbsoluteDiscrete { atomic_step_size: step_size }, Discrete)
                }
            }
            SendOsc { arg_descriptor, .. }  => if let Some(desc) = arg_descriptor {
                use OscTypeTag::*;
                match desc.type_tag() {
                    Float | Double => (ControlType::AbsoluteContinuousRetriggerable, Continuous),
                    Bool => (ControlType::AbsoluteContinuousRetriggerable, Switch),
                    Nil | Inf => (ControlType::AbsoluteContinuousRetriggerable, Trigger),
                    _ => (ControlType::AbsoluteContinuousRetriggerable, Trigger),
                }
            } else {
                (ControlType::AbsoluteContinuousRetriggerable, Trigger)
            }
            ClipTransport { .. } => (ControlType::AbsoluteContinuousRetriggerable, Switch)
        }
    }
    /// Notifies about other events which can affect the resulting `ReaperTarget`.
    ///
    /// The resulting `ReaperTarget` doesn't change only if one of our the model properties changes.
    /// It can also change if a track is removed or FX focus changes. We don't include
    /// those in `changed()` because they are global in nature. If we listen to n targets,
    /// we don't want to listen to those global events n times. Just 1 time is enough!
    pub fn potential_static_change_events() -> impl UnitEvent {
        let rx = Global::control_surface_rx();
        rx
            // Considering fx_focused() as static event is okay as long as we don't have a target
            // which switches focus between different FX. As soon as we have that, we must treat
            // fx_focused() as a dynamic event, like track_selection_changed().
            .fx_focused()
            .map_to(())
            .merge(rx.project_switched().map_to(()))
            .merge(rx.track_added().map_to(()))
            .merge(rx.track_removed().map_to(()))
            .merge(rx.tracks_reordered().map_to(()))
            .merge(rx.track_name_changed().map_to(()))
            .merge(rx.fx_added().map_to(()))
            .merge(rx.fx_removed().map_to(()))
            .merge(rx.fx_reordered().map_to(()))
            .merge(rx.bookmarks_changed())
            .merge(rx.receive_count_changed().map_to(()))
            .merge(rx.track_send_count_changed().map_to(()))
            .merge(rx.hardware_output_send_count_changed().map_to(()))
    }

    pub fn is_potential_static_change_event(evt: &ChangeEvent) -> bool {
        use ChangeEvent::*;
        matches!(
            evt,
            FxFocused(_)
                | ProjectSwitched(_)
                | TrackAdded(_)
                | TrackRemoved(_)
                | TracksReordered(_)
                | TrackNameChanged(_)
                | FxAdded(_)
                | FxRemoved(_)
                | FxReordered(_)
                | BookmarksChanged(_)
                | ReceiveCountChanged(_)
                | TrackSendCountChanged(_)
                | HardwareOutputSendCountChanged(_)
        )
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
        let rx = Global::control_surface_rx();
        rx.track_selected_changed().map_to(())
    }

    pub fn is_potential_dynamic_change_event(evt: &ChangeEvent) -> bool {
        use ChangeEvent::*;
        matches!(evt, TrackSelectedChanged(_))
    }

    /// This is eventually going to replace Rx (touched method), at least for domain layer.
    // TODO-medium Unlike the Rx stuff, this doesn't yet contain "Action touch". At the moment
    //  this leads to "Last touched target" to not work with actions - which might even desirable
    //  and should only added as soon as we allow explicitly enabling/disabling target types for
    //  this. The 2nd effect is that actions are not available for global learning which could be
    //  improved.
    pub fn touched_from_change_event(evt: ChangeEvent) -> Option<ReaperTarget> {
        use ChangeEvent::*;
        use ReaperTarget::*;
        let target = match evt {
            TrackVolumeChanged(e) if e.touched => TrackVolume { track: e.track },
            TrackPanChanged(e) if e.touched => {
                if let AvailablePanValue::Complete(new_value) = e.new_value {
                    figure_out_touched_pan_component(e.track, e.old_value, new_value)
                } else {
                    // Shouldn't result in this if touched.
                    return None;
                }
            }
            TrackRouteVolumeChanged(e) if e.touched => TrackRouteVolume { route: e.route },
            TrackRoutePanChanged(e) if e.touched => TrackRoutePan { route: e.route },
            TrackArmChanged(e) => TrackArm {
                track: e.track,
                exclusivity: Default::default(),
            },
            TrackMuteChanged(e) if e.touched => TrackMute {
                track: e.track,
                exclusivity: Default::default(),
            },
            TrackSoloChanged(e) => {
                // When we press the solo button of some track, REAPER actually sends many
                // change events, starting with the change event for the master track. This is
                // not cool for learning because we could only ever learn master-track solo,
                // which doesn't even make sense. So let's just filter it out.
                if e.track.is_master_track() {
                    return None;
                }
                TrackSolo {
                    track: e.track,
                    behavior: Default::default(),
                    exclusivity: Default::default(),
                }
            }
            TrackSelectedChanged(e) if e.new_value => {
                if track_sel_on_mouse_is_enabled() {
                    // If this REAPER preference is enabled, it's often a false positive so better
                    // we don't let this happen at all.
                    return None;
                }
                TrackSelection {
                    track: e.track,
                    exclusivity: Default::default(),
                    scroll_arrange_view: false,
                    scroll_mixer: false,
                }
            }
            FxEnabledChanged(e) => FxEnable { fx: e.fx },
            FxParameterValueChanged(e) if e.touched => FxParameter { param: e.parameter },
            FxPresetChanged(e) => FxPreset { fx: e.fx },
            MasterTempoChanged(e) if e.touched => Tempo {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            },
            MasterPlayrateChanged(e) if e.touched => Playrate {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            },
            TrackAutomationModeChanged(e) => TrackAutomationMode {
                track: e.track,
                exclusivity: Default::default(),
                mode: e.new_value,
            },
            GlobalAutomationOverrideChanged(e) => AutomationModeOverride {
                mode_override: e.new_value,
            },
            _ => return None,
        };
        Some(target)
    }

    // TODO-medium This is the last Rx trace we have in processing logic and we should replace it
    //  in favor of async/await or direct calls. Still used by local learning and "Filter target".
    pub fn touched() -> impl Event<Rc<ReaperTarget>> {
        use ReaperTarget::*;
        let reaper = Reaper::get();
        let csurf_rx = Global::control_surface_rx();
        let action_rx = Global::action_rx();
        observable::empty()
            .merge(
                csurf_rx
                    .fx_parameter_touched()
                    .map(move |param| FxParameter { param }.into()),
            )
            .merge(
                csurf_rx
                    .fx_enabled_changed()
                    .map(move |fx| FxEnable { fx }.into()),
            )
            .merge(
                csurf_rx
                    .fx_preset_changed()
                    .map(move |fx| FxPreset { fx }.into()),
            )
            .merge(
                csurf_rx
                    .track_volume_touched()
                    .map(move |track| TrackVolume { track }.into()),
            )
            .merge(csurf_rx.track_pan_touched().map(move |(track, old, new)| {
                figure_out_touched_pan_component(track, old, new).into()
            }))
            .merge(csurf_rx.track_arm_changed().map(move |track| {
                TrackArm {
                    track,
                    exclusivity: Default::default(),
                }
                .into()
            }))
            .merge(
                csurf_rx
                    .track_selected_changed()
                    .filter(|(_, new_value)| {
                        // If this REAPER preference is enabled, it's often a false positive so
                        // better we don't let this happen at all.
                        *new_value && !track_sel_on_mouse_is_enabled()
                    })
                    .map(move |(track, _)| {
                        TrackSelection {
                            track,
                            exclusivity: Default::default(),
                            scroll_arrange_view: false,
                            scroll_mixer: false,
                        }
                        .into()
                    }),
            )
            .merge(csurf_rx.track_mute_touched().map(move |track| {
                TrackMute {
                    track,
                    exclusivity: Default::default(),
                }
                .into()
            }))
            .merge(csurf_rx.track_automation_mode_changed().map(move |track| {
                let mode = track.automation_mode();
                TrackAutomationMode {
                    track,
                    exclusivity: Default::default(),
                    mode,
                }
                .into()
            }))
            .merge(
                csurf_rx
                    .track_solo_changed()
                    // When we press the solo button of some track, REAPER actually sends many
                    // change events, starting with the change event for the master track. This is
                    // not cool for learning because we could only ever learn master-track solo,
                    // which doesn't even make sense. So let's just filter it out.
                    .filter(|track| !track.is_master_track())
                    .map(move |track| {
                        TrackSolo {
                            track,
                            behavior: Default::default(),
                            exclusivity: Default::default(),
                        }
                        .into()
                    }),
            )
            .merge(
                csurf_rx
                    .track_route_volume_touched()
                    .map(move |route| TrackRouteVolume { route }.into()),
            )
            .merge(
                csurf_rx
                    .track_route_pan_touched()
                    .map(move |route| TrackRoutePan { route }.into()),
            )
            .merge(
                action_rx
                    .action_invoked()
                    .map(move |action| determine_target_for_action((*action).clone()).into()),
            )
            .merge(
                csurf_rx
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
                csurf_rx
                    .master_playrate_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Playrate {
                            project: reaper.current_project(),
                        }
                        .into()
                    }),
            )
            .merge(csurf_rx.global_automation_override_changed().map(move |_| {
                AutomationModeOverride {
                    mode_override: Reaper::get().global_automation_override(),
                }
                .into()
            }))
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
                fx_preset_unit_value(fx, index)
            }
            SelectedTrack { project, .. } => {
                let index = if value == 0 { None } else { Some(value - 1) };
                selected_track_unit_value(*project, index)
            }
            FxNavigate { fx_chain, .. } => {
                let index = if value == 0 { None } else { Some(value - 1) };
                shown_fx_unit_value(fx_chain, index)
            }
            FxParameter { param } => {
                let step_size = param.step_size().ok_or("not supported")?;
                (value as f64 * step_size).try_into()?
            }
            SendMidi { pattern, .. } => {
                if let Some(step_size) = pattern.step_size() {
                    (value as f64 * step_size.get()).try_into()?
                } else {
                    UnitValue::MIN
                }
            }
            Action { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | Tempo { .. }
            | Playrate { .. }
            | FxEnable { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | LoadFxSnapshot { .. }
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => return Err("not supported"),
        };
        Ok(result)
    }

    fn parse_value_from_discrete_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(text.parse().map_err(|_| "not a discrete value")?)
    }

    pub fn project(&self) -> Option<Project> {
        use ReaperTarget::*;
        let project = match self {
            Action { .. }
            | Transport { .. }
            | AutomationModeOverride { .. }
            | SendMidi { .. }
            | SendOsc { .. } => {
                return None;
            }
            FxParameter { param } => param.fx().project()?,
            TrackVolume { track }
            | TrackPan { track }
            | TrackWidth { track }
            | TrackArm { track, .. }
            | TrackSelection { track, .. }
            | TrackMute { track, .. }
            | TrackShow { track, .. }
            | TrackAutomationMode { track, .. }
            | TrackSolo { track, .. }
            | AutomationTouchState { track, .. }
            | AllTrackFxEnable { track, .. } => track.project(),
            TrackRoutePan { route } | TrackRouteMute { route } | TrackRouteVolume { route } => {
                route.track().project()
            }
            GoToBookmark { project, .. }
            | Tempo { project }
            | Playrate { project }
            | SelectedTrack { project, .. }
            | Seek { project, .. } => *project,
            ClipTransport { track, .. } => return track.as_ref().map(|t| t.project()),
            FxNavigate { fx_chain, .. } => fx_chain.project()?,
            FxOpen { fx, .. } | FxEnable { fx } | FxPreset { fx } | LoadFxSnapshot { fx, .. } => {
                fx.project()?
            }
        };
        Some(project)
    }

    pub fn track(&self) -> Option<&Track> {
        use ReaperTarget::*;
        let track = match self {
            FxParameter { param } => param.fx().track()?,
            TrackVolume { track }
            | TrackPan { track }
            | TrackWidth { track }
            | TrackArm { track, .. }
            | TrackSelection { track, .. }
            | TrackMute { track, .. }
            | TrackShow { track, .. }
            | TrackAutomationMode { track, .. }
            | AutomationTouchState { track, .. }
            | TrackSolo { track, .. } => track,
            TrackRoutePan { route } | TrackRouteMute { route } | TrackRouteVolume { route } => {
                route.track()
            }
            FxNavigate { fx_chain, .. } => fx_chain.track()?,
            FxOpen { fx, .. } | FxEnable { fx } | FxPreset { fx } | LoadFxSnapshot { fx, .. } => {
                fx.track()?
            }
            AllTrackFxEnable { track, .. } => track,
            Action { .. }
            | Tempo { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | GoToBookmark { .. }
            | Seek { .. }
            | AutomationModeOverride { .. }
            | Transport { .. }
            | SendMidi { .. }
            | SendOsc { .. } => return None,
            ClipTransport { track, .. } => return track.as_ref(),
        };
        Some(track)
    }

    pub fn fx(&self) -> Option<&Fx> {
        use ReaperTarget::*;
        let fx = match self {
            FxParameter { param } => param.fx(),
            FxOpen { fx, .. } | FxEnable { fx } | FxPreset { fx } | LoadFxSnapshot { fx, .. } => fx,
            Action { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | Tempo { .. }
            | AutomationModeOverride { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | FxNavigate { .. }
            | Transport { .. }
            | SendMidi { .. }
            | ClipTransport { .. }
            | SendOsc { .. } => return None,
        };
        Some(fx)
    }

    pub fn route(&self) -> Option<&TrackRoute> {
        use ReaperTarget::*;
        let route = match self {
            TrackRoutePan { route } | TrackRouteVolume { route } | TrackRouteMute { route } => {
                route
            }
            FxParameter { .. }
            | FxEnable { .. }
            | FxPreset { .. }
            | Action { .. }
            | TrackVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackAutomationMode { .. }
            | AutomationModeOverride { .. }
            | FxOpen { .. }
            | FxNavigate { .. }
            | GoToBookmark { .. }
            | TrackSolo { .. }
            | Tempo { .. }
            | Playrate { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | LoadFxSnapshot { .. }
            | Seek { .. }
            | Transport { .. }
            | SendMidi { .. }
            | ClipTransport { .. }
            | SendOsc { .. } => return None,
        };
        Some(route)
    }

    pub fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        use ReaperTarget::*;
        match self {
            TrackSolo { exclusivity, .. }
            | TrackArm { exclusivity, .. }
            | TrackSelection { exclusivity, .. }
            | TrackMute { exclusivity, .. }
            | TrackShow { exclusivity, .. }
            | TrackAutomationMode { exclusivity, .. }
            | AllTrackFxEnable { exclusivity, .. }
            | AutomationTouchState { exclusivity, .. } => Some(*exclusivity),
            Action { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | Tempo { .. }
            | GoToBookmark { .. }
            | Playrate { .. }
            | FxEnable { .. }
            | FxOpen { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | Transport { .. }
            | LoadFxSnapshot { .. }
            | AutomationModeOverride { .. }
            | Seek { .. }
            | SendMidi { .. }
            | ClipTransport { .. }
            | SendOsc { .. } => None,
        }
    }

    pub fn supports_automatic_feedback(&self) -> bool {
        use ReaperTarget::*;
        match self {
            Action { .. }
            | FxParameter { .. }
            | TrackVolume { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackSolo { .. }
            | TrackRoutePan { .. }
            | Tempo { .. }
            | Playrate { .. }
            | FxEnable { .. }
            | FxOpen { .. }
            | FxPreset { .. }
            | GoToBookmark { .. }
            | SelectedTrack { .. }
            | FxNavigate { .. }
            | LoadFxSnapshot { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            | AutomationModeOverride { .. }
            | TrackAutomationMode { .. }
            | ClipTransport { .. }
            | Transport { .. } => true,
            TrackShow { .. }
            | AllTrackFxEnable { .. }
            | TrackRouteMute { .. }
            | SendMidi { .. }
            | SendOsc { .. } => false,
        }
    }

    pub fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use AdditionalFeedbackEvent::*;
        use ReaperTarget::*;
        match self {
            Action { action, .. } => match evt {
                // We can't provide a value from the event itself because the action hooks don't
                // pass values.
                ActionInvoked(e) if e.command_id == action.command_id() => (true, None),
                _ => (false, None),
            },
            LoadFxSnapshot { fx, .. } => match evt {
                // We can't provide a value from the event itself because it's on/off depending on
                // the mappings which use the FX snapshot target with that FX and which chunk (hash)
                // their snapshot has.
                FxSnapshotLoaded(e) if &e.fx == fx => (true, None),
                _ => (false, None),
            },
            FxParameter { param } => match evt {
                RealearnMonitoringFxParameterValueChanged(e) if &e.parameter == param => (
                    true,
                    Some(fx_parameter_unit_value(&e.parameter, e.new_value)),
                ),
                _ => (false, None),
            },
            AutomationTouchState {
                track,
                parameter_type,
                ..
            } => match evt {
                ParameterAutomationTouchStateChanged(e)
                    if e.track == track.raw() && e.parameter_type == *parameter_type =>
                {
                    (true, Some(touched_unit_value(e.new_value)))
                }
                _ => (false, None),
            },
            GoToBookmark {
                project,
                bookmark_type,
                index,
                ..
            } => match evt {
                PlayPositionChanged(e) if e.project == *project => {
                    let v =
                        current_value_of_bookmark(*project, *bookmark_type, *index, e.new_value);
                    (true, Some(v))
                }
                _ => (false, None),
            },
            Seek { project, options } => match evt {
                PlayPositionChanged(e) if e.project == *project => {
                    let v = current_value_of_seek(*project, *options, e.new_value);
                    (true, Some(v))
                }
                _ => (false, None),
            },
            // This is necessary at the moment because control surface SetPlayState callback works
            // for currently active project tab already.
            Transport { project, action } if *action != TransportAction::Repeat => match evt {
                PlayPositionChanged(e)
                    if e.project == *project && e.project != Reaper::get().current_project() =>
                {
                    (true, None)
                }
                _ => (false, None),
            },
            _ => (false, None),
        }
    }

    pub fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use InstanceFeedbackEvent::*;
        use ReaperTarget::*;
        match self {
            ClipTransport {
                slot_index, action, ..
            } => {
                use TransportAction::*;
                match *action {
                    PlayStop | PlayPause | Stop | Pause => match evt {
                        ClipPlayStateChanged(e) if e.slot_index == *slot_index => {
                            (true, Some(clip_play_state_unit_value(*action, e.new_value)))
                        }
                        _ => (false, None),
                    },
                    // Not supported at the moment.
                    Record => (false, None),
                    Repeat => match evt {
                        ClipRepeatChanged(e) if e.slot_index == *slot_index => {
                            (true, Some(transport_is_enabled_unit_value(e.new_value)))
                        }
                        _ => (false, None),
                    },
                }
            }
            _ => (false, None),
        }
    }

    /// Might return the new value if changed.
    pub fn value_changed_from_change_event(&self, evt: &ChangeEvent) -> (bool, Option<UnitValue>) {
        use ChangeEvent::*;
        use ReaperTarget::*;
        match self {
            FxParameter { param } => {
                match evt {
                    FxParameterValueChanged(e) if &e.parameter == param => (
                        true,
                        Some(fx_parameter_unit_value(&e.parameter, e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackVolume { track } => {
                match evt {
                    TrackVolumeChanged(e) if &e.track == track => (
                        true,
                        Some(volume_unit_value(Volume::from_reaper_value(e.new_value)))
                    ),
                    _ => (false, None)
                }
            }
            TrackRouteVolume { route } => {
                match evt {
                    TrackRouteVolumeChanged(e) if &e.route == route => (
                        true,
                        Some(volume_unit_value(Volume::from_reaper_value(e.new_value)))
                    ),
                    _ => (false, None)
                }
            }
            TrackPan { track } => {
                match evt {
                    TrackPanChanged(e) if &e.track == track => (
                        true,
                        {
                            let pan = match e.new_value {
                                AvailablePanValue::Complete(v) => v.main_pan(),
                                AvailablePanValue::Incomplete(pan) => pan
                            };
                            Some(pan_unit_value(Pan::from_reaper_value(pan)))
                        }
                    ),
                    _ => (false, None)
                }
            }
            TrackWidth { track } => {
                match evt {
                    TrackPanChanged(e) if &e.track == track => (
                        true,
                        match e.new_value {
                            AvailablePanValue::Complete(v) => if let Some(width) = v.width() {
                                Some(width_unit_value(Width::from_reaper_value(width)))
                            } else {
                                None
                            }
                            AvailablePanValue::Incomplete(_) => None
                        }
                    ),
                    _ => (false, None)
                }
            }
            TrackArm { track, .. } => {
                match evt {
                    TrackArmChanged(e) if &e.track == track => (
                        true,
                        Some(track_arm_unit_value(e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackSelection { track, .. } => {
                match evt {
                    TrackSelectedChanged(e) if &e.track == track => (
                        true,
                        Some(track_selected_unit_value(e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackMute { track, .. } => {
                match evt {
                    TrackMuteChanged(e) if &e.track == track => (
                        true,
                        Some(mute_unit_value(e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackAutomationMode { track, mode, .. } => {
                match evt {
                    TrackAutomationModeChanged(e) if &e.track == track => (
                        true,
                        Some(track_automation_mode_unit_value(*mode, e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackSolo { track, .. } => {
                match evt {
                    TrackSoloChanged(e) if &e.track == track => (
                        true,
                        Some(track_solo_unit_value(e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            TrackRoutePan { route } => {
                match evt {
                    TrackRoutePanChanged(e) if &e.route == route => (
                        true,
                        Some(pan_unit_value(Pan::from_reaper_value(e.new_value)))
                    ),
                    _ => (false, None)
                }
            }
            Tempo { project } => match evt {
                MasterTempoChanged(e) if e.project == *project => (
                    true,
                    Some(tempo_unit_value(reaper_high::Tempo::from_bpm(e.new_value)))
                ),
                _ => (false, None)
            },
            Playrate { project } => match evt {
                MasterPlayrateChanged(e) if e.project == *project => (
                    true,
                    Some(playrate_unit_value(PlayRate::from_playback_speed_factor(e.new_value)))
                ),
                _ => (false, None)
            },
            FxEnable { fx } => {
                match evt {
                    FxEnabledChanged(e) if &e.fx == fx => (
                        true,
                        Some(fx_enable_unit_value(e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            FxOpen { fx, .. } => {
                match evt {
                    FxOpened(e) if &e.fx == fx => (
                        true,
                        None
                    ),
                    FxClosed(e) if &e.fx == fx => (
                        true,
                        None
                    ),
                    _ => (false, None)
                }
            }
            FxNavigate { fx_chain, .. } => {
                match evt {
                    FxOpened(e) if e.fx.chain() == fx_chain => (
                        true,
                        None
                    ),
                    FxClosed(e) if e.fx.chain() == fx_chain => (
                        true,
                        None
                    ),
                    _ => (false, None)
                }
            }
            FxPreset { fx } => {
                match evt {
                    FxPresetChanged(e) if &e.fx == fx => (true, None),
                    _ => (false, None)
                }
            }
            SelectedTrack { project, .. } => {
                match evt {
                    TrackSelectedChanged(e) if e.new_value && &e.track.project() == project => (
                        true,
                        Some(selected_track_unit_value(*project, e.track.index()))
                    ),
                    _ => (false, None)
                }
            }
            Transport { project, action, .. } => {
                use TransportAction::*;
                match *action {
                    PlayStop | PlayPause => match evt {
                        PlayStateChanged(e) if e.project == *project => (
                            true,
                            Some(transport_is_enabled_unit_value(e.new_value.is_playing))
                        ),
                        _ => (false, None)
                    }
                    Stop => match evt {
                        PlayStateChanged(e) if e.project == *project => (
                            true,
                            Some(transport_is_enabled_unit_value(!e.new_value.is_playing && !e.new_value.is_paused))
                        ),
                        _ => (false, None)
                    }
                    Pause => match evt {
                        PlayStateChanged(e) if e.project == *project => (
                            true,
                            Some(transport_is_enabled_unit_value(e.new_value.is_paused))
                        ),
                        _ => (false, None)
                    }
                    Record => match evt {
                        PlayStateChanged(e) if e.project == *project => (
                            true,
                            Some(transport_is_enabled_unit_value(e.new_value.is_recording))
                        ),
                        _ => (false, None)
                    }
                    Repeat => match evt {
                        RepeatStateChanged(e) if e.project == *project => (
                            true,
                            Some(transport_is_enabled_unit_value(e.new_value))
                        ),
                        _ => (false, None)
                    }
                }
            }
            // Handled both from control-surface and non-control-surface callbacks.
            GoToBookmark { project, .. } => {
                match evt {
                    BookmarksChanged(e) if e.project == *project => (
                        true,
                        None
                    ),
                    _ => (false, None)
                }
            }
            AutomationModeOverride { mode_override } => {
                match evt {
                    GlobalAutomationOverrideChanged(e) => (
                        true,
                        Some(global_automation_mode_override_unit_value(*mode_override, e.new_value))
                    ),
                    _ => (false, None)
                }
            }
            // Handled from non-control-surface callbacks only.
            Action { .. }
            | LoadFxSnapshot { .. }
            | AutomationTouchState { .. }
            | Seek { .. }
            // Handled from instance-scoped feedback events.
            | ClipTransport { .. }
            // No value change notification available.
            | TrackShow { .. }
            | TrackRouteMute { .. }
            | AllTrackFxEnable { .. }
            | SendMidi { .. }
            | SendOsc { .. }
             => (false, None),
        }
    }
}

impl<'a> Target<'a> for ReaperTarget {
    // An option because we don't have the context available e.g. if some target variants are
    // controlled from real-time processor.
    type Context = Option<ControlContext<'a>>;

    fn current_value(&self, context: Option<ControlContext>) -> Option<UnitValue> {
        use ReaperTarget::*;
        let result = match self {
            Action { action, .. } => {
                if let Some(state) = action.is_on() {
                    // Toggle action: Return toggle state as 0 or 1.
                    convert_bool_to_unit_value(state)
                } else {
                    // Non-toggle action. Try to return current absolute value if this is a
                    // MIDI CC/mousewheel action.
                    if let Some(value) = action.normalized_value() {
                        UnitValue::new(value)
                    } else {
                        UnitValue::MIN
                    }
                }
            }
            FxParameter { param } => {
                fx_parameter_unit_value(param, param.reaper_normalized_value())
            }
            TrackVolume { track } => volume_unit_value(track.volume()),
            TrackRouteVolume { route } => volume_unit_value(route.volume()),
            TrackPan { track } => pan_unit_value(track.pan()),
            TrackWidth { track } => width_unit_value(track.width()),
            TrackArm { track, .. } => track_arm_unit_value(track.is_armed(false)),
            TrackSelection { track, .. } => track_selected_unit_value(track.is_selected()),
            TrackMute { track, .. } => mute_unit_value(track.is_muted()),
            TrackShow { track, area, .. } => {
                let is_shown = track.is_shown(*area);
                convert_bool_to_unit_value(is_shown)
            }
            TrackSolo { track, .. } => track_solo_unit_value(track.is_solo()),
            TrackAutomationMode { track, mode, .. } => {
                track_automation_mode_unit_value(*mode, track.automation_mode())
            }
            AutomationModeOverride { mode_override, .. } => {
                global_automation_mode_override_unit_value(
                    *mode_override,
                    Reaper::get().global_automation_override(),
                )
            }
            TrackRoutePan { route } => pan_unit_value(route.pan()),
            TrackRouteMute { route } => mute_unit_value(route.is_muted()),
            Tempo { project } => tempo_unit_value(project.tempo()),
            Playrate { project } => playrate_unit_value(project.play_rate()),
            FxEnable { fx } => fx_enable_unit_value(fx.is_enabled()),
            FxOpen { fx, display_type } => {
                use FxDisplayType::*;
                let is_open = match display_type {
                    FloatingWindow => fx.floating_window().is_some(),
                    Chain => {
                        use FxChainVisibility::*;
                        match fx.chain().visibility() {
                            Hidden | Visible(None) | Unknown(_) => false,
                            Visible(Some(i)) => fx.index() == i,
                        }
                    }
                };
                convert_bool_to_unit_value(is_open)
            }
            FxPreset { fx } => fx_preset_unit_value(fx, fx.preset_index().ok()?),
            SelectedTrack { project, .. } => {
                let track_index = project
                    .first_selected_track(MasterTrackBehavior::ExcludeMasterTrack)
                    .and_then(|t| t.index());
                selected_track_unit_value(*project, track_index)
            }
            FxNavigate {
                fx_chain,
                display_type,
            } => {
                use FxDisplayType::*;
                let fx_index = match display_type {
                    FloatingWindow => fx_chain
                        .index_based_fxs()
                        .position(|fx| fx.floating_window().is_some())
                        .map(|i| i as u32),
                    Chain => {
                        use FxChainVisibility::*;
                        match fx_chain.visibility() {
                            Hidden | Visible(None) | Unknown(_) => None,
                            Visible(Some(i)) => Some(i),
                        }
                    }
                };
                shown_fx_unit_value(fx_chain, fx_index)
            }
            AllTrackFxEnable { track, .. } => all_track_fx_enable_unit_value(track.fx_is_enabled()),
            Transport { project, action } => {
                use TransportAction::*;
                let play_state = project.play_state();
                match action {
                    PlayStop | PlayPause => transport_is_enabled_unit_value(play_state.is_playing),
                    Stop => transport_is_enabled_unit_value(
                        !play_state.is_playing && !play_state.is_paused,
                    ),
                    Pause => transport_is_enabled_unit_value(play_state.is_paused),
                    Record => transport_is_enabled_unit_value(play_state.is_recording),
                    Repeat => transport_is_enabled_unit_value(project.repeat_is_enabled()),
                }
            }
            LoadFxSnapshot { fx, chunk_hash, .. } => {
                let is_loaded = BackboneState::target_context()
                    .borrow()
                    .current_fx_snapshot_chunk_hash(fx)
                    == Some(*chunk_hash);
                convert_bool_to_unit_value(is_loaded)
            }
            AutomationTouchState {
                track,
                parameter_type,
                ..
            } => {
                let is_touched = BackboneState::target_context()
                    .borrow()
                    .automation_parameter_is_touched(track.raw(), *parameter_type);
                touched_unit_value(is_touched)
            }
            GoToBookmark {
                project,
                bookmark_type,
                index,
                ..
            } => current_value_of_bookmark(
                *project,
                *bookmark_type,
                *index,
                project.play_or_edit_cursor_position(),
            ),
            Seek { project, options } => {
                current_value_of_seek(*project, *options, project.play_or_edit_cursor_position())
            }
            SendMidi { .. } | SendOsc { .. } => return None,
            ClipTransport {
                slot_index, action, ..
            } => {
                let context = context.as_ref()?;
                let instance_state = context.instance_state.borrow();
                use TransportAction::*;
                match action {
                    PlayStop | PlayPause | Stop | Pause => {
                        let play_state = instance_state.get_play_state(*slot_index).ok()?;
                        clip_play_state_unit_value(*action, play_state)
                    }
                    Repeat => {
                        let is_looped = instance_state.get_is_looped(*slot_index).ok()?;
                        transport_is_enabled_unit_value(is_looped)
                    }
                    Record => return None,
                }
            }
        };
        Some(result)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

// Panics if called with repeat or record.
fn clip_play_state_unit_value(action: TransportAction, play_state: ClipPlayState) -> UnitValue {
    use TransportAction::*;
    match action {
        PlayStop | PlayPause | Stop | Pause => match action {
            PlayStop | PlayPause => {
                transport_is_enabled_unit_value(play_state == ClipPlayState::Playing)
            }
            Stop => transport_is_enabled_unit_value(play_state == ClipPlayState::Stopped),
            Pause => transport_is_enabled_unit_value(play_state == ClipPlayState::Paused),
            _ => unreachable!(),
        },
        _ => panic!("wrong argument"),
    }
}

fn current_value_of_bookmark(
    project: Project,
    bookmark_type: BookmarkType,
    index: u32,
    pos: PositionInSeconds,
) -> UnitValue {
    let current_bookmark = project.current_bookmark_at(pos);
    let relevant_index = match bookmark_type {
        BookmarkType::Marker => current_bookmark.marker_index,
        BookmarkType::Region => current_bookmark.region_index,
    };
    let is_current = relevant_index == Some(index);
    convert_bool_to_unit_value(is_current)
}

fn current_value_of_seek(
    project: Project,
    options: SeekOptions,
    pos: PositionInSeconds,
) -> UnitValue {
    let info = match get_seek_info(project, options) {
        None => return UnitValue::MIN,
        Some(i) => i,
    };
    if pos < info.start_pos {
        UnitValue::MIN
    } else {
        let pos_within_range = pos.get() - info.start_pos.get();
        UnitValue::new_clamped(pos_within_range / info.length())
    }
}

/// Converts a number of possible values to a step size.
fn convert_count_to_step_size(n: u32) -> UnitValue {
    // Dividing 1.0 by n would divide the unit interval (0..=1) into n same-sized
    // sub intervals, which means we would have n + 1 possible values. We want to
    // represent just n values, so we need n - 1 same-sized sub intervals.
    if n == 0 || n == 1 {
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
    let db = Volume::try_from_soft_normalized_value(value.get())
        .unwrap_or(Volume::MIN)
        .db();
    if db == Db::MINUS_INF {
        "-inf".to_string()
    } else {
        format!("{:.2}", db.get())
    }
}

fn format_value_as_db(value: UnitValue) -> String {
    Volume::try_from_soft_normalized_value(value.get())
        .unwrap_or(Volume::MIN)
        .to_string()
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
    convert_unit_to_discrete_value_with_none(value, fx.preset_count().ok()?)
}

fn convert_unit_value_to_track_index(project: Project, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, project.track_count())
}

fn convert_unit_value_to_fx_index(fx_chain: &FxChain, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, fx_chain.fx_count())
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

fn selected_track_unit_value(project: Project, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, project.track_count())
}

fn shown_fx_unit_value(fx_chain: &FxChain, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx_chain.fx_count())
}

fn fx_preset_unit_value(fx: &Fx, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx.preset_count().unwrap_or(0))
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
            if count == 0 {
                return UnitValue::MIN;
            }
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
        ActionInvocationType::Absolute
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
    #[serde(rename = "stop")]
    #[display(fmt = "Stop")]
    Stop,
    #[serde(rename = "pause")]
    #[display(fmt = "Pause")]
    Pause,
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

fn determine_target_for_action(action: Action) -> ReaperTarget {
    let project = Reaper::get().current_project();
    match action.command_id().get() {
        // Play button | stop button
        1007 | 1016 => ReaperTarget::Transport {
            project,
            action: TransportAction::PlayStop,
        },
        // Pause button
        1008 => ReaperTarget::Transport {
            project,
            action: TransportAction::PlayPause,
        },
        // Record button
        1013 => ReaperTarget::Transport {
            project,
            action: TransportAction::Record,
        },
        // Repeat button
        1068 => ReaperTarget::Transport {
            project,
            action: TransportAction::Repeat,
        },
        _ => ReaperTarget::Action {
            action,
            invocation_type: ActionInvocationType::Trigger,
            project,
        },
    }
}

trait PanExt {
    /// Returns the pan value. In case of dual-pan, returns the left pan value.
    fn main_pan(self) -> ReaperPanValue;
    fn width(self) -> Option<ReaperWidthValue>;
}

impl PanExt for reaper_medium::Pan {
    /// Returns the pan value. In case of dual-pan, returns the left pan value.
    fn main_pan(self) -> ReaperPanValue {
        use reaper_medium::Pan::*;
        match self {
            BalanceV1(p) => p,
            BalanceV4(p) => p,
            StereoPan { pan, .. } => pan,
            DualPan { left, .. } => left,
            Unknown(_) => ReaperPanValue::CENTER,
        }
    }

    fn width(self) -> Option<ReaperWidthValue> {
        if let reaper_medium::Pan::StereoPan { width, .. } = self {
            Some(width)
        } else {
            None
        }
    }
}

fn figure_out_touched_pan_component(
    track: Track,
    old: reaper_medium::Pan,
    new: reaper_medium::Pan,
) -> ReaperTarget {
    if old.width() != new.width() {
        ReaperTarget::TrackWidth { track }
    } else {
        ReaperTarget::TrackPan { track }
    }
}

fn fx_parameter_unit_value(param: &FxParameter, value: ReaperNormalizedFxParamValue) -> UnitValue {
    let v = value.get();
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
        return UnitValue::new_clamped(v);
    }
    UnitValue::new(v)
}

fn volume_unit_value(volume: Volume) -> UnitValue {
    // The soft-normalized value can be > 1.0, e.g. when we have a volume of 12 dB and then
    // lower the volume fader limit to a lower value. In that case we just report the
    // highest possible value ... not much else we can do.
    UnitValue::new_clamped(volume.soft_normalized_value())
}

fn pan_unit_value(pan: Pan) -> UnitValue {
    UnitValue::new(pan.normalized_value())
}

fn width_unit_value(width: Width) -> UnitValue {
    UnitValue::new(width.normalized_value())
}

fn track_arm_unit_value(is_armed: bool) -> UnitValue {
    convert_bool_to_unit_value(is_armed)
}

fn track_selected_unit_value(is_selected: bool) -> UnitValue {
    convert_bool_to_unit_value(is_selected)
}

fn mute_unit_value(is_mute: bool) -> UnitValue {
    convert_bool_to_unit_value(is_mute)
}

fn touched_unit_value(is_touched: bool) -> UnitValue {
    convert_bool_to_unit_value(is_touched)
}

fn track_solo_unit_value(is_solo: bool) -> UnitValue {
    convert_bool_to_unit_value(is_solo)
}

fn track_automation_mode_unit_value(
    desired_automation_mode: AutomationMode,
    actual_automation_mode: AutomationMode,
) -> UnitValue {
    let is_on = desired_automation_mode == actual_automation_mode;
    convert_bool_to_unit_value(is_on)
}

fn global_automation_mode_override_unit_value(
    desired_mode_override: Option<GlobalAutomationModeOverride>,
    actual_mode_override: Option<GlobalAutomationModeOverride>,
) -> UnitValue {
    convert_bool_to_unit_value(actual_mode_override == desired_mode_override)
}

fn tempo_unit_value(tempo: Tempo) -> UnitValue {
    UnitValue::new(tempo.normalized_value())
}

fn playrate_unit_value(playrate: PlayRate) -> UnitValue {
    UnitValue::new(playrate.normalized_value().get())
}

fn fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

fn all_track_fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

fn transport_is_enabled_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

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
pub enum SoloBehavior {
    #[display(fmt = "Solo in place")]
    InPlace,
    #[display(fmt = "Solo (ignore routing)")]
    IgnoreRouting,
    #[display(fmt = "Use REAPER preference")]
    ReaperPreference,
}

impl Default for SoloBehavior {
    fn default() -> Self {
        // We could choose ReaperPreference as default but that would be a bit against ReaLearn's
        // initial idea of being the number one tool for very project-specific mappings.
        SoloBehavior::InPlace
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TouchedParameterType {
    Volume,
    Pan,
    Width,
}

impl Default for TouchedParameterType {
    fn default() -> Self {
        TouchedParameterType::Volume
    }
}

impl TouchedParameterType {
    pub fn try_from_reaper(
        reaper_type: reaper_medium::TouchedParameterType,
    ) -> Result<Self, &'static str> {
        use reaper_medium::TouchedParameterType::*;
        let res = match reaper_type {
            Volume => Self::Volume,
            Pan => Self::Pan,
            Width => Self::Width,
            Unknown(_) => return Err("unknown touch parameter type"),
        };
        Ok(res)
    }
}

/// Returns if "Mouse click on volume/pan faders and track buttons changes track selection"
/// is enabled in the REAPER preferences.
fn track_sel_on_mouse_is_enabled() -> bool {
    use once_cell::sync::Lazy;
    static IS_ENABLED: Lazy<bool> = Lazy::new(query_track_sel_on_mouse_is_enabled);
    *IS_ENABLED
}

fn query_track_sel_on_mouse_is_enabled() -> bool {
    if let Some(res) = Reaper::get()
        .medium_reaper()
        .get_config_var("trackselonmouse")
    {
        if res.size != 4 {
            // Shouldn't be.
            return false;
        }
        let ptr = res.value.as_ptr() as *const u32;
        let value = unsafe { *ptr };
        // The second flag corresponds to that setting.
        (value & 2) != 0
    } else {
        false
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TrackExclusivity {
    #[display(fmt = "No")]
    NonExclusive,
    #[display(fmt = "Within project")]
    ExclusiveAll,
    #[display(fmt = "Within folder")]
    ExclusiveFolder,
}

impl Default for TrackExclusivity {
    fn default() -> Self {
        TrackExclusivity::NonExclusive
    }
}

impl HierarchyEntryProvider for Project {
    type Entry = Track;

    fn find_entry_by_index(&self, index: u32) -> Option<Self::Entry> {
        // TODO-medium This could be made faster by separating between heavy-weight and
        //  light-weight tracks in reaper-rs.
        self.track_by_index(index)
    }

    fn entry_count(&self) -> u32 {
        self.track_count()
    }
}

impl HierarchyEntry for Track {
    fn folder_depth_change(&self) -> i32 {
        self.folder_depth_change()
    }
}

fn handle_track_exclusivity(
    track: &Track,
    exclusivity: TrackExclusivity,
    mut f: impl FnMut(&Track),
) {
    let track_index = match track.index() {
        // We consider the master track as its own folder (same as non-exclusive).
        None => return,
        Some(i) => i,
    };
    handle_exclusivity(
        &track.project(),
        exclusivity,
        track_index,
        track,
        |_, track| f(track),
    );
}

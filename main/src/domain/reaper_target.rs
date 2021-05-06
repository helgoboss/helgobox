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
    PositionInSeconds, ReaperNormalizedFxParamValue, ReaperPanValue, ReaperVolumeValue,
    ReaperWidthValue, SetEditCurPosOptions, SoloMode, TrackArea, UndoBehavior,
};
use rx_util::{Event, UnitEvent};
use rxrust::prelude::*;

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use slog::warn;

use crate::core::Global;
use crate::domain::ui_util::{
    convert_bool_to_unit_value, format_as_double_percentage_without_unit,
    format_as_percentage_without_unit, format_as_symmetric_percentage_without_unit,
    format_value_as_db, format_value_as_db_without_unit, fx_parameter_unit_value,
    parse_from_double_percentage, parse_from_symmetric_percentage,
    parse_unit_value_from_percentage, parse_value_from_db, reaper_volume_unit_value,
    volume_unit_value,
};
use crate::domain::{
    handle_exclusivity, ActionTarget, AdditionalFeedbackEvent, AllTrackFxEnableTarget,
    AutomationModeOverrideTarget, BackboneState, ClipChangedEvent, ClipPlayState, ControlContext,
    FeedbackAudioHookTask, FeedbackOutput, FxEnableTarget, FxNavigateTarget, FxOpenTarget,
    FxParameterTarget, FxPresetTarget, HierarchyEntry, HierarchyEntryProvider,
    InstanceFeedbackEvent, LoadFxSnapshotTarget, MidiDestination, MidiSendTarget, OscDeviceId,
    OscFeedbackTask, PlayrateTarget, RealearnTarget, RouteMuteTarget, RoutePanTarget,
    RouteVolumeTarget, SelectedTrackTarget, SlotPlayOptions, TempoTarget, TrackArmTarget,
    TrackAutomationModeTarget, TrackMuteTarget, TrackPanTarget, TrackPeakTarget,
    TrackSelectionTarget, TrackShowTarget, TrackSoloTarget, TrackVolumeTarget, TrackWidthTarget,
    TransportTarget,
};
use rosc::OscMessage;
use std::convert::TryInto;
use std::num::NonZeroU32;
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
// TODO-high The Clone can probably be removed now!
// TODO-medium We should introduce enum_dispatch
#[derive(Clone, Debug, PartialEq)]
pub enum ReaperTarget {
    Action(ActionTarget),
    FxParameter(FxParameterTarget),
    TrackVolume(TrackVolumeTarget),
    TrackPeak(TrackPeakTarget),
    TrackRouteVolume(RouteVolumeTarget),
    TrackPan(TrackPanTarget),
    TrackWidth(TrackWidthTarget),
    TrackArm(TrackArmTarget),
    TrackSelection(TrackSelectionTarget),
    TrackMute(TrackMuteTarget),
    TrackShow(TrackShowTarget),
    TrackSolo(TrackSoloTarget),
    TrackAutomationMode(TrackAutomationModeTarget),
    TrackRoutePan(RoutePanTarget),
    TrackRouteMute(RouteMuteTarget),
    Tempo(TempoTarget),
    Playrate(PlayrateTarget),
    AutomationModeOverride(AutomationModeOverrideTarget),
    FxEnable(FxEnableTarget),
    FxOpen(FxOpenTarget),
    FxPreset(FxPresetTarget),
    SelectedTrack(SelectedTrackTarget),
    FxNavigate(FxNavigateTarget),
    AllTrackFxEnable(AllTrackFxEnableTarget),
    Transport(TransportTarget),
    LoadFxSnapshot(LoadFxSnapshotTarget),
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
    SendMidi(MidiSendTarget),
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
    ClipSeek {
        slot_index: usize,
        feedback_resolution: PlayPosFeedbackResolution,
    },
    ClipVolume {
        slot_index: usize,
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
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        use ReaperTarget::*;
        use TargetCharacter::*;
        match self {
            AutomationTouchState { exclusivity, .. } => {
                get_control_type_and_character_for_track_exclusivity(*exclusivity)
            }
            // TODO-low "Seek" could support rounding/discrete (beats, measures, seconds, ...)
            Seek { .. } | ClipSeek { .. } | ClipVolume { .. } => {
                (ControlType::AbsoluteContinuous, Continuous)
            }
            LoadFxSnapshot { .. } | GoToBookmark { .. } => {
                (ControlType::AbsoluteContinuousRetriggerable, Trigger)
            }
            SendOsc { arg_descriptor, .. } => {
                if let Some(desc) = arg_descriptor {
                    use OscTypeTag::*;
                    match desc.type_tag() {
                        Float | Double => {
                            (ControlType::AbsoluteContinuousRetriggerable, Continuous)
                        }
                        Bool => (ControlType::AbsoluteContinuousRetriggerable, Switch),
                        Nil | Inf => (ControlType::AbsoluteContinuousRetriggerable, Trigger),
                        _ => (ControlType::AbsoluteContinuousRetriggerable, Trigger),
                    }
                } else {
                    (ControlType::AbsoluteContinuousRetriggerable, Trigger)
                }
            }
            ClipTransport { .. } => (ControlType::AbsoluteContinuousRetriggerable, Switch),
            SendMidi(t) => t.control_type_and_character(),
            TrackPeak(t) => t.control_type_and_character(),
            Action(t) => t.control_type_and_character(),
            FxParameter(t) => t.control_type_and_character(),
            TrackVolume(t) => t.control_type_and_character(),
            TrackPan(t) => t.control_type_and_character(),
            TrackWidth(t) => t.control_type_and_character(),
            TrackArm(t) => t.control_type_and_character(),
            TrackRouteVolume(t) => t.control_type_and_character(),
            TrackSelection(t) => t.control_type_and_character(),
            TrackMute(t) => t.control_type_and_character(),
            TrackShow(t) => t.control_type_and_character(),
            TrackSolo(t) => t.control_type_and_character(),
            TrackAutomationMode(t) => t.control_type_and_character(),
            TrackRoutePan(t) => t.control_type_and_character(),
            TrackRouteMute(t) => t.control_type_and_character(),
            Tempo(t) => t.control_type_and_character(),
            Playrate(t) => t.control_type_and_character(),
            AutomationModeOverride(t) => t.control_type_and_character(),
            FxEnable(t) => t.control_type_and_character(),
            FxOpen(t) => t.control_type_and_character(),
            FxPreset(t) => t.control_type_and_character(),
            SelectedTrack(t) => t.control_type_and_character(),
            FxNavigate(t) => t.control_type_and_character(),
            AllTrackFxEnable(t) => t.control_type_and_character(),
            Transport(t) => t.control_type_and_character(),
        }
    }

    fn open(&self) {
        if let ReaperTarget::Action(t) = self {
            t.open();
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
            ClipVolume { .. } => parse_value_from_db(text),
            FxNavigate { .. } | SelectedTrack { .. } => self.parse_value_from_discrete_value(text),
            // Default: Percentage
            Action(_)
            | LoadFxSnapshot(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | TrackSolo(_)
            | TrackRouteMute(_)
            | GoToBookmark { .. }
            | FxEnable(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Transport(_)
            | SendOsc { .. }
            | ClipTransport { .. }
            | ClipSeek { .. }
            | Seek { .. } => parse_unit_value_from_percentage(text),
            SendMidi(t) => t.parse_as_value(text),
            TrackPeak(t) => t.parse_as_value(text),
            FxParameter(t) => t.parse_as_value(text),
            TrackVolume(t) => t.parse_as_value(text),
            TrackPan(t) => t.parse_as_value(text),
            TrackWidth(t) => t.parse_as_value(text),
            TrackArm(t) => t.parse_as_value(text),
            TrackRouteVolume(t) => t.parse_as_value(text),
            TrackRoutePan(t) => t.parse_as_value(text),
            Tempo(t) => t.parse_as_value(text),
            Playrate(t) => t.parse_as_value(text),
            FxPreset(t) => t.parse_as_value(text),
        }
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        use ReaperTarget::*;
        match self {
            FxPreset { .. } | FxNavigate { .. } | SelectedTrack { .. } => {
                self.parse_value_from_discrete_value(text)
            }
            // Default: Percentage
            Action(_)
            | LoadFxSnapshot(_)
            | ClipVolume { .. }
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | FxEnable(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Transport(_)
            | SendOsc { .. }
            | ClipTransport { .. }
            | ClipSeek { .. }
            | Seek { .. } => parse_unit_value_from_percentage(text),
            SendMidi(t) => t.parse_as_step_size(text),
            TrackPeak(t) => t.parse_as_step_size(text),
            FxParameter(t) => t.parse_as_step_size(text),
            TrackVolume(t) => t.parse_as_step_size(text),
            TrackPan(t) => t.parse_as_step_size(text),
            TrackWidth(t) => t.parse_as_step_size(text),
            TrackArm(t) => t.parse_as_step_size(text),
            TrackRouteVolume(t) => t.parse_as_step_size(text),
            Tempo(t) => t.parse_as_step_size(text),
            Playrate(t) => t.parse_as_step_size(text),
        }
    }

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        if self.control_type().is_relative() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        use ReaperTarget::*;
        let result = match self {
            // Default: Not supported
            Action(_)
            | ClipVolume { .. }
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | Tempo(_)
            | Playrate(_)
            | FxEnable(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | LoadFxSnapshot(_)
            | Seek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | ClipSeek { .. }
            | Transport { .. } => return Err("not supported"),
            SendMidi(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackPeak(t) => return t.convert_unit_value_to_discrete_value(input),
            FxParameter(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackVolume(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackPan(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackWidth(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackArm(t) => return t.convert_unit_value_to_discrete_value(input),
            TrackRouteVolume(t) => return t.convert_unit_value_to_discrete_value(input),
            FxPreset(t) => return t.convert_unit_value_to_discrete_value(input),
            SelectedTrack(t) => return t.convert_unit_value_to_discrete_value(input),
            FxNavigate(t) => return t.convert_unit_value_to_discrete_value(input),
        };
        Ok(result)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            ClipVolume { .. } => format_value_as_db_without_unit(value),
            // Default: Percentage
            Action(_)
            | LoadFxSnapshot(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRouteMute(_)
            | FxEnable(_)
            | FxPreset(_)
            | SelectedTrack(_)
            | FxNavigate(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => format_as_percentage_without_unit(value),
            SendMidi(t) => t.format_value_without_unit(value),
            TrackPeak(t) => t.format_value_without_unit(value),
            FxParameter(t) => t.format_value_without_unit(value),
            TrackVolume(t) => t.format_value_without_unit(value),
            TrackPan(t) => t.format_value_without_unit(value),
            TrackWidth(t) => t.format_value_without_unit(value),
            TrackArm(t) => t.format_value_without_unit(value),
            TrackRouteVolume(t) => t.format_value_without_unit(value),
            TrackRoutePan(t) => t.format_value_without_unit(value),
            Tempo(t) => t.format_value_without_unit(value),
            Playrate(t) => t.format_value_without_unit(value),
        }
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            // Default: Percentage
            Action(_)
            | LoadFxSnapshot(_)
            | ClipVolume { .. }
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | FxEnable(_)
            | FxPreset(_)
            | SelectedTrack(_)
            | FxNavigate(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => format_as_percentage_without_unit(step_size),
            SendMidi(t) => t.format_step_size_without_unit(step_size),
            TrackPeak(t) => t.format_step_size_without_unit(step_size),
            FxParameter(t) => t.format_step_size_without_unit(step_size),
            TrackVolume(t) => t.format_step_size_without_unit(step_size),
            TrackPan(t) => t.format_step_size_without_unit(step_size),
            TrackWidth(t) => t.format_step_size_without_unit(step_size),
            TrackArm(t) => t.format_step_size_without_unit(step_size),
            TrackRouteVolume(t) => t.format_step_size_without_unit(step_size),
            Tempo(t) => t.format_step_size_without_unit(step_size),
            Playrate(t) => t.format_step_size_without_unit(step_size),
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
            ClipVolume { .. } => "dB",
            // Default: percentage
            Action(_)
            | LoadFxSnapshot(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRouteMute(_)
            | FxEnable(_)
            | FxPreset(_)
            | SelectedTrack(_)
            | FxNavigate(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => "%",
            SendMidi(t) => t.value_unit(),
            TrackPeak(t) => t.value_unit(),
            FxParameter(t) => t.value_unit(),
            TrackVolume(t) => t.value_unit(),
            TrackPan(t) => t.value_unit(),
            TrackWidth(t) => t.value_unit(),
            TrackArm(t) => t.value_unit(),
            TrackRouteVolume(t) => t.value_unit(),
            TrackRoutePan(t) => t.value_unit(),
            Tempo(t) => t.value_unit(),
            Playrate(t) => t.value_unit(),
        }
    }

    fn step_size_unit(&self) -> &'static str {
        use ReaperTarget::*;
        match self {
            // Default: Percentage
            Action(_)
            | LoadFxSnapshot(_)
            | ClipVolume { .. }
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRouteMute(_)
            | FxEnable(_)
            | FxPreset(_)
            | SelectedTrack(_)
            | FxNavigate(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => "%",
            SendMidi(t) => t.step_size_unit(),
            TrackPeak(t) => t.step_size_unit(),
            FxParameter(t) => t.step_size_unit(),
            TrackVolume(t) => t.step_size_unit(),
            TrackPan(t) => t.step_size_unit(),
            TrackWidth(t) => t.step_size_unit(),
            TrackArm(t) => t.step_size_unit(),
            TrackRouteVolume(t) => t.step_size_unit(),
            TrackRoutePan(t) => t.step_size_unit(),
            Tempo(t) => t.step_size_unit(),
            Playrate(t) => t.step_size_unit(),
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            ClipVolume { .. } => format_value_as_db(value),
            GoToBookmark { .. } => format_value_as_on_off(value).to_string(),
            Tempo { .. }
            | Playrate { .. }
            | AllTrackFxEnable { .. }
            | AutomationTouchState { .. }
            | Transport { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. } => self.format_value_generic(value),
            LoadFxSnapshot { .. } => "".to_owned(),
            SendMidi(t) => t.format_value(value),
            TrackPeak(t) => t.format_value(value),
            Action(t) => t.format_value(value),
            FxParameter(t) => t.format_value(value),
            TrackVolume(t) => t.format_value(value),
            TrackPan(t) => t.format_value(value),
            TrackWidth(t) => t.format_value(value),
            TrackArm(t) => t.format_value(value),
            TrackRouteVolume(t) => t.format_value(value),
            TrackSelection(t) => t.format_value(value),
            TrackMute(t) => t.format_value(value),
            TrackShow(t) => t.format_value(value),
            TrackSolo(t) => t.format_value(value),
            TrackAutomationMode(t) => t.format_value(value),
            TrackRoutePan(t) => t.format_value(value),
            TrackRouteMute(t) => t.format_value(value),
            AutomationModeOverride(t) => t.format_value(value),
            FxEnable(t) => t.format_value(value),
            FxOpen(t) => t.format_value(value),
            FxPreset(t) => t.format_value(value),
            SelectedTrack(t) => t.format_value(value),
            FxNavigate(t) => t.format_value(value),
        }
    }

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        use ControlValue::*;
        use ReaperTarget::*;
        match self {
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
                let info = get_seek_info(*project, *options);
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
                    .try_send(OscFeedbackTask::new(effective_dev_id, msg))
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
                            instance_state.play(*slot_index, track.clone(), *play_options)?;
                        } else {
                            instance_state.stop(*slot_index, !play_options.next_bar)?;
                        }
                    }
                    PlayPause => {
                        if on {
                            instance_state.play(*slot_index, track.clone(), *play_options)?;
                        } else {
                            instance_state.pause(*slot_index)?;
                        }
                    }
                    Stop => {
                        if on {
                            instance_state.stop(*slot_index, !play_options.next_bar)?;
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
                        instance_state.toggle_repeat(*slot_index)?;
                    }
                };
            }
            ClipSeek { slot_index, .. } => {
                let value = value.as_absolute()?;
                let mut instance_state = context.instance_state.borrow_mut();
                instance_state.seek_slot(*slot_index, value)?;
            }
            ClipVolume { slot_index } => {
                let volume = Volume::try_from_soft_normalized_value(value.as_absolute()?.get());
                let mut instance_state = context.instance_state.borrow_mut();
                instance_state
                    .set_volume(*slot_index, volume.unwrap_or(Volume::MIN).reaper_value())?;
            }
            SendMidi(t) => return t.control(value, context),
            TrackPeak(t) => return t.control(value, context),
            Action(t) => return t.control(value, context),
            FxParameter(t) => return t.control(value, context),
            TrackVolume(t) => return t.control(value, context),
            TrackPan(t) => return t.control(value, context),
            TrackWidth(t) => return t.control(value, context),
            TrackArm(t) => return t.control(value, context),
            TrackRouteVolume(t) => return t.control(value, context),
            TrackSelection(t) => return t.control(value, context),
            TrackMute(t) => return t.control(value, context),
            TrackShow(t) => return t.control(value, context),
            TrackSolo(t) => return t.control(value, context),
            TrackAutomationMode(t) => return t.control(value, context),
            TrackRoutePan(t) => return t.control(value, context),
            TrackRouteMute(t) => return t.control(value, context),
            Tempo(t) => return t.control(value, context),
            Playrate(t) => return t.control(value, context),
            AutomationModeOverride(t) => return t.control(value, context),
            FxEnable(t) => return t.control(value, context),
            FxOpen(t) => return t.control(value, context),
            FxPreset(t) => return t.control(value, context),
            LoadFxSnapshot(t) => return t.control(value, context),
            SelectedTrack(t) => return t.control(value, context),
            FxNavigate(t) => return t.control(value, context),
            AllTrackFxEnable(t) => return t.control(value, context),
            Transport(t) => return t.control(value, context),
        };
        Ok(())
    }

    fn can_report_current_value(&self) -> bool {
        use ReaperTarget::*;
        !matches!(self, SendMidi { .. } | SendOsc { .. })
    }

    /// Used for "Last touched" target only at the moment.
    fn is_available(&self) -> bool {
        use ReaperTarget::*;
        match self {
            AutomationTouchState { track, .. } => track.is_available(),
            GoToBookmark { project, .. } | Seek { project, .. } => project.is_available(),
            // TODO-medium With clip targets we should check the control context (instance state) if
            //  slot filled.
            ClipTransport { track, .. } => {
                if let Some(t) = track {
                    if !t.is_available() {
                        return false;
                    }
                }
                true
            }
            ClipSeek { .. } | ClipVolume { .. } => true,
            SendOsc { .. } => true,
            SendMidi(t) => t.is_available(),
            TrackPeak(t) => t.is_available(),
            Action(t) => t.is_available(),
            FxParameter(t) => t.is_available(),
            TrackVolume(t) => t.is_available(),
            TrackPan(t) => t.is_available(),
            TrackWidth(t) => t.is_available(),
            TrackArm(t) => t.is_available(),
            TrackRouteVolume(t) => t.is_available(),
            TrackSelection(t) => t.is_available(),
            TrackMute(t) => t.is_available(),
            TrackShow(t) => t.is_available(),
            TrackSolo(t) => t.is_available(),
            TrackAutomationMode(t) => t.is_available(),
            TrackRoutePan(t) => t.is_available(),
            TrackRouteMute(t) => t.is_available(),
            Tempo(t) => t.is_available(),
            Playrate(t) => t.is_available(),
            AutomationModeOverride(t) => t.is_available(),
            FxEnable(t) => t.is_available(),
            FxOpen(t) => t.is_available(),
            FxPreset(t) => t.is_available(),
            LoadFxSnapshot(t) => t.is_available(),
            SelectedTrack(t) => t.is_available(),
            FxNavigate(t) => t.is_available(),
            AllTrackFxEnable(t) => t.is_available(),
            Transport(t) => t.is_available(),
        }
    }

    fn project(&self) -> Option<Project> {
        use ReaperTarget::*;
        let project = match self {
            // Default: None
            Action(_)
            | Transport { .. }
            | AutomationModeOverride(_)
            | ClipSeek { .. }
            | ClipVolume { .. }
            | SendOsc { .. } => {
                return None;
            }
            AutomationTouchState { track, .. } => track.project(),
            GoToBookmark { project, .. } | Seek { project, .. } => *project,
            ClipTransport { track, .. } => return track.as_ref().map(|t| t.project()),
            SendMidi(t) => return t.project(),
            TrackPeak(t) => return t.project(),
            FxParameter(t) => return t.project(),
            TrackVolume(t) => return t.project(),
            TrackPan(t) => return t.project(),
            TrackWidth(t) => return t.project(),
            TrackArm(t) => return t.project(),
            TrackRouteVolume(t) => return t.project(),
            TrackSelection(t) => return t.project(),
            TrackMute(t) => return t.project(),
            TrackShow(t) => return t.project(),
            TrackSolo(t) => return t.project(),
            TrackAutomationMode(t) => return t.project(),
            TrackRoutePan(t) => return t.project(),
            TrackRouteMute(t) => return t.project(),
            Tempo(t) => return t.project(),
            Playrate(t) => return t.project(),
            FxEnable(t) => return t.project(),
            FxOpen(t) => return t.project(),
            FxPreset(t) => return t.project(),
            LoadFxSnapshot(t) => return t.project(),
            SelectedTrack(t) => return t.project(),
            FxNavigate(t) => return t.project(),
            AllTrackFxEnable(t) => return t.project(),
        };
        Some(project)
    }

    fn track(&self) -> Option<&Track> {
        use ReaperTarget::*;
        let track = match self {
            AutomationTouchState { track, .. } => track,
            // Default: None
            Action(_)
            | Tempo(_)
            | Playrate(_)
            | SelectedTrack(_)
            | GoToBookmark { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | ClipVolume { .. }
            | AutomationModeOverride(_)
            | Transport { .. }
            | SendOsc { .. } => return None,
            ClipTransport { track, .. } => return track.as_ref(),
            SendMidi(t) => return t.track(),
            TrackPeak(t) => return t.track(),
            FxParameter(t) => return t.track(),
            TrackVolume(t) => return t.track(),
            TrackPan(t) => return t.track(),
            TrackWidth(t) => return t.track(),
            TrackArm(t) => return t.track(),
            TrackRouteVolume(t) => return t.track(),
            TrackSelection(t) => return t.track(),
            TrackMute(t) => return t.track(),
            TrackShow(t) => return t.track(),
            TrackSolo(t) => return t.track(),
            TrackAutomationMode(t) => return t.track(),
            TrackRoutePan(t) => return t.track(),
            TrackRouteMute(t) => return t.track(),
            FxEnable(t) => return t.track(),
            FxOpen(t) => return t.track(),
            FxPreset(t) => return t.track(),
            LoadFxSnapshot(t) => return t.track(),
            FxNavigate(t) => return t.track(),
            AllTrackFxEnable(t) => return t.track(),
        };
        Some(track)
    }

    fn fx(&self) -> Option<&Fx> {
        use ReaperTarget::*;
        let fx = match self {
            // Default: None
            Action(_)
            | TrackRouteVolume(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | Tempo(_)
            | AutomationModeOverride(_)
            | Playrate(_)
            | SelectedTrack(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | FxNavigate(_)
            | Transport { .. }
            | ClipTransport { .. }
            | ClipVolume { .. }
            | SendOsc { .. } => return None,
            SendMidi(t) => return t.fx(),
            TrackPeak(t) => return t.fx(),
            FxParameter(t) => return t.fx(),
            TrackVolume(t) => return t.fx(),
            TrackPan(t) => return t.fx(),
            TrackWidth(t) => return t.fx(),
            TrackArm(t) => return t.fx(),
            FxEnable(t) => return t.fx(),
            FxOpen(t) => return t.fx(),
            FxPreset(t) => return t.fx(),
            LoadFxSnapshot(t) => return t.fx(),
        };
        Some(fx)
    }

    fn route(&self) -> Option<&TrackRoute> {
        use ReaperTarget::*;
        let route = match self {
            // Default: None
            Action(_)
            | FxEnable(_)
            | FxPreset(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | FxNavigate(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | Tempo(_)
            | Playrate(_)
            | SelectedTrack(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | LoadFxSnapshot(_)
            | Seek { .. }
            | ClipSeek { .. }
            | Transport { .. }
            | ClipTransport { .. }
            | ClipVolume { .. }
            | SendOsc { .. } => return None,
            SendMidi(t) => return t.route(),
            TrackPeak(t) => return t.route(),
            FxParameter(t) => return t.route(),
            TrackVolume(t) => return t.route(),
            TrackPan(t) => return t.route(),
            TrackWidth(t) => return t.route(),
            TrackArm(t) => return t.route(),
            TrackRouteVolume(t) => return t.route(),
            TrackRoutePan(t) => return t.route(),
            TrackRouteMute(t) => return t.route(),
        };
        Some(route)
    }

    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        use ReaperTarget::*;
        match self {
            AutomationTouchState { exclusivity, .. } => Some(*exclusivity),
            // Default: None
            Action(_)
            | TrackRouteVolume(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | Tempo(_)
            | GoToBookmark { .. }
            | Playrate(_)
            | FxEnable(_)
            | FxOpen(_)
            | FxPreset(_)
            | SelectedTrack(_)
            | FxNavigate(_)
            | Transport { .. }
            | LoadFxSnapshot(_)
            | AutomationModeOverride(_)
            | Seek { .. }
            | ClipSeek { .. }
            | ClipTransport { .. }
            | ClipVolume { .. }
            | SendOsc { .. } => None,
            SendMidi(t) => t.track_exclusivity(),
            TrackPeak(t) => t.track_exclusivity(),
            FxParameter(t) => t.track_exclusivity(),
            TrackVolume(t) => t.track_exclusivity(),
            TrackPan(t) => t.track_exclusivity(),
            TrackWidth(t) => t.track_exclusivity(),
            TrackArm(t) => t.track_exclusivity(),
            TrackSelection(t) => t.track_exclusivity(),
            TrackMute(t) => t.track_exclusivity(),
            TrackShow(t) => t.track_exclusivity(),
            TrackSolo(t) => t.track_exclusivity(),
            TrackAutomationMode(t) => t.track_exclusivity(),
            AllTrackFxEnable(t) => t.track_exclusivity(),
        }
    }

    fn supports_automatic_feedback(&self) -> bool {
        use ReaperTarget::*;
        match self {
            // Default: true
            Action(_)
            | TrackRouteVolume(_)
            | TrackSelection(_)
            | TrackMute(_)
            | TrackSolo(_)
            | TrackRoutePan(_)
            | Tempo(_)
            | Playrate(_)
            | FxEnable(_)
            | FxOpen(_)
            | FxPreset(_)
            | GoToBookmark { .. }
            | SelectedTrack(_)
            | FxNavigate(_)
            | LoadFxSnapshot(_)
            | AutomationTouchState { .. }
            | Seek { .. }
            | ClipSeek { .. }
            | AutomationModeOverride(_)
            | TrackAutomationMode(_)
            | ClipTransport { .. }
            | ClipVolume { .. }
            | Transport { .. } => true,
            AllTrackFxEnable { .. } | SendOsc { .. } => false,
            SendMidi(t) => t.supports_automatic_feedback(),
            TrackPeak(t) => t.supports_automatic_feedback(),
            FxParameter(t) => t.supports_automatic_feedback(),
            TrackVolume(t) => t.supports_automatic_feedback(),
            TrackPan(t) => t.supports_automatic_feedback(),
            TrackWidth(t) => t.supports_automatic_feedback(),
            TrackArm(t) => t.supports_automatic_feedback(),
            TrackShow(t) => t.supports_automatic_feedback(),
            TrackRouteMute(t) => t.supports_automatic_feedback(),
        }
    }

    /// Might return the new value if changed.
    ///
    /// Is called in any case (even if feedback not enabled). So we can use it for general-purpose
    /// change event reactions such as reacting to transport stop.
    #[allow(clippy::single_match)]
    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        control_context: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        use ChangeEvent::*;
        use ReaperTarget::*;
        match self {
            // Handled both from control-surface and non-control-surface callbacks.
            GoToBookmark { project, .. } => match evt {
                BookmarksChanged(e) if e.project == *project => (true, None),
                _ => (false, None),
            },
            // Handled from non-control-surface callbacks only.
            LoadFxSnapshot { .. } | AutomationTouchState { .. } | Seek { .. } => (false, None),
            // Feedback handled from instance-scoped feedback events.
            ClipTransport { .. } => {
                match evt {
                    PlayStateChanged(e) => {
                        let mut instance_state = control_context.instance_state.borrow_mut();
                        instance_state.process_transport_change(e.new_value);
                    }
                    _ => {}
                };
                (false, None)
            }
            ClipSeek { .. } | ClipVolume { .. } => (false, None),
            // No value change notification available.
            TrackShow { .. } | TrackRouteMute { .. } | AllTrackFxEnable(_) | SendOsc { .. } => {
                (false, None)
            }
            SendMidi(t) => t.process_change_event(evt, control_context),
            TrackPeak(t) => t.process_change_event(evt, control_context),
            Action(t) => t.process_change_event(evt, control_context),
            FxParameter(t) => t.process_change_event(evt, control_context),
            TrackVolume(t) => t.process_change_event(evt, control_context),
            TrackPan(t) => t.process_change_event(evt, control_context),
            TrackWidth(t) => t.process_change_event(evt, control_context),
            TrackArm(t) => t.process_change_event(evt, control_context),
            TrackRouteVolume(t) => t.process_change_event(evt, control_context),
            TrackSelection(t) => t.process_change_event(evt, control_context),
            TrackMute(t) => t.process_change_event(evt, control_context),
            TrackSolo(t) => t.process_change_event(evt, control_context),
            TrackAutomationMode(t) => t.process_change_event(evt, control_context),
            TrackRoutePan(t) => t.process_change_event(evt, control_context),
            Tempo(t) => t.process_change_event(evt, control_context),
            Playrate(t) => t.process_change_event(evt, control_context),
            AutomationModeOverride(t) => t.process_change_event(evt, control_context),
            FxEnable(t) => t.process_change_event(evt, control_context),
            FxOpen(t) => t.process_change_event(evt, control_context),
            FxPreset(t) => t.process_change_event(evt, control_context),
            SelectedTrack(t) => t.process_change_event(evt, control_context),
            FxNavigate(t) => t.process_change_event(evt, control_context),
            Transport(t) => t.process_change_event(evt, control_context),
        }
    }

    /// Like `convert_unit_value_to_discrete_value()` but in the other direction.
    ///
    /// Used for parsing discrete values of discrete targets that can't do real parsing according to
    /// `can_parse_values()`.
    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
        if self.control_type().is_relative() {
            return (value as f64 / 63.0).try_into();
        }
        use ReaperTarget::*;
        let result = match self {
            // Default: Percentage
            Action(_)
            | TrackRouteVolume(_)
            | ClipVolume { .. }
            | TrackSelection(_)
            | TrackMute(_)
            | TrackShow(_)
            | TrackAutomationMode(_)
            | AutomationModeOverride(_)
            | FxOpen(_)
            | GoToBookmark { .. }
            | TrackSolo(_)
            | TrackRoutePan(_)
            | TrackRouteMute(_)
            | Tempo(_)
            | Playrate(_)
            | FxEnable(_)
            | AllTrackFxEnable(_)
            | AutomationTouchState { .. }
            | LoadFxSnapshot(_)
            | Seek { .. }
            | ClipSeek { .. }
            | SendOsc { .. }
            | ClipTransport { .. }
            | Transport { .. } => return Err("not supported"),
            SendMidi(t) => return t.convert_discrete_value_to_unit_value(value),
            TrackPeak(t) => return t.convert_discrete_value_to_unit_value(value),
            FxParameter(t) => return t.convert_discrete_value_to_unit_value(value),
            TrackVolume(t) => return t.convert_discrete_value_to_unit_value(value),
            TrackPan(t) => return t.convert_discrete_value_to_unit_value(value),
            TrackWidth(t) => return t.convert_discrete_value_to_unit_value(value),
            TrackArm(t) => return t.convert_discrete_value_to_unit_value(value),
            FxPreset(t) => return t.convert_discrete_value_to_unit_value(value),
            SelectedTrack(t) => return t.convert_discrete_value_to_unit_value(value),
            FxNavigate(t) => return t.convert_discrete_value_to_unit_value(value),
        };
        Ok(result)
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use AdditionalFeedbackEvent::*;
        use ReaperTarget::*;
        match self {
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
                BeatChanged(e) if e.project == *project => {
                    let v =
                        current_value_of_bookmark(*project, *bookmark_type, *index, e.new_value);
                    (true, Some(v))
                }
                _ => (false, None),
            },
            Seek { project, options } => match evt {
                BeatChanged(e) if e.project == *project => {
                    let v = current_value_of_seek(*project, *options, e.new_value);
                    (true, Some(v))
                }
                _ => (false, None),
            },
            // If feedback resolution is high, we use the special ClipChangedEvent to do our job
            // (in order to not lock mutex of playing clips more than once per main loop cycle).
            ClipSeek {
                feedback_resolution,
                ..
            } if *feedback_resolution == PlayPosFeedbackResolution::Beat => match evt {
                BeatChanged(_) => (true, None),
                _ => (false, None),
            },
            // This is necessary at the moment because control surface SetPlayState callback works
            // for currently active project tab already.
            Action(t) => t.value_changed_from_additional_feedback_event(evt),
            FxParameter(t) => t.value_changed_from_additional_feedback_event(evt),
            LoadFxSnapshot(t) => t.value_changed_from_additional_feedback_event(evt),
            Transport(t) => t.value_changed_from_additional_feedback_event(evt),
            _ => (false, None),
        }
    }

    #[allow(clippy::collapsible_match)]
    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use InstanceFeedbackEvent::*;
        use ReaperTarget::*;
        match self {
            ClipTransport {
                slot_index, action, ..
            } => {
                match evt {
                    ClipChanged {
                        slot_index: si,
                        event,
                    } if si == slot_index => {
                        use TransportAction::*;
                        match *action {
                            PlayStop | PlayPause | Stop | Pause => match event {
                                ClipChangedEvent::PlayStateChanged(new_state) => {
                                    (true, Some(clip_play_state_unit_value(*action, *new_state)))
                                }
                                _ => (false, None),
                            },
                            // Not supported at the moment.
                            Record => (false, None),
                            Repeat => match event {
                                ClipChangedEvent::ClipRepeatChanged(new_state) => {
                                    (true, Some(transport_is_enabled_unit_value(*new_state)))
                                }
                                _ => (false, None),
                            },
                        }
                    }
                    _ => (false, None),
                }
            }
            // When feedback resolution is beat, we only react to the main timeline beat changes.
            ClipSeek {
                slot_index,
                feedback_resolution,
                ..
            } if *feedback_resolution == PlayPosFeedbackResolution::High => match evt {
                ClipChanged {
                    slot_index: si,
                    event,
                } if si == slot_index => match event {
                    ClipChangedEvent::ClipPositionChanged(new_position) => {
                        (true, Some(*new_position))
                    }
                    ClipChangedEvent::PlayStateChanged(ClipPlayState::Stopped) => {
                        (true, Some(UnitValue::MIN))
                    }
                    _ => (false, None),
                },
                _ => (false, None),
            },
            ClipVolume { slot_index } => match evt {
                ClipChanged {
                    slot_index: si,
                    event,
                } if si == slot_index => match event {
                    ClipChangedEvent::ClipVolumeChanged(new_value) => {
                        (true, Some(reaper_volume_unit_value(*new_value)))
                    }
                    _ => (false, None),
                },
                _ => (false, None),
            },
            _ => (false, None),
        }
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        if let ReaperTarget::SendMidi(t) = self {
            t.splinter_real_time_target()
        } else {
            None
        }
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

fn get_seek_info(project: Project, options: SeekOptions) -> SeekInfo {
    if options.use_time_selection {
        if let Some(r) = project.time_selection() {
            return SeekInfo::from_time_range(r);
        }
    }
    if options.use_loop_points {
        if let Some(r) = project.loop_points() {
            return SeekInfo::from_time_range(r);
        }
    }
    if options.use_regions {
        let bm = project.current_bookmark();
        if let Some(i) = bm.region_index {
            if let Some(bm) = project.find_bookmark_by_index(i) {
                let info = bm.basic_info();
                if let Some(end_pos) = info.region_end_position {
                    return SeekInfo::new(info.position, end_pos);
                }
            }
        }
    }
    if options.use_project {
        let length = project.length();
        if length.get() > 0.0 {
            return SeekInfo::new(
                PositionInSeconds::new(0.0),
                PositionInSeconds::new(length.get()),
            );
        }
    }
    // Last fallback: Viewport seeking. We always have a viewport
    let result = Reaper::get()
        .medium_reaper()
        .get_set_arrange_view_2_get(project.context(), 0, 0);
    SeekInfo::new(result.start_time, result.end_time)
}

impl ReaperTarget {
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
            TrackVolumeChanged(e) if e.touched => TrackVolume(TrackVolumeTarget { track: e.track }),
            TrackPanChanged(e) if e.touched => {
                if let AvailablePanValue::Complete(new_value) = e.new_value {
                    figure_out_touched_pan_component(e.track, e.old_value, new_value)
                } else {
                    // Shouldn't result in this if touched.
                    return None;
                }
            }
            TrackRouteVolumeChanged(e) if e.touched => {
                TrackRouteVolume(RouteVolumeTarget { route: e.route })
            }
            TrackRoutePanChanged(e) if e.touched => {
                TrackRoutePan(RoutePanTarget { route: e.route })
            }
            TrackArmChanged(e) => TrackArm(TrackArmTarget {
                track: e.track,
                exclusivity: Default::default(),
            }),
            TrackMuteChanged(e) if e.touched => TrackMute(TrackMuteTarget {
                track: e.track,
                exclusivity: Default::default(),
            }),
            TrackSoloChanged(e) => {
                // When we press the solo button of some track, REAPER actually sends many
                // change events, starting with the change event for the master track. This is
                // not cool for learning because we could only ever learn master-track solo,
                // which doesn't even make sense. So let's just filter it out.
                if e.track.is_master_track() {
                    return None;
                }
                TrackSolo(TrackSoloTarget {
                    track: e.track,
                    behavior: Default::default(),
                    exclusivity: Default::default(),
                })
            }
            TrackSelectedChanged(e) if e.new_value => {
                if track_sel_on_mouse_is_enabled() {
                    // If this REAPER preference is enabled, it's often a false positive so better
                    // we don't let this happen at all.
                    return None;
                }
                TrackSelection(TrackSelectionTarget {
                    track: e.track,
                    exclusivity: Default::default(),
                    scroll_arrange_view: false,
                    scroll_mixer: false,
                })
            }
            FxEnabledChanged(e) => FxEnable(FxEnableTarget { fx: e.fx }),
            FxParameterValueChanged(e) if e.touched => {
                FxParameter(FxParameterTarget { param: e.parameter })
            }
            FxPresetChanged(e) => FxPreset(FxPresetTarget { fx: e.fx }),
            MasterTempoChanged(e) if e.touched => Tempo(TempoTarget {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            }),
            MasterPlayrateChanged(e) if e.touched => Playrate(PlayrateTarget {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            }),
            TrackAutomationModeChanged(e) => TrackAutomationMode(TrackAutomationModeTarget {
                track: e.track,
                exclusivity: Default::default(),
                mode: e.new_value,
            }),
            GlobalAutomationOverrideChanged(e) => {
                AutomationModeOverride(AutomationModeOverrideTarget {
                    mode_override: e.new_value,
                })
            }
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
                    .map(move |param| FxParameter(FxParameterTarget { param }).into()),
            )
            .merge(
                csurf_rx
                    .fx_enabled_changed()
                    .map(move |fx| FxEnable(FxEnableTarget { fx }).into()),
            )
            .merge(
                csurf_rx
                    .fx_preset_changed()
                    .map(move |fx| FxPreset(FxPresetTarget { fx }).into()),
            )
            .merge(
                csurf_rx
                    .track_volume_touched()
                    .map(move |track| TrackVolume(TrackVolumeTarget { track }).into()),
            )
            .merge(csurf_rx.track_pan_touched().map(move |(track, old, new)| {
                figure_out_touched_pan_component(track, old, new).into()
            }))
            .merge(csurf_rx.track_arm_changed().map(move |track| {
                TrackArm(TrackArmTarget {
                    track,
                    exclusivity: Default::default(),
                })
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
                        TrackSelection(TrackSelectionTarget {
                            track,
                            exclusivity: Default::default(),
                            scroll_arrange_view: false,
                            scroll_mixer: false,
                        })
                        .into()
                    }),
            )
            .merge(csurf_rx.track_mute_touched().map(move |track| {
                TrackMute(TrackMuteTarget {
                    track,
                    exclusivity: Default::default(),
                })
                .into()
            }))
            .merge(csurf_rx.track_automation_mode_changed().map(move |track| {
                let mode = track.automation_mode();
                TrackAutomationMode(TrackAutomationModeTarget {
                    track,
                    exclusivity: Default::default(),
                    mode,
                })
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
                        TrackSolo(TrackSoloTarget {
                            track,
                            behavior: Default::default(),
                            exclusivity: Default::default(),
                        })
                        .into()
                    }),
            )
            .merge(
                csurf_rx
                    .track_route_volume_touched()
                    .map(move |route| TrackRouteVolume(RouteVolumeTarget { route }).into()),
            )
            .merge(
                csurf_rx
                    .track_route_pan_touched()
                    .map(move |route| TrackRoutePan(RoutePanTarget { route }).into()),
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
                        Tempo(TempoTarget {
                            project: reaper.current_project(),
                        })
                        .into()
                    }),
            )
            .merge(
                csurf_rx
                    .master_playrate_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Playrate(PlayrateTarget {
                            project: reaper.current_project(),
                        })
                        .into()
                    }),
            )
            .merge(csurf_rx.global_automation_override_changed().map(move |_| {
                AutomationModeOverride(AutomationModeOverrideTarget {
                    mode_override: Reaper::get().global_automation_override(),
                })
                .into()
            }))
    }
}

impl<'a> Target<'a> for ReaperTarget {
    // An option because we don't have the context available e.g. if some target variants are
    // controlled from real-time processor.
    // TODO-high This can be changed now!!!
    type Context = Option<ControlContext<'a>>;

    fn current_value(&self, context: Option<ControlContext>) -> Option<UnitValue> {
        use ReaperTarget::*;
        let result = match self {
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
            SendOsc { .. } => return None,
            ClipTransport {
                slot_index, action, ..
            } => {
                let context = context.as_ref()?;
                let instance_state = context.instance_state.borrow();
                use TransportAction::*;
                match action {
                    PlayStop | PlayPause | Stop | Pause => {
                        let play_state = instance_state.get_slot(*slot_index).ok()?.play_state();
                        clip_play_state_unit_value(*action, play_state)
                    }
                    Repeat => {
                        let is_looped = instance_state
                            .get_slot(*slot_index)
                            .ok()?
                            .repeat_is_enabled();
                        transport_is_enabled_unit_value(is_looped)
                    }
                    Record => return None,
                }
            }
            ClipSeek { slot_index, .. } => {
                let context = context.as_ref()?;
                let instance_state = context.instance_state.borrow();
                instance_state.get_slot(*slot_index).ok()?.position().ok()?
            }
            ClipVolume { slot_index } => {
                let context = context.as_ref()?;
                let instance_state = context.instance_state.borrow();
                let volume = instance_state.get_slot(*slot_index).ok()?.volume();
                reaper_volume_unit_value(volume)
            }
            SendMidi(t) => return t.current_value(()),
            TrackPeak(t) => return t.current_value(context),
            Action(t) => return t.current_value(()),
            FxParameter(t) => return t.current_value(()),
            TrackVolume(t) => return t.current_value(()),
            TrackPan(t) => return t.current_value(()),
            TrackWidth(t) => return t.current_value(()),
            TrackArm(t) => return t.current_value(()),
            TrackRouteVolume(t) => return t.current_value(()),
            TrackSelection(t) => return t.current_value(()),
            TrackMute(t) => return t.current_value(()),
            TrackShow(t) => return t.current_value(()),
            TrackSolo(t) => return t.current_value(()),
            TrackAutomationMode(t) => return t.current_value(()),
            TrackRoutePan(t) => return t.current_value(()),
            TrackRouteMute(t) => return t.current_value(()),
            Tempo(t) => return t.current_value(()),
            Playrate(t) => return t.current_value(()),
            AutomationModeOverride(t) => return t.current_value(()),
            FxEnable(t) => return t.current_value(()),
            FxOpen(t) => return t.current_value(()),
            FxPreset(t) => return t.current_value(()),
            LoadFxSnapshot(t) => return t.current_value(()),
            SelectedTrack(t) => return t.current_value(()),
            FxNavigate(t) => return t.current_value(()),
            AllTrackFxEnable(t) => return t.current_value(()),
            Transport(t) => return t.current_value(()),
        };
        Some(result)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
impl<'a> Target<'a> for RealTimeReaperTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        use RealTimeReaperTarget::*;
        match self {
            SendMidi(t) => t.current_value(()),
        }
    }

    fn control_type(&self) -> ControlType {
        use RealTimeReaperTarget::*;
        match self {
            SendMidi(t) => t.control_type(),
        }
    }
}

// Panics if called with repeat or record.
fn clip_play_state_unit_value(action: TransportAction, play_state: ClipPlayState) -> UnitValue {
    use TransportAction::*;
    match action {
        PlayStop | PlayPause | Stop | Pause => match action {
            PlayStop | PlayPause => play_state.feedback_value(),
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
    let info = get_seek_info(project, options);
    if pos < info.start_pos {
        UnitValue::MIN
    } else {
        let pos_within_range = pos.get() - info.start_pos.get();
        UnitValue::new_clamped(pos_within_range / info.length())
    }
}

/// Converts a number of possible values to a step size.
pub fn convert_count_to_step_size(n: u32) -> UnitValue {
    // Dividing 1.0 by n would divide the unit interval (0..=1) into n same-sized
    // sub intervals, which means we would have n + 1 possible values. We want to
    // represent just n values, so we need n - 1 same-sized sub intervals.
    if n == 0 || n == 1 {
        return UnitValue::MAX;
    }
    UnitValue::new(1.0 / (n - 1) as f64)
}

pub fn format_value_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(value.get()));
    format_playback_speed(play_rate.playback_speed_factor().get())
}

fn format_playback_speed(speed: f64) -> String {
    format!("{:.4}", speed)
}

pub fn format_step_size_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0x
    // 1.0 => 3.75x
    let speed_increment = value.get() * playback_speed_factor_span();
    format_playback_speed(speed_increment)
}

pub fn format_value_as_bpm_without_unit(value: UnitValue) -> String {
    let tempo = Tempo::from_normalized_value(value.get());
    format_bpm(tempo.bpm().get())
}

pub fn format_step_size_as_bpm_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0 bpm
    // 1.0 => 959.0 bpm
    let bpm_increment = value.get() * bpm_span();
    format_bpm(bpm_increment)
}

// Should be 959.0
pub fn bpm_span() -> f64 {
    Bpm::MAX.get() - Bpm::MIN.get()
}

fn format_bpm(bpm: f64) -> String {
    format!("{:.4}", bpm)
}

pub fn format_value_as_pan(value: UnitValue) -> String {
    Pan::from_normalized_value(value.get()).to_string()
}

pub fn format_value_as_on_off(value: UnitValue) -> &'static str {
    if value.is_zero() { "Off" } else { "On" }
}

pub fn convert_unit_value_to_preset_index(fx: &Fx, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, fx.preset_count().ok()?)
}

pub fn convert_unit_value_to_track_index(project: Project, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, project.track_count())
}

pub fn convert_unit_value_to_fx_index(fx_chain: &FxChain, value: UnitValue) -> Option<u32> {
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

pub fn selected_track_unit_value(project: Project, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, project.track_count())
}

pub fn shown_fx_unit_value(fx_chain: &FxChain, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx_chain.fx_count())
}

pub fn fx_preset_unit_value(fx: &Fx, index: Option<u32>) -> UnitValue {
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

pub fn parse_value_from_pan(text: &str) -> Result<UnitValue, &'static str> {
    let pan: Pan = text.parse()?;
    pan.normalized_value().try_into()
}

pub fn parse_value_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let factor: PlaybackSpeedFactor = decimal.try_into().map_err(|_| "not in play rate range")?;
    PlayRate::from_playback_speed_factor(factor)
        .normalized_value()
        .get()
        .try_into()
}

pub fn parse_step_size_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
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
pub fn playback_speed_factor_span() -> f64 {
    PlaybackSpeedFactor::MAX.get() - PlaybackSpeedFactor::MIN.get()
}

pub fn parse_value_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let bpm: Bpm = decimal.try_into().map_err(|_| "not in BPM range")?;
    Tempo::from_bpm(bpm).normalized_value().try_into()
}

pub fn parse_step_size_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
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
        1007 | 1016 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::PlayStop,
        }),
        // Pause button
        1008 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::PlayPause,
        }),
        // Record button
        1013 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::Record,
        }),
        // Repeat button
        1068 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::Repeat,
        }),
        _ => ReaperTarget::Action(ActionTarget {
            action,
            invocation_type: ActionInvocationType::Trigger,
            project,
        }),
    }
}

pub trait PanExt {
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
        ReaperTarget::TrackWidth(TrackWidthTarget { track })
    } else {
        ReaperTarget::TrackPan(TrackPanTarget { track })
    }
}

pub fn pan_unit_value(pan: Pan) -> UnitValue {
    UnitValue::new(pan.normalized_value())
}

pub fn width_unit_value(width: Width) -> UnitValue {
    UnitValue::new(width.normalized_value())
}

pub fn track_arm_unit_value(is_armed: bool) -> UnitValue {
    convert_bool_to_unit_value(is_armed)
}

pub fn track_selected_unit_value(is_selected: bool) -> UnitValue {
    convert_bool_to_unit_value(is_selected)
}

pub fn mute_unit_value(is_mute: bool) -> UnitValue {
    convert_bool_to_unit_value(is_mute)
}

fn touched_unit_value(is_touched: bool) -> UnitValue {
    convert_bool_to_unit_value(is_touched)
}

pub fn track_solo_unit_value(is_solo: bool) -> UnitValue {
    convert_bool_to_unit_value(is_solo)
}

pub fn track_automation_mode_unit_value(
    desired_automation_mode: AutomationMode,
    actual_automation_mode: AutomationMode,
) -> UnitValue {
    let is_on = desired_automation_mode == actual_automation_mode;
    convert_bool_to_unit_value(is_on)
}

pub fn global_automation_mode_override_unit_value(
    desired_mode_override: Option<GlobalAutomationModeOverride>,
    actual_mode_override: Option<GlobalAutomationModeOverride>,
) -> UnitValue {
    convert_bool_to_unit_value(actual_mode_override == desired_mode_override)
}

pub fn tempo_unit_value(tempo: Tempo) -> UnitValue {
    UnitValue::new(tempo.normalized_value())
}

pub fn playrate_unit_value(playrate: PlayRate) -> UnitValue {
    UnitValue::new(playrate.normalized_value().get())
}

pub fn fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

pub fn all_track_fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

pub fn transport_is_enabled_unit_value(is_enabled: bool) -> UnitValue {
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

pub fn handle_track_exclusivity(
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

#[derive(Clone, Debug, PartialEq)]
pub enum RealTimeReaperTarget {
    SendMidi(MidiSendTarget),
}

pub fn get_control_type_and_character_for_track_exclusivity(
    exclusivity: TrackExclusivity,
) -> (ControlType, TargetCharacter) {
    if exclusivity == TrackExclusivity::NonExclusive {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    } else {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }
}

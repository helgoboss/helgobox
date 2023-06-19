//! This is the API for building clip engine presets.
//!
//! It is designed using the following conventions:
//!
//! - Fields are definitely optional if they have a totally natural default or are an optional
//!   override of an otherwise inherited value. Fields that are added later must also be optional
//!   (for backward compatibility).
//! - Fat enum variants are used to distinguish between multiple alternatives, but not as a general
//!   rule. For UI purposes, it's sometimes desirable to save data even it's not actually in use.
//!   In the processing layer this would be different.
//! - For the same reasons, boolean data types are allowed and not in general substituted by On/Off
//!   enums.
//! - Only a subset of the possible Rust data structuring possibilities are used. The ones that
//!   work well with ReaLearn Script (`lua_serializer.rs`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp;
use std::path::PathBuf;

// TODO-medium Add start time detection
// TODO-medium Add legato

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimePersistenceRoot {
    _matrix: Matrix,
    _even_quantization: EvenQuantization,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Matrix {
    /// All columns from left to right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<Column>>,
    /// All rows from top to bottom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Row>>,
    pub clip_play_settings: MatrixClipPlaySettings,
    pub clip_record_settings: MatrixClipRecordSettings,
    pub common_tempo_range: TempoRange,
}

impl Matrix {
    /// Calculates the effective number of rows, taking the rows vector *and* the slots in each
    /// column into account.
    pub fn necessary_row_count(&self) -> usize {
        cmp::max(
            self.necessary_row_count_of_columns(),
            self.necessary_row_count_of_rows(),
        )
    }

    fn necessary_row_count_of_columns(&self) -> usize {
        let Some(columns) = self.columns.as_ref() else {
            return 0;
        };
        columns
            .iter()
            .map(|col| col.necessary_row_count())
            .max()
            .unwrap_or(0)
    }

    fn necessary_row_count_of_rows(&self) -> usize {
        let Some(rows) = self.rows.as_ref() else {
            return 0;
        };
        rows.len()
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TempoRange {
    min: Bpm,
    max: Bpm,
}

impl TempoRange {
    pub fn new(min: Bpm, max: Bpm) -> PlaytimeApiResult<Self> {
        if min > max {
            return Err("min must be <= max");
        }
        Ok(Self { min, max })
    }

    pub const fn min(&self) -> Bpm {
        self.min
    }

    pub const fn max(&self) -> Bpm {
        self.max
    }
}

impl Default for TempoRange {
    fn default() -> Self {
        Self {
            min: Bpm(60.0),
            max: Bpm(200.0),
        }
    }
}

/// Matrix-global settings related to playing clips.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MatrixClipPlaySettings {
    pub start_timing: ClipPlayStartTiming,
    pub stop_timing: ClipPlayStopTiming,
    pub audio_settings: MatrixClipPlayAudioSettings,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MatrixClipPlayAudioSettings {
    pub resample_mode: VirtualResampleMode,
    pub time_stretch_mode: AudioTimeStretchMode,
    pub cache_behavior: AudioCacheBehavior,
}

/// Matrix-global settings related to recording clips.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MatrixClipRecordSettings {
    pub start_timing: ClipRecordStartTiming,
    pub stop_timing: ClipRecordStopTiming,
    pub duration: RecordLength,
    pub play_start_timing: ClipPlayStartTimingOverrideAfterRecording,
    pub play_stop_timing: ClipPlayStopTimingOverrideAfterRecording,
    pub time_base: ClipRecordTimeBase,
    /// If `true`, starts playing the clip right after recording.
    pub looped: bool,
    /// If `true`, sets the global tempo to the tempo of this clip right after recording.
    // TODO-clip-implement
    pub lead_tempo: bool,
    pub midi_settings: MatrixClipRecordMidiSettings,
    pub audio_settings: MatrixClipRecordAudioSettings,
}

impl MatrixClipRecordSettings {
    pub fn downbeat_detection_enabled(&self, is_midi: bool) -> bool {
        if is_midi {
            self.midi_settings.detect_downbeat
        } else {
            self.audio_settings.detect_downbeat
        }
    }

    pub fn effective_play_start_timing(
        &self,
        initial_play_start_timing: ClipPlayStartTiming,
        current_play_start_timing: ClipPlayStartTiming,
    ) -> Option<ClipPlayStartTiming> {
        use ClipPlayStartTimingOverrideAfterRecording as T;
        match self.play_start_timing {
            T::Inherit => None,
            T::Override(t) => Some(t.value),
            T::DeriveFromRecordTiming => self
                .start_timing
                .derive_play_start_timing(initial_play_start_timing, current_play_start_timing),
        }
    }

    pub fn effective_play_stop_timing(
        &self,
        initial_play_start_timing: ClipPlayStartTiming,
        current_play_start_timing: ClipPlayStartTiming,
    ) -> Option<ClipPlayStopTiming> {
        use ClipPlayStopTimingOverrideAfterRecording as T;
        match self.play_stop_timing {
            T::Inherit => None,
            T::Override(t) => Some(t.value),
            T::DeriveFromRecordTiming => self.stop_timing.derive_play_stop_timing(
                self.start_timing,
                initial_play_start_timing,
                current_play_start_timing,
            ),
        }
    }

    pub fn effective_play_time_base(
        &self,
        initial_play_start_timing: ClipPlayStartTiming,
        audio_tempo: Option<Bpm>,
        time_signature: TimeSignature,
        downbeat: PositiveBeat,
    ) -> ClipTimeBase {
        use ClipRecordTimeBase::*;
        let beat_based = match self.time_base {
            DeriveFromRecordTiming => match self.start_timing {
                ClipRecordStartTiming::LikeClipPlayStartTiming => match initial_play_start_timing {
                    ClipPlayStartTiming::Immediately => false,
                    ClipPlayStartTiming::Quantized(_) => true,
                },
                ClipRecordStartTiming::Immediately => false,
                ClipRecordStartTiming::Quantized(_) => true,
            },
            Time => false,
            Beat => true,
        };
        if beat_based {
            let beat_time_base = BeatTimeBase {
                audio_tempo,
                time_signature,
                downbeat,
            };
            ClipTimeBase::Beat(beat_time_base)
        } else {
            ClipTimeBase::Time
        }
    }
}

impl Default for MatrixClipRecordSettings {
    fn default() -> Self {
        Self {
            start_timing: Default::default(),
            stop_timing: Default::default(),
            duration: Default::default(),
            play_start_timing: Default::default(),
            play_stop_timing: Default::default(),
            time_base: Default::default(),
            looped: true,
            lead_tempo: false,
            midi_settings: Default::default(),
            audio_settings: Default::default(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MatrixClipRecordMidiSettings {
    pub record_mode: MidiClipRecordMode,
    /// If `true`, attempts to detect the actual start of the recorded MIDI material and derives
    /// the downbeat position from that.
    pub detect_downbeat: bool,
    /// Makes the global record button work for MIDI by allowing global input detection.
    // TODO-clip-implement
    pub detect_input: bool,
    /// Applies quantization while recording using the current quantization settings.
    // TODO-clip-implement
    pub auto_quantize: bool,
    /// These are the MIDI settings each recorded clip will get.
    #[serde(default)]
    pub clip_settings: ClipMidiSettings,
}

impl Default for MatrixClipRecordMidiSettings {
    fn default() -> Self {
        Self {
            record_mode: Default::default(),
            detect_downbeat: true,
            detect_input: true,
            auto_quantize: false,
            clip_settings: Default::default(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MatrixClipRecordAudioSettings {
    /// If `true`, attempts to detect the actual start of the recorded audio material and derives
    /// the downbeat position from that.
    // TODO-clip-implement
    pub detect_downbeat: bool,
    /// Makes the global record button work for audio by allowing global input detection.
    // TODO-clip-implement
    pub detect_input: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum RecordLength {
    /// Records open-ended until the user decides to stop.
    OpenEnd,
    /// Records exactly as much material as defined by the given quantization.
    Quantized(EvenQuantization),
}

impl Default for RecordLength {
    fn default() -> Self {
        Self::OpenEnd
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRecordTimeBase {
    /// Derives the time base of the resulting clip from the clip start timing.
    DeriveFromRecordTiming,
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Time`].
    Time,
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Beat`].
    Beat,
}

impl Default for ClipRecordTimeBase {
    fn default() -> Self {
        Self::DeriveFromRecordTiming
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRecordStartTiming {
    /// Uses the inherited clip play start timing (from column or matrix).
    LikeClipPlayStartTiming,
    /// Starts recording immediately.
    Immediately,
    /// Starts recording according to the given quantization.
    Quantized(EvenQuantization),
}

impl Default for ClipRecordStartTiming {
    fn default() -> Self {
        Self::LikeClipPlayStartTiming
    }
}

impl ClipRecordStartTiming {
    pub fn derive_play_start_timing(
        &self,
        initial_play_start_timing: ClipPlayStartTiming,
        current_play_start_timing: ClipPlayStartTiming,
    ) -> Option<ClipPlayStartTiming> {
        use ClipRecordStartTiming::*;
        match self {
            LikeClipPlayStartTiming => {
                if initial_play_start_timing == current_play_start_timing {
                    None
                } else {
                    Some(initial_play_start_timing)
                }
            }
            Immediately => Some(ClipPlayStartTiming::Immediately),
            Quantized(q) => Some(ClipPlayStartTiming::Quantized(*q)),
        }
    }

    pub fn suggests_beat_based_material(&self, play_start_timing: ClipPlayStartTiming) -> bool {
        use ClipRecordStartTiming::*;
        match self {
            LikeClipPlayStartTiming => play_start_timing.suggests_beat_based_material(),
            Immediately => false,
            Quantized(_) => true,
        }
    }
}

impl Default for ClipRecordStopTiming {
    fn default() -> Self {
        Self::LikeClipRecordStartTiming
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRecordStopTiming {
    /// Uses the record start timing.
    LikeClipRecordStartTiming,
    /// Stops recording immediately.
    Immediately,
    /// Stops recording according to the given quantization.
    Quantized(EvenQuantization),
}

impl ClipRecordStopTiming {
    pub fn derive_play_stop_timing(
        &self,
        record_start_timing: ClipRecordStartTiming,
        initial_play_start_timing: ClipPlayStartTiming,
        current_play_start_timing: ClipPlayStartTiming,
    ) -> Option<ClipPlayStopTiming> {
        use ClipRecordStopTiming::*;
        match self {
            LikeClipRecordStartTiming => record_start_timing
                .derive_play_start_timing(initial_play_start_timing, current_play_start_timing)
                .map(|play_start_timing| match play_start_timing {
                    ClipPlayStartTiming::Immediately => ClipPlayStopTiming::Immediately,
                    ClipPlayStartTiming::Quantized(q) => ClipPlayStopTiming::Quantized(q),
                }),
            Immediately => Some(ClipPlayStopTiming::Immediately),
            Quantized(q) => Some(ClipPlayStopTiming::Quantized(*q)),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum MidiClipRecordMode {
    /// Creates an empty clip and records MIDI material in it.
    Normal,
    /// Records more material onto an existing clip, leaving existing material in place.
    ///
    /// Falls back to Normal when used on an empty slot.
    Overdub,
    /// Records more material onto an existing clip, overwriting existing material.
    ///
    /// Falls back to Normal when used on an empty slot.
    // TODO-clip-implement
    Replace,
}

impl Default for MidiClipRecordMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipPlayStartTiming {
    /// Starts playing immediately.
    Immediately,
    /// Starts playing according to the given quantization.
    Quantized(EvenQuantization),
}

impl Default for ClipPlayStartTiming {
    fn default() -> Self {
        Self::Quantized(Default::default())
    }
}

impl ClipPlayStartTiming {
    pub fn suggests_beat_based_material(&self) -> bool {
        use ClipPlayStartTiming::*;
        match self {
            Immediately => false,
            Quantized(_) => true,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipPlayStopTiming {
    /// Uses the play start timing.
    LikeClipStartTiming,
    /// Stops playing immediately.
    Immediately,
    /// Stops playing according to the given quantization.
    Quantized(EvenQuantization),
    /// Keeps playing until the end of the clip.
    UntilEndOfClip,
}

impl Default for ClipPlayStopTiming {
    fn default() -> Self {
        Self::LikeClipStartTiming
    }
}

// TODO-low This was generic before (9265c028). Now it's duplicated instead for easier Dart
//  conversion. If we need to use generics one day more extensively, there's probably a way to make
//  it work.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipPlayStartTimingOverrideAfterRecording {
    /// Doesn't apply any override.
    #[default]
    Inherit,
    /// Overrides the setting with the given value.
    Override(ClipPlayStartTimingOverride),
    /// Derives the setting from the record timing.
    ///
    /// If the record timing is set to the global clip timing, that means it will not apply any
    /// override. If it's set to something specific, it will apply the appropriate override.
    DeriveFromRecordTiming,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipPlayStopTimingOverrideAfterRecording {
    /// Doesn't apply any override.
    #[default]
    Inherit,
    /// Overrides the setting with the given value.
    Override(ClipPlayStopTimingOverride),
    /// Derives the setting from the record timing.
    ///
    /// If the record timing is set to the global clip timing, that means it will not apply any
    /// override. If it's set to something specific, it will apply the appropriate override.
    DeriveFromRecordTiming,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPlayStartTimingOverride {
    pub value: ClipPlayStartTiming,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPlayStopTimingOverride {
    pub value: ClipPlayStopTiming,
}

/// An even quantization.
///
/// Even in the sense of that's it's not swing or dotted.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvenQuantization {
    numerator: u32,
    denominator: u32,
}

impl Default for EvenQuantization {
    fn default() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }
}

impl EvenQuantization {
    pub const ONE_BAR: Self = EvenQuantization {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: u32, denominator: u32) -> PlaytimeApiResult<Self> {
        if numerator == 0 {
            return Err("numerator must be > 0");
        }
        if denominator == 0 {
            return Err("denominator must be > 0");
        }
        if numerator > 1 && denominator > 1 {
            return Err("if numerator > 1, denominator must be 1");
        }
        let q = Self {
            numerator,
            denominator,
        };
        Ok(q)
    }

    /// The number of bars.
    ///
    /// Must not be zero.
    pub fn numerator(&self) -> u32 {
        self.numerator
    }

    /// Defines the fraction of a bar.
    ///
    /// Must not be zero.
    ///
    /// If the numerator is > 1, this must be 1.
    pub fn denominator(&self) -> u32 {
        self.denominator
    }
}

#[derive(
    Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, JsonSchema, derive_more::Display,
)]
pub struct ColumnId(String);

impl ColumnId {
    pub fn random() -> Self {
        Default::default()
    }
}

impl Default for ColumnId {
    fn default() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[derive(
    Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, JsonSchema, derive_more::Display,
)]
pub struct RowId(String);

impl RowId {
    pub fn random() -> Self {
        Default::default()
    }
}

impl Default for RowId {
    fn default() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[derive(
    Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, JsonSchema, derive_more::Display,
)]
pub struct SlotId(String);

impl SlotId {
    pub fn random() -> Self {
        Default::default()
    }
}

impl Default for SlotId {
    fn default() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[derive(
    Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, JsonSchema, derive_more::Display,
)]
pub struct ClipId(String);

impl ClipId {
    pub fn random() -> Self {
        Default::default()
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Column {
    #[serde(default)]
    pub id: ColumnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub clip_play_settings: ColumnClipPlaySettings,
    pub clip_record_settings: ColumnClipRecordSettings,
    /// Slots in this column.
    ///
    /// Only filled slots need to be mentioned here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<Slot>>,
}

impl Column {
    /// Calculates the necessary number of rows in this column in order to keep all the defined
    /// slots.
    pub fn necessary_row_count(&self) -> usize {
        let Some(slots) = self.slots.as_ref() else {
            return 0;
        };
        slots
            .iter()
            .map(|s| s.row)
            .max()
            .map(|max| max + 1)
            .unwrap_or(0)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ColumnClipPlaySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ColumnPlayMode>,
    /// REAPER track used for playing back clips in this column.
    ///
    /// Usually, each column should have a play track. But events might occur that leave a column
    /// in a "track-less" state, e.g. the deletion of a track. This column will be unusable until
    /// the user sets a play track again. We still want to be able to save the matrix in such a
    /// state, otherwise it could be really annoying. So we allow `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
    /// Start timing override.
    ///
    /// `None` means it uses the matrix-global start timing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_timing: Option<ClipPlayStartTiming>,
    /// Stop timing override.
    ///
    /// `None` means it uses the matrix-global stop timing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timing: Option<ClipPlayStopTiming>,
    pub audio_settings: ColumnClipPlayAudioSettings,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ColumnPlayMode {
    /// - Only one clip in the column can play at a certain point in time.
    /// - Clips are started/stopped if the corresponding scene is started/stopped.
    ExclusiveFollowingScene,
    /// - Only one clip in the column can play at a certain point in time.
    /// - Clips are not started/stopped if the corresponding scene is started/stopped.
    ExclusiveIgnoringScene,
    /// - Multiple clips can play simultaneously.
    /// - Clips are started/stopped if the corresponding scene is started/stopped
    ///   (in an exclusive manner).
    NonExclusiveFollowingScene,
    /// - Multiple clips can play simultaneously.
    /// - Clips are not started/stopped if the corresponding scene is started/stopped.
    Free,
}

impl Default for ColumnPlayMode {
    fn default() -> Self {
        Self::ExclusiveFollowingScene
    }
}

impl ColumnPlayMode {
    pub fn is_exclusive(&self) -> bool {
        use ColumnPlayMode::*;
        matches!(self, ExclusiveFollowingScene | ExclusiveIgnoringScene)
    }

    pub fn follows_scene(&self) -> bool {
        matches!(
            self,
            ColumnPlayMode::ExclusiveFollowingScene | ColumnPlayMode::NonExclusiveFollowingScene
        )
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ColumnClipRecordSettings {
    pub origin: RecordOrigin,
    /// By default, Playtime records from the play track but this settings allows to override that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ColumnClipPlayAudioSettings {
    /// Overrides the matrix-global resample mode for clips in this column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resample_mode: Option<VirtualResampleMode>,
    /// Overrides the matrix-global audio time stretch mode for clips in this column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stretch_mode: Option<AudioTimeStretchMode>,
    /// Overrides the matrix-global audio cache behavior for clips in this column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_behavior: Option<AudioCacheBehavior>,
}

/// A row represents a complete row in a matrix.
///
/// A scene is a very related concept and sometimes used interchangeably with row because there's
/// a one-to-one relationship between a row and a scene.
///
/// The difference between row and scene is of conceptual nature: A scene represents a part of a
/// song that's played exclusively whereas a row is just a row in the clip matrix. This distinction
/// results in some practical differences:
///
/// - A column can be configured to not follow scenes. The clips in that column are of
///   course still structured in rows, but they are not part of the scenes anymore.
/// - In practice, this means that whenever you launch the scene, the clips in that independent
///   column are not launched. Or when you clear the scene, the slots in that column are not
///   cleared.
/// - Whenever you read "Scene", it will only affect the columns that are configured to follow
///   scenes. Whenever you read "Row", it will affect the complete matrix row, no matter the
///   column type.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Row {
    #[serde(default)]
    pub id: RowId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An optional tempo associated with this row.
    // TODO-clip-implement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<Bpm>,
    /// An optional time signature associated with this row.
    // TODO-clip-implement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<TimeSignature>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum AudioTimeStretchMode {
    /// Doesn't just stretch/squeeze the material but also changes the pitch.
    ///
    /// Comparatively fast.
    VariSpeed,
    /// Applies a real time-stretch algorithm to the material which keeps the pitch.
    ///
    /// Comparatively slow.
    KeepingPitch(TimeStretchMode),
}

impl Default for AudioTimeStretchMode {
    fn default() -> Self {
        Self::KeepingPitch(Default::default())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum VirtualResampleMode {
    /// Uses the resample mode set as default for this REAPER project.
    ProjectDefault,
    /// Uses a specific resample mode.
    ReaperMode(ReaperResampleMode),
}

impl Default for VirtualResampleMode {
    fn default() -> Self {
        Self::ProjectDefault
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReaperResampleMode {
    pub mode: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TimeStretchMode {
    pub mode: VirtualTimeStretchMode,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum VirtualTimeStretchMode {
    /// Uses the pitch shift mode set as default for this REAPER project.
    ProjectDefault,
    /// Uses a specific REAPER pitch shift mode.
    ReaperMode(ReaperPitchShiftMode),
}

impl Default for VirtualTimeStretchMode {
    fn default() -> Self {
        Self::ProjectDefault
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReaperPitchShiftMode {
    pub mode: u32,
    pub sub_mode: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum RecordOrigin {
    /// Records using the hardware input set for the track (MIDI or stereo).
    TrackInput,
    /// Captures audio from the output of the track.
    TrackAudioOutput,
    /// Records audio flowing into the FX input.
    FxAudioInput(ChannelRange),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum SourceOrigin {
    /// Normal source.
    Normal,
    /// Frozen source.
    Frozen,
}

impl Default for SourceOrigin {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelRange {
    pub first_channel_index: u32,
    pub channel_count: u32,
}

impl Default for RecordOrigin {
    fn default() -> Self {
        Self::TrackInput
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Slot {
    #[serde(default)]
    pub id: SlotId,
    /// Slot index within the column (= row), starting at zero.
    pub row: usize,
    /// Clip which currently lives in this slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "clip")]
    pub clip_old: Option<Clip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clips: Option<Vec<Clip>>,
}

impl Slot {
    pub fn into_clips(self) -> Vec<Clip> {
        // Backward compatibility: In the past, only a single clip was possible.
        if let Some(clip) = self.clip_old {
            return vec![clip];
        }
        self.clips.unwrap_or_default()
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Clip {
    #[serde(default)]
    pub id: ClipId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Source of the audio/MIDI material of this clip.
    pub source: Source,
    /// Source with effects "rendered in", usually audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_source: Option<Source>,
    /// Which of the sources is the active one.
    #[serde(default)]
    pub active_source: SourceOrigin,
    /// Time base of the material provided by that source.
    pub time_base: ClipTimeBase,
    /// Start timing override.
    ///
    /// `None` means it uses the column start timing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_timing: Option<ClipPlayStartTiming>,
    /// Stop timing override.
    ///
    /// `None` means it uses the column stop timing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timing: Option<ClipPlayStopTiming>,
    /// Whether the clip should be played repeatedly or as a single shot.
    pub looped: bool,
    /// Relative volume adjustment of clip.
    pub volume: Db,
    /// Color of the clip.
    // TODO-clip-implement
    pub color: ClipColor,
    /// Defines which portion of the original source should be played.
    pub section: Section,
    pub audio_settings: ClipAudioSettings,
    pub midi_settings: ClipMidiSettings,
    // /// Defines the total amount of time this clip should consume and where within that range the
    // /// portion of the original source is located.
    // ///
    // /// This allows one to insert silence at the beginning and the end
    // /// as well as changing the source section without affecting beat
    // /// alignment.
    // ///
    // /// `None` means the canvas will have the same size as the source
    // /// section.
    // canvas: Option<Canvas>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipAudioSettings {
    /// Defines whether to apply automatic fades in order to fix potentially non-optimized source
    /// material.
    ///
    /// ## `false`
    ///
    /// Doesn't apply automatic fades for fixing non-optimized source material.
    ///
    /// This only prevents source-fix fades. Fades that are not about fixing the source will still
    /// be applied if necessary in order to ensure a smooth playback, such as:
    ///
    /// - Section fades (start fade-in, end fade-out)
    /// - Interaction fades (resume-after-pause fade-in, immediate stop fade-out)
    ///
    /// Fades don't overlap. Here's the order of priority (for fade-in and fade-out separately):
    ///
    /// - Interaction fades
    /// - Section fades
    /// - Source-fix fades
    ///
    /// ## `true`
    ///
    /// Applies automatic fades to fix non-optimized source material, if necessary.
    pub apply_source_fades: bool,
    /// Defines how to adjust audio material.
    ///
    /// This is usually used with the beat time base to match the tempo of the clip to the global
    /// tempo.
    ///
    /// `None` means it uses the column time stretch mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stretch_mode: Option<AudioTimeStretchMode>,
    /// Overrides the column resample mode for clips in this column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resample_mode: Option<VirtualResampleMode>,
    /// Whether to cache audio in memory.
    ///
    /// `None` means it uses the column cache behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_behavior: Option<AudioCacheBehavior>,
}

impl Default for ClipAudioSettings {
    fn default() -> Self {
        Self {
            cache_behavior: Default::default(),
            apply_source_fades: true,
            time_stretch_mode: None,
            resample_mode: None,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ClipMidiSettings {
    /// For fixing the source itself.
    pub source_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the section.
    pub section_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the complete loop.
    pub loop_reset_settings: MidiResetMessageRange,
    /// For fine-tuning instant start/stop of a MIDI clip when in the middle of a source or section.
    pub interaction_reset_settings: MidiResetMessageRange,
}

pub fn preferred_clip_midi_settings() -> ClipMidiSettings {
    let no_reset = MidiResetMessages::default();
    let light_reset = MidiResetMessages {
        on_notes_off: true,
        ..no_reset
    };
    ClipMidiSettings {
        source_reset_settings: MidiResetMessageRange {
            left: no_reset,
            right: no_reset,
        },
        section_reset_settings: MidiResetMessageRange {
            left: no_reset,
            right: no_reset,
        },
        loop_reset_settings: MidiResetMessageRange {
            left: no_reset,
            right: light_reset,
        },
        interaction_reset_settings: MidiResetMessageRange {
            left: no_reset,
            right: light_reset,
        },
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MidiResetMessageRange {
    /// Which MIDI reset messages to apply at the beginning.
    pub left: MidiResetMessages,
    /// Which MIDI reset messages to apply at the end.
    pub right: MidiResetMessages,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MidiResetMessages {
    /// Only supported at "right" position at the moment.
    #[serde(default)]
    pub on_notes_off: bool,
    pub all_notes_off: bool,
    pub all_sound_off: bool,
    pub reset_all_controllers: bool,
    pub damper_pedal_off: bool,
}

impl MidiResetMessages {
    pub fn at_least_one_enabled(&self) -> bool {
        self.on_notes_off
            || self.all_notes_off
            || self.all_sound_off
            || self.reset_all_controllers
            || self.damper_pedal_off
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    /// Position in the source from which to start.
    ///
    /// If this is greater than zero, a fade-in will be used to avoid clicks.
    pub start_pos: PositiveSecond,
    /// Length of the material to be played, starting from `start_pos`.
    ///
    /// - `None` means until original source end.
    /// - May exceed the end of the source.
    /// - If this makes the section end be located before the original source end, a fade-out will
    ///   be used to avoid clicks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<PositiveSecond>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum AudioCacheBehavior {
    /// Loads directly from the disk.
    ///
    /// Might still pre-buffer some blocks but definitely won't put the complete audio data into
    /// memory.
    DirectFromDisk,
    /// Loads the complete audio data into memory.
    CacheInMemory,
}

impl Default for AudioCacheBehavior {
    fn default() -> Self {
        Self::DirectFromDisk
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipColor {
    /// Inherits the color of the column's play track.
    PlayTrackColor,
    /// Assigns a very specific custom color.
    CustomColor(CustomClipColor),
    /// Uses a certain color from a palette.
    PaletteColor(PaletteClipColor),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomClipColor {
    pub value: RgbColor,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PaletteClipColor {
    pub index: u32,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum Source {
    /// Takes content from a media file on the file system (audio or MIDI).
    File(FileSource),
    /// Embedded MIDI data.
    MidiChunk(MidiChunkSource),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileSource {
    /// Path to the media file.
    ///
    /// If it's a relative path, it will be interpreted as relative to the REAPER project directory.
    pub path: PathBuf,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MidiChunkSource {
    /// MIDI data in the same format that REAPER uses for in-project MIDI.
    pub chunk: String,
}

/// Decides if the clip will be adjusted to the current tempo.
#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipTimeBase {
    /// Material which doesn't need to be adjusted to the current tempo.
    #[default]
    Time,
    /// Material which needs to be adjusted to the current tempo.
    Beat(BeatTimeBase),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BeatTimeBase {
    /// The clip's native tempo.
    ///
    /// Must be set for audio. Is ignored for MIDI.
    ///
    /// This information is used by the clip engine to determine how much to speed up or
    /// slow down the material depending on the current project tempo.
    // TODO-high-clip-engine Feels strange here, maybe move to audio settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tempo: Option<Bpm>,
    /// The time signature of this clip.
    ///
    /// If provided, This information is used for certain aspects of the user interface.
    pub time_signature: TimeSignature,
    /// Defines which position (in beats) is the downbeat.
    pub downbeat: PositiveBeat,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrackId(String);

impl TrackId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn get(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Bpm(f64);

impl Bpm {
    pub fn new(value: f64) -> PlaytimeApiResult<Self> {
        if value <= 0.0 {
            return Err("BPM value must be > 0.0");
        }
        Ok(Self(value))
    }

    pub const fn get(&self) -> f64 {
        self.0
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PositiveSecond(f64);

impl PositiveSecond {
    pub fn new(value: f64) -> PlaytimeApiResult<Self> {
        if value < 0.0 {
            return Err("second value must be positive");
        }
        Ok(Self(value))
    }

    pub const fn get(&self) -> f64 {
        self.0
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PositiveBeat(f64);

impl PositiveBeat {
    pub fn new(value: f64) -> PlaytimeApiResult<Self> {
        if value < 0.0 {
            return Err("beat value must be positive");
        }
        Ok(Self(value))
    }

    pub const fn get(&self) -> f64 {
        self.0
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Db(f64);

impl Db {
    pub const ZERO: Db = Db(0.0);

    pub fn new(value: f64) -> PlaytimeApiResult<Self> {
        if value.is_nan() {
            return Err("dB value must not be NaN");
        }
        Ok(Self(value))
    }

    pub const fn get(&self) -> f64 {
        self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RgbColor(pub u8, pub u8, pub u8);

type PlaytimeApiResult<T> = Result<T, &'static str>;

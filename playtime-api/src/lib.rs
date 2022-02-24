//! This is the API for building clip engine presets.
//!
//! It is designed using the following conventions:
//!
//! - Fields are optional only if they have a totally natural default or are an optional override
//!   of an otherwise inherited value.
//! - Fat enum variants are used to distinguish between multiple alternatives, but not as a general
//!   rule. For UI purposes, it's sometimes desirable to save data even it's not actually in use.
//!   In the processing layer this would be different.
//! - For the same reasons, boolean data types are allowed and not in general substituted by On/Off
//!   enums.
//! - Only a subset of the possible Rust data structuring possibilities are used. The ones that
//!   work well with ReaLearn Script (`lua_serializer.rs`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    /// All columns from left to right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<Column>>,
    /// All rows from top to bottom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Row>>,
    pub clip_play_settings: MatrixClipPlaySettings,
    pub clip_record_settings: MatrixClipRecordSettings,
    // TODO-clip-implement
    pub common_tempo_range: TempoRange,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TempoRange {
    pub min: Bpm,
    pub max: Bpm,
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
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixClipPlaySettings {
    pub start_timing: ClipPlayStartTiming,
    pub stop_timing: ClipPlayStopTiming,
    pub audio_settings: MatrixClipPlayAudioSettings,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixClipPlayAudioSettings {
    // TODO-clip-implement
    pub time_stretch_mode: AudioTimeStretchMode,
}

/// Matrix-global settings related to recording clips.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixClipRecordSettings {
    // TODO-clip-implement
    pub start_timing: ClipRecordStartTiming,
    // TODO-clip-implement
    pub stop_timing: ClipRecordStopTiming,
    // TODO-clip-implement
    pub duration: RecordLength,
    // TODO-clip-implement
    pub play_start_timing: ClipSettingOverrideAfterRecording<ClipPlayStartTiming>,
    // TODO-clip-implement
    pub play_stop_timing: ClipSettingOverrideAfterRecording<ClipPlayStopTiming>,
    // TODO-clip-implement
    pub time_base: ClipRecordTimeBase,
    /// If `true`, starts playing the clip right after recording.
    // TODO-clip-implement
    pub play_after: bool,
    /// If `true`, sets the global tempo to the tempo of this clip right after recording.
    // TODO-clip-implement
    pub lead_tempo: bool,
    pub midi_settings: MatrixClipRecordMidiSettings,
    pub audio_settings: MatrixClipRecordAudioSettings,
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
            play_after: true,
            lead_tempo: false,
            midi_settings: Default::default(),
            audio_settings: Default::default(),
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixClipRecordMidiSettings {
    // TODO-clip-implement
    pub record_mode: MidiClipRecordMode,
    /// If `true`, attempts to detect the actual start of the recorded MIDI material and derives
    /// the downbeat position from that.
    // TODO-clip-implement
    pub detect_downbeat: bool,
    /// Makes the global record button work for MIDI by allowing global input detection.
    // TODO-clip-implement
    pub detect_input: bool,
    /// Applies quantization while recording using the current quantization settings.
    // TODO-clip-implement
    pub auto_quantize: bool,
}

impl Default for MatrixClipRecordMidiSettings {
    fn default() -> Self {
        Self {
            record_mode: Default::default(),
            detect_downbeat: true,
            detect_input: true,
            auto_quantize: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixClipRecordAudioSettings {
    /// If `true`, attempts to detect the actual start of the recorded audio material and derives
    /// the downbeat position from that.
    // TODO-clip-implement
    pub detect_downbeat: bool,
    /// Makes the global record button work for audio by allowing global input detection.
    // TODO-clip-implement
    pub detect_input: bool,
}

impl Default for MatrixClipRecordAudioSettings {
    fn default() -> Self {
        Self {
            detect_downbeat: false,
            detect_input: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum ClipRecordTimeBase {
    /// Derives the time base of the resulting clip from the clip start/stop timing.
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRecordStartTiming {
    /// Uses the global clip play start timing.
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRecordStopTiming {
    /// Uses the record start timing.
    LikeClipRecordStartTiming,
    /// Stops recording immediately.
    Immediately,
    /// Stops recording according to the given quantization.
    Quantized(EvenQuantization),
}

impl Default for ClipRecordStopTiming {
    fn default() -> Self {
        Self::LikeClipRecordStartTiming
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum MidiClipRecordMode {
    /// Creates an empty clip and records MIDI material in it.
    Normal,
    /// Records more material onto an existing clip, leaving existing material in place.
    Overdub,
    /// Records more material onto an existing clip, overwriting existing material.
    Replace,
}

impl Default for MidiClipRecordMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipSettingOverrideAfterRecording<T> {
    /// Doesn't apply any override.
    Inherit,
    /// Overrides the setting with the given value.
    Override(Override<T>),
    /// Overrides the setting with a value derived from the record timing.
    OverrideFromRecordTiming,
}

impl<T> Default for ClipSettingOverrideAfterRecording<T> {
    fn default() -> Self {
        Self::Inherit
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Override<T> {
    pub value: T,
}

/// An even quantization.
///
/// Even in the sense of that's it's not swing or dotted.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub clip_play_settings: ColumnClipPlaySettings,
    pub clip_record_settings: ColumnClipRecordSettings,
    /// Slots in this column.
    ///
    /// Only filled slots need to be mentioned here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<Slot>>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColumnClipPlaySettings {
    /// REAPER track used for playing back clips in this column.
    ///
    /// Usually, each column should have a play track. But events might occur that leave a column
    /// in a "track-less" state, e.g. the deletion of a track. This column will be unusable until
    /// the user sets a play track again. We still want to be able to save the matrix in such a
    /// state, otherwise it could be really annoying. So we allow `None`.
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColumnClipRecordSettings {
    /// By default, Playtime records from the play track but this settings allows to override that.
    // TODO-clip-implement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
    // TODO-clip-implement
    pub origin: TrackRecordOrigin,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColumnClipPlayAudioSettings {
    /// Overrides the matrix-global audio time stretch mode for clips in this column.
    // TODO-clip-implement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stretch_mode: Option<AudioTimeStretchMode>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub scene: Scene,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    // TODO-clip-implement
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum AudioTimeStretchMode {
    /// Doesn't just stretch/squeeze the material but also changes the pitch.
    ///
    /// Comparatively fast.
    VariSpeed(VariSpeedMode),
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariSpeedMode {
    pub mode: VirtualVariSpeedMode,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum VirtualVariSpeedMode {
    /// Uses the resample mode set as default for this REAPER project.
    ProjectDefault,
    /// Uses a specific resample mode.
    ReaperMode(ReaperResampleMode),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReaperResampleMode {
    pub mode: u32,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeStretchMode {
    pub mode: VirtualTimeStretchMode,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReaperPitchShiftMode {
    pub mode: u32,
    pub sub_mode: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum TrackRecordOrigin {
    /// Records using the hardware input set for the track (MIDI or stereo).
    TrackInput,
    /// Captures audio from the output of the track.
    TrackAudioOutput,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    /// Slot index within the column (= row), starting at zero.
    pub row: usize,
    /// Clip which currently lives in this slot.
    pub clip: Option<Clip>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    /// Source of the audio/MIDI material of this clip.
    pub source: Source,
    /// Time base of the material provided by that source.
    // TODO-clip-implement
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
    // TODO-clip-implement
    pub volume: Db,
    /// Color of the clip.
    // TODO-clip-implement
    pub color: ClipColor,
    /// Defines which portion of the original source should be played.
    // TODO-clip-implement
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

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipAudioSettings {
    /// Whether to cache audio in memory.
    // TODO-clip-implement
    pub cache_behavior: AudioCacheBehavior,
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
    // TODO-clip-implement
    pub apply_source_fades: bool,
    /// Defines how to adjust audio material.
    ///
    /// This is usually used with the beat time base to match the tempo of the clip to the global
    /// tempo.
    ///
    /// `None` means it uses the column time stretch mode.
    // TODO-clip-implement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_stretch_mode: Option<AudioTimeStretchMode>,
}

// struct Canvas {
//     /// Should be long enough to let the source section fit in.
//     length: DurationInSeconds,
//     /// Position of the source section.
//     section_pos: PositionInSeconds,
// }

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipMidiSettings {
    /// For fixing the source itself.
    // TODO-clip-implement
    pub source_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the section.
    // TODO-clip-implement
    pub section_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the complete loop.
    // TODO-clip-implement
    pub loop_reset_settings: MidiResetMessageRange,
    /// For fine-tuning instant start/stop of a MIDI clip when in the middle of a source or section.
    // TODO-clip-implement
    pub interaction_reset_settings: MidiResetMessageRange,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MidiResetMessageRange {
    /// Which MIDI reset messages to apply at the beginning.
    pub left: MidiResetMessages,
    /// Which MIDI reset messages to apply at the end.
    pub right: MidiResetMessages,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MidiResetMessages {
    pub all_notes_off: bool,
    pub all_sounds_off: bool,
    pub sustain_off: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Position in the source from which to start.
    ///
    /// If this is greater than zero, a fade-in will be used to avoid clicks.
    pub start_pos: Seconds,
    /// Length of the material to be played, starting from `start_pos`.
    ///
    /// - `None` means until original source end.
    /// - May exceed the end of the source.
    /// - If this makes the section end be located before the original source end, a fade-out will
    ///   be used to avoid clicks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<Seconds>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum AudioCacheBehavior {
    /// Loads directly from the disk.
    ///
    /// Might still pre-buffer some blocks but definitely won't put the complete audio data into
    /// memory.
    DirectFromDisk,
    /// Loads the complete audio data into memory.
    CacheInMemory,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipColor {
    /// Inherits the color of the column's play track.
    PlayTrackColor,
    /// Assigns a very specific custom color.
    CustomColor(CustomClipColor),
    /// Uses a certain color from a palette.
    PaletteColor(PaletteClipColor),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomClipColor {
    pub value: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaletteClipColor {
    pub index: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum Source {
    /// Takes content from a media file on the file system (audio or MIDI).
    File(FileSource),
    /// Embedded MIDI data.
    MidiChunk(MidiChunkSource),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    /// Path to the media file.
    ///
    /// If it's a relative path, it will be interpreted as relative to the REAPER project directory.
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MidiChunkSource {
    /// MIDI data in the same format that REAPER uses for in-project MIDI.
    pub chunk: String,
}

/// Decides if the clip will be adjusted to the current tempo.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipTimeBase {
    /// Material which doesn't need to be adjusted to the current tempo.
    Time,
    /// Material which needs to be adjusted to the current tempo.
    Beat(BeatTimeBase),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeatTimeBase {
    /// The clip's native tempo.
    ///
    /// Must be set for audio. Is ignored for MIDI.
    ///
    /// This information is used by the clip engine to determine how much to speed up or
    /// slow down the material depending on the current project tempo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_bpm: Option<Bpm>,
    /// The time signature of this clip.
    ///
    /// If provided, This information is used for certain aspects of the user interface.
    pub time_signature: TimeSignature,
    /// Defines which position (in beats) is the downbeat.
    pub downbeat: f64,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrackId(pub String);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Bpm(pub f64);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Seconds(pub f64);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Db(pub f64);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RgbColor(pub u8, pub u8, pub u8);

type PlaytimeApiResult<T> = Result<T, &'static str>;

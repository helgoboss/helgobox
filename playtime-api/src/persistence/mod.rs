//! This is the API for building clip engine presets.
//!
//! It is designed using the following conventions:
//!
//! - Fields are definitely optional if they have a totally natural default or are an optional
//!   override of an otherwise inherited value.
//! - For fields that are added later, we must take care of backward compatibility. There are
//!   two approaches to achieve that. One is `#[serde(default]`. This is the easiest approach and
//!   very tempting to choose if the type has a natural default. However: This means this field
//!   will *always* be present when serializing the state. And that means 1) there's no way of
//!   leaving the value of that field open until the user actually sets it (often okay).
//!   And 2) "Export as Lua without default values" will always contain those fields, even if they
//!   are defaults (see `ConversionStyle`). If you don't want this, use the second approach:
//!   An `Option` annotated with `#[serde(skip_serializing_if = "Option::is_none")]`.
//! - Fat enum variants are used to distinguish between multiple alternatives, but not as a general
//!   rule. For UI purposes, it's sometimes desirable to save data even it's not actually in use.
//!   In the processing layer this would be different.
//! - For the same reasons, boolean data types are allowed and not in general substituted by On/Off
//!   enums.
//! - Only a subset of the possible Rust data structuring possibilities are used. The ones that
//!   work well with ReaLearn Script (`lua_serializer.rs`) **and** our Rust-to-Dart converter.
//!   For the latter, we need to avoid things like `#[serde(flatten)]` or type parameters!

use std::cmp;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_ENGINE;
use base64::Engine;
use camino::Utf8PathBuf;
use chrono::NaiveDateTime;
use derive_more::Display;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_common_types::{Bpm, Db, DurationInBeats, DurationInSeconds, Semitones};
use serde::{Deserialize, Deserializer, Serialize};
use strum::EnumIter;

use crate::runtime::CellAddress;

mod serialization;

/// Global settings that apply to all Playtime instances.
///
/// This is different from the app settings (which are also global) in that the app settings are only about GUI
/// aspects (less important) and managed by the app. Whereas the engine settings configure settings that
/// apply even without GUI, and they are managed by the engine.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct PlaytimeSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo_latency: Option<TempoLatency>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TempoLatency {
    #[default]
    Auto,
    Manual(ManualTempoLatency),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ManualTempoLatency {
    pub millis: u32,
}

impl Default for ManualTempoLatency {
    fn default() -> Self {
        Self { millis: 200 }
    }
}

// TODO-high-playtime-after-release Add start time detection
// TODO-high-playtime-after-release Add legato

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Matrix {
    /// All columns from left to right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<Column>>,
    /// All rows from top to bottom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Row>>,
    #[serde(default)]
    pub clip_play_settings: MatrixClipPlaySettings,
    #[serde(default)]
    pub clip_record_settings: MatrixClipRecordSettings,
    #[serde(default)]
    pub transport_sync_mode: TransportSyncMode,
    #[serde(default)]
    pub common_tempo_range: TempoRange,
    /// Whether to automatically activate a slot when it's triggered.
    ///
    /// This might seem like a purely visual setting at first, but it's not! Activating
    /// a slot happens on "server-side", so certain actions can be carried out on the
    /// active slot and the active slot might even be persisted in future to recall a
    /// specific setup.
    #[serde(default)]
    pub activate_slot_on_trigger: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_volume: Option<Db>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_roll_bars: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_palette: Option<ColorPalette>,
    #[serde(default)]
    pub content_quantization_settings: ContentQuantizationSettings,
    #[serde(default)]
    pub sequencer: MatrixSequencer,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub unknown_props: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[repr(usize)]
pub enum TransportSyncMode {
    /// Partial sync.
    ///
    /// - REAPER start => Playtime start
    /// - REAPER stop => Playtime stop
    /// - Playtime start => -
    /// - Playtime stop => REAPER stop (feels weird otherwise)
    #[default]
    Partial,
    /// Full sync.
    ///
    /// - REAPER start => Playtime start
    /// - REAPER stop => Playtime stop
    /// - Playtime start => REAPER start
    /// - Playtime stop => REAPER stop (feels weird otherwise)
    Full,
}

// This default is only used to create the initial undo point.
impl Default for Matrix {
    fn default() -> Self {
        Self {
            columns: None,
            rows: None,
            clip_play_settings: Default::default(),
            clip_record_settings: Default::default(),
            transport_sync_mode: Default::default(),
            common_tempo_range: Default::default(),
            activate_slot_on_trigger: ACTIVATE_SLOT_ON_TRIGGER_DEFAULT,
            click_volume: None,
            pre_roll_bars: None,
            color_palette: None,
            content_quantization_settings: Default::default(),
            sequencer: Default::default(),
            unknown_props: None,
        }
    }
}

pub const ACTIVATE_SLOT_ON_TRIGGER_DEFAULT: bool = true;
pub const PRE_ROLL_BARS_DEFAULT: u32 = 2;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixSequencer {
    #[serde(default)]
    pub sequences: Vec<MatrixSequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_sequence: Option<MatrixSequenceId>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequence {
    pub id: MatrixSequenceId,
    pub info: MatrixSequenceInfo,
    pub data: MatrixSequenceData,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequenceInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixSequenceData {
    pub ppq: u64,
    pub count_in: u64,
    pub events: Vec<MatrixSequenceEvent>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct MatrixSequenceEvent {
    pub pulse_diff: u32,
    pub message: MatrixSequenceMessage,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum MatrixSequenceMessage {
    PanicMatrix,
    StopMatrix,
    PanicColumn(MatrixSequenceColumnMessage),
    StopColumn(MatrixSequenceColumnMessage),
    StartScene(MatrixSequenceRowMessage),
    PanicSlot(MatrixSequenceSlotMessage),
    StartSlot(MatrixSequenceStartSlotMessage),
    StopSlot(MatrixSequenceSlotMessage),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequenceColumnMessage {
    pub index: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequenceRowMessage {
    pub index: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequenceSlotMessage {
    pub column_index: u32,
    pub row_index: u32,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSequenceStartSlotMessage {
    pub column_index: u32,
    pub row_index: u32,
    pub velocity: f64,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ContentQuantizationSettings {
    pub quantization: EvenQuantization,
}

impl Default for ContentQuantizationSettings {
    fn default() -> Self {
        Self {
            quantization: EvenQuantization {
                numerator: 1,
                denominator: 16,
            },
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
#[serde(untagged)]
pub enum FlexibleMatrix {
    Unsigned(Box<Matrix>),
    Signed(SignedMatrix),
}

// We implement serializer on our own because #[serde(untagged)] produces error messages that are not helpful at all.
impl<'de> Deserialize<'de> for FlexibleMatrix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Signed matrices are still an unused feature in March 2025. We can add support for its deserialization later.
        // At the moment, deserializing the unsigned matrix directly (ignoring the possibility that it could be a
        // signed matrix) is the fastest path to good error messages.
        Ok(Self::Unsigned(Box::new(Matrix::deserialize(deserializer)?)))
    }
}

impl Default for FlexibleMatrix {
    fn default() -> Self {
        Self::Unsigned(Default::default())
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SignedMatrix {
    pub matrix: String,
    pub signature: String,
}

impl SignedMatrix {
    pub fn encode_value(matrix: &Matrix) -> anyhow::Result<String> {
        let bytes = rmp_serde::to_vec_named(matrix)?;
        Ok(BASE64_ENGINE.encode(bytes))
    }

    pub fn decode_value(&self) -> anyhow::Result<Matrix> {
        let bytes = BASE64_ENGINE.decode(self.matrix.as_bytes())?;
        Ok(rmp_serde::from_slice(&bytes)?)
    }
}

/// This is redundant (already contained in [`Matrix`]) but used to transfer settings only, without all the content
/// (columns, rows, slots).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixSettings {
    pub clip_play_settings: MatrixClipPlaySettings,
    pub clip_record_settings: MatrixClipRecordSettings,
    pub common_tempo_range: TempoRange,
    pub color_palette: ColorPalette,
    pub content_quantization_settings: ContentQuantizationSettings,
    pub activate_slot_on_trigger: bool,
    pub transport_sync_mode: TransportSyncMode,
    pub pre_roll_bars: u32,
}

impl Matrix {
    pub fn column_count(&self) -> usize {
        let Some(columns) = &self.columns else {
            return 0;
        };
        columns.len()
    }

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

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TempoRange {
    min: Bpm,
    max: Bpm,
}

impl TempoRange {
    pub fn new(min: Bpm, max: Bpm) -> PlaytimeApiResult<Self> {
        if min > max {
            return Err("min must be <= max".into());
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
            min: Bpm::new_panic(60.0),
            max: Bpm::new_panic(200.0),
        }
    }
}

/// Matrix-global settings related to playing clips.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixClipPlaySettings {
    #[serde(default)]
    pub trigger_behavior: TriggerSlotBehavior,
    #[serde(default)]
    pub start_timing: ClipPlayStartTiming,
    #[serde(default)]
    pub stop_timing: ClipPlayStopTiming,
    #[serde(default)]
    pub audio_settings: MatrixClipPlayAudioSettings,
    #[serde(default)]
    pub velocity_sensitivity: f64,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixClipPlayAudioSettings {
    pub resample_mode: VirtualResampleMode,
    pub time_stretch_mode: AudioTimeStretchMode,
    pub cache_behavior: AudioCacheBehavior,
}

/// Matrix-global settings related to recording clips.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatrixClipRecordSettings {
    #[serde(default)]
    pub start_timing: ClipRecordStartTiming,
    #[serde(default)]
    pub stop_timing: ClipRecordStopTiming,
    #[serde(default)]
    pub length_mode: RecordLengthMode,
    /// If `true`, starting to record in stopped state when metronome off, does a tempo detection recording.
    ///
    /// Otherwise, it does a count-in recording without a metronome.
    #[serde(default = "allow_tempo_detection_recording_default")]
    pub allow_tempo_detection_recording: bool,
    #[serde(default)]
    pub custom_length: EvenQuantization,
    #[serde(default)]
    pub play_start_timing: ClipPlayStartTimingOverrideAfterRecording,
    #[serde(default)]
    pub play_stop_timing: ClipPlayStopTimingOverrideAfterRecording,
    #[serde(default)]
    pub time_base: ClipRecordTimeBase,
    /// If `true`, starts playing the clip right after recording.
    #[serde(default = "looped_default")]
    pub looped: bool,
    #[serde(default)]
    pub midi_settings: MatrixClipRecordMidiSettings,
    #[serde(default)]
    pub audio_settings: MatrixClipRecordAudioSettings,
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    Default,
    Serialize,
    Deserialize,
    EnumIter,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RecordLengthMode {
    /// Records open-ended until the user decides to stop.
    #[display(fmt = "Open end")]
    #[default]
    OpenEnd = 0,
    /// Records at a maximum as much material as defined by `custom_length`.
    #[display(fmt = "Custom length")]
    CustomLength = 1,
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
        time_signature: TimeSignature,
        downbeat: DurationInBeats,
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
            #[allow(deprecated)]
            let beat_time_base = BeatTimeBase {
                audio_tempo: None,
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
            length_mode: Default::default(),
            custom_length: Default::default(),
            play_start_timing: Default::default(),
            play_stop_timing: Default::default(),
            time_base: Default::default(),
            looped: looped_default(),
            midi_settings: Default::default(),
            audio_settings: Default::default(),
            allow_tempo_detection_recording: allow_tempo_detection_recording_default(),
        }
    }
}

fn looped_default() -> bool {
    true
}

fn allow_tempo_detection_recording_default() -> bool {
    true
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixClipRecordMidiSettings {
    #[serde(default)]
    pub record_mode: MidiClipRecordMode,
    /// If `true`, attempts to detect the actual start of the recorded MIDI material and derives
    /// the downbeat position from that.
    #[serde(default)]
    pub detect_downbeat: bool,
    /// Makes the global record button work for MIDI by allowing global input detection.
    // TODO-high-playtime-after-release
    #[serde(default)]
    pub detect_input: bool,
    /// Applies quantization while recording using the current quantization settings.
    #[serde(default)]
    pub auto_quantize: bool,
    /// These are the MIDI settings each recorded and otherwise created clip will get.
    #[serde(default)]
    pub clip_settings: ClipMidiSettings,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MatrixClipRecordAudioSettings {
    /// If `true`, attempts to detect the actual start of the recorded audio material and derives
    /// the downbeat position from that.
    // TODO-high-playtime-after-release
    #[serde(default)]
    pub detect_downbeat: bool,
    /// Makes the global record button work for audio by allowing global input detection.
    // TODO-high-playtime-after-release
    #[serde(default)]
    pub detect_input: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ClipRecordTimeBase {
    /// Derives the time base of the resulting clip from the clip start timing.
    DeriveFromRecordTiming,
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Time`].
    Time,
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Beat`].
    Beat,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TriggerSlotBehavior {
    /// Press once = play, press again = stop.
    #[default]
    TogglePlayStop,
    /// Press once = play, release = stop.
    MomentaryPlayStop,
    /// Press once = play, press again = retrigger.
    Retrigger,
}

impl Default for ClipRecordTimeBase {
    fn default() -> Self {
        Self::DeriveFromRecordTiming
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum MidiClipRecordMode {
    /// Creates an empty clip and records MIDI material in it.
    Normal,
    /// Records more material onto an existing clip, leaving existing material in place.
    Overdub,
    /// Records more material onto an existing clip, overwriting existing material.
    // TODO-high-playtime-after-release
    Replace,
}

impl Default for MidiClipRecordMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

impl ClipPlayStopTiming {
    pub fn resolve(&self, start_timing: ClipPlayStartTiming) -> ResolvedClipPlayStopTiming {
        use ClipPlayStopTiming::*;
        match self {
            LikeClipStartTiming => match start_timing {
                ClipPlayStartTiming::Immediately => ResolvedClipPlayStopTiming::Immediately,
                ClipPlayStartTiming::Quantized(q) => ResolvedClipPlayStopTiming::Quantized(q),
            },
            Immediately => ResolvedClipPlayStopTiming::Immediately,
            Quantized(q) => ResolvedClipPlayStopTiming::Quantized(*q),
            UntilEndOfClip => ResolvedClipPlayStopTiming::UntilEndOfClip,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ResolvedClipPlayStopTiming {
    Immediately,
    /// Stops playing according to the given quantization.
    Quantized(EvenQuantization),
    /// Keeps playing until the end of the clip.
    UntilEndOfClip,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClipPlayStartTimingOverride {
    pub value: ClipPlayStartTiming,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClipPlayStopTimingOverride {
    pub value: ClipPlayStopTiming,
}

/// An even quantization.
///
/// "Even" in the sense that it's not swing or dotted.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(try_from = "RawEvenQuantization")]
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

#[derive(Deserialize)]
struct RawEvenQuantization {
    numerator: u32,
    denominator: u32,
}

impl TryFrom<RawEvenQuantization> for EvenQuantization {
    type Error = PlaytimeApiError;

    fn try_from(value: RawEvenQuantization) -> PlaytimeApiResult<Self> {
        EvenQuantization::new(value.numerator, value.denominator)
    }
}

impl EvenQuantization {
    pub const ONE_BAR: Self = EvenQuantization {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: u32, denominator: u32) -> PlaytimeApiResult<Self> {
        if numerator == 0 {
            return Err("numerator must be > 0".into());
        }
        if denominator == 0 {
            return Err("denominator must be > 0".into());
        }
        if numerator > 1 && denominator > 1 {
            return Err("if numerator > 1, denominator must be 1".into());
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

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, derive_more::Display)]
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

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, derive_more::Display)]
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

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, derive_more::Display)]
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

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, derive_more::Display)]
pub struct ClipId(String);

impl ClipId {
    pub fn random() -> Self {
        Self(nanoid::nanoid!())
    }

    pub fn new(raw: String) -> Self {
        Self(raw)
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::random()
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize, derive_more::Display)]
pub struct MatrixSequenceId(String);

impl MatrixSequenceId {
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    pub fn random() -> Self {
        Default::default()
    }
}

impl Default for MatrixSequenceId {
    fn default() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Column {
    #[serde(default)]
    pub id: ColumnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub clip_play_settings: ColumnClipPlaySettings,
    #[serde(default)]
    pub clip_record_settings: ColumnClipRecordSettings,
    /// Slots in this column.
    ///
    /// Only filled slots need to be mentioned here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<Slot>>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ColumnSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub clip_play_settings: ColumnClipPlaySettings,
    pub clip_record_settings: ColumnClipRecordSettings,
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

pub fn default_follows_scene() -> bool {
    true
}

pub fn default_exclusive() -> bool {
    true
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ColumnClipPlaySettings {
    #[serde(default = "default_follows_scene")]
    pub follows_scene: bool,
    #[serde(default = "default_exclusive")]
    pub exclusive: bool,
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
    /// Trigger behavior override.
    ///
    /// `None` means it uses the matrix-global trigger behavior.
    pub trigger_behavior: Option<TriggerSlotBehavior>,
    /// Velocity sensitivity override.
    ///
    /// `None` means it uses the matrix-global sensitivity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity_sensitivity: Option<f64>,
    #[serde(default)]
    pub audio_settings: ColumnClipPlayAudioSettings,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ColumnClipRecordSettings {
    #[serde(default)]
    pub origin: RecordOrigin,
    /// By default, Playtime records from the play track but this settings allows to override that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackId>,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
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
/// song that's played exclusively whereas a row is just a row in the Playtime matrix. This distinction
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
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Row {
    #[serde(default)]
    pub id: RowId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An optional tempo associated with this row.
    // TODO-high-playtime-after-release
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo: Option<Bpm>,
    /// An optional time signature associated with this row.
    // TODO-high-playtime-after-release
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<TimeSignature>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

impl AudioTimeStretchMode {
    pub fn is_vari_speed(&self) -> bool {
        matches!(self, Self::VariSpeed)
    }
}

impl Default for AudioTimeStretchMode {
    fn default() -> Self {
        Self::KeepingPitch(Default::default())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReaperResampleMode {
    pub mode: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct TimeStretchMode {
    pub mode: VirtualTimeStretchMode,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReaperPitchShiftMode {
    pub mode: u32,
    pub sub_mode: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RecordOrigin {
    /// Records using the hardware input set for the track (MIDI or stereo).
    TrackInput,
    /// Captures audio from the output of the track.
    TrackAudioOutput,
    /// Records audio flowing into the FX input.
    FxAudioInput(ChannelRange),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ChannelRange {
    pub first_channel_index: u32,
    pub channel_count: u32,
}

impl Default for RecordOrigin {
    fn default() -> Self {
        Self::TrackInput
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
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
    #[serde(default)]
    pub ignited: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Clip {
    #[serde(default)]
    pub id: ClipId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Source of the audio/MIDI material of this clip.
    #[serde(default)]
    pub source: Source,
    /// Source with effects "rendered in", usually audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_source: Option<Source>,
    /// Which of the sources is the active one.
    #[serde(default)]
    pub active_source: SourceOrigin,
    /// Time base of the material provided by that source.
    #[serde(default)]
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
    /// Velocity sensitivity override.
    ///
    /// `None` means it uses the column sensitivity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity_sensitivity: Option<f64>,
    /// Whether the clip should be played repeatedly or as a single shot.
    #[serde(default = "looped_default")]
    pub looped: bool,
    /// Relative volume adjustment of clip.
    #[serde(default)]
    pub volume: Db,
    /// Color of the clip.
    #[serde(default)]
    pub color: ClipColor,
    /// A more variable kind of section within the main section.
    ///
    /// Intended for playing with section bounds as a part of the performance *without* destroying
    /// the original section.
    #[serde(default)]
    pub dynamic_section: Section,
    /// Defines which portion of the original source should be played.
    ///
    /// This section is especially important for the way that Playtime records audio clips: It
    /// records the count-in phase as well and more samples than actually necessary at the end.
    /// It then sets this section to the portion that actually matters. Once set, this section
    /// rarely changes.
    #[serde(alias = "section")]
    #[serde(default)]
    pub fixed_section: Section,
    #[serde(default)]
    pub pitch_shift: Semitones,
    #[serde(default)]
    pub audio_settings: ClipAudioSettings,
    #[serde(default)]
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

impl Default for Clip {
    fn default() -> Self {
        Self {
            id: Default::default(),
            name: None,
            source: Default::default(),
            frozen_source: None,
            active_source: Default::default(),
            time_base: Default::default(),
            start_timing: None,
            stop_timing: None,
            velocity_sensitivity: None,
            looped: looped_default(),
            volume: Default::default(),
            color: Default::default(),
            dynamic_section: Default::default(),
            fixed_section: Default::default(),
            pitch_shift: Default::default(),
            audio_settings: Default::default(),
            midi_settings: Default::default(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClipAudioSettings {
    /// Defines whether to apply automatic fades in order to fix potentially non-optimized source
    /// material.
    ///
    /// ## `false`
    ///
    /// Doesn't apply automatic fades for fixing non-optimized source material.
    ///
    /// This only prevents fix fades at source level, that is fades fixing the source file itself or the source cut
    /// (= static/fixed section). Fades that are not about fixing the source will still be applied if necessary in order
    /// to ensure a smooth playback, such as:
    ///
    /// - Dynamic section fades (start fade-in, end fade-out)
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
    #[serde(default = "apply_source_fades_default")]
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
    /// The clip's native tempo.
    ///
    /// This information is used by the clip engine to determine how much to speed up or
    /// slow down the material depending on the current project tempo.
    ///
    /// The tempo is not kept in the [`ClipTimeBase`] variant `Beat` anymore because then it would
    /// be too easy to get lost when temporarily switching to variant `Time` for fun. It could be
    /// hard figuring out the original tempo later after loading the project again. One could
    /// argue that the same is true for time signature, but that's quite easy to figure out again,
    /// plus, it's not an audio-only setting.
    ///
    /// The tempo is also not kept as part of [`Source`] because it's just additional meta
    /// information that's not strictly necessary to load the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_tempo: Option<Bpm>,
}

impl Default for ClipAudioSettings {
    fn default() -> Self {
        Self {
            cache_behavior: Default::default(),
            apply_source_fades: apply_source_fades_default(),
            time_stretch_mode: None,
            resample_mode: None,
            original_tempo: None,
        }
    }
}

fn apply_source_fades_default() -> bool {
    true
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ClipMidiSettings {
    /// If set, all MIDI channel MIDI events will be remapped to this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_channel: Option<u8>,
    /// Reset settings.
    #[serde(default)]
    pub reset_settings: ClipMidiResetSettings,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClipMidiResetSettings {
    /// For fine-tuning the start and the end of one particular "play" session of a clip.
    ///
    /// - The left interaction reset messages are sent when clip playback starts:
    ///     - For immediate starts: Immediately at trigger time (done in looper)
    ///     - For quantized starts that are triggered before the quantized position:
    ///       At the time the quantized position is reached (done in looper)
    ///     - For quantized starts that are triggered a tiny bit after the quantized position ("last minute triggering"):
    ///       Immediately at trigger time (done in interaction handler)
    /// - The right interaction reset messages are sent when clip playback ends:
    ///     - For immediate stops/retriggers: Immediately at trigger time (done in interaction handler)
    ///     - For quantized stops/retriggers: At the time the quantized position is reached (done in interaction handler)
    ///     - For naturally ending one-shots or stop-triggered loops with "Until end of clip":
    ///       At the time when the clip ends naturally (done in looper)
    pub interaction_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the section (done in section handler).
    ///
    /// - The left section reset messages are sent when the effective section start position > 0 and playback hits the
    ///   section start position.
    /// - The right section reset messages are sent when a section length is defined and playback hits the section end
    ///   position.
    /// - If the clip is looped, the messages will be sent at each loop cycle.
    pub section_reset_settings: MidiResetMessageRange,
    /// For fixing the source itself (done in start-end handler).
    ///
    /// - The left source reset messages are sent when the effective section start position == 0 and playback hits
    ///   the start of the source.
    /// - The right source reset messages are sent when no section length is defined and playback hits the end of the
    ///   source.
    /// - If the clip is looped, the messages will be sent at *each* loop cycle.
    ///
    /// This exists separately from the section reset settings because one might prefer using the original MIDI
    /// sequence with less reset logic when playing without section. After all, the source itself might already
    /// be perfect as it is. But as soon as we introduce a section, this is not guaranteed anymore and introducing
    /// reset messages almost always makes sense.
    pub source_reset_settings: MidiResetMessageRange,
}

impl Default for ClipMidiResetSettings {
    fn default() -> Self {
        Self::RIGHT_LIGHT
    }
}

impl ClipMidiResetSettings {
    pub const NONE: Self = Self {
        interaction_reset_settings: MidiResetMessageRange::NONE,
        section_reset_settings: MidiResetMessageRange::NONE,
        source_reset_settings: MidiResetMessageRange::NONE,
    };

    pub const RIGHT_LIGHT: Self = Self {
        interaction_reset_settings: MidiResetMessageRange::RIGHT_LIGHT,
        section_reset_settings: MidiResetMessageRange::RIGHT_LIGHT,
        source_reset_settings: MidiResetMessageRange::RIGHT_LIGHT,
    };
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MidiResetMessageRange {
    /// Which MIDI reset messages to apply at the beginning.
    pub left: MidiResetMessages,
    /// Which MIDI reset messages to apply at the end.
    pub right: MidiResetMessages,
}

impl MidiResetMessageRange {
    pub const NONE: Self = Self {
        left: MidiResetMessages::NONE,
        right: MidiResetMessages::NONE,
    };

    pub const RIGHT_LIGHT: Self = Self {
        left: MidiResetMessages::NONE,
        right: MidiResetMessages::LIGHT,
    };
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MidiResetMessages {
    /// Sends note-off events for all notes that are currently playing in this clip.
    #[serde(default)]
    pub on_notes_off: bool,
    /// Sends MIDI CC 123 (all-notes-off).
    pub all_notes_off: bool,
    /// Sends MIDI CC 120 (all-sound-off).
    pub all_sound_off: bool,
    /// Sends MIDI CC121 (reset-all-controllers).
    pub reset_all_controllers: bool,
    /// Sends CC64 value 0 events (damper-pedal-off) for all damper pedals that are currently pressed in this clip.
    #[serde(alias = "damper_pedal_off")]
    pub on_damper_pedal_off: bool,
}

impl MidiResetMessages {
    pub const NONE: Self = Self {
        on_notes_off: false,
        all_notes_off: false,
        all_sound_off: false,
        reset_all_controllers: false,
        on_damper_pedal_off: false,
    };

    pub const LIGHT: Self = Self {
        on_notes_off: true,
        all_notes_off: false,
        all_sound_off: false,
        reset_all_controllers: false,
        on_damper_pedal_off: true,
    };

    pub fn at_least_one_enabled(&self) -> bool {
        self.on_notes_off
            || self.all_notes_off
            || self.all_sound_off
            || self.reset_all_controllers
            || self.on_damper_pedal_off
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Section {
    /// Position in the source from which to start.
    ///
    /// If this is greater than zero, a fade-in will be used to avoid clicks.
    #[serde(default)]
    pub start_pos: DurationInSeconds,
    /// Length of the material to be played, starting from `start_pos`.
    ///
    /// - `None` means until original source end.
    /// - May exceed the end of the source.
    /// - If this makes the section end be located before the original source end, a fade-out will
    ///   be used to avoid clicks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<DurationInSeconds>,
}

impl Section {
    pub fn combine_with_inner(&self, inner: Section) -> Section {
        Section {
            start_pos: self.start_pos + inner.start_pos,
            length: match (self.length, inner.length) {
                (Some(l), None) => Some(l.saturating_sub(inner.start_pos)),
                (_, l) => l,
            },
        }
    }

    pub fn effective_length(&self, source_length: DurationInSeconds) -> DurationInSeconds {
        if let Some(length) = self.length {
            length
        } else {
            source_length.saturating_sub(self.start_pos)
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ClipColor {
    /// Inherits the color of the column's play track.
    #[default]
    PlayTrackColor,
    /// Assigns a very specific custom color.
    CustomColor(CustomClipColor),
    /// Uses a certain color from a palette.
    PaletteColor(PaletteClipColor),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct CustomClipColor {
    pub value: RgbColor,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct PaletteClipColor {
    pub index: u32,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Source {
    /// Takes content from a media file on the file system (audio).
    File(FileSource),
    /// Embedded MIDI data.
    MidiChunk(MidiChunkSource),
}

impl Default for Source {
    fn default() -> Self {
        Source::MidiChunk(Default::default())
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct FileSource {
    /// Path to the media file.
    ///
    /// - If it's a relative path, it will be interpreted as relative to the REAPER project directory.
    /// - This should use slash as path segment separator, even on Windows.
    pub path: Utf8PathBuf,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MidiChunkSource {
    /// MIDI data in the same format that REAPER uses for in-project MIDI.
    pub chunk: String,
}

/// Decides if the clip will be adjusted to the current tempo.
#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ClipTimeBase {
    /// Material which doesn't need to be adjusted to the current tempo.
    #[default]
    Time,
    /// Material which needs to be adjusted to the current tempo.
    Beat(BeatTimeBase),
}

impl ClipTimeBase {
    pub fn is_time_based(&self) -> bool {
        matches!(self, Self::Time)
    }

    pub fn is_beat_based(&self) -> bool {
        matches!(self, Self::Beat(_))
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct BeatTimeBase {
    #[deprecated(note = "moved to ClipAudioSettings#original_tempo")]
    #[serde(skip_serializing)]
    pub audio_tempo: Option<Bpm>,
    /// The time signature of this clip.
    ///
    /// If provided, this information is used for certain aspects of the user interface.
    pub time_signature: TimeSignature,
    /// Defines which position (in beats) is the downbeat.
    pub downbeat: DurationInBeats,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ColorPalette {
    pub entries: Vec<ColorPaletteEntry>,
}

const TAILWIND_SLATE_500: RgbColor = RgbColor(0x64, 0x74, 0x8B);
const TAILWIND_GRAY_500: RgbColor = RgbColor(0x6b, 0x72, 0x80);
const TAILWIND_ZINC_500: RgbColor = RgbColor(0x71, 0x71, 0x7A);
const TAILWIND_NEUTRAL_500: RgbColor = RgbColor(0x73, 0x73, 0x73);
const TAILWIND_STONE_500: RgbColor = RgbColor(0x78, 0x71, 0x6c);
const TAILWIND_RED_500: RgbColor = RgbColor(0xef, 0x44, 0x44);
const TAILWIND_ORANGE_500: RgbColor = RgbColor(0xf9, 0x73, 0x16);
const TAILWIND_AMBER_500: RgbColor = RgbColor(0xf5, 0x9e, 0x0b);
const TAILWIND_YELLOW_500: RgbColor = RgbColor(0xea, 0xb3, 0x08);
const TAILWIND_LIME_500: RgbColor = RgbColor(0x84, 0xcc, 0x16);
const TAILWIND_GREEN_500: RgbColor = RgbColor(0x22, 0xc5, 0x5e);
const TAILWIND_EMERALD_500: RgbColor = RgbColor(0x10, 0xb9, 0x81);
const TAILWIND_TEAL_500: RgbColor = RgbColor(0x14, 0xb8, 0xa6);
const TAILWIND_CYAN_500: RgbColor = RgbColor(0x06, 0xb6, 0xd4);
const TAILWIND_SKY_500: RgbColor = RgbColor(0x0e, 0xa5, 0xe9);
const TAILWIND_BLUE_500: RgbColor = RgbColor(0x3b, 0x82, 0xf6);
const TAILWIND_INDIGO_500: RgbColor = RgbColor(0x63, 0x66, 0xf1);
const TAILWIND_VIOLET_500: RgbColor = RgbColor(0x8b, 0x5c, 0xf6);
const TAILWIND_PURPLE_500: RgbColor = RgbColor(0xa8, 0x55, 0xf7);
const TAILWIND_FUCHSIA_500: RgbColor = RgbColor(0xd9, 0x46, 0xef);
const TAILWIND_PINK_500: RgbColor = RgbColor(0xec, 0x48, 0x99);
const TAILWIND_ROSE_500: RgbColor = RgbColor(0xf4, 0x3f, 0x5e);

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            entries: [
                TAILWIND_SLATE_500,
                TAILWIND_GRAY_500,
                TAILWIND_ZINC_500,
                TAILWIND_NEUTRAL_500,
                TAILWIND_STONE_500,
                TAILWIND_RED_500,
                TAILWIND_ORANGE_500,
                TAILWIND_AMBER_500,
                TAILWIND_YELLOW_500,
                TAILWIND_LIME_500,
                TAILWIND_GREEN_500,
                TAILWIND_EMERALD_500,
                TAILWIND_TEAL_500,
                TAILWIND_CYAN_500,
                TAILWIND_SKY_500,
                TAILWIND_BLUE_500,
                TAILWIND_INDIGO_500,
                TAILWIND_VIOLET_500,
                TAILWIND_PURPLE_500,
                TAILWIND_FUCHSIA_500,
                TAILWIND_PINK_500,
                TAILWIND_ROSE_500,
            ]
            .into_iter()
            .map(|color| ColorPaletteEntry { color })
            .collect(),
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ColorPaletteEntry {
    pub color: RgbColor,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TrackId(String);

impl TrackId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn get(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct RgbColor(pub u8, pub u8, pub u8);

type PlaytimeApiResult<T> = Result<T, PlaytimeApiError>;

/// Important: Since some of these types are going to be used in real-time contexts, we don't
/// want heap-allocated types in here!
#[derive(thiserror::Error, Debug)]
#[error("{msg}")]
pub struct PlaytimeApiError {
    msg: &'static str,
}

impl From<&'static str> for PlaytimeApiError {
    fn from(msg: &'static str) -> Self {
        Self { msg }
    }
}

impl From<PlaytimeApiError> for &'static str {
    fn from(value: PlaytimeApiError) -> Self {
        value.msg
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ColumnAddress {
    pub index: usize,
}

impl Display for ColumnAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column {}", self.index + 1)
    }
}

impl ColumnAddress {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct RowAddress {
    pub index: usize,
}

impl Display for RowAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Row {}", self.index + 1)
    }
}

impl RowAddress {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct SlotAddress {
    pub column_index: usize,
    pub row_index: usize,
}

impl SlotAddress {
    pub fn new(column: usize, row: usize) -> Self {
        Self {
            column_index: column,
            row_index: row,
        }
    }

    pub fn column(&self) -> usize {
        self.column_index
    }

    pub fn row(&self) -> usize {
        self.row_index
    }

    pub fn to_cell_address(&self) -> CellAddress {
        CellAddress::slot(self.column_index, self.row_index)
    }
}

impl Display for SlotAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Slot {}/{}", self.column_index + 1, self.row_index + 1)
    }
}

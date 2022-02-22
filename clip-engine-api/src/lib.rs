// TODO-high Revise defaults. I think we should only make things optional that do have a totally
//  natural default or are a override of something! Especially we shouldn't fall into the trap of choosing sane GUI faults here.
// TODO-high Also double check that we don't fall in the rich enum trap too much. For persistent
//  data we often want to keep settings even they are not active.

use std::path::PathBuf;

struct Matrix {
    /// All columns from left to right.
    columns: Vec<Column>,
    /// All rows from top to bottom.
    rows: Vec<Row>,
    clip_play_settings: MatrixClipPlaySettings,
    clip_record_settings: MatrixClipRecordSettings,
    common_tempo_range: TempoRange,
}

struct TempoRange {
    min: Bpm,
    max: Bpm,
}

/// Matrix-global settings related to playing clips.
struct MatrixClipPlaySettings {
    start_timing: ClipPlayStartTiming,
    stop_timing: ClipPlayStopTiming,
    audio_settings: MatrixClipPlayAudioSettings,
}

struct MatrixClipPlayAudioSettings {
    time_stretch_mode: AudioTimeStretchMode,
}

/// Matrix-global settings related to recording clips.
struct MatrixClipRecordSettings {
    start_timing: ClipRecordStartTiming,
    stop_timing: ClipRecordStopTiming,
    duration: RecordLength,
    play_start_timing: ClipSettingOverrideAfterRecording<ClipPlayStartTiming>,
    play_stop_timing: ClipSettingOverrideAfterRecording<ClipPlayStopTiming>,
    time_base: ClipRecordTimeBase,
    /// If `true`, starts playing the clip right after recording.
    play_after: bool,
    /// If `true`, sets the global tempo to the tempo of this clip right after recording.
    lead_tempo: bool,
    midi_settings: MatrixClipRecordMidiSettings,
    audio_settings: MatrixClipRecordAudioSettings,
}

struct MatrixClipRecordMidiSettings {
    record_mode: MidiClipRecordMode,
    /// If `true`, attempts to detect the actual start of the recorded MIDI material and derives
    /// the downbeat position from that.
    detect_downbeat: bool,
    /// Makes the global record button work for MIDI by allowing global input detection.
    detect_input: bool,
    /// Applies quantization while recording using the current quantization settings.
    auto_quantize: bool,
}

struct MatrixClipRecordAudioSettings {
    /// If `true`, attempts to detect the actual start of the recorded audio material and derives
    /// the downbeat position from that.
    detect_downbeat: bool,
    /// Makes the global record button work for audio by allowing global input detection.
    detect_input: bool,
}

enum RecordLength {
    /// Records open-ended until the user decides to stop.
    OpenEnd,
    /// Records exactly as much material as defined by the given quantization.
    Quantized(EvenQuantization),
}

enum ClipRecordTimeBase {
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Time`].
    Time,
    /// Sets the time base of the recorded clip to [`ClipTimeBase::Beat`].
    Beat,
    /// Derives the time base of the resulting clip from the clip start/stop timing.
    DeriveFromRecordTiming,
}

enum ClipRecordStartTiming {
    /// Uses the global clip play start timing.
    LikeClipPlayStartTiming,
    /// Starts recording immediately.
    Immediately,
    /// Starts recording according to the given quantization.
    Quantized(EvenQuantization),
}

enum ClipRecordStopTiming {
    /// Uses the record start timing.
    LikeClipRecordStartTiming,
    /// Stops recording immediately.
    Immediately,
    /// Stops recording according to the given quantization.
    Quantized(EvenQuantization),
}

enum MidiClipRecordMode {
    /// Creates an empty clip and records MIDI material in it.
    Normal,
    /// Records more material onto an existing clip, leaving existing material in place.
    Overdub,
    /// Records more material onto an existing clip, overwriting existing material.
    Replace,
}

enum ClipPlayStartTiming {
    /// Starts playing immediately.
    Immediately,
    /// Starts playing according to the given quantization.
    Quantized(EvenQuantization),
}

enum ClipPlayStopTiming {
    /// Uses the play start timing.
    LikeClipStartTiming,
    /// Stops playing immediately.
    Immediately,
    /// Stops playing according to the given quantization.
    Quantized(EvenQuantization),
    /// Keeps playing until the end of the clip.
    UntilEndOfClip,
}

enum ClipSettingOverrideAfterRecording<T> {
    /// Doesn't apply any override.
    Inherit,
    /// Overrides the setting with the given value.
    Override(T),
    /// Overrides the setting with a value derived from the record timing.
    OverrideFromRecordTiming,
}

/// An even quantization.
///
/// Even in the sense of that's it's not swing or dotted.
struct EvenQuantization {
    /// The number of bars.
    ///
    /// Must not be zero.
    numerator: u32,
    /// Defines the fraction of a bar.
    ///
    /// Must not be zero.
    ///
    /// If the numerator is > 1, this must be 1.
    denominator: u32,
}

struct Column {
    clip_play_settings: ColumnClipPlaySettings,
    clip_record_settings: ColumnClipRecordSettings,
    /// Slots in this column.
    ///
    /// Only filled slots need to be mentioned here.
    slots: Vec<Slot>,
}

struct ColumnClipPlaySettings {
    /// REAPER track used for playing back clips in this column.
    ///
    /// Usually, each column should have a play track. But events might occur that leave a column
    /// in a "track-less" state, e.g. the deletion of a track. This column will be unusable until
    /// the user sets a play track again. We still want to be able to save the matrix in such a
    /// state, otherwise it could be really annoying. So we allow `None`.
    track: Option<TrackId>,
    /// Start timing override.
    ///
    /// `None` means it uses the matrix-global start timing.
    start_timing: Option<ClipPlayStartTiming>,
    /// Stop timing override.
    ///
    /// `None` means it uses the matrix-global stop timing.
    stop_timing: Option<ClipPlayStopTiming>,
    audio_settings: ColumnClipPlayAudioSettings,
}

struct ColumnClipRecordSettings {
    /// By default, Playtime records from the play track but this settings allows to override that.
    track: Option<TrackId>,
    origin: TrackRecordOrigin,
}

struct ColumnClipPlayAudioSettings {
    /// Overrides the matrix-global audio time stretch mode for clips in this column.
    time_stretch_mode: Option<AudioTimeStretchMode>,
}

struct Row {
    /// An optional tempo associated with this row.
    tempo: Option<Bpm>,
    /// An optional time signature associated with this row.
    time_signature: Option<TimeSignature>,
}

enum AudioTimeStretchMode {
    /// Doesn't just stretch/squeeze the material but also changes the pitch.
    ///
    /// Comparatively fast.
    VariSpeed(VariSpeedAudioTimeStretchMode),
    /// Applies a real time-stretch algorithm to the material which keeps the pitch.
    ///
    /// Comparatively slow.
    KeepingPitch(KeepingPitchAudioTimeStretchMode),
}

enum VariSpeedAudioTimeStretchMode {
    /// Uses the resample mode set as default for this REAPER project.
    ProjectDefault,
    /// Uses a specific resample mode.
    ReaperMode(ReaperResampleMode),
}

struct ReaperResampleMode {
    mode: u32,
}

enum KeepingPitchAudioTimeStretchMode {
    /// Uses the pitch shift mode set as default for this REAPER project.
    ProjectDefault,
    /// Uses a specific REAPER pitch shift mode.
    ReaperMode(ReaperPitchShiftMode),
}

struct ReaperPitchShiftMode {
    mode: u32,
    sub_mode: u32,
}

enum TrackRecordOrigin {
    /// Records using the hardware input set for the track (MIDI or stereo).
    TrackInput,
    /// Captures audio from the output of the track.
    TrackAudioOutput,
}

struct Slot {
    /// Slot index within the column (= row), starting at zero.
    row: usize,
    /// Clip which currently lives in this slot.
    clip: Option<Clip>,
}

struct Clip {
    /// Source of the audio/MIDI material of this clip.
    source: Source,
    /// Time base of the material provided by that source.
    time_base: ClipTimeBase,
    /// Start timing override.
    ///
    /// `None` means it uses the column start timing.
    start_timing: Option<ClipPlayStartTiming>,
    /// Stop timing override.
    ///
    /// `None` means it uses the column stop timing.
    stop_timing: Option<ClipPlayStopTiming>,
    /// Whether the clip should be played repeatedly or as a single shot.
    looped: bool,
    /// Color of the clip.
    color: ClipColor,
    /// Defines which portion of the original source should be played.
    section: Section,
    audio_settings: ClipAudioSettings,
    midi_settings: ClipMidiSettings,
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

struct ClipAudioSettings {
    /// Whether to cache audio in memory.
    cache_behavior: AudioCacheBehavior,
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
    apply_source_fades: bool,
    /// Defines how to adjust audio material.
    ///
    /// This is usually used with the beat time base to match the tempo of the clip to the global
    /// tempo.
    ///
    /// `None` means it uses the column time stretch mode.
    time_stretch_mode: Option<AudioTimeStretchMode>,
}

// struct Canvas {
//     /// Should be long enough to let the source section fit in.
//     length: DurationInSeconds,
//     /// Position of the source section.
//     section_pos: PositionInSeconds,
// }

struct ClipMidiSettings {
    /// For fixing the source itself.
    source_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the section.
    section_reset_settings: MidiResetMessageRange,
    /// For fine-tuning the complete loop.
    loop_reset_settings: MidiResetMessageRange,
    /// For fine-tuning instant start/stop of a MIDI clip when in the middle of a source or section.
    interaction_reset_settings: MidiResetMessageRange,
}

struct MidiResetMessageRange {
    /// Which MIDI reset messages to apply at the beginning.
    start: MidiResetMessages,
    /// Which MIDI reset messages to apply at the beginning.
    end: MidiResetMessages,
}

struct MidiResetMessages {
    all_notes_off: bool,
    all_sounds_off: bool,
    sustain_off: bool,
}

struct Section {
    /// Position in the source from which to start.
    ///
    /// If this is greater than zero, a fade-in will be used to avoid clicks.
    start_pos: Seconds,
    /// Length of the material to be played, starting from `start_pos`.
    ///
    /// - `None` means until original source end.
    /// - May exceed the end of the source.
    /// - If this makes the section end be located before the original source end, a fade-out will
    ///   be used to avoid clicks.
    length: Option<Seconds>,
}

enum AudioCacheBehavior {
    /// Loads directly from the disk.
    ///
    /// Might still pre-buffer some blocks but definitely won't put the complete audio data into
    /// memory.
    DirectFromDisk,
    /// Loads the complete audio data into memory.
    CacheInMemory,
}

enum ClipColor {
    /// Inherits the color of the column's play track.
    PlayTrackColor,
    /// Assigns a very specific custom color.
    CustomColor(RgbColor),
    /// Uses a certain color from a palette.
    PaletteColor(u32),
}

enum Source {
    /// Takes content from a media file on the file system (audio or MIDI).
    File(FileSource),
    /// Embedded MIDI data.
    MidiChunk(MidiChunkSource),
}

struct FileSource {
    /// Path to the media file.
    ///
    /// If it's a relative path, it will be interpreted as relative to the REAPER project directory.
    path: PathBuf,
}

struct MidiChunkSource {
    /// MIDI data in the same format that REAPER uses for in-project MIDI.
    chunk: String,
}

/// Decides if the clip will be adjusted to the current tempo.
enum ClipTimeBase {
    /// Material which doesn't need to be adjusted to the current tempo.
    Time,
    /// Material which needs to be adjusted to the current tempo.
    Beat(BeatTimeBase),
}

struct BeatTimeBase {
    /// The clip's native tempo.
    ///
    /// Must be set for audio. Is ignored for MIDI.
    ///
    /// This information is used by the clip engine to determine how much to speed up or
    /// slow down the material depending on the current project tempo.
    audio_bpm: Option<Bpm>,
    /// The time signature of this clip.
    ///
    /// If provided, This information is used for certain aspects of the user interface.
    time_signature: TimeSignature,
    /// Defines which position (in beats) is the downbeat.
    downbeat: f64,
}

struct TimeSignature {
    numerator: u32,
    denominator: u32,
}

struct TrackId(String);
struct Bpm(f64);
struct Seconds(f64);
pub struct RgbColor(pub u8, pub u8, pub u8);

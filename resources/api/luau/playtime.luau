--!strict

--- Attention: This file is generated from Rust code! Don't modify it directly!

local module = {}

export type PlaytimePersistenceRoot = {
	matrix: Matrix,
	even_quantization: EvenQuantization,
}

export type Matrix = {
	columns: { Column }?,
	rows: { Row }?,
	clip_play_settings: MatrixClipPlaySettings,
	clip_record_settings: MatrixClipRecordSettings,
	common_tempo_range: TempoRange,
	color_palette: ColorPalette?,
	content_quantization_settings: ContentQuantizationSettings?,
	sequencer: MatrixSequencer?,
}

export type MatrixSequencer = {
	sequences: { MatrixSequence }?,
	active_sequence: MatrixSequenceId?,
}

export type MatrixSequence = {
	id: MatrixSequenceId,
	info: MatrixSequenceInfo,
	data: MatrixSequenceData,
}

export type MatrixSequenceInfo = {
	name: string?,
	created_at: string,
}

export type MatrixSequenceData = {
	ppq: number,
	count_in: number,
	events: { MatrixSequenceEvent },
}

export type MatrixSequenceEvent = {
	pulse_diff: number,
	message: MatrixSequenceMessage,
}

export type MatrixSequenceMessage =
	"PanicMatrix"
	| "StopMatrix"
	| "PanicColumn"
	| "StopColumn"
	| "StartScene"
	| "PanicSlot"
	| "StartSlot"
	| "StopSlot"

export type MatrixSequenceColumnMessage = {
	index: number,
}

export type MatrixSequenceRowMessage = {
	index: number,
}

export type MatrixSequenceSlotMessage = {
	column_index: number,
	row_index: number,
}

export type ContentQuantizationSettings = {
	quantization: EvenQuantization,
}

export type SignedMatrix = {
	matrix: string,
	signature: string,
}

export type MatrixSettings = {
	clip_play_settings: MatrixClipPlaySettings,
	clip_record_settings: MatrixClipRecordSettings,
	common_tempo_range: TempoRange,
	color_palette: ColorPalette,
	content_quantization_settings: ContentQuantizationSettings,
}

export type TempoRange = {
	min: Bpm,
	max: Bpm,
}

export type MatrixClipPlaySettings = {
	trigger_behavior: TriggerPlayBehavior?,
	start_timing: ClipPlayStartTiming,
	stop_timing: ClipPlayStopTiming,
	audio_settings: MatrixClipPlayAudioSettings,
}

export type MatrixClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode,
	time_stretch_mode: AudioTimeStretchMode,
	cache_behavior: AudioCacheBehavior,
}

export type MatrixClipRecordSettings = {
	start_timing: ClipRecordStartTiming,
	stop_timing: ClipRecordStopTiming,
	duration: RecordLength,
	play_start_timing: ClipPlayStartTimingOverrideAfterRecording,
	play_stop_timing: ClipPlayStopTimingOverrideAfterRecording,
	time_base: ClipRecordTimeBase,
	looped: boolean,
	lead_tempo: boolean,
	midi_settings: MatrixClipRecordMidiSettings,
	audio_settings: MatrixClipRecordAudioSettings,
}

export type MatrixClipRecordMidiSettings = {
	record_mode: MidiClipRecordMode,
	detect_downbeat: boolean,
	detect_input: boolean,
	auto_quantize: boolean,
	clip_settings: ClipMidiSettings?,
}

export type MatrixClipRecordAudioSettings = {
	detect_downbeat: boolean,
	detect_input: boolean,
}

export type RecordLength_OpenEnd = { kind: "OpenEnd" }
export type RecordLength_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type RecordLength = RecordLength_OpenEnd | RecordLength_Quantized

export type ClipRecordTimeBase_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }
export type ClipRecordTimeBase_Time = { kind: "Time" }
export type ClipRecordTimeBase_Beat = { kind: "Beat" }

export type ClipRecordTimeBase =
	ClipRecordTimeBase_DeriveFromRecordTiming
	| ClipRecordTimeBase_Time
	| ClipRecordTimeBase_Beat

export type TriggerPlayBehavior_TogglePlayStop = { kind: "TogglePlayStop" }
export type TriggerPlayBehavior_MomentaryPlayStop = { kind: "MomentaryPlayStop" }
export type TriggerPlayBehavior_Retrigger = { kind: "Retrigger" }

export type TriggerPlayBehavior =
	TriggerPlayBehavior_TogglePlayStop
	| TriggerPlayBehavior_MomentaryPlayStop
	| TriggerPlayBehavior_Retrigger

export type ClipRecordStartTiming_LikeClipPlayStartTiming = { kind: "LikeClipPlayStartTiming" }
export type ClipRecordStartTiming_Immediately = { kind: "Immediately" }
export type ClipRecordStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipRecordStartTiming =
	ClipRecordStartTiming_LikeClipPlayStartTiming
	| ClipRecordStartTiming_Immediately
	| ClipRecordStartTiming_Quantized

export type ClipRecordStopTiming_LikeClipRecordStartTiming = { kind: "LikeClipRecordStartTiming" }
export type ClipRecordStopTiming_Immediately = { kind: "Immediately" }
export type ClipRecordStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipRecordStopTiming =
	ClipRecordStopTiming_LikeClipRecordStartTiming
	| ClipRecordStopTiming_Immediately
	| ClipRecordStopTiming_Quantized

export type MidiClipRecordMode_Normal = { kind: "Normal" }
export type MidiClipRecordMode_Overdub = { kind: "Overdub" }
export type MidiClipRecordMode_Replace = { kind: "Replace" }

export type MidiClipRecordMode = MidiClipRecordMode_Normal | MidiClipRecordMode_Overdub | MidiClipRecordMode_Replace

export type ClipPlayStartTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipPlayStartTiming = ClipPlayStartTiming_Immediately | ClipPlayStartTiming_Quantized

export type ClipPlayStopTiming_LikeClipStartTiming = { kind: "LikeClipStartTiming" }
export type ClipPlayStopTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }
export type ClipPlayStopTiming_UntilEndOfClip = { kind: "UntilEndOfClip" }

export type ClipPlayStopTiming =
	ClipPlayStopTiming_LikeClipStartTiming
	| ClipPlayStopTiming_Immediately
	| ClipPlayStopTiming_Quantized
	| ClipPlayStopTiming_UntilEndOfClip

export type ResolvedClipPlayStopTiming = "Immediately" | "Quantized" | "UntilEndOfClip"

export type ClipPlayStartTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStartTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStartTiming }
export type ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

export type ClipPlayStartTimingOverrideAfterRecording =
	ClipPlayStartTimingOverrideAfterRecording_Inherit
	| ClipPlayStartTimingOverrideAfterRecording_Override
	| ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming

export type ClipPlayStopTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStopTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStopTiming }
export type ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

export type ClipPlayStopTimingOverrideAfterRecording =
	ClipPlayStopTimingOverrideAfterRecording_Inherit
	| ClipPlayStopTimingOverrideAfterRecording_Override
	| ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming

export type ClipPlayStartTimingOverride = {
	value: ClipPlayStartTiming,
}

export type ClipPlayStopTimingOverride = {
	value: ClipPlayStopTiming,
}

export type EvenQuantization = {
	numerator: number,
	denominator: number,
}

export type ColumnId = string

export type RowId = string

export type SlotId = string

export type ClipId = string

export type MatrixSequenceId = string

export type Column = {
	id: ColumnId?,
	name: string?,
	clip_play_settings: ColumnClipPlaySettings,
	clip_record_settings: ColumnClipRecordSettings,
	slots: { Slot }?,
}

export type ColumnSettings = {
	name: string?,
	clip_play_settings: ColumnClipPlaySettings,
	clip_record_settings: ColumnClipRecordSettings,
}

export type ColumnClipPlaySettings = {
	follows_scene: boolean,
	exclusive: boolean,
	track: TrackId?,
	start_timing: ClipPlayStartTiming?,
	stop_timing: ClipPlayStopTiming?,
	audio_settings: ColumnClipPlayAudioSettings,
}

export type ColumnClipRecordSettings = {
	origin: RecordOrigin,
	track: TrackId?,
}

export type ColumnClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode?,
	time_stretch_mode: AudioTimeStretchMode?,
	cache_behavior: AudioCacheBehavior?,
}

export type Row = {
	id: RowId?,
	name: string?,
	tempo: Bpm?,
	time_signature: TimeSignature?,
}

export type AudioTimeStretchMode_VariSpeed = { kind: "VariSpeed" }
export type AudioTimeStretchMode_KeepingPitch = { kind: "KeepingPitch", mode: VirtualTimeStretchMode }

export type AudioTimeStretchMode = AudioTimeStretchMode_VariSpeed | AudioTimeStretchMode_KeepingPitch

export type VirtualResampleMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualResampleMode_ReaperMode = { kind: "ReaperMode", mode: number }

export type VirtualResampleMode = VirtualResampleMode_ProjectDefault | VirtualResampleMode_ReaperMode

export type ReaperResampleMode = {
	mode: number,
}

export type TimeStretchMode = {
	mode: VirtualTimeStretchMode,
}

export type VirtualTimeStretchMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualTimeStretchMode_ReaperMode = { kind: "ReaperMode", mode: number, sub_mode: number }

export type VirtualTimeStretchMode = VirtualTimeStretchMode_ProjectDefault | VirtualTimeStretchMode_ReaperMode

export type ReaperPitchShiftMode = {
	mode: number,
	sub_mode: number,
}

export type RecordOrigin_TrackInput = { kind: "TrackInput" }
export type RecordOrigin_TrackAudioOutput = { kind: "TrackAudioOutput" }
export type RecordOrigin_FxAudioInput = { kind: "FxAudioInput", first_channel_index: number, channel_count: number }

export type RecordOrigin = RecordOrigin_TrackInput | RecordOrigin_TrackAudioOutput | RecordOrigin_FxAudioInput

export type SourceOrigin = "Normal" | "Frozen"

export type ChannelRange = {
	first_channel_index: number,
	channel_count: number,
}

export type Slot = {
	id: SlotId?,
	row: number,
	clip_old: Clip?,
	clips: { Clip }?,
}

export type Clip = {
	id: ClipId?,
	name: string?,
	source: Source,
	frozen_source: Source?,
	active_source: SourceOrigin?,
	time_base: ClipTimeBase,
	start_timing: ClipPlayStartTiming?,
	stop_timing: ClipPlayStopTiming?,
	looped: boolean,
	volume: Db,
	color: ClipColor,
	dynamic_section: Section?,
	fixed_section: Section,
	audio_settings: ClipAudioSettings,
	midi_settings: ClipMidiSettings,
}

export type ClipAudioSettings = {
	apply_source_fades: boolean,
	time_stretch_mode: AudioTimeStretchMode?,
	resample_mode: VirtualResampleMode?,
	cache_behavior: AudioCacheBehavior?,
	original_tempo: Bpm?,
}

export type ClipMidiSettings = {
	source_reset_settings: MidiResetMessageRange,
	section_reset_settings: MidiResetMessageRange,
	loop_reset_settings: MidiResetMessageRange,
	interaction_reset_settings: MidiResetMessageRange,
}

export type MidiResetMessageRange = {
	left: MidiResetMessages,
	right: MidiResetMessages,
}

export type MidiResetMessages = {
	on_notes_off: boolean?,
	all_notes_off: boolean,
	all_sound_off: boolean,
	reset_all_controllers: boolean,
	damper_pedal_off: boolean,
}

export type Section = {
	start_pos: PositiveSecond,
	length: PositiveSecond?,
}

export type AudioCacheBehavior_DirectFromDisk = { kind: "DirectFromDisk" }
export type AudioCacheBehavior_CacheInMemory = { kind: "CacheInMemory" }

export type AudioCacheBehavior = AudioCacheBehavior_DirectFromDisk | AudioCacheBehavior_CacheInMemory

export type ClipColor_PlayTrackColor = { kind: "PlayTrackColor" }
export type ClipColor_CustomColor = { kind: "CustomColor", value: RgbColor }
export type ClipColor_PaletteColor = { kind: "PaletteColor", index: number }

export type ClipColor = ClipColor_PlayTrackColor | ClipColor_CustomColor | ClipColor_PaletteColor

export type CustomClipColor = {
	value: RgbColor,
}

export type PaletteClipColor = {
	index: number,
}

export type Source_File = { kind: "File", path: string }
export type Source_MidiChunk = { kind: "MidiChunk", chunk: string }

export type Source = Source_File | Source_MidiChunk

export type FileSource = {
	path: string,
}

export type MidiChunkSource = {
	chunk: string,
}

export type ClipTimeBase_Time = { kind: "Time" }
export type ClipTimeBase_Beat = { kind: "Beat", audio_tempo: Bpm?, time_signature: TimeSignature, downbeat: PositiveBeat }

export type ClipTimeBase = ClipTimeBase_Time | ClipTimeBase_Beat

export type BeatTimeBase = {
	audio_tempo: Bpm?,
	time_signature: TimeSignature,
	downbeat: PositiveBeat,
}

export type ColorPalette = {
	entries: { ColorPaletteEntry },
}

export type ColorPaletteEntry = {
	color: RgbColor,
}

export type TimeSignature = {
	numerator: number,
	denominator: number,
}

export type TrackId = string

export type Bpm = number

export type PositiveSecond = number

export type PositiveBeat = number

export type Db = number

export type RgbColor = { number }

export type ColumnAddress = {
	index: number,
}

export type RowAddress = {
	index: number,
}

export type SlotAddress = {
	column_index: number,
	row_index: number,
}

return module

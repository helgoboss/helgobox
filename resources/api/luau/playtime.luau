--!strict

--- Attention: This file is generated from Rust code! Don't modify it directly!

local module = {}

export type PlaytimePersistenceRoot = {
	matrix: Matrix,
	even_quantization: EvenQuantization,
}
--- Creates a PlaytimePersistenceRoot value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.PlaytimePersistenceRoot(value: PlaytimePersistenceRoot): PlaytimePersistenceRoot
	return value
end

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
--- Creates a Matrix value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Matrix(value: Matrix): Matrix
	return value
end

export type MatrixSequencer = {
	sequences: { MatrixSequence }?,
	active_sequence: MatrixSequenceId?,
}
--- Creates a MatrixSequencer value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequencer(value: MatrixSequencer): MatrixSequencer
	return value
end

export type MatrixSequence = {
	id: MatrixSequenceId,
	info: MatrixSequenceInfo,
	data: MatrixSequenceData,
}
--- Creates a MatrixSequence value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequence(value: MatrixSequence): MatrixSequence
	return value
end

export type MatrixSequenceInfo = {
	name: string?,
	created_at: string,
}
--- Creates a MatrixSequenceInfo value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceInfo(value: MatrixSequenceInfo): MatrixSequenceInfo
	return value
end

export type MatrixSequenceData = {
	ppq: number,
	count_in: number,
	events: { MatrixSequenceEvent },
}
--- Creates a MatrixSequenceData value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceData(value: MatrixSequenceData): MatrixSequenceData
	return value
end

export type MatrixSequenceEvent = {
	pulse_diff: number,
	message: MatrixSequenceMessage,
}
--- Creates a MatrixSequenceEvent value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceEvent(value: MatrixSequenceEvent): MatrixSequenceEvent
	return value
end

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
--- Creates a MatrixSequenceColumnMessage value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceColumnMessage(value: MatrixSequenceColumnMessage): MatrixSequenceColumnMessage
	return value
end

export type MatrixSequenceRowMessage = {
	index: number,
}
--- Creates a MatrixSequenceRowMessage value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceRowMessage(value: MatrixSequenceRowMessage): MatrixSequenceRowMessage
	return value
end

export type MatrixSequenceSlotMessage = {
	column_index: number,
	row_index: number,
}
--- Creates a MatrixSequenceSlotMessage value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSequenceSlotMessage(value: MatrixSequenceSlotMessage): MatrixSequenceSlotMessage
	return value
end

export type ContentQuantizationSettings = {
	quantization: EvenQuantization,
}
--- Creates a ContentQuantizationSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ContentQuantizationSettings(value: ContentQuantizationSettings): ContentQuantizationSettings
	return value
end

export type SignedMatrix = {
	matrix: string,
	signature: string,
}
--- Creates a SignedMatrix value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.SignedMatrix(value: SignedMatrix): SignedMatrix
	return value
end

export type MatrixSettings = {
	clip_play_settings: MatrixClipPlaySettings,
	clip_record_settings: MatrixClipRecordSettings,
	common_tempo_range: TempoRange,
	color_palette: ColorPalette,
	content_quantization_settings: ContentQuantizationSettings,
}
--- Creates a MatrixSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixSettings(value: MatrixSettings): MatrixSettings
	return value
end

export type TempoRange = {
	min: Bpm,
	max: Bpm,
}
--- Creates a TempoRange value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.TempoRange(value: TempoRange): TempoRange
	return value
end

export type MatrixClipPlaySettings = {
	trigger_behavior: TriggerPlayBehavior?,
	start_timing: ClipPlayStartTiming,
	stop_timing: ClipPlayStopTiming,
	audio_settings: MatrixClipPlayAudioSettings,
}
--- Creates a MatrixClipPlaySettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixClipPlaySettings(value: MatrixClipPlaySettings): MatrixClipPlaySettings
	return value
end

export type MatrixClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode,
	time_stretch_mode: AudioTimeStretchMode,
	cache_behavior: AudioCacheBehavior,
}
--- Creates a MatrixClipPlayAudioSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixClipPlayAudioSettings(value: MatrixClipPlayAudioSettings): MatrixClipPlayAudioSettings
	return value
end

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
--- Creates a MatrixClipRecordSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixClipRecordSettings(value: MatrixClipRecordSettings): MatrixClipRecordSettings
	return value
end

export type MatrixClipRecordMidiSettings = {
	record_mode: MidiClipRecordMode,
	detect_downbeat: boolean,
	detect_input: boolean,
	auto_quantize: boolean,
	clip_settings: ClipMidiSettings?,
}
--- Creates a MatrixClipRecordMidiSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixClipRecordMidiSettings(value: MatrixClipRecordMidiSettings): MatrixClipRecordMidiSettings
	return value
end

export type MatrixClipRecordAudioSettings = {
	detect_downbeat: boolean,
	detect_input: boolean,
}
--- Creates a MatrixClipRecordAudioSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MatrixClipRecordAudioSettings(value: MatrixClipRecordAudioSettings): MatrixClipRecordAudioSettings
	return value
end

export type RecordLength_OpenEnd = { kind: "OpenEnd" }
export type RecordLength_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type RecordLength = RecordLength_OpenEnd | RecordLength_Quantized

--- A helper table to create RecordLength values of different kinds.
module.RecordLength = {}

--- Creates a RecordLength of kind OpenEnd.
function module.RecordLength.OpenEnd(): RecordLength_OpenEnd
	return {
		kind = "OpenEnd",
	}
end

--- Creates a RecordLength of kind Quantized.
function module.RecordLength.Quantized(value: EvenQuantization): RecordLength_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

export type ClipRecordTimeBase_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }
export type ClipRecordTimeBase_Time = { kind: "Time" }
export type ClipRecordTimeBase_Beat = { kind: "Beat" }

export type ClipRecordTimeBase =
	ClipRecordTimeBase_DeriveFromRecordTiming
	| ClipRecordTimeBase_Time
	| ClipRecordTimeBase_Beat

--- A helper table to create ClipRecordTimeBase values of different kinds.
module.ClipRecordTimeBase = {}

--- Creates a ClipRecordTimeBase of kind DeriveFromRecordTiming.
function module.ClipRecordTimeBase.DeriveFromRecordTiming(): ClipRecordTimeBase_DeriveFromRecordTiming
	return {
		kind = "DeriveFromRecordTiming",
	}
end

--- Creates a ClipRecordTimeBase of kind Time.
function module.ClipRecordTimeBase.Time(): ClipRecordTimeBase_Time
	return {
		kind = "Time",
	}
end

--- Creates a ClipRecordTimeBase of kind Beat.
function module.ClipRecordTimeBase.Beat(): ClipRecordTimeBase_Beat
	return {
		kind = "Beat",
	}
end

export type TriggerPlayBehavior_TogglePlayStop = { kind: "TogglePlayStop" }
export type TriggerPlayBehavior_MomentaryPlayStop = { kind: "MomentaryPlayStop" }
export type TriggerPlayBehavior_Retrigger = { kind: "Retrigger" }

export type TriggerPlayBehavior =
	TriggerPlayBehavior_TogglePlayStop
	| TriggerPlayBehavior_MomentaryPlayStop
	| TriggerPlayBehavior_Retrigger

--- A helper table to create TriggerPlayBehavior values of different kinds.
module.TriggerPlayBehavior = {}

--- Creates a TriggerPlayBehavior of kind TogglePlayStop.
function module.TriggerPlayBehavior.TogglePlayStop(): TriggerPlayBehavior_TogglePlayStop
	return {
		kind = "TogglePlayStop",
	}
end

--- Creates a TriggerPlayBehavior of kind MomentaryPlayStop.
function module.TriggerPlayBehavior.MomentaryPlayStop(): TriggerPlayBehavior_MomentaryPlayStop
	return {
		kind = "MomentaryPlayStop",
	}
end

--- Creates a TriggerPlayBehavior of kind Retrigger.
function module.TriggerPlayBehavior.Retrigger(): TriggerPlayBehavior_Retrigger
	return {
		kind = "Retrigger",
	}
end

export type ClipRecordStartTiming_LikeClipPlayStartTiming = { kind: "LikeClipPlayStartTiming" }
export type ClipRecordStartTiming_Immediately = { kind: "Immediately" }
export type ClipRecordStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipRecordStartTiming =
	ClipRecordStartTiming_LikeClipPlayStartTiming
	| ClipRecordStartTiming_Immediately
	| ClipRecordStartTiming_Quantized

--- A helper table to create ClipRecordStartTiming values of different kinds.
module.ClipRecordStartTiming = {}

--- Creates a ClipRecordStartTiming of kind LikeClipPlayStartTiming.
function module.ClipRecordStartTiming.LikeClipPlayStartTiming(): ClipRecordStartTiming_LikeClipPlayStartTiming
	return {
		kind = "LikeClipPlayStartTiming",
	}
end

--- Creates a ClipRecordStartTiming of kind Immediately.
function module.ClipRecordStartTiming.Immediately(): ClipRecordStartTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipRecordStartTiming of kind Quantized.
function module.ClipRecordStartTiming.Quantized(value: EvenQuantization): ClipRecordStartTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

export type ClipRecordStopTiming_LikeClipRecordStartTiming = { kind: "LikeClipRecordStartTiming" }
export type ClipRecordStopTiming_Immediately = { kind: "Immediately" }
export type ClipRecordStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipRecordStopTiming =
	ClipRecordStopTiming_LikeClipRecordStartTiming
	| ClipRecordStopTiming_Immediately
	| ClipRecordStopTiming_Quantized

--- A helper table to create ClipRecordStopTiming values of different kinds.
module.ClipRecordStopTiming = {}

--- Creates a ClipRecordStopTiming of kind LikeClipRecordStartTiming.
function module.ClipRecordStopTiming.LikeClipRecordStartTiming(): ClipRecordStopTiming_LikeClipRecordStartTiming
	return {
		kind = "LikeClipRecordStartTiming",
	}
end

--- Creates a ClipRecordStopTiming of kind Immediately.
function module.ClipRecordStopTiming.Immediately(): ClipRecordStopTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipRecordStopTiming of kind Quantized.
function module.ClipRecordStopTiming.Quantized(value: EvenQuantization): ClipRecordStopTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

export type MidiClipRecordMode_Normal = { kind: "Normal" }
export type MidiClipRecordMode_Overdub = { kind: "Overdub" }
export type MidiClipRecordMode_Replace = { kind: "Replace" }

export type MidiClipRecordMode = MidiClipRecordMode_Normal | MidiClipRecordMode_Overdub | MidiClipRecordMode_Replace

--- A helper table to create MidiClipRecordMode values of different kinds.
module.MidiClipRecordMode = {}

--- Creates a MidiClipRecordMode of kind Normal.
function module.MidiClipRecordMode.Normal(): MidiClipRecordMode_Normal
	return {
		kind = "Normal",
	}
end

--- Creates a MidiClipRecordMode of kind Overdub.
function module.MidiClipRecordMode.Overdub(): MidiClipRecordMode_Overdub
	return {
		kind = "Overdub",
	}
end

--- Creates a MidiClipRecordMode of kind Replace.
function module.MidiClipRecordMode.Replace(): MidiClipRecordMode_Replace
	return {
		kind = "Replace",
	}
end

export type ClipPlayStartTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

export type ClipPlayStartTiming = ClipPlayStartTiming_Immediately | ClipPlayStartTiming_Quantized

--- A helper table to create ClipPlayStartTiming values of different kinds.
module.ClipPlayStartTiming = {}

--- Creates a ClipPlayStartTiming of kind Immediately.
function module.ClipPlayStartTiming.Immediately(): ClipPlayStartTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipPlayStartTiming of kind Quantized.
function module.ClipPlayStartTiming.Quantized(value: EvenQuantization): ClipPlayStartTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

export type ClipPlayStopTiming_LikeClipStartTiming = { kind: "LikeClipStartTiming" }
export type ClipPlayStopTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }
export type ClipPlayStopTiming_UntilEndOfClip = { kind: "UntilEndOfClip" }

export type ClipPlayStopTiming =
	ClipPlayStopTiming_LikeClipStartTiming
	| ClipPlayStopTiming_Immediately
	| ClipPlayStopTiming_Quantized
	| ClipPlayStopTiming_UntilEndOfClip

--- A helper table to create ClipPlayStopTiming values of different kinds.
module.ClipPlayStopTiming = {}

--- Creates a ClipPlayStopTiming of kind LikeClipStartTiming.
function module.ClipPlayStopTiming.LikeClipStartTiming(): ClipPlayStopTiming_LikeClipStartTiming
	return {
		kind = "LikeClipStartTiming",
	}
end

--- Creates a ClipPlayStopTiming of kind Immediately.
function module.ClipPlayStopTiming.Immediately(): ClipPlayStopTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipPlayStopTiming of kind Quantized.
function module.ClipPlayStopTiming.Quantized(value: EvenQuantization): ClipPlayStopTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

--- Creates a ClipPlayStopTiming of kind UntilEndOfClip.
function module.ClipPlayStopTiming.UntilEndOfClip(): ClipPlayStopTiming_UntilEndOfClip
	return {
		kind = "UntilEndOfClip",
	}
end

export type ResolvedClipPlayStopTiming = "Immediately" | "Quantized" | "UntilEndOfClip"

export type ClipPlayStartTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStartTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStartTiming }
export type ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

export type ClipPlayStartTimingOverrideAfterRecording =
	ClipPlayStartTimingOverrideAfterRecording_Inherit
	| ClipPlayStartTimingOverrideAfterRecording_Override
	| ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming

--- A helper table to create ClipPlayStartTimingOverrideAfterRecording values of different kinds.
module.ClipPlayStartTimingOverrideAfterRecording = {}

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind Inherit.
function module.ClipPlayStartTimingOverrideAfterRecording.Inherit(): ClipPlayStartTimingOverrideAfterRecording_Inherit
	return {
		kind = "Inherit",
	}
end

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind Override.
function module.ClipPlayStartTimingOverrideAfterRecording.Override(
	value: ClipPlayStartTimingOverride
): ClipPlayStartTimingOverrideAfterRecording_Override
	local t: any = table.clone(value)
	t.kind = "Override"
	return t
end

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind DeriveFromRecordTiming.
function module.ClipPlayStartTimingOverrideAfterRecording.DeriveFromRecordTiming(
): ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming
	return {
		kind = "DeriveFromRecordTiming",
	}
end

export type ClipPlayStopTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStopTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStopTiming }
export type ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

export type ClipPlayStopTimingOverrideAfterRecording =
	ClipPlayStopTimingOverrideAfterRecording_Inherit
	| ClipPlayStopTimingOverrideAfterRecording_Override
	| ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming

--- A helper table to create ClipPlayStopTimingOverrideAfterRecording values of different kinds.
module.ClipPlayStopTimingOverrideAfterRecording = {}

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind Inherit.
function module.ClipPlayStopTimingOverrideAfterRecording.Inherit(): ClipPlayStopTimingOverrideAfterRecording_Inherit
	return {
		kind = "Inherit",
	}
end

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind Override.
function module.ClipPlayStopTimingOverrideAfterRecording.Override(
	value: ClipPlayStopTimingOverride
): ClipPlayStopTimingOverrideAfterRecording_Override
	local t: any = table.clone(value)
	t.kind = "Override"
	return t
end

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind DeriveFromRecordTiming.
function module.ClipPlayStopTimingOverrideAfterRecording.DeriveFromRecordTiming(
): ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming
	return {
		kind = "DeriveFromRecordTiming",
	}
end

export type ClipPlayStartTimingOverride = {
	value: ClipPlayStartTiming,
}
--- Creates a ClipPlayStartTimingOverride value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ClipPlayStartTimingOverride(value: ClipPlayStartTimingOverride): ClipPlayStartTimingOverride
	return value
end

export type ClipPlayStopTimingOverride = {
	value: ClipPlayStopTiming,
}
--- Creates a ClipPlayStopTimingOverride value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ClipPlayStopTimingOverride(value: ClipPlayStopTimingOverride): ClipPlayStopTimingOverride
	return value
end

export type EvenQuantization = {
	numerator: number,
	denominator: number,
}
--- Creates a EvenQuantization value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.EvenQuantization(value: EvenQuantization): EvenQuantization
	return value
end

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
--- Creates a Column value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Column(value: Column): Column
	return value
end

export type ColumnSettings = {
	name: string?,
	clip_play_settings: ColumnClipPlaySettings,
	clip_record_settings: ColumnClipRecordSettings,
}
--- Creates a ColumnSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColumnSettings(value: ColumnSettings): ColumnSettings
	return value
end

export type ColumnClipPlaySettings = {
	follows_scene: boolean,
	exclusive: boolean,
	track: TrackId?,
	start_timing: ClipPlayStartTiming?,
	stop_timing: ClipPlayStopTiming?,
	audio_settings: ColumnClipPlayAudioSettings,
}
--- Creates a ColumnClipPlaySettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColumnClipPlaySettings(value: ColumnClipPlaySettings): ColumnClipPlaySettings
	return value
end

export type ColumnClipRecordSettings = {
	origin: RecordOrigin,
	track: TrackId?,
}
--- Creates a ColumnClipRecordSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColumnClipRecordSettings(value: ColumnClipRecordSettings): ColumnClipRecordSettings
	return value
end

export type ColumnClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode?,
	time_stretch_mode: AudioTimeStretchMode?,
	cache_behavior: AudioCacheBehavior?,
}
--- Creates a ColumnClipPlayAudioSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColumnClipPlayAudioSettings(value: ColumnClipPlayAudioSettings): ColumnClipPlayAudioSettings
	return value
end

export type Row = {
	id: RowId?,
	name: string?,
	tempo: Bpm?,
	time_signature: TimeSignature?,
}
--- Creates a Row value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Row(value: Row): Row
	return value
end

export type AudioTimeStretchMode_VariSpeed = { kind: "VariSpeed" }
export type AudioTimeStretchMode_KeepingPitch = { kind: "KeepingPitch", mode: VirtualTimeStretchMode }

export type AudioTimeStretchMode = AudioTimeStretchMode_VariSpeed | AudioTimeStretchMode_KeepingPitch

--- A helper table to create AudioTimeStretchMode values of different kinds.
module.AudioTimeStretchMode = {}

--- Creates a AudioTimeStretchMode of kind VariSpeed.
function module.AudioTimeStretchMode.VariSpeed(): AudioTimeStretchMode_VariSpeed
	return {
		kind = "VariSpeed",
	}
end

--- Creates a AudioTimeStretchMode of kind KeepingPitch.
function module.AudioTimeStretchMode.KeepingPitch(value: TimeStretchMode): AudioTimeStretchMode_KeepingPitch
	local t: any = table.clone(value)
	t.kind = "KeepingPitch"
	return t
end

export type VirtualResampleMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualResampleMode_ReaperMode = { kind: "ReaperMode", mode: number }

export type VirtualResampleMode = VirtualResampleMode_ProjectDefault | VirtualResampleMode_ReaperMode

--- A helper table to create VirtualResampleMode values of different kinds.
module.VirtualResampleMode = {}

--- Creates a VirtualResampleMode of kind ProjectDefault.
function module.VirtualResampleMode.ProjectDefault(): VirtualResampleMode_ProjectDefault
	return {
		kind = "ProjectDefault",
	}
end

--- Creates a VirtualResampleMode of kind ReaperMode.
function module.VirtualResampleMode.ReaperMode(value: ReaperResampleMode): VirtualResampleMode_ReaperMode
	local t: any = table.clone(value)
	t.kind = "ReaperMode"
	return t
end

export type ReaperResampleMode = {
	mode: number,
}
--- Creates a ReaperResampleMode value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ReaperResampleMode(value: ReaperResampleMode): ReaperResampleMode
	return value
end

export type TimeStretchMode = {
	mode: VirtualTimeStretchMode,
}
--- Creates a TimeStretchMode value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.TimeStretchMode(value: TimeStretchMode): TimeStretchMode
	return value
end

export type VirtualTimeStretchMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualTimeStretchMode_ReaperMode = { kind: "ReaperMode", mode: number, sub_mode: number }

export type VirtualTimeStretchMode = VirtualTimeStretchMode_ProjectDefault | VirtualTimeStretchMode_ReaperMode

--- A helper table to create VirtualTimeStretchMode values of different kinds.
module.VirtualTimeStretchMode = {}

--- Creates a VirtualTimeStretchMode of kind ProjectDefault.
function module.VirtualTimeStretchMode.ProjectDefault(): VirtualTimeStretchMode_ProjectDefault
	return {
		kind = "ProjectDefault",
	}
end

--- Creates a VirtualTimeStretchMode of kind ReaperMode.
function module.VirtualTimeStretchMode.ReaperMode(value: ReaperPitchShiftMode): VirtualTimeStretchMode_ReaperMode
	local t: any = table.clone(value)
	t.kind = "ReaperMode"
	return t
end

export type ReaperPitchShiftMode = {
	mode: number,
	sub_mode: number,
}
--- Creates a ReaperPitchShiftMode value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ReaperPitchShiftMode(value: ReaperPitchShiftMode): ReaperPitchShiftMode
	return value
end

export type RecordOrigin_TrackInput = { kind: "TrackInput" }
export type RecordOrigin_TrackAudioOutput = { kind: "TrackAudioOutput" }
export type RecordOrigin_FxAudioInput = { kind: "FxAudioInput", first_channel_index: number, channel_count: number }

export type RecordOrigin = RecordOrigin_TrackInput | RecordOrigin_TrackAudioOutput | RecordOrigin_FxAudioInput

--- A helper table to create RecordOrigin values of different kinds.
module.RecordOrigin = {}

--- Creates a RecordOrigin of kind TrackInput.
function module.RecordOrigin.TrackInput(): RecordOrigin_TrackInput
	return {
		kind = "TrackInput",
	}
end

--- Creates a RecordOrigin of kind TrackAudioOutput.
function module.RecordOrigin.TrackAudioOutput(): RecordOrigin_TrackAudioOutput
	return {
		kind = "TrackAudioOutput",
	}
end

--- Creates a RecordOrigin of kind FxAudioInput.
function module.RecordOrigin.FxAudioInput(value: ChannelRange): RecordOrigin_FxAudioInput
	local t: any = table.clone(value)
	t.kind = "FxAudioInput"
	return t
end

export type SourceOrigin = "Normal" | "Frozen"

export type ChannelRange = {
	first_channel_index: number,
	channel_count: number,
}
--- Creates a ChannelRange value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ChannelRange(value: ChannelRange): ChannelRange
	return value
end

export type Slot = {
	id: SlotId?,
	row: number,
	clip_old: Clip?,
	clips: { Clip }?,
}
--- Creates a Slot value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Slot(value: Slot): Slot
	return value
end

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
--- Creates a Clip value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Clip(value: Clip): Clip
	return value
end

export type ClipAudioSettings = {
	apply_source_fades: boolean,
	time_stretch_mode: AudioTimeStretchMode?,
	resample_mode: VirtualResampleMode?,
	cache_behavior: AudioCacheBehavior?,
	original_tempo: Bpm?,
}
--- Creates a ClipAudioSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ClipAudioSettings(value: ClipAudioSettings): ClipAudioSettings
	return value
end

export type ClipMidiSettings = {
	source_reset_settings: MidiResetMessageRange,
	section_reset_settings: MidiResetMessageRange,
	loop_reset_settings: MidiResetMessageRange,
	interaction_reset_settings: MidiResetMessageRange,
}
--- Creates a ClipMidiSettings value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ClipMidiSettings(value: ClipMidiSettings): ClipMidiSettings
	return value
end

export type MidiResetMessageRange = {
	left: MidiResetMessages,
	right: MidiResetMessages,
}
--- Creates a MidiResetMessageRange value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MidiResetMessageRange(value: MidiResetMessageRange): MidiResetMessageRange
	return value
end

export type MidiResetMessages = {
	on_notes_off: boolean?,
	all_notes_off: boolean,
	all_sound_off: boolean,
	reset_all_controllers: boolean,
	damper_pedal_off: boolean,
}
--- Creates a MidiResetMessages value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MidiResetMessages(value: MidiResetMessages): MidiResetMessages
	return value
end

export type Section = {
	start_pos: PositiveSecond,
	length: PositiveSecond?,
}
--- Creates a Section value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.Section(value: Section): Section
	return value
end

export type AudioCacheBehavior_DirectFromDisk = { kind: "DirectFromDisk" }
export type AudioCacheBehavior_CacheInMemory = { kind: "CacheInMemory" }

export type AudioCacheBehavior = AudioCacheBehavior_DirectFromDisk | AudioCacheBehavior_CacheInMemory

--- A helper table to create AudioCacheBehavior values of different kinds.
module.AudioCacheBehavior = {}

--- Creates a AudioCacheBehavior of kind DirectFromDisk.
function module.AudioCacheBehavior.DirectFromDisk(): AudioCacheBehavior_DirectFromDisk
	return {
		kind = "DirectFromDisk",
	}
end

--- Creates a AudioCacheBehavior of kind CacheInMemory.
function module.AudioCacheBehavior.CacheInMemory(): AudioCacheBehavior_CacheInMemory
	return {
		kind = "CacheInMemory",
	}
end

export type ClipColor_PlayTrackColor = { kind: "PlayTrackColor" }
export type ClipColor_CustomColor = { kind: "CustomColor", value: RgbColor }
export type ClipColor_PaletteColor = { kind: "PaletteColor", index: number }

export type ClipColor = ClipColor_PlayTrackColor | ClipColor_CustomColor | ClipColor_PaletteColor

--- A helper table to create ClipColor values of different kinds.
module.ClipColor = {}

--- Creates a ClipColor of kind PlayTrackColor.
function module.ClipColor.PlayTrackColor(): ClipColor_PlayTrackColor
	return {
		kind = "PlayTrackColor",
	}
end

--- Creates a ClipColor of kind CustomColor.
function module.ClipColor.CustomColor(value: CustomClipColor): ClipColor_CustomColor
	local t: any = table.clone(value)
	t.kind = "CustomColor"
	return t
end

--- Creates a ClipColor of kind PaletteColor.
function module.ClipColor.PaletteColor(value: PaletteClipColor): ClipColor_PaletteColor
	local t: any = table.clone(value)
	t.kind = "PaletteColor"
	return t
end

export type CustomClipColor = {
	value: RgbColor,
}
--- Creates a CustomClipColor value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.CustomClipColor(value: CustomClipColor): CustomClipColor
	return value
end

export type PaletteClipColor = {
	index: number,
}
--- Creates a PaletteClipColor value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.PaletteClipColor(value: PaletteClipColor): PaletteClipColor
	return value
end

export type Source_File = { kind: "File", path: string }
export type Source_MidiChunk = { kind: "MidiChunk", chunk: string }

export type Source = Source_File | Source_MidiChunk

--- A helper table to create Source values of different kinds.
module.Source = {}

--- Creates a Source of kind File.
function module.Source.File(value: FileSource): Source_File
	local t: any = table.clone(value)
	t.kind = "File"
	return t
end

--- Creates a Source of kind MidiChunk.
function module.Source.MidiChunk(value: MidiChunkSource): Source_MidiChunk
	local t: any = table.clone(value)
	t.kind = "MidiChunk"
	return t
end

export type FileSource = {
	path: string,
}
--- Creates a FileSource value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.FileSource(value: FileSource): FileSource
	return value
end

export type MidiChunkSource = {
	chunk: string,
}
--- Creates a MidiChunkSource value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.MidiChunkSource(value: MidiChunkSource): MidiChunkSource
	return value
end

export type ClipTimeBase_Time = { kind: "Time" }
export type ClipTimeBase_Beat = { kind: "Beat", audio_tempo: Bpm?, time_signature: TimeSignature, downbeat: PositiveBeat }

export type ClipTimeBase = ClipTimeBase_Time | ClipTimeBase_Beat

--- A helper table to create ClipTimeBase values of different kinds.
module.ClipTimeBase = {}

--- Creates a ClipTimeBase of kind Time.
function module.ClipTimeBase.Time(): ClipTimeBase_Time
	return {
		kind = "Time",
	}
end

--- Creates a ClipTimeBase of kind Beat.
function module.ClipTimeBase.Beat(value: BeatTimeBase): ClipTimeBase_Beat
	local t: any = table.clone(value)
	t.kind = "Beat"
	return t
end

export type BeatTimeBase = {
	audio_tempo: Bpm?,
	time_signature: TimeSignature,
	downbeat: PositiveBeat,
}
--- Creates a BeatTimeBase value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.BeatTimeBase(value: BeatTimeBase): BeatTimeBase
	return value
end

export type ColorPalette = {
	entries: { ColorPaletteEntry },
}
--- Creates a ColorPalette value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColorPalette(value: ColorPalette): ColorPalette
	return value
end

export type ColorPaletteEntry = {
	color: RgbColor,
}
--- Creates a ColorPaletteEntry value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColorPaletteEntry(value: ColorPaletteEntry): ColorPaletteEntry
	return value
end

export type TimeSignature = {
	numerator: number,
	denominator: number,
}
--- Creates a TimeSignature value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.TimeSignature(value: TimeSignature): TimeSignature
	return value
end

export type TrackId = string

export type Bpm = number

export type PositiveSecond = number

export type PositiveBeat = number

export type Db = number

export type RgbColor = { number }

export type ColumnAddress = {
	index: number,
}
--- Creates a ColumnAddress value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.ColumnAddress(value: ColumnAddress): ColumnAddress
	return value
end

export type RowAddress = {
	index: number,
}
--- Creates a RowAddress value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.RowAddress(value: RowAddress): RowAddress
	return value
end

export type SlotAddress = {
	column_index: number,
	row_index: number,
}
--- Creates a SlotAddress value.
---
--- This simply returns the given table, enabling type checking on the
--- way.
function module.SlotAddress(value: SlotAddress): SlotAddress
	return value
end

return module

--!strict

--- Attention: This file is generated from Rust code! Don't modify it directly!

--- Contains types and helper functions for building Playtime presets
local module = {}

--- Global settings that apply to all Playtime instances.
---
--- This is different from the app settings (which are also global) in that the app settings are only about GUI
--- aspects (less important) and managed by the app. Whereas the engine settings configure settings that
--- apply even without GUI, and they are managed by the engine.
export type PlaytimeSettings = {
	tempo_latency: TempoLatency?,
}
--- Creates a PlaytimeSettings value.
--- Global settings that apply to all Playtime instances.
---
--- This is different from the app settings (which are also global) in that the app settings are only about GUI
--- aspects (less important) and managed by the app. Whereas the engine settings configure settings that
--- apply even without GUI, and they are managed by the engine.
function module.PlaytimeSettings(value: PlaytimeSettings): PlaytimeSettings
	return value
end

export type TempoLatency_Auto = { kind: "Auto" }

export type TempoLatency_Manual = { kind: "Manual", millis: number }
export type TempoLatency = TempoLatency_Auto | TempoLatency_Manual

--- A type that represents all possible kinds of TempoLatency.
export type TempoLatencyKind = "Auto" | "Manual"

--- Helper table to create TempoLatency values of different kinds.
module.TempoLatency = {}

--- Creates a TempoLatency of kind Auto.
function module.TempoLatency.Auto(): TempoLatency_Auto
	return {
		kind = "Auto",
	}
end

--- Creates a TempoLatency of kind Manual.
function module.TempoLatency.Manual(value: ManualTempoLatency): TempoLatency_Manual
	local t: any = table.clone(value)
	t.kind = "Manual"
	return t
end

export type ManualTempoLatency = {
	millis: number,
}
--- Creates a ManualTempoLatency value.
function module.ManualTempoLatency(value: ManualTempoLatency): ManualTempoLatency
	return value
end

export type Matrix = {
	columns: { Column }?,
	rows: { Row }?,
	clip_play_settings: MatrixClipPlaySettings,
	clip_record_settings: MatrixClipRecordSettings,
	transport_sync_mode: TransportSyncMode?,
	common_tempo_range: TempoRange,
	activate_slot_on_trigger: boolean?,
	click_volume: number?,
	color_palette: ColorPalette?,
	content_quantization_settings: ContentQuantizationSettings?,
	sequencer: MatrixSequencer?,
	unknown_props: BTreeMap<string, any>?,
}
--- Creates a Matrix value.
function module.Matrix(value: Matrix): Matrix
	return value
end

export type TransportSyncMode = "Partial" | "Full"

export type MatrixSequencer = {
	sequences: { MatrixSequence }?,
	active_sequence: MatrixSequenceId?,
}
--- Creates a MatrixSequencer value.
function module.MatrixSequencer(value: MatrixSequencer): MatrixSequencer
	return value
end

export type MatrixSequence = {
	id: MatrixSequenceId,
	info: MatrixSequenceInfo,
	data: MatrixSequenceData,
}
--- Creates a MatrixSequence value.
function module.MatrixSequence(value: MatrixSequence): MatrixSequence
	return value
end

export type MatrixSequenceInfo = {
	name: string?,
	created_at: string,
}
--- Creates a MatrixSequenceInfo value.
function module.MatrixSequenceInfo(value: MatrixSequenceInfo): MatrixSequenceInfo
	return value
end

export type MatrixSequenceData = {
	ppq: number,
	count_in: number,
	events: { MatrixSequenceEvent },
}
--- Creates a MatrixSequenceData value.
function module.MatrixSequenceData(value: MatrixSequenceData): MatrixSequenceData
	return value
end

export type MatrixSequenceEvent = {
	pulse_diff: number,
	message: MatrixSequenceMessage,
}
--- Creates a MatrixSequenceEvent value.
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
function module.MatrixSequenceColumnMessage(value: MatrixSequenceColumnMessage): MatrixSequenceColumnMessage
	return value
end

export type MatrixSequenceRowMessage = {
	index: number,
}
--- Creates a MatrixSequenceRowMessage value.
function module.MatrixSequenceRowMessage(value: MatrixSequenceRowMessage): MatrixSequenceRowMessage
	return value
end

export type MatrixSequenceSlotMessage = {
	column_index: number,
	row_index: number,
}
--- Creates a MatrixSequenceSlotMessage value.
function module.MatrixSequenceSlotMessage(value: MatrixSequenceSlotMessage): MatrixSequenceSlotMessage
	return value
end

export type MatrixSequenceStartSlotMessage = {
	column_index: number,
	row_index: number,
	velocity: number,
}
--- Creates a MatrixSequenceStartSlotMessage value.
function module.MatrixSequenceStartSlotMessage(value: MatrixSequenceStartSlotMessage): MatrixSequenceStartSlotMessage
	return value
end

export type ContentQuantizationSettings = {
	quantization: EvenQuantization,
}
--- Creates a ContentQuantizationSettings value.
function module.ContentQuantizationSettings(value: ContentQuantizationSettings): ContentQuantizationSettings
	return value
end

export type SignedMatrix = {
	matrix: string,
	signature: string,
}
--- Creates a SignedMatrix value.
function module.SignedMatrix(value: SignedMatrix): SignedMatrix
	return value
end

--- This is redundant (already contained in [`Matrix`]) but used to transfer settings only.
export type MatrixSettings = {
	clip_play_settings: MatrixClipPlaySettings,
	clip_record_settings: MatrixClipRecordSettings,
	common_tempo_range: TempoRange,
	color_palette: ColorPalette,
	content_quantization_settings: ContentQuantizationSettings,
	activate_slot_on_trigger: boolean,
	transport_sync_mode: TransportSyncMode,
}
--- Creates a MatrixSettings value.
--- This is redundant (already contained in [`Matrix`]) but used to transfer settings only.
function module.MatrixSettings(value: MatrixSettings): MatrixSettings
	return value
end

export type TempoRange = {
	min: number,
	max: number,
}
--- Creates a TempoRange value.
function module.TempoRange(value: TempoRange): TempoRange
	return value
end

--- Matrix-global settings related to playing clips.
export type MatrixClipPlaySettings = {
	trigger_behavior: TriggerSlotBehavior?,
	start_timing: ClipPlayStartTiming,
	stop_timing: ClipPlayStopTiming,
	audio_settings: MatrixClipPlayAudioSettings,
	velocity_sensitivity: number?,
}
--- Creates a MatrixClipPlaySettings value.
--- Matrix-global settings related to playing clips.
function module.MatrixClipPlaySettings(value: MatrixClipPlaySettings): MatrixClipPlaySettings
	return value
end

export type MatrixClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode,
	time_stretch_mode: AudioTimeStretchMode,
	cache_behavior: AudioCacheBehavior,
}
--- Creates a MatrixClipPlayAudioSettings value.
function module.MatrixClipPlayAudioSettings(value: MatrixClipPlayAudioSettings): MatrixClipPlayAudioSettings
	return value
end

--- Matrix-global settings related to recording clips.
export type MatrixClipRecordSettings = {
	start_timing: ClipRecordStartTiming,
	stop_timing: ClipRecordStopTiming,
	length_mode: RecordLengthMode?,
	custom_length: EvenQuantization?,
	play_start_timing: ClipPlayStartTimingOverrideAfterRecording,
	play_stop_timing: ClipPlayStopTimingOverrideAfterRecording,
	time_base: ClipRecordTimeBase,
	looped: boolean,
	midi_settings: MatrixClipRecordMidiSettings,
	audio_settings: MatrixClipRecordAudioSettings,
}
--- Creates a MatrixClipRecordSettings value.
--- Matrix-global settings related to recording clips.
function module.MatrixClipRecordSettings(value: MatrixClipRecordSettings): MatrixClipRecordSettings
	return value
end

export type RecordLengthMode = "OpenEnd" | "CustomLength"

export type MatrixClipRecordMidiSettings = {
	record_mode: MidiClipRecordMode,
	detect_downbeat: boolean,
	detect_input: boolean,
	auto_quantize: boolean,
	clip_settings: ClipMidiSettings?,
}
--- Creates a MatrixClipRecordMidiSettings value.
function module.MatrixClipRecordMidiSettings(value: MatrixClipRecordMidiSettings): MatrixClipRecordMidiSettings
	return value
end

export type MatrixClipRecordAudioSettings = {
	detect_downbeat: boolean,
	detect_input: boolean,
}
--- Creates a MatrixClipRecordAudioSettings value.
function module.MatrixClipRecordAudioSettings(value: MatrixClipRecordAudioSettings): MatrixClipRecordAudioSettings
	return value
end

--- Derives the time base of the resulting clip from the clip start timing.
export type ClipRecordTimeBase_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

--- Sets the time base of the recorded clip to [`ClipTimeBase::Time`].
export type ClipRecordTimeBase_Time = { kind: "Time" }

--- Sets the time base of the recorded clip to [`ClipTimeBase::Beat`].
export type ClipRecordTimeBase_Beat = { kind: "Beat" }
export type ClipRecordTimeBase =
	ClipRecordTimeBase_DeriveFromRecordTiming
	| ClipRecordTimeBase_Time
	| ClipRecordTimeBase_Beat

--- A type that represents all possible kinds of ClipRecordTimeBase.
export type ClipRecordTimeBaseKind = "DeriveFromRecordTiming" | "Time" | "Beat"

--- Helper table to create ClipRecordTimeBase values of different kinds.
module.ClipRecordTimeBase = {}

--- Creates a ClipRecordTimeBase of kind DeriveFromRecordTiming.
--- Derives the time base of the resulting clip from the clip start timing.
function module.ClipRecordTimeBase.DeriveFromRecordTiming(): ClipRecordTimeBase_DeriveFromRecordTiming
	return {
		kind = "DeriveFromRecordTiming",
	}
end

--- Creates a ClipRecordTimeBase of kind Time.
--- Sets the time base of the recorded clip to [`ClipTimeBase::Time`].
function module.ClipRecordTimeBase.Time(): ClipRecordTimeBase_Time
	return {
		kind = "Time",
	}
end

--- Creates a ClipRecordTimeBase of kind Beat.
--- Sets the time base of the recorded clip to [`ClipTimeBase::Beat`].
function module.ClipRecordTimeBase.Beat(): ClipRecordTimeBase_Beat
	return {
		kind = "Beat",
	}
end

--- Press once = play, press again = stop.
export type TriggerSlotBehavior_TogglePlayStop = { kind: "TogglePlayStop" }

--- Press once = play, release = stop.
export type TriggerSlotBehavior_MomentaryPlayStop = { kind: "MomentaryPlayStop" }

--- Press once = play, press again = retrigger.
export type TriggerSlotBehavior_Retrigger = { kind: "Retrigger" }
export type TriggerSlotBehavior =
	TriggerSlotBehavior_TogglePlayStop
	| TriggerSlotBehavior_MomentaryPlayStop
	| TriggerSlotBehavior_Retrigger

--- A type that represents all possible kinds of TriggerSlotBehavior.
export type TriggerSlotBehaviorKind = "TogglePlayStop" | "MomentaryPlayStop" | "Retrigger"

--- Helper table to create TriggerSlotBehavior values of different kinds.
module.TriggerSlotBehavior = {}

--- Creates a TriggerSlotBehavior of kind TogglePlayStop.
--- Press once = play, press again = stop.
function module.TriggerSlotBehavior.TogglePlayStop(): TriggerSlotBehavior_TogglePlayStop
	return {
		kind = "TogglePlayStop",
	}
end

--- Creates a TriggerSlotBehavior of kind MomentaryPlayStop.
--- Press once = play, release = stop.
function module.TriggerSlotBehavior.MomentaryPlayStop(): TriggerSlotBehavior_MomentaryPlayStop
	return {
		kind = "MomentaryPlayStop",
	}
end

--- Creates a TriggerSlotBehavior of kind Retrigger.
--- Press once = play, press again = retrigger.
function module.TriggerSlotBehavior.Retrigger(): TriggerSlotBehavior_Retrigger
	return {
		kind = "Retrigger",
	}
end

--- Uses the inherited clip play start timing (from column or matrix).
export type ClipRecordStartTiming_LikeClipPlayStartTiming = { kind: "LikeClipPlayStartTiming" }

--- Starts recording immediately.
export type ClipRecordStartTiming_Immediately = { kind: "Immediately" }

--- Starts recording according to the given quantization.
export type ClipRecordStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }
export type ClipRecordStartTiming =
	ClipRecordStartTiming_LikeClipPlayStartTiming
	| ClipRecordStartTiming_Immediately
	| ClipRecordStartTiming_Quantized

--- A type that represents all possible kinds of ClipRecordStartTiming.
export type ClipRecordStartTimingKind = "LikeClipPlayStartTiming" | "Immediately" | "Quantized"

--- Helper table to create ClipRecordStartTiming values of different kinds.
module.ClipRecordStartTiming = {}

--- Creates a ClipRecordStartTiming of kind LikeClipPlayStartTiming.
--- Uses the inherited clip play start timing (from column or matrix).
function module.ClipRecordStartTiming.LikeClipPlayStartTiming(): ClipRecordStartTiming_LikeClipPlayStartTiming
	return {
		kind = "LikeClipPlayStartTiming",
	}
end

--- Creates a ClipRecordStartTiming of kind Immediately.
--- Starts recording immediately.
function module.ClipRecordStartTiming.Immediately(): ClipRecordStartTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipRecordStartTiming of kind Quantized.
--- Starts recording according to the given quantization.
function module.ClipRecordStartTiming.Quantized(value: EvenQuantization): ClipRecordStartTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

--- Uses the record start timing.
export type ClipRecordStopTiming_LikeClipRecordStartTiming = { kind: "LikeClipRecordStartTiming" }

--- Stops recording immediately.
export type ClipRecordStopTiming_Immediately = { kind: "Immediately" }

--- Stops recording according to the given quantization.
export type ClipRecordStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }
export type ClipRecordStopTiming =
	ClipRecordStopTiming_LikeClipRecordStartTiming
	| ClipRecordStopTiming_Immediately
	| ClipRecordStopTiming_Quantized

--- A type that represents all possible kinds of ClipRecordStopTiming.
export type ClipRecordStopTimingKind = "LikeClipRecordStartTiming" | "Immediately" | "Quantized"

--- Helper table to create ClipRecordStopTiming values of different kinds.
module.ClipRecordStopTiming = {}

--- Creates a ClipRecordStopTiming of kind LikeClipRecordStartTiming.
--- Uses the record start timing.
function module.ClipRecordStopTiming.LikeClipRecordStartTiming(): ClipRecordStopTiming_LikeClipRecordStartTiming
	return {
		kind = "LikeClipRecordStartTiming",
	}
end

--- Creates a ClipRecordStopTiming of kind Immediately.
--- Stops recording immediately.
function module.ClipRecordStopTiming.Immediately(): ClipRecordStopTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipRecordStopTiming of kind Quantized.
--- Stops recording according to the given quantization.
function module.ClipRecordStopTiming.Quantized(value: EvenQuantization): ClipRecordStopTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

--- Creates an empty clip and records MIDI material in it.
export type MidiClipRecordMode_Normal = { kind: "Normal" }

--- Records more material onto an existing clip, leaving existing material in place.
export type MidiClipRecordMode_Overdub = { kind: "Overdub" }

--- Records more material onto an existing clip, overwriting existing material.
export type MidiClipRecordMode_Replace = { kind: "Replace" }
export type MidiClipRecordMode = MidiClipRecordMode_Normal | MidiClipRecordMode_Overdub | MidiClipRecordMode_Replace

--- A type that represents all possible kinds of MidiClipRecordMode.
export type MidiClipRecordModeKind = "Normal" | "Overdub" | "Replace"

--- Helper table to create MidiClipRecordMode values of different kinds.
module.MidiClipRecordMode = {}

--- Creates a MidiClipRecordMode of kind Normal.
--- Creates an empty clip and records MIDI material in it.
function module.MidiClipRecordMode.Normal(): MidiClipRecordMode_Normal
	return {
		kind = "Normal",
	}
end

--- Creates a MidiClipRecordMode of kind Overdub.
--- Records more material onto an existing clip, leaving existing material in place.
function module.MidiClipRecordMode.Overdub(): MidiClipRecordMode_Overdub
	return {
		kind = "Overdub",
	}
end

--- Creates a MidiClipRecordMode of kind Replace.
--- Records more material onto an existing clip, overwriting existing material.
function module.MidiClipRecordMode.Replace(): MidiClipRecordMode_Replace
	return {
		kind = "Replace",
	}
end

--- Starts playing immediately.
export type ClipPlayStartTiming_Immediately = { kind: "Immediately" }

--- Starts playing according to the given quantization.
export type ClipPlayStartTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }
export type ClipPlayStartTiming = ClipPlayStartTiming_Immediately | ClipPlayStartTiming_Quantized

--- A type that represents all possible kinds of ClipPlayStartTiming.
export type ClipPlayStartTimingKind = "Immediately" | "Quantized"

--- Helper table to create ClipPlayStartTiming values of different kinds.
module.ClipPlayStartTiming = {}

--- Creates a ClipPlayStartTiming of kind Immediately.
--- Starts playing immediately.
function module.ClipPlayStartTiming.Immediately(): ClipPlayStartTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipPlayStartTiming of kind Quantized.
--- Starts playing according to the given quantization.
function module.ClipPlayStartTiming.Quantized(value: EvenQuantization): ClipPlayStartTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

--- Uses the play start timing.
export type ClipPlayStopTiming_LikeClipStartTiming = { kind: "LikeClipStartTiming" }

--- Stops playing immediately.
export type ClipPlayStopTiming_Immediately = { kind: "Immediately" }

--- Stops playing according to the given quantization.
export type ClipPlayStopTiming_Quantized = { kind: "Quantized", numerator: number, denominator: number }

--- Keeps playing until the end of the clip.
export type ClipPlayStopTiming_UntilEndOfClip = { kind: "UntilEndOfClip" }
export type ClipPlayStopTiming =
	ClipPlayStopTiming_LikeClipStartTiming
	| ClipPlayStopTiming_Immediately
	| ClipPlayStopTiming_Quantized
	| ClipPlayStopTiming_UntilEndOfClip

--- A type that represents all possible kinds of ClipPlayStopTiming.
export type ClipPlayStopTimingKind = "LikeClipStartTiming" | "Immediately" | "Quantized" | "UntilEndOfClip"

--- Helper table to create ClipPlayStopTiming values of different kinds.
module.ClipPlayStopTiming = {}

--- Creates a ClipPlayStopTiming of kind LikeClipStartTiming.
--- Uses the play start timing.
function module.ClipPlayStopTiming.LikeClipStartTiming(): ClipPlayStopTiming_LikeClipStartTiming
	return {
		kind = "LikeClipStartTiming",
	}
end

--- Creates a ClipPlayStopTiming of kind Immediately.
--- Stops playing immediately.
function module.ClipPlayStopTiming.Immediately(): ClipPlayStopTiming_Immediately
	return {
		kind = "Immediately",
	}
end

--- Creates a ClipPlayStopTiming of kind Quantized.
--- Stops playing according to the given quantization.
function module.ClipPlayStopTiming.Quantized(value: EvenQuantization): ClipPlayStopTiming_Quantized
	local t: any = table.clone(value)
	t.kind = "Quantized"
	return t
end

--- Creates a ClipPlayStopTiming of kind UntilEndOfClip.
--- Keeps playing until the end of the clip.
function module.ClipPlayStopTiming.UntilEndOfClip(): ClipPlayStopTiming_UntilEndOfClip
	return {
		kind = "UntilEndOfClip",
	}
end

export type ResolvedClipPlayStopTiming = "Immediately" | "Quantized" | "UntilEndOfClip"

--- Doesn't apply any override.
export type ClipPlayStartTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }

--- Overrides the setting with the given value.
export type ClipPlayStartTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStartTiming }

--- Derives the setting from the record timing.
---
--- If the record timing is set to the global clip timing, that means it will not apply any
--- override. If it's set to something specific, it will apply the appropriate override.
export type ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }
export type ClipPlayStartTimingOverrideAfterRecording =
	ClipPlayStartTimingOverrideAfterRecording_Inherit
	| ClipPlayStartTimingOverrideAfterRecording_Override
	| ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming

--- A type that represents all possible kinds of ClipPlayStartTimingOverrideAfterRecording.
export type ClipPlayStartTimingOverrideAfterRecordingKind = "Inherit" | "Override" | "DeriveFromRecordTiming"

--- Helper table to create ClipPlayStartTimingOverrideAfterRecording values of different kinds.
module.ClipPlayStartTimingOverrideAfterRecording = {}

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind Inherit.
--- Doesn't apply any override.
function module.ClipPlayStartTimingOverrideAfterRecording.Inherit(): ClipPlayStartTimingOverrideAfterRecording_Inherit
	return {
		kind = "Inherit",
	}
end

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind Override.
--- Overrides the setting with the given value.
function module.ClipPlayStartTimingOverrideAfterRecording.Override(
	value: ClipPlayStartTimingOverride
): ClipPlayStartTimingOverrideAfterRecording_Override
	local t: any = table.clone(value)
	t.kind = "Override"
	return t
end

--- Creates a ClipPlayStartTimingOverrideAfterRecording of kind DeriveFromRecordTiming.
--- Derives the setting from the record timing.
---
--- If the record timing is set to the global clip timing, that means it will not apply any
--- override. If it's set to something specific, it will apply the appropriate override.
function module.ClipPlayStartTimingOverrideAfterRecording.DeriveFromRecordTiming(
): ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming
	return {
		kind = "DeriveFromRecordTiming",
	}
end

--- Doesn't apply any override.
export type ClipPlayStopTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }

--- Overrides the setting with the given value.
export type ClipPlayStopTimingOverrideAfterRecording_Override = { kind: "Override", value: ClipPlayStopTiming }

--- Derives the setting from the record timing.
---
--- If the record timing is set to the global clip timing, that means it will not apply any
--- override. If it's set to something specific, it will apply the appropriate override.
export type ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }
export type ClipPlayStopTimingOverrideAfterRecording =
	ClipPlayStopTimingOverrideAfterRecording_Inherit
	| ClipPlayStopTimingOverrideAfterRecording_Override
	| ClipPlayStopTimingOverrideAfterRecording_DeriveFromRecordTiming

--- A type that represents all possible kinds of ClipPlayStopTimingOverrideAfterRecording.
export type ClipPlayStopTimingOverrideAfterRecordingKind = "Inherit" | "Override" | "DeriveFromRecordTiming"

--- Helper table to create ClipPlayStopTimingOverrideAfterRecording values of different kinds.
module.ClipPlayStopTimingOverrideAfterRecording = {}

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind Inherit.
--- Doesn't apply any override.
function module.ClipPlayStopTimingOverrideAfterRecording.Inherit(): ClipPlayStopTimingOverrideAfterRecording_Inherit
	return {
		kind = "Inherit",
	}
end

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind Override.
--- Overrides the setting with the given value.
function module.ClipPlayStopTimingOverrideAfterRecording.Override(
	value: ClipPlayStopTimingOverride
): ClipPlayStopTimingOverrideAfterRecording_Override
	local t: any = table.clone(value)
	t.kind = "Override"
	return t
end

--- Creates a ClipPlayStopTimingOverrideAfterRecording of kind DeriveFromRecordTiming.
--- Derives the setting from the record timing.
---
--- If the record timing is set to the global clip timing, that means it will not apply any
--- override. If it's set to something specific, it will apply the appropriate override.
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
function module.ClipPlayStartTimingOverride(value: ClipPlayStartTimingOverride): ClipPlayStartTimingOverride
	return value
end

export type ClipPlayStopTimingOverride = {
	value: ClipPlayStopTiming,
}
--- Creates a ClipPlayStopTimingOverride value.
function module.ClipPlayStopTimingOverride(value: ClipPlayStopTimingOverride): ClipPlayStopTimingOverride
	return value
end

--- An even quantization.
---
--- "Even" in the sense that it's not swing or dotted.
export type EvenQuantization = {
	numerator: number,
	denominator: number,
}
--- Creates a EvenQuantization value.
--- An even quantization.
---
--- "Even" in the sense that it's not swing or dotted.
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
function module.Column(value: Column): Column
	return value
end

export type ColumnSettings = {
	name: string?,
	clip_play_settings: ColumnClipPlaySettings,
	clip_record_settings: ColumnClipRecordSettings,
}
--- Creates a ColumnSettings value.
function module.ColumnSettings(value: ColumnSettings): ColumnSettings
	return value
end

export type ColumnClipPlaySettings = {
	follows_scene: boolean,
	exclusive: boolean,
	track: TrackId?,
	start_timing: ClipPlayStartTiming?,
	stop_timing: ClipPlayStopTiming?,
	trigger_behavior: TriggerSlotBehavior?,
	velocity_sensitivity: number?,
	audio_settings: ColumnClipPlayAudioSettings,
}
--- Creates a ColumnClipPlaySettings value.
function module.ColumnClipPlaySettings(value: ColumnClipPlaySettings): ColumnClipPlaySettings
	return value
end

export type ColumnClipRecordSettings = {
	origin: RecordOrigin,
	track: TrackId?,
}
--- Creates a ColumnClipRecordSettings value.
function module.ColumnClipRecordSettings(value: ColumnClipRecordSettings): ColumnClipRecordSettings
	return value
end

export type ColumnClipPlayAudioSettings = {
	resample_mode: VirtualResampleMode?,
	time_stretch_mode: AudioTimeStretchMode?,
	cache_behavior: AudioCacheBehavior?,
}
--- Creates a ColumnClipPlayAudioSettings value.
function module.ColumnClipPlayAudioSettings(value: ColumnClipPlayAudioSettings): ColumnClipPlayAudioSettings
	return value
end

--- A row represents a complete row in a matrix.
---
--- A scene is a very related concept and sometimes used interchangeably with row because there's
--- a one-to-one relationship between a row and a scene.
---
--- The difference between row and scene is of conceptual nature: A scene represents a part of a
--- song that's played exclusively whereas a row is just a row in the clip matrix. This distinction
--- results in some practical differences:
---
--- - A column can be configured to not follow scenes. The clips in that column are of
---   course still structured in rows, but they are not part of the scenes anymore.
--- - In practice, this means that whenever you launch the scene, the clips in that independent
---   column are not launched. Or when you clear the scene, the slots in that column are not
---   cleared.
--- - Whenever you read "Scene", it will only affect the columns that are configured to follow
---   scenes. Whenever you read "Row", it will affect the complete matrix row, no matter the
---   column type.
export type Row = {
	id: RowId?,
	name: string?,
	tempo: number?,
	time_signature: TimeSignature?,
}
--- Creates a Row value.
--- A row represents a complete row in a matrix.
---
--- A scene is a very related concept and sometimes used interchangeably with row because there's
--- a one-to-one relationship between a row and a scene.
---
--- The difference between row and scene is of conceptual nature: A scene represents a part of a
--- song that's played exclusively whereas a row is just a row in the clip matrix. This distinction
--- results in some practical differences:
---
--- - A column can be configured to not follow scenes. The clips in that column are of
---   course still structured in rows, but they are not part of the scenes anymore.
--- - In practice, this means that whenever you launch the scene, the clips in that independent
---   column are not launched. Or when you clear the scene, the slots in that column are not
---   cleared.
--- - Whenever you read "Scene", it will only affect the columns that are configured to follow
---   scenes. Whenever you read "Row", it will affect the complete matrix row, no matter the
---   column type.
function module.Row(value: Row): Row
	return value
end

--- Doesn't just stretch/squeeze the material but also changes the pitch.
---
--- Comparatively fast.
export type AudioTimeStretchMode_VariSpeed = { kind: "VariSpeed" }

--- Applies a real time-stretch algorithm to the material which keeps the pitch.
---
--- Comparatively slow.
export type AudioTimeStretchMode_KeepingPitch = { kind: "KeepingPitch", mode: VirtualTimeStretchMode }
export type AudioTimeStretchMode = AudioTimeStretchMode_VariSpeed | AudioTimeStretchMode_KeepingPitch

--- A type that represents all possible kinds of AudioTimeStretchMode.
export type AudioTimeStretchModeKind = "VariSpeed" | "KeepingPitch"

--- Helper table to create AudioTimeStretchMode values of different kinds.
module.AudioTimeStretchMode = {}

--- Creates a AudioTimeStretchMode of kind VariSpeed.
--- Doesn't just stretch/squeeze the material but also changes the pitch.
---
--- Comparatively fast.
function module.AudioTimeStretchMode.VariSpeed(): AudioTimeStretchMode_VariSpeed
	return {
		kind = "VariSpeed",
	}
end

--- Creates a AudioTimeStretchMode of kind KeepingPitch.
--- Applies a real time-stretch algorithm to the material which keeps the pitch.
---
--- Comparatively slow.
function module.AudioTimeStretchMode.KeepingPitch(value: TimeStretchMode): AudioTimeStretchMode_KeepingPitch
	local t: any = table.clone(value)
	t.kind = "KeepingPitch"
	return t
end

--- Uses the resample mode set as default for this REAPER project.
export type VirtualResampleMode_ProjectDefault = { kind: "ProjectDefault" }

--- Uses a specific resample mode.
export type VirtualResampleMode_ReaperMode = { kind: "ReaperMode", mode: number }
export type VirtualResampleMode = VirtualResampleMode_ProjectDefault | VirtualResampleMode_ReaperMode

--- A type that represents all possible kinds of VirtualResampleMode.
export type VirtualResampleModeKind = "ProjectDefault" | "ReaperMode"

--- Helper table to create VirtualResampleMode values of different kinds.
module.VirtualResampleMode = {}

--- Creates a VirtualResampleMode of kind ProjectDefault.
--- Uses the resample mode set as default for this REAPER project.
function module.VirtualResampleMode.ProjectDefault(): VirtualResampleMode_ProjectDefault
	return {
		kind = "ProjectDefault",
	}
end

--- Creates a VirtualResampleMode of kind ReaperMode.
--- Uses a specific resample mode.
function module.VirtualResampleMode.ReaperMode(value: ReaperResampleMode): VirtualResampleMode_ReaperMode
	local t: any = table.clone(value)
	t.kind = "ReaperMode"
	return t
end

export type ReaperResampleMode = {
	mode: number,
}
--- Creates a ReaperResampleMode value.
function module.ReaperResampleMode(value: ReaperResampleMode): ReaperResampleMode
	return value
end

export type TimeStretchMode = {
	mode: VirtualTimeStretchMode,
}
--- Creates a TimeStretchMode value.
function module.TimeStretchMode(value: TimeStretchMode): TimeStretchMode
	return value
end

--- Uses the pitch shift mode set as default for this REAPER project.
export type VirtualTimeStretchMode_ProjectDefault = { kind: "ProjectDefault" }

--- Uses a specific REAPER pitch shift mode.
export type VirtualTimeStretchMode_ReaperMode = { kind: "ReaperMode", mode: number, sub_mode: number }
export type VirtualTimeStretchMode = VirtualTimeStretchMode_ProjectDefault | VirtualTimeStretchMode_ReaperMode

--- A type that represents all possible kinds of VirtualTimeStretchMode.
export type VirtualTimeStretchModeKind = "ProjectDefault" | "ReaperMode"

--- Helper table to create VirtualTimeStretchMode values of different kinds.
module.VirtualTimeStretchMode = {}

--- Creates a VirtualTimeStretchMode of kind ProjectDefault.
--- Uses the pitch shift mode set as default for this REAPER project.
function module.VirtualTimeStretchMode.ProjectDefault(): VirtualTimeStretchMode_ProjectDefault
	return {
		kind = "ProjectDefault",
	}
end

--- Creates a VirtualTimeStretchMode of kind ReaperMode.
--- Uses a specific REAPER pitch shift mode.
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
function module.ReaperPitchShiftMode(value: ReaperPitchShiftMode): ReaperPitchShiftMode
	return value
end

--- Records using the hardware input set for the track (MIDI or stereo).
export type RecordOrigin_TrackInput = { kind: "TrackInput" }

--- Captures audio from the output of the track.
export type RecordOrigin_TrackAudioOutput = { kind: "TrackAudioOutput" }

--- Records audio flowing into the FX input.
export type RecordOrigin_FxAudioInput = { kind: "FxAudioInput", first_channel_index: number, channel_count: number }
export type RecordOrigin = RecordOrigin_TrackInput | RecordOrigin_TrackAudioOutput | RecordOrigin_FxAudioInput

--- A type that represents all possible kinds of RecordOrigin.
export type RecordOriginKind = "TrackInput" | "TrackAudioOutput" | "FxAudioInput"

--- Helper table to create RecordOrigin values of different kinds.
module.RecordOrigin = {}

--- Creates a RecordOrigin of kind TrackInput.
--- Records using the hardware input set for the track (MIDI or stereo).
function module.RecordOrigin.TrackInput(): RecordOrigin_TrackInput
	return {
		kind = "TrackInput",
	}
end

--- Creates a RecordOrigin of kind TrackAudioOutput.
--- Captures audio from the output of the track.
function module.RecordOrigin.TrackAudioOutput(): RecordOrigin_TrackAudioOutput
	return {
		kind = "TrackAudioOutput",
	}
end

--- Creates a RecordOrigin of kind FxAudioInput.
--- Records audio flowing into the FX input.
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
function module.ChannelRange(value: ChannelRange): ChannelRange
	return value
end

export type Slot = {
	id: SlotId?,
	row: number,
	clip_old: Clip?,
	clips: { Clip }?,
	ignited: boolean?,
}
--- Creates a Slot value.
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
	velocity_sensitivity: number?,
	looped: boolean,
	volume: number,
	color: ClipColor,
	dynamic_section: Section?,
	fixed_section: Section,
	pitch_shift: Semitones?,
	audio_settings: ClipAudioSettings,
	midi_settings: ClipMidiSettings,
}
--- Creates a Clip value.
function module.Clip(value: Clip): Clip
	return value
end

export type ClipAudioSettings = {
	apply_source_fades: boolean,
	time_stretch_mode: AudioTimeStretchMode?,
	resample_mode: VirtualResampleMode?,
	cache_behavior: AudioCacheBehavior?,
	original_tempo: number?,
}
--- Creates a ClipAudioSettings value.
function module.ClipAudioSettings(value: ClipAudioSettings): ClipAudioSettings
	return value
end

export type ClipMidiSettings = {
	destination_channel: number?,
	reset_settings: ClipMidiResetSettings?,
}
--- Creates a ClipMidiSettings value.
function module.ClipMidiSettings(value: ClipMidiSettings): ClipMidiSettings
	return value
end

export type ClipMidiResetSettings = {
	interaction_reset_settings: MidiResetMessageRange,
	section_reset_settings: MidiResetMessageRange,
	source_reset_settings: MidiResetMessageRange,
}
--- Creates a ClipMidiResetSettings value.
function module.ClipMidiResetSettings(value: ClipMidiResetSettings): ClipMidiResetSettings
	return value
end

export type MidiResetMessageRange = {
	left: MidiResetMessages,
	right: MidiResetMessages,
}
--- Creates a MidiResetMessageRange value.
function module.MidiResetMessageRange(value: MidiResetMessageRange): MidiResetMessageRange
	return value
end

export type MidiResetMessages = {
	on_notes_off: boolean?,
	all_notes_off: boolean,
	all_sound_off: boolean,
	reset_all_controllers: boolean,
	on_damper_pedal_off: boolean,
}
--- Creates a MidiResetMessages value.
function module.MidiResetMessages(value: MidiResetMessages): MidiResetMessages
	return value
end

export type Section = {
	start_pos: number,
	length: number?,
}
--- Creates a Section value.
function module.Section(value: Section): Section
	return value
end

--- Loads directly from the disk.
---
--- Might still pre-buffer some blocks but definitely won't put the complete audio data into
--- memory.
export type AudioCacheBehavior_DirectFromDisk = { kind: "DirectFromDisk" }

--- Loads the complete audio data into memory.
export type AudioCacheBehavior_CacheInMemory = { kind: "CacheInMemory" }
export type AudioCacheBehavior = AudioCacheBehavior_DirectFromDisk | AudioCacheBehavior_CacheInMemory

--- A type that represents all possible kinds of AudioCacheBehavior.
export type AudioCacheBehaviorKind = "DirectFromDisk" | "CacheInMemory"

--- Helper table to create AudioCacheBehavior values of different kinds.
module.AudioCacheBehavior = {}

--- Creates a AudioCacheBehavior of kind DirectFromDisk.
--- Loads directly from the disk.
---
--- Might still pre-buffer some blocks but definitely won't put the complete audio data into
--- memory.
function module.AudioCacheBehavior.DirectFromDisk(): AudioCacheBehavior_DirectFromDisk
	return {
		kind = "DirectFromDisk",
	}
end

--- Creates a AudioCacheBehavior of kind CacheInMemory.
--- Loads the complete audio data into memory.
function module.AudioCacheBehavior.CacheInMemory(): AudioCacheBehavior_CacheInMemory
	return {
		kind = "CacheInMemory",
	}
end

--- Inherits the color of the column's play track.
export type ClipColor_PlayTrackColor = { kind: "PlayTrackColor" }

--- Assigns a very specific custom color.
export type ClipColor_CustomColor = { kind: "CustomColor", value: RgbColor }

--- Uses a certain color from a palette.
export type ClipColor_PaletteColor = { kind: "PaletteColor", index: number }
export type ClipColor = ClipColor_PlayTrackColor | ClipColor_CustomColor | ClipColor_PaletteColor

--- A type that represents all possible kinds of ClipColor.
export type ClipColorKind = "PlayTrackColor" | "CustomColor" | "PaletteColor"

--- Helper table to create ClipColor values of different kinds.
module.ClipColor = {}

--- Creates a ClipColor of kind PlayTrackColor.
--- Inherits the color of the column's play track.
function module.ClipColor.PlayTrackColor(): ClipColor_PlayTrackColor
	return {
		kind = "PlayTrackColor",
	}
end

--- Creates a ClipColor of kind CustomColor.
--- Assigns a very specific custom color.
function module.ClipColor.CustomColor(value: CustomClipColor): ClipColor_CustomColor
	local t: any = table.clone(value)
	t.kind = "CustomColor"
	return t
end

--- Creates a ClipColor of kind PaletteColor.
--- Uses a certain color from a palette.
function module.ClipColor.PaletteColor(value: PaletteClipColor): ClipColor_PaletteColor
	local t: any = table.clone(value)
	t.kind = "PaletteColor"
	return t
end

export type CustomClipColor = {
	value: RgbColor,
}
--- Creates a CustomClipColor value.
function module.CustomClipColor(value: CustomClipColor): CustomClipColor
	return value
end

export type PaletteClipColor = {
	index: number,
}
--- Creates a PaletteClipColor value.
function module.PaletteClipColor(value: PaletteClipColor): PaletteClipColor
	return value
end

--- Takes content from a media file on the file system (audio).
export type Source_File = { kind: "File", path: string }

--- Embedded MIDI data.
export type Source_MidiChunk = { kind: "MidiChunk", chunk: string }
export type Source = Source_File | Source_MidiChunk

--- A type that represents all possible kinds of Source.
export type SourceKind = "File" | "MidiChunk"

--- Helper table to create Source values of different kinds.
module.Source = {}

--- Creates a Source of kind File.
--- Takes content from a media file on the file system (audio).
function module.Source.File(value: FileSource): Source_File
	local t: any = table.clone(value)
	t.kind = "File"
	return t
end

--- Creates a Source of kind MidiChunk.
--- Embedded MIDI data.
function module.Source.MidiChunk(value: MidiChunkSource): Source_MidiChunk
	local t: any = table.clone(value)
	t.kind = "MidiChunk"
	return t
end

export type FileSource = {
	path: string,
}
--- Creates a FileSource value.
function module.FileSource(value: FileSource): FileSource
	return value
end

export type MidiChunkSource = {
	chunk: string,
}
--- Creates a MidiChunkSource value.
function module.MidiChunkSource(value: MidiChunkSource): MidiChunkSource
	return value
end

--- Material which doesn't need to be adjusted to the current tempo.
export type ClipTimeBase_Time = { kind: "Time" }

--- Material which needs to be adjusted to the current tempo.
export type ClipTimeBase_Beat = { kind: "Beat", audio_tempo: number?, time_signature: TimeSignature, downbeat: number }
--- Decides if the clip will be adjusted to the current tempo.
export type ClipTimeBase = ClipTimeBase_Time | ClipTimeBase_Beat

--- A type that represents all possible kinds of ClipTimeBase.
export type ClipTimeBaseKind = "Time" | "Beat"

--- Helper table to create ClipTimeBase values of different kinds.
--- Decides if the clip will be adjusted to the current tempo.
module.ClipTimeBase = {}

--- Creates a ClipTimeBase of kind Time.
--- Material which doesn't need to be adjusted to the current tempo.
function module.ClipTimeBase.Time(): ClipTimeBase_Time
	return {
		kind = "Time",
	}
end

--- Creates a ClipTimeBase of kind Beat.
--- Material which needs to be adjusted to the current tempo.
function module.ClipTimeBase.Beat(value: BeatTimeBase): ClipTimeBase_Beat
	local t: any = table.clone(value)
	t.kind = "Beat"
	return t
end

export type BeatTimeBase = {
	audio_tempo: number?,
	time_signature: TimeSignature,
	downbeat: number,
}
--- Creates a BeatTimeBase value.
function module.BeatTimeBase(value: BeatTimeBase): BeatTimeBase
	return value
end

export type ColorPalette = {
	entries: { ColorPaletteEntry },
}
--- Creates a ColorPalette value.
function module.ColorPalette(value: ColorPalette): ColorPalette
	return value
end

export type ColorPaletteEntry = {
	color: RgbColor,
}
--- Creates a ColorPaletteEntry value.
function module.ColorPaletteEntry(value: ColorPaletteEntry): ColorPaletteEntry
	return value
end

export type TimeSignature = {
	numerator: number,
	denominator: number,
}
--- Creates a TimeSignature value.
function module.TimeSignature(value: TimeSignature): TimeSignature
	return value
end

export type TrackId = string

export type RgbColor = { number }

export type ColumnAddress = {
	index: number,
}
--- Creates a ColumnAddress value.
function module.ColumnAddress(value: ColumnAddress): ColumnAddress
	return value
end

export type RowAddress = {
	index: number,
}
--- Creates a RowAddress value.
function module.RowAddress(value: RowAddress): RowAddress
	return value
end

export type SlotAddress = {
	column_index: number,
	row_index: number,
}
--- Creates a SlotAddress value.
function module.SlotAddress(value: SlotAddress): SlotAddress
	return value
end

return module

--!strict

--- Attention: This file is generated from Rust code! Don't modify it directly!

local playtime = require("playtime")

local module = {}

export type CompartmentContent = {
	default_group: Group?,
	parameters: { Parameter }?,
	groups: { Group }?,
	mappings: { Mapping }?,
	custom_data: { [string]: any }?,
	notes: string?,
}

export type Glue = {
	absolute_mode: AbsoluteMode?,
	source_interval: Interval<number>?,
	target_interval: Interval<number>?,
	reverse: boolean?,
	out_of_range_behavior: OutOfRangeBehavior?,
	target_value_sequence: string?,
	round_target_value: boolean?,
	wrap: boolean?,
	jump_interval: Interval<number>?,
	takeover_mode: TakeoverMode?,
	control_transformation: string?,
	step_size_interval: Interval<number>?,
	step_factor_interval: Interval<number>?,
	button_filter: ButtonFilter?,
	encoder_filter: EncoderFilter?,
	relative_mode: RelativeMode?,
	interaction: Interaction?,
	fire_mode: FireMode?,
	feedback: Feedback?,
	feedback_value_table: FeedbackValueTable?,
}

export type FeedbackValueTable_FromTextToDiscrete = { kind: "FromTextToDiscrete", value: { [string]: number } }
export type FeedbackValueTable_FromTextToContinuous = { kind: "FromTextToContinuous", value: { [string]: number } }

export type FeedbackValueTable = FeedbackValueTable_FromTextToDiscrete | FeedbackValueTable_FromTextToContinuous

export type DiscreteFeedbackValueTableContent = {
	value: { [string]: number },
}

export type ContinuousFeedbackValueTableContent = {
	value: { [string]: number },
}

export type AbsoluteMode = "Normal" | "IncrementalButton" | "ToggleButton" | "MakeRelative" | "PerformanceControl"

export type RelativeMode = "Normal" | "MakeAbsolute"

export type FireMode_Normal = { kind: "Normal", press_duration_interval: Interval<number>? }
export type FireMode_AfterTimeout = { kind: "AfterTimeout", timeout: number? }
export type FireMode_AfterTimeoutKeepFiring = { kind: "AfterTimeoutKeepFiring", timeout: number?, rate: number? }
export type FireMode_OnSinglePress = { kind: "OnSinglePress", max_duration: number? }
export type FireMode_OnDoublePress = { kind: "OnDoublePress" }

export type FireMode =
	FireMode_Normal
	| FireMode_AfterTimeout
	| FireMode_AfterTimeoutKeepFiring
	| FireMode_OnSinglePress
	| FireMode_OnDoublePress

export type NormalFireMode = {
	press_duration_interval: Interval<number>?,
}

export type AfterTimeoutFireMode = {
	timeout: number?,
}

export type AfterTimeoutKeepFiringFireMode = {
	timeout: number?,
	rate: number?,
}

export type OnSinglePressFireMode = {
	max_duration: number?,
}

export type VirtualColor_Rgb = RgbColor
export type VirtualColor_Prop = PropColor

export type VirtualColor = VirtualColor_Rgb | VirtualColor_Prop

export type RgbColor = { number }

export type PropColor = {
	prop: string,
}

export type OutOfRangeBehavior = "MinOrMax" | "Min" | "Ignore"

export type TakeoverMode = "Off" | "PickUpTolerant" | "PickUp" | "LongTimeNoSee" | "Parallel" | "CatchUp"

export type ButtonFilter = "PressOnly" | "ReleaseOnly"

export type EncoderFilter = "IncrementOnly" | "DecrementOnly"

export type Interaction =
	"SameControl"
	| "SameTargetValue"
	| "InverseControl"
	| "InverseTargetValue"
	| "InverseTargetValueOnOnly"
	| "InverseTargetValueOffOnly"

export type FeedbackCommons = {
	color: VirtualColor?,
	background_color: VirtualColor?,
}

export type Feedback_Numeric = { kind: "Numeric", commons: FeedbackCommons, transformation: string? }
export type Feedback_Text = { kind: "Text", commons: FeedbackCommons, text_expression: string? }
export type Feedback_Dynamic = { kind: "Dynamic", commons: FeedbackCommons, script: string? }

export type Feedback = Feedback_Numeric | Feedback_Text | Feedback_Dynamic

export type NumericFeedback = {
	commons: FeedbackCommons,
	transformation: string?,
}

export type TextFeedback = {
	commons: FeedbackCommons,
	text_expression: string?,
}

export type DynamicFeedback = {
	commons: FeedbackCommons,
	script: string?,
}

export type Interval<T> = { T }

export type Group = {
	id: string?,
	name: string?,
	tags: { string }?,
	control_enabled: boolean?,
	feedback_enabled: boolean?,
	activation_condition: ActivationCondition?,
}

export type Mapping = {
	id: string?,
	name: string?,
	tags: { string }?,
	group: string?,
	visible_in_projection: boolean?,
	enabled: boolean?,
	control_enabled: boolean?,
	feedback_enabled: boolean?,
	activation_condition: ActivationCondition?,
	on_activate: LifecycleHook?,
	on_deactivate: LifecycleHook?,
	source: Source?,
	glue: Glue?,
	target: Target?,
	success_audio_feedback: SuccessAudioFeedback?,
	unprocessed: { [string]: any }?,
}

export type LifecycleHook = {
	send_midi_feedback: { SendMidiFeedbackAction }?,
}

export type SendMidiFeedbackAction_Raw = { kind: "Raw", message: RawMidiMessage }

export type SendMidiFeedbackAction = SendMidiFeedbackAction_Raw

export type RawMidiMessage_HexString = string
export type RawMidiMessage_ByteArray = { number }

export type RawMidiMessage = RawMidiMessage_HexString | RawMidiMessage_ByteArray

export type SuccessAudioFeedback_Simple = { kind: "Simple" }

export type SuccessAudioFeedback = SuccessAudioFeedback_Simple

export type ActivationCondition_Modifier = { kind: "Modifier", modifiers: { ModifierState }? }
export type ActivationCondition_Bank = { kind: "Bank", parameter: ParamRef, bank_index: number }
export type ActivationCondition_Eel = { kind: "Eel", condition: string }
export type ActivationCondition_Expression = { kind: "Expression", condition: string }
export type ActivationCondition_TargetValue = { kind: "TargetValue", mapping: string?, condition: string }

export type ActivationCondition =
	ActivationCondition_Modifier
	| ActivationCondition_Bank
	| ActivationCondition_Eel
	| ActivationCondition_Expression
	| ActivationCondition_TargetValue

export type ModifierActivationCondition = {
	modifiers: { ModifierState }?,
}

export type ModifierState = {
	parameter: ParamRef,
	on: boolean,
}

export type BankActivationCondition = {
	parameter: ParamRef,
	bank_index: number,
}

export type EelActivationCondition = {
	condition: string,
}

export type ExpressionActivationCondition = {
	condition: string,
}

export type TargetValueActivationCondition = {
	mapping: string?,
	condition: string,
}

export type ParamRef_Index = number
export type ParamRef_Key = string

export type ParamRef = ParamRef_Index | ParamRef_Key

export type VirtualControlElementId_Indexed = number
export type VirtualControlElementId_Named = string

export type VirtualControlElementId = VirtualControlElementId_Indexed | VirtualControlElementId_Named

export type VirtualControlElementCharacter = "Multi" | "Button"

export type OscArgument = {
	index: number?,
	arg_kind: OscArgKind?,
	value_range: Interval<number>?,
}

export type OscArgKind =
	"Float"
	| "Double"
	| "Bool"
	| "Nil"
	| "Inf"
	| "Int"
	| "String"
	| "Blob"
	| "Time"
	| "Long"
	| "Char"
	| "Color"
	| "Midi"
	| "Array"

export type Parameter = {
	index: number,
	id: string?,
	name: string?,
	value_count: number?,
	value_labels: { string }?,
}

export type Source_None = { kind: "None" }
export type Source_MidiDeviceChanges = { kind: "MidiDeviceChanges" }
export type Source_RealearnInstanceStart = { kind: "RealearnInstanceStart" }
export type Source_Timer = { kind: "Timer", duration: number }
export type Source_RealearnParameter = { kind: "RealearnParameter", parameter_index: number }
export type Source_Speech = { kind: "Speech" }
export type Source_MidiNoteVelocity = {
	kind: "MidiNoteVelocity",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}
export type Source_MidiNoteKeyNumber = {
	kind: "MidiNoteKeyNumber",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
export type Source_MidiPolyphonicKeyPressureAmount = {
	kind: "MidiPolyphonicKeyPressureAmount",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}
export type Source_MidiControlChangeValue = {
	kind: "MidiControlChangeValue",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	controller_number: number?,
	character: SourceCharacter?,
	fourteen_bit: boolean?,
}
export type Source_MidiProgramChangeNumber = {
	kind: "MidiProgramChangeNumber",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
export type Source_MidiSpecificProgramChange = {
	kind: "MidiSpecificProgramChange",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	program_number: number?,
}
export type Source_MidiChannelPressureAmount = {
	kind: "MidiChannelPressureAmount",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
export type Source_MidiPitchBendChangeValue = {
	kind: "MidiPitchBendChangeValue",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
export type Source_MidiParameterNumberValue = {
	kind: "MidiParameterNumberValue",
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	number: number?,
	fourteen_bit: boolean?,
	registered: boolean?,
	character: SourceCharacter?,
}
export type Source_MidiClockTempo = { kind: "MidiClockTempo" }
export type Source_MidiClockTransport = { kind: "MidiClockTransport", message: MidiClockTransportMessage? }
export type Source_MidiRaw = {
	kind: "MidiRaw",
	feedback_behavior: FeedbackBehavior?,
	pattern: string?,
	character: SourceCharacter?,
}
export type Source_MidiScript = { kind: "MidiScript", script_kind: MidiScriptKind?, script: string? }
export type Source_MackieLcd = { kind: "MackieLcd", extender_index: number?, channel: number?, line: number? }
export type Source_XTouchMackieLcd = { kind: "XTouchMackieLcd", extender_index: number?, channel: number?, line: number? }
export type Source_MackieSevenSegmentDisplay = {
	kind: "MackieSevenSegmentDisplay",
	scope: MackieSevenSegmentDisplayScope?,
}
export type Source_SlKeyboardDisplay = { kind: "SlKeyboardDisplay", section: number?, line: number? }
export type Source_SiniConE24Display = { kind: "SiniConE24Display", cell_index: number?, item_index: number? }
export type Source_LaunchpadProScrollingTextDisplay = { kind: "LaunchpadProScrollingTextDisplay" }
export type Source_Osc = {
	kind: "Osc",
	feedback_behavior: FeedbackBehavior?,
	address: string?,
	argument: OscArgument?,
	relative: boolean?,
	feedback_arguments: { string }?,
}
export type Source_Key = { kind: "Key", keystroke: Keystroke? }
export type Source_Virtual = { kind: "Virtual", id: VirtualControlElementId, character: VirtualControlElementCharacter? }

export type Source =
	Source_None
	| Source_MidiDeviceChanges
	| Source_RealearnInstanceStart
	| Source_Timer
	| Source_RealearnParameter
	| Source_Speech
	| Source_MidiNoteVelocity
	| Source_MidiNoteKeyNumber
	| Source_MidiPolyphonicKeyPressureAmount
	| Source_MidiControlChangeValue
	| Source_MidiProgramChangeNumber
	| Source_MidiSpecificProgramChange
	| Source_MidiChannelPressureAmount
	| Source_MidiPitchBendChangeValue
	| Source_MidiParameterNumberValue
	| Source_MidiClockTempo
	| Source_MidiClockTransport
	| Source_MidiRaw
	| Source_MidiScript
	| Source_MackieLcd
	| Source_XTouchMackieLcd
	| Source_MackieSevenSegmentDisplay
	| Source_SlKeyboardDisplay
	| Source_SiniConE24Display
	| Source_LaunchpadProScrollingTextDisplay
	| Source_Osc
	| Source_Key
	| Source_Virtual

export type FeedbackBehavior = "Normal" | "SendFeedbackAfterControl" | "PreventEchoFeedback"

export type MidiNoteVelocitySource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}

export type MidiNoteKeyNumberSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}

export type MidiPolyphonicKeyPressureAmountSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}

export type MidiControlChangeValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	controller_number: number?,
	character: SourceCharacter?,
	fourteen_bit: boolean?,
}

export type MidiProgramChangeNumberSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}

export type MidiSpecificProgramChangeSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	program_number: number?,
}

export type MidiChannelPressureAmountSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}

export type MidiPitchBendChangeValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}

export type MidiParameterNumberValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	number: number?,
	fourteen_bit: boolean?,
	registered: boolean?,
	character: SourceCharacter?,
}

export type MidiClockTransportSource = {
	message: MidiClockTransportMessage?,
}

export type MidiRawSource = {
	feedback_behavior: FeedbackBehavior?,
	pattern: string?,
	character: SourceCharacter?,
}

export type MidiScriptSource = {
	script_kind: MidiScriptKind?,
	script: string?,
}

export type MidiScriptKind = "Eel" | "Lua"

export type SourceCharacter = "Range" | "Button" | "Relative1" | "Relative2" | "Relative3" | "StatefulButton"

export type MidiClockTransportMessage = "Start" | "Continue" | "Stop"

export type MackieLcdSource = {
	extender_index: number?,
	channel: number?,
	line: number?,
}

export type XTouchMackieLcdSource = {
	extender_index: number?,
	channel: number?,
	line: number?,
}

export type SlKeyboardDisplaySource = {
	section: number?,
	line: number?,
}

export type MackieSevenSegmentDisplaySource = {
	scope: MackieSevenSegmentDisplayScope?,
}

export type MackieSevenSegmentDisplayScope =
	"All"
	| "Assignment"
	| "Tc"
	| "TcHoursBars"
	| "TcMinutesBeats"
	| "TcSecondsSub"
	| "TcFramesTicks"

export type SiniConE24DisplaySource = {
	cell_index: number?,
	item_index: number?,
}

export type OscSource = {
	feedback_behavior: FeedbackBehavior?,
	address: string?,
	argument: OscArgument?,
	relative: boolean?,
	feedback_arguments: { string }?,
}

export type RealearnParameterSource = {
	parameter_index: number,
}

export type TimerSource = {
	duration: number,
}

export type KeySource = {
	keystroke: Keystroke?,
}

export type Keystroke = {
	modifiers: number,
	key: number,
}

export type VirtualSource = {
	id: VirtualControlElementId,
	character: VirtualControlElementCharacter?,
}

export type LearnableTargetKind =
	"TrackVolume"
	| "TrackPan"
	| "RouteVolume"
	| "RoutePan"
	| "TrackArmState"
	| "TrackMuteState"
	| "TrackSoloState"
	| "TrackSelectionState"
	| "FxOnOffState"
	| "FxParameterValue"
	| "BrowseFxPresets"
	| "PlayRate"
	| "Tempo"
	| "TrackAutomationMode"
	| "TrackMonitoringMode"
	| "AutomationModeOverride"
	| "ReaperAction"
	| "TransportAction"

export type TargetTouchCause = "Any" | "Realearn" | "Reaper"

export type Target_Mouse = { kind: "Mouse", commons: TargetCommons, action: MouseAction }
export type Target_LastTouched = {
	kind: "LastTouched",
	commons: TargetCommons,
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}
export type Target_AutomationModeOverride = {
	kind: "AutomationModeOverride",
	commons: TargetCommons,
	override_value: AutomationModeOverride?,
}
export type Target_ReaperAction = {
	kind: "ReaperAction",
	commons: TargetCommons,
	command: ReaperCommand?,
	invocation: ActionInvocationKind?,
	track: TrackDescriptor?,
}
export type Target_TransportAction = { kind: "TransportAction", commons: TargetCommons, action: TransportAction }
export type Target_AnyOn = { kind: "AnyOn", commons: TargetCommons, parameter: AnyOnParameter }
export type Target_BrowseTracks = {
	kind: "BrowseTracks",
	commons: TargetCommons,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
	mode: BrowseTracksMode?,
}
export type Target_Seek = {
	kind: "Seek",
	commons: TargetCommons,
	use_time_selection: boolean?,
	use_loop_points: boolean?,
	use_regions: boolean?,
	use_project: boolean?,
	move_view: boolean?,
	seek_play: boolean?,
	feedback_resolution: FeedbackResolution?,
	behavior: SeekBehavior?,
}
export type Target_PlayRate = { kind: "PlayRate", commons: TargetCommons }
export type Target_Tempo = { kind: "Tempo", commons: TargetCommons }
export type Target_GoToBookmark = {
	kind: "GoToBookmark",
	commons: TargetCommons,
	bookmark: BookmarkDescriptor,
	set_time_selection: boolean?,
	set_loop_points: boolean?,
	seek_behavior: SeekBehavior?,
}
export type Target_TrackArmState = {
	kind: "TrackArmState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackParentSendState = {
	kind: "TrackParentSendState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
}
export type Target_AllTrackFxOnOffState = {
	kind: "AllTrackFxOnOffState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
}
export type Target_TrackMuteState = {
	kind: "TrackMuteState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackPeak = { kind: "TrackPeak", commons: TargetCommons, track: TrackDescriptor? }
export type Target_TrackPhase = {
	kind: "TrackPhase",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackSelectionState = {
	kind: "TrackSelectionState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
}
export type Target_TrackAutomationMode = {
	kind: "TrackAutomationMode",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: AutomationMode,
}
export type Target_TrackMonitoringMode = {
	kind: "TrackMonitoringMode",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: MonitoringMode,
	use_selection_ganging: boolean?,
}
export type Target_TrackAutomationTouchState = {
	kind: "TrackAutomationTouchState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	touched_parameter: TouchedTrackParameter,
}
export type Target_TrackPan = {
	kind: "TrackPan",
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackWidth = {
	kind: "TrackWidth",
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackVolume = {
	kind: "TrackVolume",
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_TrackTool = {
	kind: "TrackTool",
	commons: TargetCommons,
	track: TrackDescriptor?,
	action: TrackToolAction?,
	instance_tags: { string }?,
}
export type Target_TrackVisibility = {
	kind: "TrackVisibility",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	area: TrackArea,
}
export type Target_TrackSoloState = {
	kind: "TrackSoloState",
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	behavior: SoloBehavior?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
export type Target_BrowseFxChain = {
	kind: "BrowseFxChain",
	commons: TargetCommons,
	chain: FxChainDescriptor,
	display_kind: FxDisplayKind?,
}
export type Target_FxOnOffState = { kind: "FxOnOffState", commons: TargetCommons, fx: FxDescriptor? }
export type Target_FxOnlineOfflineState = { kind: "FxOnlineOfflineState", commons: TargetCommons, fx: FxDescriptor? }
export type Target_LoadFxSnapshot = {
	kind: "LoadFxSnapshot",
	commons: TargetCommons,
	fx: FxDescriptor?,
	snapshot: FxSnapshot?,
}
export type Target_BrowseFxPresets = { kind: "BrowseFxPresets", commons: TargetCommons, fx: FxDescriptor? }
export type Target_FxTool = {
	kind: "FxTool",
	commons: TargetCommons,
	fx: FxDescriptor?,
	action: FxToolAction?,
	instance_tags: { string }?,
}
export type Target_FxVisibility = {
	kind: "FxVisibility",
	commons: TargetCommons,
	fx: FxDescriptor?,
	display_kind: FxDisplayKind?,
}
export type Target_FxParameterValue = {
	kind: "FxParameterValue",
	commons: TargetCommons,
	parameter: FxParameterDescriptor,
	poll_for_feedback: boolean?,
	retrigger: boolean?,
}
export type Target_FxParameterAutomationTouchState = {
	kind: "FxParameterAutomationTouchState",
	commons: TargetCommons,
	parameter: FxParameterDescriptor,
}
export type Target_RouteAutomationMode = {
	kind: "RouteAutomationMode",
	commons: TargetCommons,
	route: RouteDescriptor,
	mode: AutomationMode,
	poll_for_feedback: boolean?,
}
export type Target_RouteMonoState = {
	kind: "RouteMonoState",
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
export type Target_RouteMuteState = {
	kind: "RouteMuteState",
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
export type Target_RoutePhase = {
	kind: "RoutePhase",
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
export type Target_RoutePan = { kind: "RoutePan", commons: TargetCommons, route: RouteDescriptor }
export type Target_RouteVolume = { kind: "RouteVolume", commons: TargetCommons, route: RouteDescriptor }
export type Target_RouteTouchState = {
	kind: "RouteTouchState",
	commons: TargetCommons,
	route: RouteDescriptor,
	touched_parameter: TouchedRouteParameter,
}
export type Target_PlaytimeSlotTransportAction = {
	kind: "PlaytimeSlotTransportAction",
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	action: ClipTransportAction,
	record_only_if_track_armed: boolean?,
	stop_column_if_slot_empty: boolean?,
	play_start_timing: playtime.ClipPlayStartTiming?,
	play_stop_timing: playtime.ClipPlayStopTiming?,
}
export type Target_PlaytimeColumnAction = {
	kind: "PlaytimeColumnAction",
	commons: TargetCommons,
	column: ClipColumnDescriptor,
	action: ClipColumnAction,
}
export type Target_PlaytimeRowAction = {
	kind: "PlaytimeRowAction",
	commons: TargetCommons,
	row: ClipRowDescriptor,
	action: ClipRowAction,
}
export type Target_PlaytimeMatrixAction = {
	kind: "PlaytimeMatrixAction",
	commons: TargetCommons,
	action: ClipMatrixAction,
}
export type Target_PlaytimeControlUnitScroll = { kind: "PlaytimeControlUnitScroll", commons: TargetCommons, axis: Axis }
export type Target_PlaytimeSlotSeek = {
	kind: "PlaytimeSlotSeek",
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	feedback_resolution: FeedbackResolution?,
}
export type Target_PlaytimeSlotVolume = { kind: "PlaytimeSlotVolume", commons: TargetCommons, slot: ClipSlotDescriptor }
export type Target_PlaytimeSlotManagementAction = {
	kind: "PlaytimeSlotManagementAction",
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	action: ClipManagementAction,
}
export type Target_SendMidi = {
	kind: "SendMidi",
	commons: TargetCommons,
	message: string?,
	destination: MidiDestination?,
}
export type Target_SendOsc = {
	kind: "SendOsc",
	commons: TargetCommons,
	address: string?,
	argument: OscArgument?,
	destination: OscDestination?,
}
export type Target_Dummy = { kind: "Dummy", commons: TargetCommons }
export type Target_EnableInstances = {
	kind: "EnableInstances",
	commons: TargetCommons,
	tags: { string }?,
	exclusivity: InstanceExclusivity?,
}
export type Target_EnableMappings = {
	kind: "EnableMappings",
	commons: TargetCommons,
	tags: { string }?,
	exclusivity: MappingExclusivity?,
}
export type Target_ModifyMapping = {
	kind: "ModifyMapping",
	commons: TargetCommons,
	session: string?,
	mapping: string?,
	modification: MappingModification,
}
export type Target_CompartmentParameterValue = {
	kind: "CompartmentParameterValue",
	commons: TargetCommons,
	parameter: CompartmentParameterDescriptor,
}
export type Target_LoadMappingSnapshot = {
	kind: "LoadMappingSnapshot",
	commons: TargetCommons,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: MappingSnapshotDescForLoad?,
	default_value: TargetValue?,
}
export type Target_TakeMappingSnapshot = {
	kind: "TakeMappingSnapshot",
	commons: TargetCommons,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: BackwardCompatibleMappingSnapshotDescForTake,
}
export type Target_BrowseGroupMappings = {
	kind: "BrowseGroupMappings",
	commons: TargetCommons,
	exclusivity: GroupMappingExclusivity?,
	group: string?,
}
export type Target_BrowsePotFilterItems = {
	kind: "BrowsePotFilterItems",
	commons: TargetCommons,
	item_kind: PotFilterKind?,
}
export type Target_BrowsePotPresets = { kind: "BrowsePotPresets", commons: TargetCommons }
export type Target_PreviewPotPreset = { kind: "PreviewPotPreset", commons: TargetCommons }
export type Target_LoadPotPreset = { kind: "LoadPotPreset", commons: TargetCommons, fx: FxDescriptor? }
export type Target_Virtual = { kind: "Virtual", id: VirtualControlElementId, character: VirtualControlElementCharacter? }

export type Target =
	Target_Mouse
	| Target_LastTouched
	| Target_AutomationModeOverride
	| Target_ReaperAction
	| Target_TransportAction
	| Target_AnyOn
	| Target_BrowseTracks
	| Target_Seek
	| Target_PlayRate
	| Target_Tempo
	| Target_GoToBookmark
	| Target_TrackArmState
	| Target_TrackParentSendState
	| Target_AllTrackFxOnOffState
	| Target_TrackMuteState
	| Target_TrackPeak
	| Target_TrackPhase
	| Target_TrackSelectionState
	| Target_TrackAutomationMode
	| Target_TrackMonitoringMode
	| Target_TrackAutomationTouchState
	| Target_TrackPan
	| Target_TrackWidth
	| Target_TrackVolume
	| Target_TrackTool
	| Target_TrackVisibility
	| Target_TrackSoloState
	| Target_BrowseFxChain
	| Target_FxOnOffState
	| Target_FxOnlineOfflineState
	| Target_LoadFxSnapshot
	| Target_BrowseFxPresets
	| Target_FxTool
	| Target_FxVisibility
	| Target_FxParameterValue
	| Target_FxParameterAutomationTouchState
	| Target_RouteAutomationMode
	| Target_RouteMonoState
	| Target_RouteMuteState
	| Target_RoutePhase
	| Target_RoutePan
	| Target_RouteVolume
	| Target_RouteTouchState
	| Target_PlaytimeSlotTransportAction
	| Target_PlaytimeColumnAction
	| Target_PlaytimeRowAction
	| Target_PlaytimeMatrixAction
	| Target_PlaytimeControlUnitScroll
	| Target_PlaytimeSlotSeek
	| Target_PlaytimeSlotVolume
	| Target_PlaytimeSlotManagementAction
	| Target_SendMidi
	| Target_SendOsc
	| Target_Dummy
	| Target_EnableInstances
	| Target_EnableMappings
	| Target_ModifyMapping
	| Target_CompartmentParameterValue
	| Target_LoadMappingSnapshot
	| Target_TakeMappingSnapshot
	| Target_BrowseGroupMappings
	| Target_BrowsePotFilterItems
	| Target_BrowsePotPresets
	| Target_PreviewPotPreset
	| Target_LoadPotPreset
	| Target_Virtual

export type TargetCommons = {
	unit: TargetUnit?,
}

export type TargetUnit = "Native" | "Percent"

export type MouseTarget = {
	commons: TargetCommons,
	action: MouseAction,
}

export type LastTouchedTarget = {
	commons: TargetCommons,
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}

export type AutomationModeOverrideTarget = {
	commons: TargetCommons,
	override_value: AutomationModeOverride?,
}

export type ReaperActionTarget = {
	commons: TargetCommons,
	command: ReaperCommand?,
	invocation: ActionInvocationKind?,
	track: TrackDescriptor?,
}

export type TransportActionTarget = {
	commons: TargetCommons,
	action: TransportAction,
}

export type AnyOnTarget = {
	commons: TargetCommons,
	parameter: AnyOnParameter,
}

export type BrowseTracksTarget = {
	commons: TargetCommons,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
	mode: BrowseTracksMode?,
}

export type SeekTarget = {
	commons: TargetCommons,
	use_time_selection: boolean?,
	use_loop_points: boolean?,
	use_regions: boolean?,
	use_project: boolean?,
	move_view: boolean?,
	seek_play: boolean?,
	feedback_resolution: FeedbackResolution?,
	behavior: SeekBehavior?,
}

export type PlayRateTarget = {
	commons: TargetCommons,
}

export type TempoTarget = {
	commons: TargetCommons,
}

export type GoToBookmarkTarget = {
	commons: TargetCommons,
	bookmark: BookmarkDescriptor,
	set_time_selection: boolean?,
	set_loop_points: boolean?,
	seek_behavior: SeekBehavior?,
}

export type TrackArmStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackParentSendStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
}

export type AllTrackFxOnOffStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
}

export type TrackMuteStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackPeakTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
}

export type TrackPhaseTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackSelectionStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
}

export type TrackAutomationModeTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: AutomationMode,
}

export type TrackMonitoringModeTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: MonitoringMode,
	use_selection_ganging: boolean?,
}

export type TrackAutomationTouchStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	touched_parameter: TouchedTrackParameter,
}

export type TrackPanTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackWidthTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackVolumeTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type TrackToolTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	action: TrackToolAction?,
	instance_tags: { string }?,
}

export type TrackToolAction = "DoNothing" | "SetAsUnitTrack" | "PinAsUnitTrack"

export type MouseAction_MoveTo = { kind: "MoveTo", axis: Axis? }
export type MouseAction_MoveBy = { kind: "MoveBy", axis: Axis? }
export type MouseAction_PressOrRelease = { kind: "PressOrRelease", button: MouseButton? }
export type MouseAction_Scroll = { kind: "Scroll", axis: Axis? }

export type MouseAction = MouseAction_MoveTo | MouseAction_MoveBy | MouseAction_PressOrRelease | MouseAction_Scroll

export type Axis = "X" | "Y"

export type MouseButton = "Left" | "Middle" | "Right"

export type TrackVisibilityTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	area: TrackArea,
}

export type TrackSoloStateTarget = {
	commons: TargetCommons,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	behavior: SoloBehavior?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type BrowseFxChainTarget = {
	commons: TargetCommons,
	chain: FxChainDescriptor,
	display_kind: FxDisplayKind?,
}

export type FxOnOffStateTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
}

export type FxOnlineOfflineStateTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
}

export type LoadFxSnapshotTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
	snapshot: FxSnapshot?,
}

export type BrowseFxPresetsTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
}

export type FxToolTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
	action: FxToolAction?,
	instance_tags: { string }?,
}

export type FxToolAction = "DoNothing" | "SetAsUnitFx" | "PinAsUnitFx"

export type FxVisibilityTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
	display_kind: FxDisplayKind?,
}

export type FxParameterValueTarget = {
	commons: TargetCommons,
	parameter: FxParameterDescriptor,
	poll_for_feedback: boolean?,
	retrigger: boolean?,
}

export type CompartmentParameterValueTarget = {
	commons: TargetCommons,
	parameter: CompartmentParameterDescriptor,
}

export type FxParameterAutomationTouchStateTarget = {
	commons: TargetCommons,
	parameter: FxParameterDescriptor,
}

export type RouteAutomationModeTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
	mode: AutomationMode,
	poll_for_feedback: boolean?,
}

export type RouteMonoStateTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type RouteMuteStateTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type RoutePhaseTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type RoutePanTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
}

export type RouteVolumeTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
}

export type RouteTouchStateTarget = {
	commons: TargetCommons,
	route: RouteDescriptor,
	touched_parameter: TouchedRouteParameter,
}

export type PlaytimeSlotTransportActionTarget = {
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	action: ClipTransportAction,
	record_only_if_track_armed: boolean?,
	stop_column_if_slot_empty: boolean?,
	play_start_timing: playtime.ClipPlayStartTiming?,
	play_stop_timing: playtime.ClipPlayStopTiming?,
}

export type PlaytimeColumnActionTarget = {
	commons: TargetCommons,
	column: ClipColumnDescriptor,
	action: ClipColumnAction,
}

export type PlaytimeRowActionTarget = {
	commons: TargetCommons,
	row: ClipRowDescriptor,
	action: ClipRowAction,
}

export type PlaytimeMatrixActionTarget = {
	commons: TargetCommons,
	action: ClipMatrixAction,
}

export type PlaytimeControlUnitScrollTarget = {
	commons: TargetCommons,
	axis: Axis,
}

export type PlaytimeSlotSeekTarget = {
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	feedback_resolution: FeedbackResolution?,
}

export type PlaytimeSlotVolumeTarget = {
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
}

export type PlaytimeSlotManagementActionTarget = {
	commons: TargetCommons,
	slot: ClipSlotDescriptor,
	action: ClipManagementAction,
}

export type ClipManagementAction_ClearSlot = { kind: "ClearSlot" }
export type ClipManagementAction_FillSlotWithSelectedItem = { kind: "FillSlotWithSelectedItem" }
export type ClipManagementAction_EditClip = { kind: "EditClip" }
export type ClipManagementAction_CopyOrPasteClip = { kind: "CopyOrPasteClip" }
export type ClipManagementAction_AdjustClipSectionLength = { kind: "AdjustClipSectionLength", factor: number }

export type ClipManagementAction =
	ClipManagementAction_ClearSlot
	| ClipManagementAction_FillSlotWithSelectedItem
	| ClipManagementAction_EditClip
	| ClipManagementAction_CopyOrPasteClip
	| ClipManagementAction_AdjustClipSectionLength

export type AdjustClipSectionLengthAction = {
	factor: number,
}

export type SendMidiTarget = {
	commons: TargetCommons,
	message: string?,
	destination: MidiDestination?,
}

export type DummyTarget = {
	commons: TargetCommons,
}

export type SendOscTarget = {
	commons: TargetCommons,
	address: string?,
	argument: OscArgument?,
	destination: OscDestination?,
}

export type EnableInstancesTarget = {
	commons: TargetCommons,
	tags: { string }?,
	exclusivity: InstanceExclusivity?,
}

export type EnableMappingsTarget = {
	commons: TargetCommons,
	tags: { string }?,
	exclusivity: MappingExclusivity?,
}

export type ModifyMappingTarget = {
	commons: TargetCommons,
	session: string?,
	mapping: string?,
	modification: MappingModification,
}

export type MappingModificationKind = "LearnTarget" | "SetTargetToLastTouched"

export type MappingModification_LearnTarget = {
	kind: "LearnTarget",
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}
export type MappingModification_SetTargetToLastTouched = {
	kind: "SetTargetToLastTouched",
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}

export type MappingModification = MappingModification_LearnTarget | MappingModification_SetTargetToLastTouched

export type LearnTargetMappingModification = {
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}

export type SetTargetToLastTouchedMappingModification = {
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}

export type LoadMappingSnapshotTarget = {
	commons: TargetCommons,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: MappingSnapshotDescForLoad?,
	default_value: TargetValue?,
}

export type TakeMappingSnapshotTarget = {
	commons: TargetCommons,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: BackwardCompatibleMappingSnapshotDescForTake,
}

export type BackwardCompatibleMappingSnapshotDescForTake_Old = string
export type BackwardCompatibleMappingSnapshotDescForTake_New = MappingSnapshotDescForTake

export type BackwardCompatibleMappingSnapshotDescForTake =
	BackwardCompatibleMappingSnapshotDescForTake_Old
	| BackwardCompatibleMappingSnapshotDescForTake_New

export type MappingSnapshotDescForLoad_Initial = { kind: "Initial" }
export type MappingSnapshotDescForLoad_ById = { kind: "ById", id: string }

export type MappingSnapshotDescForLoad = MappingSnapshotDescForLoad_Initial | MappingSnapshotDescForLoad_ById

export type MappingSnapshotDescForTake_LastLoaded = { kind: "LastLoaded" }
export type MappingSnapshotDescForTake_ById = { kind: "ById", id: string }

export type MappingSnapshotDescForTake = MappingSnapshotDescForTake_LastLoaded | MappingSnapshotDescForTake_ById

export type BrowseGroupMappingsTarget = {
	commons: TargetCommons,
	exclusivity: GroupMappingExclusivity?,
	group: string?,
}

export type BrowsePotFilterItemsTarget = {
	commons: TargetCommons,
	item_kind: PotFilterKind?,
}

export type BrowsePotPresetsTarget = {
	commons: TargetCommons,
}

export type PreviewPotPresetTarget = {
	commons: TargetCommons,
}

export type LoadPotPresetTarget = {
	commons: TargetCommons,
	fx: FxDescriptor?,
}

export type PotFilterKind =
	"Database"
	| "IsAvailable"
	| "IsSupported"
	| "IsUser"
	| "ProductKind"
	| "IsFavorite"
	| "Project"
	| "Bank"
	| "SubBank"
	| "Category"
	| "SubCategory"
	| "Mode"
	| "HasPreview"

export type VirtualTarget = {
	id: VirtualControlElementId,
	character: VirtualControlElementCharacter?,
}

export type AutomationModeOverride_Bypass = { kind: "Bypass" }
export type AutomationModeOverride_Mode = { kind: "Mode", mode: AutomationMode }

export type AutomationModeOverride = AutomationModeOverride_Bypass | AutomationModeOverride_Mode

export type AutomationMode = "TrimRead" | "Read" | "Touch" | "Write" | "Latch" | "LatchPreview"

export type MonitoringMode = "Off" | "Normal" | "TapeStyle"

export type TransportAction = "PlayStop" | "PlayPause" | "Stop" | "Pause" | "Record" | "Repeat"

export type ClipTransportAction =
	"Trigger"
	| "PlayStop"
	| "PlayPause"
	| "Stop"
	| "Pause"
	| "RecordStop"
	| "RecordPlayStop"
	| "Looped"

export type ClipColumnAction = "Stop"

export type ClipRowAction = "PlayScene" | "BuildScene" | "ClearScene" | "CopyOrPasteScene"

export type ClipMatrixAction =
	"Stop"
	| "Undo"
	| "Redo"
	| "BuildScene"
	| "SetRecordDurationToOpenEnd"
	| "SetRecordDurationToOneBar"
	| "SetRecordDurationToTwoBars"
	| "SetRecordDurationToFourBars"
	| "SetRecordDurationToEightBars"

export type AnyOnParameter = "TrackSolo" | "TrackMute" | "TrackArm" | "TrackSelection"

export type ActionInvocationKind = "Trigger" | "Absolute14Bit" | "Absolute7Bit" | "Relative"

export type ReaperCommand_Id = number
export type ReaperCommand_Name = string

export type ReaperCommand = ReaperCommand_Id | ReaperCommand_Name

export type TrackDescriptor_This = { address: "This", commons: TrackDescriptorCommons }
export type TrackDescriptor_Master = { address: "Master", commons: TrackDescriptorCommons }
export type TrackDescriptor_Instance = { address: "Instance", commons: TrackDescriptorCommons }
export type TrackDescriptor_Selected = { address: "Selected", allow_multiple: boolean? }
export type TrackDescriptor_Dynamic = {
	address: "Dynamic",
	commons: TrackDescriptorCommons,
	expression: string,
	scope: TrackScope?,
}
export type TrackDescriptor_ById = { address: "ById", commons: TrackDescriptorCommons, id: string? }
export type TrackDescriptor_ByIndex = {
	address: "ByIndex",
	commons: TrackDescriptorCommons,
	index: number,
	scope: TrackScope?,
}
export type TrackDescriptor_ByName = {
	address: "ByName",
	commons: TrackDescriptorCommons,
	name: string,
	allow_multiple: boolean?,
}
export type TrackDescriptor_FromClipColumn = {
	address: "FromClipColumn",
	commons: TrackDescriptorCommons,
	column: ClipColumnDescriptor,
	context: ClipColumnTrackContext,
}

export type TrackDescriptor =
	TrackDescriptor_This
	| TrackDescriptor_Master
	| TrackDescriptor_Instance
	| TrackDescriptor_Selected
	| TrackDescriptor_Dynamic
	| TrackDescriptor_ById
	| TrackDescriptor_ByIndex
	| TrackDescriptor_ByName
	| TrackDescriptor_FromClipColumn

export type ClipColumnTrackContext = "Playback" | "Recording"

export type TrackDescriptorCommons = {
	track_must_be_selected: boolean?,
}

export type FeedbackResolution = "Beat" | "High"

export type TrackExclusivity = "WithinProject" | "WithinFolder" | "WithinProjectOnOnly" | "WithinFolderOnOnly"

export type InstanceExclusivity = "Exclusive" | "ExclusiveOnOnly"

export type MappingExclusivity = "Exclusive" | "ExclusiveOnOnly"

export type GroupMappingExclusivity = "Exclusive"

export type TouchedTrackParameter = "Volume" | "Pan" | "Width"

export type TouchedRouteParameter = "Volume" | "Pan"

export type TrackArea = "Tcp" | "Mcp"

export type SoloBehavior = "InPlace" | "IgnoreRouting" | "ReaperPreference"

export type SeekBehavior = "Immediate" | "Smooth" | "ReaperPreference"

export type BookmarkDescriptor_Marker = { kind: "Marker", id: number } | { kind: "Marker", index: number }
export type BookmarkDescriptor_Region = { kind: "Region", id: number } | { kind: "Region", index: number }

export type BookmarkDescriptor = BookmarkDescriptor_Marker | BookmarkDescriptor_Region

export type BookmarkRef_ById = { id: number }
export type BookmarkRef_ByIndex = { index: number }

export type BookmarkRef = BookmarkRef_ById | BookmarkRef_ByIndex

export type FxDescriptorCommons = {
	fx_must_have_focus: boolean?,
}

export type FxDescriptor_This = { address: "This", commons: FxDescriptorCommons }
export type FxDescriptor_Focused = { address: "Focused" }
export type FxDescriptor_Instance = { address: "Instance", commons: FxDescriptorCommons }
export type FxDescriptor_Dynamic = {
	address: "Dynamic",
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	expression: string,
}
export type FxDescriptor_ById = { address: "ById", commons: FxDescriptorCommons, chain: FxChainDescriptor, id: string? }
export type FxDescriptor_ByIndex = {
	address: "ByIndex",
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	index: number,
}
export type FxDescriptor_ByName = {
	address: "ByName",
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	name: string,
	allow_multiple: boolean?,
}

export type FxDescriptor =
	FxDescriptor_This
	| FxDescriptor_Focused
	| FxDescriptor_Instance
	| FxDescriptor_Dynamic
	| FxDescriptor_ById
	| FxDescriptor_ByIndex
	| FxDescriptor_ByName

export type FxChainDescriptor_Track = { address: "Track", track: TrackDescriptor?, chain: TrackFxChain? }

export type FxChainDescriptor = FxChainDescriptor_Track

export type TrackFxChain = "Normal" | "Input"

export type FxDisplayKind = "FloatingWindow" | "Chain"

export type FxSnapshot = {
	fx_kind: string?,
	fx_name: string?,
	preset_name: string?,
	content: FxSnapshotContent,
}

export type FxSnapshotContent_Chunk = { kind: "Chunk", chunk: string }

export type FxSnapshotContent = FxSnapshotContent_Chunk

export type FxParameterDescriptor_Dynamic = { address: "Dynamic", fx: FxDescriptor?, expression: string }
export type FxParameterDescriptor_ById = { address: "ById", fx: FxDescriptor?, index: number }
export type FxParameterDescriptor_ByIndex = { address: "ByIndex", fx: FxDescriptor?, index: number }
export type FxParameterDescriptor_ByName = { address: "ByName", fx: FxDescriptor?, name: string }

export type FxParameterDescriptor =
	FxParameterDescriptor_Dynamic
	| FxParameterDescriptor_ById
	| FxParameterDescriptor_ByIndex
	| FxParameterDescriptor_ByName

export type CompartmentParameterDescriptor_ById = { address: "ById", index: number }

export type CompartmentParameterDescriptor = CompartmentParameterDescriptor_ById

export type RouteDescriptorCommons = {
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
}

export type RouteDescriptor_Dynamic = { address: "Dynamic", commons: RouteDescriptorCommons, expression: string }
export type RouteDescriptor_ById = { address: "ById", commons: RouteDescriptorCommons, id: string? }
export type RouteDescriptor_ByIndex = { address: "ByIndex", commons: RouteDescriptorCommons, index: number }
export type RouteDescriptor_ByName = { address: "ByName", commons: RouteDescriptorCommons, name: string }

export type RouteDescriptor =
	RouteDescriptor_Dynamic
	| RouteDescriptor_ById
	| RouteDescriptor_ByIndex
	| RouteDescriptor_ByName

export type TrackRouteKind = "Send" | "Receive" | "HardwareOutput"

export type ClipSlotDescriptor_Selected = { address: "Selected" }
export type ClipSlotDescriptor_ByIndex = { address: "ByIndex", column_index: number, row_index: number }
export type ClipSlotDescriptor_Dynamic = { address: "Dynamic", column_expression: string, row_expression: string }

export type ClipSlotDescriptor = ClipSlotDescriptor_Selected | ClipSlotDescriptor_ByIndex | ClipSlotDescriptor_Dynamic

export type ClipColumnDescriptor_Selected = { address: "Selected" }
export type ClipColumnDescriptor_ByIndex = { address: "ByIndex", index: number }
export type ClipColumnDescriptor_Dynamic = { address: "Dynamic", expression: string }

export type ClipColumnDescriptor =
	ClipColumnDescriptor_Selected
	| ClipColumnDescriptor_ByIndex
	| ClipColumnDescriptor_Dynamic

export type ClipRowDescriptor_Selected = { address: "Selected" }
export type ClipRowDescriptor_ByIndex = { address: "ByIndex", index: number }
export type ClipRowDescriptor_Dynamic = { address: "Dynamic", expression: string }

export type ClipRowDescriptor = ClipRowDescriptor_Selected | ClipRowDescriptor_ByIndex | ClipRowDescriptor_Dynamic

export type MidiDestination_FxOutput = { kind: "FxOutput" }
export type MidiDestination_FeedbackOutput = { kind: "FeedbackOutput" }

export type MidiDestination = MidiDestination_FxOutput | MidiDestination_FeedbackOutput

export type OscDestination_FeedbackOutput = { kind: "FeedbackOutput" }
export type OscDestination_Device = { kind: "Device", id: string }

export type OscDestination = OscDestination_FeedbackOutput | OscDestination_Device

export type TrackScope = "AllTracks" | "TracksVisibleInTcp" | "TracksVisibleInMcp"

export type BrowseTracksMode =
	"AllTracks"
	| "TracksVisibleInTcp"
	| "TracksVisibleInTcpAllowTwoSelections"
	| "TracksVisibleInMcp"
	| "TracksVisibleInMcpAllowTwoSelections"

export type TargetValue_Unit = { kind: "Unit", value: number }
export type TargetValue_Discrete = { kind: "Discrete", value: number }

export type TargetValue = TargetValue_Unit | TargetValue_Discrete

return module

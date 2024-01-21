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

export type FeedbackValueTable_FromTextToDiscrete =
	{ kind: "FromTextToDiscrete" }
	& FeedbackValueTableContent<string, number>
export type FeedbackValueTable_FromTextToContinuous =
	{ kind: "FromTextToContinuous" }
	& FeedbackValueTableContent<string, number>

export type FeedbackValueTable = FeedbackValueTable_FromTextToDiscrete | FeedbackValueTable_FromTextToContinuous

export type FeedbackValueTableContent<K, V> = {
	value: { [K]: V },
}

export type AbsoluteMode = "Normal" | "IncrementalButton" | "ToggleButton" | "MakeRelative" | "PerformanceControl"

export type RelativeMode = "Normal" | "MakeAbsolute"

export type FireMode_Normal = { kind: "Normal" } & NormalFireMode
export type FireMode_AfterTimeout = { kind: "AfterTimeout" } & AfterTimeoutFireMode
export type FireMode_AfterTimeoutKeepFiring = { kind: "AfterTimeoutKeepFiring" } & AfterTimeoutKeepFiringFireMode
export type FireMode_OnSinglePress = { kind: "OnSinglePress" } & OnSinglePressFireMode
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

export type Feedback_Numeric = { kind: "Numeric" } & NumericFeedback
export type Feedback_Text = { kind: "Text" } & TextFeedback
export type Feedback_Dynamic = { kind: "Dynamic" } & DynamicFeedback

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

export type SendMidiFeedbackAction_Raw = { kind: "Raw" } & {
	message: RawMidiMessage,
}

export type SendMidiFeedbackAction = SendMidiFeedbackAction_Raw

export type RawMidiMessage_HexString = string
export type RawMidiMessage_ByteArray = { number }

export type RawMidiMessage = RawMidiMessage_HexString | RawMidiMessage_ByteArray

export type SuccessAudioFeedback_Simple = { kind: "Simple" }

export type SuccessAudioFeedback = SuccessAudioFeedback_Simple

export type ActivationCondition_Modifier = { kind: "Modifier" } & ModifierActivationCondition
export type ActivationCondition_Bank = { kind: "Bank" } & BankActivationCondition
export type ActivationCondition_Eel = { kind: "Eel" } & EelActivationCondition
export type ActivationCondition_Expression = { kind: "Expression" } & ExpressionActivationCondition
export type ActivationCondition_TargetValue = { kind: "TargetValue" } & TargetValueActivationCondition

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
export type Source_Timer = { kind: "Timer" } & TimerSource
export type Source_RealearnParameter = { kind: "RealearnParameter" } & RealearnParameterSource
export type Source_Speech = { kind: "Speech" }
export type Source_MidiNoteVelocity = { kind: "MidiNoteVelocity" } & MidiNoteVelocitySource
export type Source_MidiNoteKeyNumber = { kind: "MidiNoteKeyNumber" } & MidiNoteKeyNumberSource
export type Source_MidiPolyphonicKeyPressureAmount =
	{ kind: "MidiPolyphonicKeyPressureAmount" }
	& MidiPolyphonicKeyPressureAmountSource
export type Source_MidiControlChangeValue = { kind: "MidiControlChangeValue" } & MidiControlChangeValueSource
export type Source_MidiProgramChangeNumber = { kind: "MidiProgramChangeNumber" } & MidiProgramChangeNumberSource
export type Source_MidiSpecificProgramChange = { kind: "MidiSpecificProgramChange" } & MidiSpecificProgramChangeSource
export type Source_MidiChannelPressureAmount = { kind: "MidiChannelPressureAmount" } & MidiChannelPressureAmountSource
export type Source_MidiPitchBendChangeValue = { kind: "MidiPitchBendChangeValue" } & MidiPitchBendChangeValueSource
export type Source_MidiParameterNumberValue = { kind: "MidiParameterNumberValue" } & MidiParameterNumberValueSource
export type Source_MidiClockTempo = { kind: "MidiClockTempo" }
export type Source_MidiClockTransport = { kind: "MidiClockTransport" } & MidiClockTransportSource
export type Source_MidiRaw = { kind: "MidiRaw" } & MidiRawSource
export type Source_MidiScript = { kind: "MidiScript" } & MidiScriptSource
export type Source_MackieLcd = { kind: "MackieLcd" } & MackieLcdSource
export type Source_XTouchMackieLcd = { kind: "XTouchMackieLcd" } & XTouchMackieLcdSource
export type Source_MackieSevenSegmentDisplay = { kind: "MackieSevenSegmentDisplay" } & MackieSevenSegmentDisplaySource
export type Source_SlKeyboardDisplay = { kind: "SlKeyboardDisplay" } & SlKeyboardDisplaySource
export type Source_SiniConE24Display = { kind: "SiniConE24Display" } & SiniConE24DisplaySource
export type Source_LaunchpadProScrollingTextDisplay = { kind: "LaunchpadProScrollingTextDisplay" }
export type Source_Osc = { kind: "Osc" } & OscSource
export type Source_Key = { kind: "Key" } & KeySource
export type Source_Virtual = { kind: "Virtual" } & VirtualSource

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

export type Target_Mouse = { kind: "Mouse" } & MouseTarget
export type Target_LastTouched = { kind: "LastTouched" } & LastTouchedTarget
export type Target_AutomationModeOverride = { kind: "AutomationModeOverride" } & AutomationModeOverrideTarget
export type Target_ReaperAction = { kind: "ReaperAction" } & ReaperActionTarget
export type Target_TransportAction = { kind: "TransportAction" } & TransportActionTarget
export type Target_AnyOn = { kind: "AnyOn" } & AnyOnTarget
export type Target_BrowseTracks = { kind: "BrowseTracks" } & BrowseTracksTarget
export type Target_Seek = { kind: "Seek" } & SeekTarget
export type Target_PlayRate = { kind: "PlayRate" } & PlayRateTarget
export type Target_Tempo = { kind: "Tempo" } & TempoTarget
export type Target_GoToBookmark = { kind: "GoToBookmark" } & GoToBookmarkTarget
export type Target_TrackArmState = { kind: "TrackArmState" } & TrackArmStateTarget
export type Target_TrackParentSendState = { kind: "TrackParentSendState" } & TrackParentSendStateTarget
export type Target_AllTrackFxOnOffState = { kind: "AllTrackFxOnOffState" } & AllTrackFxOnOffStateTarget
export type Target_TrackMuteState = { kind: "TrackMuteState" } & TrackMuteStateTarget
export type Target_TrackPeak = { kind: "TrackPeak" } & TrackPeakTarget
export type Target_TrackPhase = { kind: "TrackPhase" } & TrackPhaseTarget
export type Target_TrackSelectionState = { kind: "TrackSelectionState" } & TrackSelectionStateTarget
export type Target_TrackAutomationMode = { kind: "TrackAutomationMode" } & TrackAutomationModeTarget
export type Target_TrackMonitoringMode = { kind: "TrackMonitoringMode" } & TrackMonitoringModeTarget
export type Target_TrackAutomationTouchState = { kind: "TrackAutomationTouchState" } & TrackAutomationTouchStateTarget
export type Target_TrackPan = { kind: "TrackPan" } & TrackPanTarget
export type Target_TrackWidth = { kind: "TrackWidth" } & TrackWidthTarget
export type Target_TrackVolume = { kind: "TrackVolume" } & TrackVolumeTarget
export type Target_TrackTool = { kind: "TrackTool" } & TrackToolTarget
export type Target_TrackVisibility = { kind: "TrackVisibility" } & TrackVisibilityTarget
export type Target_TrackSoloState = { kind: "TrackSoloState" } & TrackSoloStateTarget
export type Target_BrowseFxChain = { kind: "BrowseFxChain" } & BrowseFxChainTarget
export type Target_FxOnOffState = { kind: "FxOnOffState" } & FxOnOffStateTarget
export type Target_FxOnlineOfflineState = { kind: "FxOnlineOfflineState" } & FxOnlineOfflineStateTarget
export type Target_LoadFxSnapshot = { kind: "LoadFxSnapshot" } & LoadFxSnapshotTarget
export type Target_BrowseFxPresets = { kind: "BrowseFxPresets" } & BrowseFxPresetsTarget
export type Target_FxTool = { kind: "FxTool" } & FxToolTarget
export type Target_FxVisibility = { kind: "FxVisibility" } & FxVisibilityTarget
export type Target_FxParameterValue = { kind: "FxParameterValue" } & FxParameterValueTarget
export type Target_FxParameterAutomationTouchState =
	{ kind: "FxParameterAutomationTouchState" }
	& FxParameterAutomationTouchStateTarget
export type Target_RouteAutomationMode = { kind: "RouteAutomationMode" } & RouteAutomationModeTarget
export type Target_RouteMonoState = { kind: "RouteMonoState" } & RouteMonoStateTarget
export type Target_RouteMuteState = { kind: "RouteMuteState" } & RouteMuteStateTarget
export type Target_RoutePhase = { kind: "RoutePhase" } & RoutePhaseTarget
export type Target_RoutePan = { kind: "RoutePan" } & RoutePanTarget
export type Target_RouteVolume = { kind: "RouteVolume" } & RouteVolumeTarget
export type Target_RouteTouchState = { kind: "RouteTouchState" } & RouteTouchStateTarget
export type Target_PlaytimeSlotTransportAction =
	{ kind: "PlaytimeSlotTransportAction" }
	& PlaytimeSlotTransportActionTarget
export type Target_PlaytimeColumnAction = { kind: "PlaytimeColumnAction" } & PlaytimeColumnActionTarget
export type Target_PlaytimeRowAction = { kind: "PlaytimeRowAction" } & PlaytimeRowActionTarget
export type Target_PlaytimeMatrixAction = { kind: "PlaytimeMatrixAction" } & PlaytimeMatrixActionTarget
export type Target_PlaytimeControlUnitScroll = { kind: "PlaytimeControlUnitScroll" } & PlaytimeControlUnitScrollTarget
export type Target_PlaytimeSlotSeek = { kind: "PlaytimeSlotSeek" } & PlaytimeSlotSeekTarget
export type Target_PlaytimeSlotVolume = { kind: "PlaytimeSlotVolume" } & PlaytimeSlotVolumeTarget
export type Target_PlaytimeSlotManagementAction =
	{ kind: "PlaytimeSlotManagementAction" }
	& PlaytimeSlotManagementActionTarget
export type Target_SendMidi = { kind: "SendMidi" } & SendMidiTarget
export type Target_SendOsc = { kind: "SendOsc" } & SendOscTarget
export type Target_Dummy = { kind: "Dummy" } & DummyTarget
export type Target_EnableInstances = { kind: "EnableInstances" } & EnableInstancesTarget
export type Target_EnableMappings = { kind: "EnableMappings" } & EnableMappingsTarget
export type Target_ModifyMapping = { kind: "ModifyMapping" } & ModifyMappingTarget
export type Target_CompartmentParameterValue = { kind: "CompartmentParameterValue" } & CompartmentParameterValueTarget
export type Target_LoadMappingSnapshot = { kind: "LoadMappingSnapshot" } & LoadMappingSnapshotTarget
export type Target_TakeMappingSnapshot = { kind: "TakeMappingSnapshot" } & TakeMappingSnapshotTarget
export type Target_BrowseGroupMappings = { kind: "BrowseGroupMappings" } & BrowseGroupMappingsTarget
export type Target_BrowsePotFilterItems = { kind: "BrowsePotFilterItems" } & BrowsePotFilterItemsTarget
export type Target_BrowsePotPresets = { kind: "BrowsePotPresets" } & BrowsePotPresetsTarget
export type Target_PreviewPotPreset = { kind: "PreviewPotPreset" } & PreviewPotPresetTarget
export type Target_LoadPotPreset = { kind: "LoadPotPreset" } & LoadPotPresetTarget
export type Target_Virtual = { kind: "Virtual" } & VirtualTarget

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

export type MouseAction_MoveTo = { kind: "MoveTo" } & {
	axis: Axis?,
}

export type MouseAction_MoveBy = { kind: "MoveBy" } & {
	axis: Axis?,
}

export type MouseAction_PressOrRelease = { kind: "PressOrRelease" } & {
	button: MouseButton?,
}

export type MouseAction_Scroll = { kind: "Scroll" } & {
	axis: Axis?,
}

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
	play_start_timing: ClipPlayStartTiming?,
	play_stop_timing: ClipPlayStopTiming?,
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
export type ClipManagementAction_AdjustClipSectionLength =
	{ kind: "AdjustClipSectionLength" }
	& AdjustClipSectionLengthAction

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

export type MappingModification_LearnTarget = { kind: "LearnTarget" } & LearnTargetMappingModification
export type MappingModification_SetTargetToLastTouched =
	{ kind: "SetTargetToLastTouched" }
	& SetTargetToLastTouchedMappingModification

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
export type MappingSnapshotDescForLoad_ById = { kind: "ById" } & {
	id: string,
}

export type MappingSnapshotDescForLoad = MappingSnapshotDescForLoad_Initial | MappingSnapshotDescForLoad_ById

export type MappingSnapshotDescForTake_LastLoaded = { kind: "LastLoaded" }
export type MappingSnapshotDescForTake_ById = { kind: "ById" } & {
	id: string,
}

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
export type AutomationModeOverride_Mode = { kind: "Mode" } & {
	mode: AutomationMode,
}

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

export type TrackDescriptor_This = { address: "This" } & {
	commons: TrackDescriptorCommons,
}

export type TrackDescriptor_Master = { address: "Master" } & {
	commons: TrackDescriptorCommons,
}

export type TrackDescriptor_Instance = { address: "Instance" } & {
	commons: TrackDescriptorCommons,
}

export type TrackDescriptor_Selected = { address: "Selected" } & {
	allow_multiple: boolean?,
}

export type TrackDescriptor_Dynamic = { address: "Dynamic" } & {
	commons: TrackDescriptorCommons,
	expression: string,
	scope: TrackScope?,
}

export type TrackDescriptor_ById = { address: "ById" } & {
	commons: TrackDescriptorCommons,
	id: string?,
}

export type TrackDescriptor_ByIndex = { address: "ByIndex" } & {
	commons: TrackDescriptorCommons,
	index: number,
	scope: TrackScope?,
}

export type TrackDescriptor_ByName = { address: "ByName" } & {
	commons: TrackDescriptorCommons,
	name: string,
	allow_multiple: boolean?,
}

export type TrackDescriptor_FromClipColumn = { address: "FromClipColumn" } & {
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

export type BookmarkDescriptor_Marker = { kind: "Marker" } & BookmarkRef
export type BookmarkDescriptor_Region = { kind: "Region" } & BookmarkRef

export type BookmarkDescriptor = BookmarkDescriptor_Marker | BookmarkDescriptor_Region

export type BookmarkRef_ById = {
	id: number,
}

export type BookmarkRef_ByIndex = {
	index: number,
}

export type BookmarkRef = BookmarkRef_ById | BookmarkRef_ByIndex

export type FxDescriptorCommons = {
	fx_must_have_focus: boolean?,
}

export type FxDescriptor_This = { address: "This" } & {
	commons: FxDescriptorCommons,
}

export type FxDescriptor_Focused = { address: "Focused" }
export type FxDescriptor_Instance = { address: "Instance" } & {
	commons: FxDescriptorCommons,
}

export type FxDescriptor_Dynamic = { address: "Dynamic" } & {
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	expression: string,
}

export type FxDescriptor_ById = { address: "ById" } & {
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	id: string?,
}

export type FxDescriptor_ByIndex = { address: "ByIndex" } & {
	commons: FxDescriptorCommons,
	chain: FxChainDescriptor,
	index: number,
}

export type FxDescriptor_ByName = { address: "ByName" } & {
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

export type FxChainDescriptor_Track = { address: "Track" } & {
	track: TrackDescriptor?,
	chain: TrackFxChain?,
}

export type FxChainDescriptor = FxChainDescriptor_Track

export type TrackFxChain = "Normal" | "Input"

export type FxDisplayKind = "FloatingWindow" | "Chain"

export type FxSnapshot = {
	fx_kind: string?,
	fx_name: string?,
	preset_name: string?,
	content: FxSnapshotContent,
}

export type FxSnapshotContent_Chunk = { kind: "Chunk" } & {
	chunk: string,
}

export type FxSnapshotContent = FxSnapshotContent_Chunk

export type FxParameterDescriptor_Dynamic = { address: "Dynamic" } & {
	fx: FxDescriptor?,
	expression: string,
}

export type FxParameterDescriptor_ById = { address: "ById" } & {
	fx: FxDescriptor?,
	index: number,
}

export type FxParameterDescriptor_ByIndex = { address: "ByIndex" } & {
	fx: FxDescriptor?,
	index: number,
}

export type FxParameterDescriptor_ByName = { address: "ByName" } & {
	fx: FxDescriptor?,
	name: string,
}

export type FxParameterDescriptor =
	FxParameterDescriptor_Dynamic
	| FxParameterDescriptor_ById
	| FxParameterDescriptor_ByIndex
	| FxParameterDescriptor_ByName

export type CompartmentParameterDescriptor_ById = { address: "ById" } & {
	index: number,
}

export type CompartmentParameterDescriptor = CompartmentParameterDescriptor_ById

export type RouteDescriptorCommons = {
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
}

export type RouteDescriptor_Dynamic = { address: "Dynamic" } & {
	commons: RouteDescriptorCommons,
	expression: string,
}

export type RouteDescriptor_ById = { address: "ById" } & {
	commons: RouteDescriptorCommons,
	id: string?,
}

export type RouteDescriptor_ByIndex = { address: "ByIndex" } & {
	commons: RouteDescriptorCommons,
	index: number,
}

export type RouteDescriptor_ByName = { address: "ByName" } & {
	commons: RouteDescriptorCommons,
	name: string,
}

export type RouteDescriptor =
	RouteDescriptor_Dynamic
	| RouteDescriptor_ById
	| RouteDescriptor_ByIndex
	| RouteDescriptor_ByName

export type TrackRouteKind = "Send" | "Receive" | "HardwareOutput"

export type ClipSlotDescriptor_Selected = { address: "Selected" }
export type ClipSlotDescriptor_ByIndex = { address: "ByIndex" } & SlotAddress
export type ClipSlotDescriptor_Dynamic = { address: "Dynamic" } & {
	column_expression: string,
	row_expression: string,
}

export type ClipSlotDescriptor = ClipSlotDescriptor_Selected | ClipSlotDescriptor_ByIndex | ClipSlotDescriptor_Dynamic

export type ClipColumnDescriptor_Selected = { address: "Selected" }
export type ClipColumnDescriptor_ByIndex = { address: "ByIndex" } & ColumnAddress
export type ClipColumnDescriptor_Dynamic = { address: "Dynamic" } & {
	expression: string,
}

export type ClipColumnDescriptor =
	ClipColumnDescriptor_Selected
	| ClipColumnDescriptor_ByIndex
	| ClipColumnDescriptor_Dynamic

export type ClipRowDescriptor_Selected = { address: "Selected" }
export type ClipRowDescriptor_ByIndex = { address: "ByIndex" } & RowAddress
export type ClipRowDescriptor_Dynamic = { address: "Dynamic" } & {
	expression: string,
}

export type ClipRowDescriptor = ClipRowDescriptor_Selected | ClipRowDescriptor_ByIndex | ClipRowDescriptor_Dynamic

export type MidiDestination_FxOutput = { kind: "FxOutput" }
export type MidiDestination_FeedbackOutput = { kind: "FeedbackOutput" }

export type MidiDestination = MidiDestination_FxOutput | MidiDestination_FeedbackOutput

export type OscDestination_FeedbackOutput = { kind: "FeedbackOutput" }
export type OscDestination_Device = { kind: "Device" } & {
	id: string,
}

export type OscDestination = OscDestination_FeedbackOutput | OscDestination_Device

export type TrackScope = "AllTracks" | "TracksVisibleInTcp" | "TracksVisibleInMcp"

export type BrowseTracksMode =
	"AllTracks"
	| "TracksVisibleInTcp"
	| "TracksVisibleInTcpAllowTwoSelections"
	| "TracksVisibleInMcp"
	| "TracksVisibleInMcpAllowTwoSelections"

export type TargetValue_Unit = { kind: "Unit" } & {
	value: number,
}

export type TargetValue_Discrete = { kind: "Discrete" } & {
	value: number,
}

export type TargetValue = TargetValue_Unit | TargetValue_Discrete

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
	content_quantization_settings: ContentQuantizationSettings,
	sequencer: MatrixSequencer,
}

export type MatrixSequencer = {
	sequences: { MatrixSequence },
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
	trigger_behavior: TriggerPlayBehavior,
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
	clip_settings: ClipMidiSettings,
}

export type MatrixClipRecordAudioSettings = {
	detect_downbeat: boolean,
	detect_input: boolean,
}

export type RecordLength_OpenEnd = { kind: "OpenEnd" }
export type RecordLength_Quantized = { kind: "Quantized" } & EvenQuantization

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
export type ClipRecordStartTiming_Quantized = { kind: "Quantized" } & EvenQuantization

export type ClipRecordStartTiming =
	ClipRecordStartTiming_LikeClipPlayStartTiming
	| ClipRecordStartTiming_Immediately
	| ClipRecordStartTiming_Quantized

export type ClipRecordStopTiming_LikeClipRecordStartTiming = { kind: "LikeClipRecordStartTiming" }
export type ClipRecordStopTiming_Immediately = { kind: "Immediately" }
export type ClipRecordStopTiming_Quantized = { kind: "Quantized" } & EvenQuantization

export type ClipRecordStopTiming =
	ClipRecordStopTiming_LikeClipRecordStartTiming
	| ClipRecordStopTiming_Immediately
	| ClipRecordStopTiming_Quantized

export type MidiClipRecordMode_Normal = { kind: "Normal" }
export type MidiClipRecordMode_Overdub = { kind: "Overdub" }
export type MidiClipRecordMode_Replace = { kind: "Replace" }

export type MidiClipRecordMode = MidiClipRecordMode_Normal | MidiClipRecordMode_Overdub | MidiClipRecordMode_Replace

export type ClipPlayStartTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStartTiming_Quantized = { kind: "Quantized" } & EvenQuantization

export type ClipPlayStartTiming = ClipPlayStartTiming_Immediately | ClipPlayStartTiming_Quantized

export type ClipPlayStopTiming_LikeClipStartTiming = { kind: "LikeClipStartTiming" }
export type ClipPlayStopTiming_Immediately = { kind: "Immediately" }
export type ClipPlayStopTiming_Quantized = { kind: "Quantized" } & EvenQuantization
export type ClipPlayStopTiming_UntilEndOfClip = { kind: "UntilEndOfClip" }

export type ClipPlayStopTiming =
	ClipPlayStopTiming_LikeClipStartTiming
	| ClipPlayStopTiming_Immediately
	| ClipPlayStopTiming_Quantized
	| ClipPlayStopTiming_UntilEndOfClip

export type ResolvedClipPlayStopTiming = "Immediately" | "Quantized" | "UntilEndOfClip"

export type ClipPlayStartTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStartTimingOverrideAfterRecording_Override = { kind: "Override" } & ClipPlayStartTimingOverride
export type ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming = { kind: "DeriveFromRecordTiming" }

export type ClipPlayStartTimingOverrideAfterRecording =
	ClipPlayStartTimingOverrideAfterRecording_Inherit
	| ClipPlayStartTimingOverrideAfterRecording_Override
	| ClipPlayStartTimingOverrideAfterRecording_DeriveFromRecordTiming

export type ClipPlayStopTimingOverrideAfterRecording_Inherit = { kind: "Inherit" }
export type ClipPlayStopTimingOverrideAfterRecording_Override = { kind: "Override" } & ClipPlayStopTimingOverride
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
	id: ColumnId,
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
	id: RowId,
	name: string?,
	tempo: Bpm?,
	time_signature: TimeSignature?,
}

export type AudioTimeStretchMode_VariSpeed = { kind: "VariSpeed" }
export type AudioTimeStretchMode_KeepingPitch = { kind: "KeepingPitch" } & TimeStretchMode

export type AudioTimeStretchMode = AudioTimeStretchMode_VariSpeed | AudioTimeStretchMode_KeepingPitch

export type VirtualResampleMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualResampleMode_ReaperMode = { kind: "ReaperMode" } & ReaperResampleMode

export type VirtualResampleMode = VirtualResampleMode_ProjectDefault | VirtualResampleMode_ReaperMode

export type ReaperResampleMode = {
	mode: number,
}

export type TimeStretchMode = {
	mode: VirtualTimeStretchMode,
}

export type VirtualTimeStretchMode_ProjectDefault = { kind: "ProjectDefault" }
export type VirtualTimeStretchMode_ReaperMode = { kind: "ReaperMode" } & ReaperPitchShiftMode

export type VirtualTimeStretchMode = VirtualTimeStretchMode_ProjectDefault | VirtualTimeStretchMode_ReaperMode

export type ReaperPitchShiftMode = {
	mode: number,
	sub_mode: number,
}

export type RecordOrigin_TrackInput = { kind: "TrackInput" }
export type RecordOrigin_TrackAudioOutput = { kind: "TrackAudioOutput" }
export type RecordOrigin_FxAudioInput = { kind: "FxAudioInput" } & ChannelRange

export type RecordOrigin = RecordOrigin_TrackInput | RecordOrigin_TrackAudioOutput | RecordOrigin_FxAudioInput

export type SourceOrigin = "Normal" | "Frozen"

export type ChannelRange = {
	first_channel_index: number,
	channel_count: number,
}

export type Slot = {
	id: SlotId,
	row: number,
	clip_old: Clip?,
	clips: { Clip }?,
}

export type Clip = {
	id: ClipId,
	name: string?,
	source: Source,
	frozen_source: Source?,
	active_source: SourceOrigin,
	time_base: ClipTimeBase,
	start_timing: ClipPlayStartTiming?,
	stop_timing: ClipPlayStopTiming?,
	looped: boolean,
	volume: Db,
	color: ClipColor,
	dynamic_section: Section,
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
	on_notes_off: boolean,
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
export type ClipColor_CustomColor = { kind: "CustomColor" } & CustomClipColor
export type ClipColor_PaletteColor = { kind: "PaletteColor" } & PaletteClipColor

export type ClipColor = ClipColor_PlayTrackColor | ClipColor_CustomColor | ClipColor_PaletteColor

export type CustomClipColor = {
	value: RgbColor,
}

export type PaletteClipColor = {
	index: number,
}

export type Source_File = { kind: "File" } & FileSource
export type Source_MidiChunk = { kind: "MidiChunk" } & MidiChunkSource

export type Source = Source_File | Source_MidiChunk

export type FileSource = {
	path: string,
}

export type MidiChunkSource = {
	chunk: string,
}

export type ClipTimeBase_Time = { kind: "Time" }
export type ClipTimeBase_Beat = { kind: "Beat" } & BeatTimeBase

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

--!strict

--- Attention: This file is generated from Rust code! Don't modify it directly!

local playtime = require("playtime")

--- Contains types and helper functions for building ReaLearn presets
local module = {}

--- Complete content of a ReaLearn compartment, including mappings, groups, parameters etc.
export type Compartment = {
	default_group: Group?,
	parameters: { Parameter }?,
	groups: { Group }?,
	mappings: { Mapping }?,
	common_lua: string?,
	custom_data: { [string]: any }?,
	notes: string?,
	unknown_props: BTreeMap<string, any>?,
}
--- Creates a Compartment value.
--- Complete content of a ReaLearn compartment, including mappings, groups, parameters etc.
function module.Compartment(value: Compartment): Compartment
	return value
end

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
--- Creates a Glue value.
function module.Glue(value: Glue): Glue
	return value
end

export type FeedbackValueTable_FromTextToDiscrete = { kind: "FromTextToDiscrete", value: { [string]: number } }

export type FeedbackValueTable_FromTextToContinuous = { kind: "FromTextToContinuous", value: { [string]: number } }
export type FeedbackValueTable = FeedbackValueTable_FromTextToDiscrete | FeedbackValueTable_FromTextToContinuous

--- A type that represents all possible kinds of FeedbackValueTable.
export type FeedbackValueTableKind = "FromTextToDiscrete" | "FromTextToContinuous"

--- Helper table to create FeedbackValueTable values of different kinds.
module.FeedbackValueTable = {}

--- Creates a FeedbackValueTable of kind FromTextToDiscrete.
function module.FeedbackValueTable.FromTextToDiscrete(
	value: DiscreteFeedbackValueTableContent
): FeedbackValueTable_FromTextToDiscrete
	local t: any = table.clone(value)
	t.kind = "FromTextToDiscrete"
	return t
end

--- Creates a FeedbackValueTable of kind FromTextToContinuous.
function module.FeedbackValueTable.FromTextToContinuous(
	value: ContinuousFeedbackValueTableContent
): FeedbackValueTable_FromTextToContinuous
	local t: any = table.clone(value)
	t.kind = "FromTextToContinuous"
	return t
end

export type DiscreteFeedbackValueTableContent = {
	value: { [string]: number },
}
--- Creates a DiscreteFeedbackValueTableContent value.
function module.DiscreteFeedbackValueTableContent(
	value: DiscreteFeedbackValueTableContent
): DiscreteFeedbackValueTableContent
	return value
end

export type ContinuousFeedbackValueTableContent = {
	value: { [string]: number },
}
--- Creates a ContinuousFeedbackValueTableContent value.
function module.ContinuousFeedbackValueTableContent(
	value: ContinuousFeedbackValueTableContent
): ContinuousFeedbackValueTableContent
	return value
end

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

--- A type that represents all possible kinds of FireMode.
export type FireModeKind = "Normal" | "AfterTimeout" | "AfterTimeoutKeepFiring" | "OnSinglePress" | "OnDoublePress"

--- Helper table to create FireMode values of different kinds.
module.FireMode = {}

--- Creates a FireMode of kind Normal.
function module.FireMode.Normal(value: NormalFireMode): FireMode_Normal
	local t: any = table.clone(value)
	t.kind = "Normal"
	return t
end

--- Creates a FireMode of kind AfterTimeout.
function module.FireMode.AfterTimeout(value: AfterTimeoutFireMode): FireMode_AfterTimeout
	local t: any = table.clone(value)
	t.kind = "AfterTimeout"
	return t
end

--- Creates a FireMode of kind AfterTimeoutKeepFiring.
function module.FireMode.AfterTimeoutKeepFiring(value: AfterTimeoutKeepFiringFireMode): FireMode_AfterTimeoutKeepFiring
	local t: any = table.clone(value)
	t.kind = "AfterTimeoutKeepFiring"
	return t
end

--- Creates a FireMode of kind OnSinglePress.
function module.FireMode.OnSinglePress(value: OnSinglePressFireMode): FireMode_OnSinglePress
	local t: any = table.clone(value)
	t.kind = "OnSinglePress"
	return t
end

--- Creates a FireMode of kind OnDoublePress.
function module.FireMode.OnDoublePress(): FireMode_OnDoublePress
	return {
		kind = "OnDoublePress",
	}
end

export type NormalFireMode = {
	press_duration_interval: Interval<number>?,
}
--- Creates a NormalFireMode value.
function module.NormalFireMode(value: NormalFireMode): NormalFireMode
	return value
end

export type AfterTimeoutFireMode = {
	timeout: number?,
}
--- Creates a AfterTimeoutFireMode value.
function module.AfterTimeoutFireMode(value: AfterTimeoutFireMode): AfterTimeoutFireMode
	return value
end

export type AfterTimeoutKeepFiringFireMode = {
	timeout: number?,
	rate: number?,
}
--- Creates a AfterTimeoutKeepFiringFireMode value.
function module.AfterTimeoutKeepFiringFireMode(value: AfterTimeoutKeepFiringFireMode): AfterTimeoutKeepFiringFireMode
	return value
end

export type OnSinglePressFireMode = {
	max_duration: number?,
}
--- Creates a OnSinglePressFireMode value.
function module.OnSinglePressFireMode(value: OnSinglePressFireMode): OnSinglePressFireMode
	return value
end

export type VirtualColor_Rgb = RgbColor

export type VirtualColor_Prop = PropColor
export type VirtualColor = VirtualColor_Rgb | VirtualColor_Prop

--- Creates a VirtualColor value.
function module.VirtualColor(value: VirtualColor): VirtualColor
	return value
end

export type RgbColor = { number }

export type PropColor = {
	prop: string,
}
--- Creates a PropColor value.
function module.PropColor(value: PropColor): PropColor
	return value
end

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
--- Creates a FeedbackCommons value.
function module.FeedbackCommons(value: FeedbackCommons): FeedbackCommons
	return value
end

export type Feedback_Numeric = {
	kind: "Numeric",
	color: VirtualColor?,
	background_color: VirtualColor?,
	transformation: string?,
}

export type Feedback_Text = {
	kind: "Text",
	color: VirtualColor?,
	background_color: VirtualColor?,
	text_expression: string?,
}

export type Feedback_Dynamic = { kind: "Dynamic", color: VirtualColor?, background_color: VirtualColor?, script: string? }
export type Feedback = Feedback_Numeric | Feedback_Text | Feedback_Dynamic

--- A type that represents all possible kinds of Feedback.
export type FeedbackKind = "Numeric" | "Text" | "Dynamic"

--- Helper table to create Feedback values of different kinds.
module.Feedback = {}

--- Creates a Feedback of kind Numeric.
function module.Feedback.Numeric(value: NumericFeedback): Feedback_Numeric
	local t: any = table.clone(value)
	t.kind = "Numeric"
	return t
end

--- Creates a Feedback of kind Text.
function module.Feedback.Text(value: TextFeedback): Feedback_Text
	local t: any = table.clone(value)
	t.kind = "Text"
	return t
end

--- Creates a Feedback of kind Dynamic.
function module.Feedback.Dynamic(value: DynamicFeedback): Feedback_Dynamic
	local t: any = table.clone(value)
	t.kind = "Dynamic"
	return t
end

export type NumericFeedback = {
	color: VirtualColor?,
	background_color: VirtualColor?,
	transformation: string?,
}
--- Creates a NumericFeedback value.
function module.NumericFeedback(value: NumericFeedback): NumericFeedback
	return value
end

export type TextFeedback = {
	color: VirtualColor?,
	background_color: VirtualColor?,
	text_expression: string?,
}
--- Creates a TextFeedback value.
function module.TextFeedback(value: TextFeedback): TextFeedback
	return value
end

export type DynamicFeedback = {
	color: VirtualColor?,
	background_color: VirtualColor?,
	script: string?,
}
--- Creates a DynamicFeedback value.
function module.DynamicFeedback(value: DynamicFeedback): DynamicFeedback
	return value
end

export type Interval<T> = { T }

export type Group = {
	id: string?,
	name: string?,
	tags: { string }?,
	control_enabled: boolean?,
	feedback_enabled: boolean?,
	activation_condition: ActivationCondition?,
}
--- Creates a Group value.
function module.Group(value: Group): Group
	return value
end

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
--- Creates a Mapping value.
function module.Mapping(value: Mapping): Mapping
	return value
end

export type LifecycleHook = {
	send_midi_feedback: { SendMidiFeedbackAction }?,
}
--- Creates a LifecycleHook value.
function module.LifecycleHook(value: LifecycleHook): LifecycleHook
	return value
end

export type SendMidiFeedbackAction_Raw = { kind: "Raw", message: RawMidiMessage }
export type SendMidiFeedbackAction = SendMidiFeedbackAction_Raw

--- A type that represents all possible kinds of SendMidiFeedbackAction.
export type SendMidiFeedbackActionKind = "Raw"

--- Helper table to create SendMidiFeedbackAction values of different kinds.
module.SendMidiFeedbackAction = {}

--- Creates a SendMidiFeedbackAction of kind Raw.
function module.SendMidiFeedbackAction.Raw(value: { message: RawMidiMessage }): SendMidiFeedbackAction_Raw
	local t: any = table.clone(value)
	t.kind = "Raw"
	return t
end

export type RawMidiMessage_HexString = string

export type RawMidiMessage_ByteArray = { number }
export type RawMidiMessage = RawMidiMessage_HexString | RawMidiMessage_ByteArray

--- Creates a RawMidiMessage value.
function module.RawMidiMessage(value: RawMidiMessage): RawMidiMessage
	return value
end

export type SuccessAudioFeedback_Simple = { kind: "Simple" }
export type SuccessAudioFeedback = SuccessAudioFeedback_Simple

--- A type that represents all possible kinds of SuccessAudioFeedback.
export type SuccessAudioFeedbackKind = "Simple"

--- Helper table to create SuccessAudioFeedback values of different kinds.
module.SuccessAudioFeedback = {}

--- Creates a SuccessAudioFeedback of kind Simple.
function module.SuccessAudioFeedback.Simple(): SuccessAudioFeedback_Simple
	return {
		kind = "Simple",
	}
end

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

--- A type that represents all possible kinds of ActivationCondition.
export type ActivationConditionKind = "Modifier" | "Bank" | "Eel" | "Expression" | "TargetValue"

--- Helper table to create ActivationCondition values of different kinds.
module.ActivationCondition = {}

--- Creates a ActivationCondition of kind Modifier.
function module.ActivationCondition.Modifier(value: ModifierActivationCondition): ActivationCondition_Modifier
	local t: any = table.clone(value)
	t.kind = "Modifier"
	return t
end

--- Creates a ActivationCondition of kind Bank.
function module.ActivationCondition.Bank(value: BankActivationCondition): ActivationCondition_Bank
	local t: any = table.clone(value)
	t.kind = "Bank"
	return t
end

--- Creates a ActivationCondition of kind Eel.
function module.ActivationCondition.Eel(value: EelActivationCondition): ActivationCondition_Eel
	local t: any = table.clone(value)
	t.kind = "Eel"
	return t
end

--- Creates a ActivationCondition of kind Expression.
function module.ActivationCondition.Expression(value: ExpressionActivationCondition): ActivationCondition_Expression
	local t: any = table.clone(value)
	t.kind = "Expression"
	return t
end

--- Creates a ActivationCondition of kind TargetValue.
function module.ActivationCondition.TargetValue(value: TargetValueActivationCondition): ActivationCondition_TargetValue
	local t: any = table.clone(value)
	t.kind = "TargetValue"
	return t
end

export type ModifierActivationCondition = {
	modifiers: { ModifierState }?,
}
--- Creates a ModifierActivationCondition value.
function module.ModifierActivationCondition(value: ModifierActivationCondition): ModifierActivationCondition
	return value
end

export type ModifierState = {
	parameter: ParamRef,
	on: boolean,
}
--- Creates a ModifierState value.
function module.ModifierState(value: ModifierState): ModifierState
	return value
end

export type BankActivationCondition = {
	parameter: ParamRef,
	bank_index: number,
}
--- Creates a BankActivationCondition value.
function module.BankActivationCondition(value: BankActivationCondition): BankActivationCondition
	return value
end

export type EelActivationCondition = {
	condition: string,
}
--- Creates a EelActivationCondition value.
function module.EelActivationCondition(value: EelActivationCondition): EelActivationCondition
	return value
end

export type ExpressionActivationCondition = {
	condition: string,
}
--- Creates a ExpressionActivationCondition value.
function module.ExpressionActivationCondition(value: ExpressionActivationCondition): ExpressionActivationCondition
	return value
end

export type TargetValueActivationCondition = {
	mapping: string?,
	condition: string,
}
--- Creates a TargetValueActivationCondition value.
function module.TargetValueActivationCondition(value: TargetValueActivationCondition): TargetValueActivationCondition
	return value
end

export type ParamRef_Index = number

export type ParamRef_Key = string
export type ParamRef = ParamRef_Index | ParamRef_Key

--- Creates a ParamRef value.
function module.ParamRef(value: ParamRef): ParamRef
	return value
end

export type VirtualControlElementId_Indexed = number

export type VirtualControlElementId_Named = string
export type VirtualControlElementId = VirtualControlElementId_Indexed | VirtualControlElementId_Named

--- Creates a VirtualControlElementId value.
function module.VirtualControlElementId(value: VirtualControlElementId): VirtualControlElementId
	return value
end

export type VirtualControlElementCharacter = "Multi" | "Button"

export type OscArgument = {
	index: number?,
	arg_kind: OscArgKind?,
	value_range: Interval<number>?,
}
--- Creates a OscArgument value.
function module.OscArgument(value: OscArgument): OscArgument
	return value
end

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
--- Creates a Parameter value.
function module.Parameter(value: Parameter): Parameter
	return value
end

export type Source_None = { kind: "None" }

export type Source_MidiDeviceChanges = { kind: "MidiDeviceChanges" }

export type Source_RealearnInstanceStart = { kind: "RealearnInstanceStart" }

export type Source_RealearnCompartmentLoaded = { kind: "RealearnCompartmentLoaded" }

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

export type Source_StreamDeck = { kind: "StreamDeck", button_index: number, button_design: StreamDeckButtonDesign? }

export type Source_Virtual = { kind: "Virtual", id: VirtualControlElementId, character: VirtualControlElementCharacter? }
export type Source =
	Source_None
	| Source_MidiDeviceChanges
	| Source_RealearnInstanceStart
	| Source_RealearnCompartmentLoaded
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
	| Source_StreamDeck
	| Source_Virtual

--- A type that represents all possible kinds of Source.
export type SourceKind =
	"None"
	| "MidiDeviceChanges"
	| "RealearnInstanceStart"
	| "RealearnCompartmentLoaded"
	| "Timer"
	| "RealearnParameter"
	| "Speech"
	| "MidiNoteVelocity"
	| "MidiNoteKeyNumber"
	| "MidiPolyphonicKeyPressureAmount"
	| "MidiControlChangeValue"
	| "MidiProgramChangeNumber"
	| "MidiSpecificProgramChange"
	| "MidiChannelPressureAmount"
	| "MidiPitchBendChangeValue"
	| "MidiParameterNumberValue"
	| "MidiClockTempo"
	| "MidiClockTransport"
	| "MidiRaw"
	| "MidiScript"
	| "MackieLcd"
	| "XTouchMackieLcd"
	| "MackieSevenSegmentDisplay"
	| "SlKeyboardDisplay"
	| "SiniConE24Display"
	| "LaunchpadProScrollingTextDisplay"
	| "Osc"
	| "Key"
	| "StreamDeck"
	| "Virtual"

--- Helper table to create Source values of different kinds.
module.Source = {}

--- Creates a Source of kind None.
function module.Source.None(): Source_None
	return {
		kind = "None",
	}
end

--- Creates a Source of kind MidiDeviceChanges.
function module.Source.MidiDeviceChanges(): Source_MidiDeviceChanges
	return {
		kind = "MidiDeviceChanges",
	}
end

--- Creates a Source of kind RealearnInstanceStart.
function module.Source.RealearnInstanceStart(): Source_RealearnInstanceStart
	return {
		kind = "RealearnInstanceStart",
	}
end

--- Creates a Source of kind RealearnCompartmentLoaded.
function module.Source.RealearnCompartmentLoaded(): Source_RealearnCompartmentLoaded
	return {
		kind = "RealearnCompartmentLoaded",
	}
end

--- Creates a Source of kind Timer.
function module.Source.Timer(value: TimerSource): Source_Timer
	local t: any = table.clone(value)
	t.kind = "Timer"
	return t
end

--- Creates a Source of kind RealearnParameter.
function module.Source.RealearnParameter(value: RealearnParameterSource): Source_RealearnParameter
	local t: any = table.clone(value)
	t.kind = "RealearnParameter"
	return t
end

--- Creates a Source of kind Speech.
function module.Source.Speech(): Source_Speech
	return {
		kind = "Speech",
	}
end

--- Creates a Source of kind MidiNoteVelocity.
function module.Source.MidiNoteVelocity(value: MidiNoteVelocitySource): Source_MidiNoteVelocity
	local t: any = table.clone(value)
	t.kind = "MidiNoteVelocity"
	return t
end

--- Creates a Source of kind MidiNoteKeyNumber.
function module.Source.MidiNoteKeyNumber(value: MidiNoteKeyNumberSource): Source_MidiNoteKeyNumber
	local t: any = table.clone(value)
	t.kind = "MidiNoteKeyNumber"
	return t
end

--- Creates a Source of kind MidiPolyphonicKeyPressureAmount.
function module.Source.MidiPolyphonicKeyPressureAmount(
	value: MidiPolyphonicKeyPressureAmountSource
): Source_MidiPolyphonicKeyPressureAmount
	local t: any = table.clone(value)
	t.kind = "MidiPolyphonicKeyPressureAmount"
	return t
end

--- Creates a Source of kind MidiControlChangeValue.
function module.Source.MidiControlChangeValue(value: MidiControlChangeValueSource): Source_MidiControlChangeValue
	local t: any = table.clone(value)
	t.kind = "MidiControlChangeValue"
	return t
end

--- Creates a Source of kind MidiProgramChangeNumber.
function module.Source.MidiProgramChangeNumber(value: MidiProgramChangeNumberSource): Source_MidiProgramChangeNumber
	local t: any = table.clone(value)
	t.kind = "MidiProgramChangeNumber"
	return t
end

--- Creates a Source of kind MidiSpecificProgramChange.
function module.Source.MidiSpecificProgramChange(
	value: MidiSpecificProgramChangeSource
): Source_MidiSpecificProgramChange
	local t: any = table.clone(value)
	t.kind = "MidiSpecificProgramChange"
	return t
end

--- Creates a Source of kind MidiChannelPressureAmount.
function module.Source.MidiChannelPressureAmount(
	value: MidiChannelPressureAmountSource
): Source_MidiChannelPressureAmount
	local t: any = table.clone(value)
	t.kind = "MidiChannelPressureAmount"
	return t
end

--- Creates a Source of kind MidiPitchBendChangeValue.
function module.Source.MidiPitchBendChangeValue(value: MidiPitchBendChangeValueSource): Source_MidiPitchBendChangeValue
	local t: any = table.clone(value)
	t.kind = "MidiPitchBendChangeValue"
	return t
end

--- Creates a Source of kind MidiParameterNumberValue.
function module.Source.MidiParameterNumberValue(value: MidiParameterNumberValueSource): Source_MidiParameterNumberValue
	local t: any = table.clone(value)
	t.kind = "MidiParameterNumberValue"
	return t
end

--- Creates a Source of kind MidiClockTempo.
function module.Source.MidiClockTempo(): Source_MidiClockTempo
	return {
		kind = "MidiClockTempo",
	}
end

--- Creates a Source of kind MidiClockTransport.
function module.Source.MidiClockTransport(value: MidiClockTransportSource): Source_MidiClockTransport
	local t: any = table.clone(value)
	t.kind = "MidiClockTransport"
	return t
end

--- Creates a Source of kind MidiRaw.
function module.Source.MidiRaw(value: MidiRawSource): Source_MidiRaw
	local t: any = table.clone(value)
	t.kind = "MidiRaw"
	return t
end

--- Creates a Source of kind MidiScript.
function module.Source.MidiScript(value: MidiScriptSource): Source_MidiScript
	local t: any = table.clone(value)
	t.kind = "MidiScript"
	return t
end

--- Creates a Source of kind MackieLcd.
function module.Source.MackieLcd(value: MackieLcdSource): Source_MackieLcd
	local t: any = table.clone(value)
	t.kind = "MackieLcd"
	return t
end

--- Creates a Source of kind XTouchMackieLcd.
function module.Source.XTouchMackieLcd(value: XTouchMackieLcdSource): Source_XTouchMackieLcd
	local t: any = table.clone(value)
	t.kind = "XTouchMackieLcd"
	return t
end

--- Creates a Source of kind MackieSevenSegmentDisplay.
function module.Source.MackieSevenSegmentDisplay(
	value: MackieSevenSegmentDisplaySource
): Source_MackieSevenSegmentDisplay
	local t: any = table.clone(value)
	t.kind = "MackieSevenSegmentDisplay"
	return t
end

--- Creates a Source of kind SlKeyboardDisplay.
function module.Source.SlKeyboardDisplay(value: SlKeyboardDisplaySource): Source_SlKeyboardDisplay
	local t: any = table.clone(value)
	t.kind = "SlKeyboardDisplay"
	return t
end

--- Creates a Source of kind SiniConE24Display.
function module.Source.SiniConE24Display(value: SiniConE24DisplaySource): Source_SiniConE24Display
	local t: any = table.clone(value)
	t.kind = "SiniConE24Display"
	return t
end

--- Creates a Source of kind LaunchpadProScrollingTextDisplay.
function module.Source.LaunchpadProScrollingTextDisplay(): Source_LaunchpadProScrollingTextDisplay
	return {
		kind = "LaunchpadProScrollingTextDisplay",
	}
end

--- Creates a Source of kind Osc.
function module.Source.Osc(value: OscSource): Source_Osc
	local t: any = table.clone(value)
	t.kind = "Osc"
	return t
end

--- Creates a Source of kind Key.
function module.Source.Key(value: KeySource): Source_Key
	local t: any = table.clone(value)
	t.kind = "Key"
	return t
end

--- Creates a Source of kind StreamDeck.
function module.Source.StreamDeck(value: StreamDeckSource): Source_StreamDeck
	local t: any = table.clone(value)
	t.kind = "StreamDeck"
	return t
end

--- Creates a Source of kind Virtual.
function module.Source.Virtual(value: VirtualSource): Source_Virtual
	local t: any = table.clone(value)
	t.kind = "Virtual"
	return t
end

export type FeedbackBehavior = "Normal" | "SendFeedbackAfterControl" | "PreventEchoFeedback"

export type MidiNoteVelocitySource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}
--- Creates a MidiNoteVelocitySource value.
function module.MidiNoteVelocitySource(value: MidiNoteVelocitySource): MidiNoteVelocitySource
	return value
end

export type MidiNoteKeyNumberSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
--- Creates a MidiNoteKeyNumberSource value.
function module.MidiNoteKeyNumberSource(value: MidiNoteKeyNumberSource): MidiNoteKeyNumberSource
	return value
end

export type MidiPolyphonicKeyPressureAmountSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	key_number: number?,
}
--- Creates a MidiPolyphonicKeyPressureAmountSource value.
function module.MidiPolyphonicKeyPressureAmountSource(
	value: MidiPolyphonicKeyPressureAmountSource
): MidiPolyphonicKeyPressureAmountSource
	return value
end

export type MidiControlChangeValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	controller_number: number?,
	character: SourceCharacter?,
	fourteen_bit: boolean?,
}
--- Creates a MidiControlChangeValueSource value.
function module.MidiControlChangeValueSource(value: MidiControlChangeValueSource): MidiControlChangeValueSource
	return value
end

export type MidiProgramChangeNumberSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
--- Creates a MidiProgramChangeNumberSource value.
function module.MidiProgramChangeNumberSource(value: MidiProgramChangeNumberSource): MidiProgramChangeNumberSource
	return value
end

export type MidiSpecificProgramChangeSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	program_number: number?,
}
--- Creates a MidiSpecificProgramChangeSource value.
function module.MidiSpecificProgramChangeSource(value: MidiSpecificProgramChangeSource): MidiSpecificProgramChangeSource
	return value
end

export type MidiChannelPressureAmountSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
--- Creates a MidiChannelPressureAmountSource value.
function module.MidiChannelPressureAmountSource(value: MidiChannelPressureAmountSource): MidiChannelPressureAmountSource
	return value
end

export type MidiPitchBendChangeValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
}
--- Creates a MidiPitchBendChangeValueSource value.
function module.MidiPitchBendChangeValueSource(value: MidiPitchBendChangeValueSource): MidiPitchBendChangeValueSource
	return value
end

export type MidiParameterNumberValueSource = {
	feedback_behavior: FeedbackBehavior?,
	channel: number?,
	number: number?,
	fourteen_bit: boolean?,
	registered: boolean?,
	character: SourceCharacter?,
}
--- Creates a MidiParameterNumberValueSource value.
function module.MidiParameterNumberValueSource(value: MidiParameterNumberValueSource): MidiParameterNumberValueSource
	return value
end

export type MidiClockTransportSource = {
	message: MidiClockTransportMessage?,
}
--- Creates a MidiClockTransportSource value.
function module.MidiClockTransportSource(value: MidiClockTransportSource): MidiClockTransportSource
	return value
end

export type MidiRawSource = {
	feedback_behavior: FeedbackBehavior?,
	pattern: string?,
	character: SourceCharacter?,
}
--- Creates a MidiRawSource value.
function module.MidiRawSource(value: MidiRawSource): MidiRawSource
	return value
end

export type MidiScriptSource = {
	script_kind: MidiScriptKind?,
	script: string?,
}
--- Creates a MidiScriptSource value.
function module.MidiScriptSource(value: MidiScriptSource): MidiScriptSource
	return value
end

--- Kind of a MIDI script
export type MidiScriptKind = "Eel" | "Lua"

export type SourceCharacter = "Range" | "Button" | "Relative1" | "Relative2" | "Relative3" | "StatefulButton"

export type MidiClockTransportMessage = "Start" | "Continue" | "Stop"

export type MackieLcdSource = {
	extender_index: number?,
	channel: number?,
	line: number?,
}
--- Creates a MackieLcdSource value.
function module.MackieLcdSource(value: MackieLcdSource): MackieLcdSource
	return value
end

export type XTouchMackieLcdSource = {
	extender_index: number?,
	channel: number?,
	line: number?,
}
--- Creates a XTouchMackieLcdSource value.
function module.XTouchMackieLcdSource(value: XTouchMackieLcdSource): XTouchMackieLcdSource
	return value
end

export type SlKeyboardDisplaySource = {
	section: number?,
	line: number?,
}
--- Creates a SlKeyboardDisplaySource value.
function module.SlKeyboardDisplaySource(value: SlKeyboardDisplaySource): SlKeyboardDisplaySource
	return value
end

export type MackieSevenSegmentDisplaySource = {
	scope: MackieSevenSegmentDisplayScope?,
}
--- Creates a MackieSevenSegmentDisplaySource value.
function module.MackieSevenSegmentDisplaySource(value: MackieSevenSegmentDisplaySource): MackieSevenSegmentDisplaySource
	return value
end

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
--- Creates a SiniConE24DisplaySource value.
function module.SiniConE24DisplaySource(value: SiniConE24DisplaySource): SiniConE24DisplaySource
	return value
end

export type OscSource = {
	feedback_behavior: FeedbackBehavior?,
	address: string?,
	argument: OscArgument?,
	relative: boolean?,
	feedback_arguments: { string }?,
}
--- Creates a OscSource value.
function module.OscSource(value: OscSource): OscSource
	return value
end

export type RealearnParameterSource = {
	parameter_index: number,
}
--- Creates a RealearnParameterSource value.
function module.RealearnParameterSource(value: RealearnParameterSource): RealearnParameterSource
	return value
end

export type TimerSource = {
	duration: number,
}
--- Creates a TimerSource value.
function module.TimerSource(value: TimerSource): TimerSource
	return value
end

export type KeySource = {
	keystroke: Keystroke?,
}
--- Creates a KeySource value.
function module.KeySource(value: KeySource): KeySource
	return value
end

export type StreamDeckSource = {
	button_index: number,
	button_design: StreamDeckButtonDesign?,
}
--- Creates a StreamDeckSource value.
function module.StreamDeckSource(value: StreamDeckSource): StreamDeckSource
	return value
end

export type StreamDeckButtonDesign = {
	background: StreamDeckButtonBackground?,
	foreground: StreamDeckButtonForeground?,
	static_text: string?,
}
--- Creates a StreamDeckButtonDesign value.
function module.StreamDeckButtonDesign(value: StreamDeckButtonDesign): StreamDeckButtonDesign
	return value
end

export type StreamDeckButtonForeground_None = { kind: "None" }

export type StreamDeckButtonForeground_FadingColor = { kind: "FadingColor" }

export type StreamDeckButtonForeground_FadingImage = { kind: "FadingImage", path: string }

export type StreamDeckButtonForeground_SlidingImage = { kind: "SlidingImage", path: string }

export type StreamDeckButtonForeground_FullBar = { kind: "FullBar" }

export type StreamDeckButtonForeground_Knob = { kind: "Knob" }
export type StreamDeckButtonForeground =
	StreamDeckButtonForeground_None
	| StreamDeckButtonForeground_FadingColor
	| StreamDeckButtonForeground_FadingImage
	| StreamDeckButtonForeground_SlidingImage
	| StreamDeckButtonForeground_FullBar
	| StreamDeckButtonForeground_Knob

--- A type that represents all possible kinds of StreamDeckButtonForeground.
export type StreamDeckButtonForegroundKind = "None" | "FadingColor" | "FadingImage" | "SlidingImage" | "FullBar" | "Knob"

--- Helper table to create StreamDeckButtonForeground values of different kinds.
module.StreamDeckButtonForeground = {}

--- Creates a StreamDeckButtonForeground of kind None.
function module.StreamDeckButtonForeground.None(): StreamDeckButtonForeground_None
	return {
		kind = "None",
	}
end

--- Creates a StreamDeckButtonForeground of kind FadingColor.
function module.StreamDeckButtonForeground.FadingColor(
	value: StreamDeckButtonFadingColorForeground
): StreamDeckButtonForeground_FadingColor
	local t: any = table.clone(value)
	t.kind = "FadingColor"
	return t
end

--- Creates a StreamDeckButtonForeground of kind FadingImage.
function module.StreamDeckButtonForeground.FadingImage(
	value: StreamDeckButtonFadingImageForeground
): StreamDeckButtonForeground_FadingImage
	local t: any = table.clone(value)
	t.kind = "FadingImage"
	return t
end

--- Creates a StreamDeckButtonForeground of kind SlidingImage.
function module.StreamDeckButtonForeground.SlidingImage(
	value: StreamDeckButtonSlidingImageForeground
): StreamDeckButtonForeground_SlidingImage
	local t: any = table.clone(value)
	t.kind = "SlidingImage"
	return t
end

--- Creates a StreamDeckButtonForeground of kind FullBar.
function module.StreamDeckButtonForeground.FullBar(
	value: StreamDeckButtonFullBarForeground
): StreamDeckButtonForeground_FullBar
	local t: any = table.clone(value)
	t.kind = "FullBar"
	return t
end

--- Creates a StreamDeckButtonForeground of kind Knob.
function module.StreamDeckButtonForeground.Knob(value: StreamDeckButtonKnobForeground): StreamDeckButtonForeground_Knob
	local t: any = table.clone(value)
	t.kind = "Knob"
	return t
end

export type StreamDeckButtonBackground_Color = { kind: "Color" }

export type StreamDeckButtonBackground_Image = { kind: "Image", path: string }
export type StreamDeckButtonBackground = StreamDeckButtonBackground_Color | StreamDeckButtonBackground_Image

--- A type that represents all possible kinds of StreamDeckButtonBackground.
export type StreamDeckButtonBackgroundKind = "Color" | "Image"

--- Helper table to create StreamDeckButtonBackground values of different kinds.
module.StreamDeckButtonBackground = {}

--- Creates a StreamDeckButtonBackground of kind Color.
function module.StreamDeckButtonBackground.Color(
	value: StreamDeckButtonColorBackground
): StreamDeckButtonBackground_Color
	local t: any = table.clone(value)
	t.kind = "Color"
	return t
end

--- Creates a StreamDeckButtonBackground of kind Image.
function module.StreamDeckButtonBackground.Image(
	value: StreamDeckButtonImageBackground
): StreamDeckButtonBackground_Image
	local t: any = table.clone(value)
	t.kind = "Image"
	return t
end

export type StreamDeckButtonFadingImageForeground = {
	path: string,
}
--- Creates a StreamDeckButtonFadingImageForeground value.
function module.StreamDeckButtonFadingImageForeground(
	value: StreamDeckButtonFadingImageForeground
): StreamDeckButtonFadingImageForeground
	return value
end

export type StreamDeckButtonSlidingImageForeground = {
	path: string,
}
--- Creates a StreamDeckButtonSlidingImageForeground value.
function module.StreamDeckButtonSlidingImageForeground(
	value: StreamDeckButtonSlidingImageForeground
): StreamDeckButtonSlidingImageForeground
	return value
end

export type StreamDeckButtonFadingColorForeground = {}
--- Creates a StreamDeckButtonFadingColorForeground value.
function module.StreamDeckButtonFadingColorForeground(
	value: StreamDeckButtonFadingColorForeground
): StreamDeckButtonFadingColorForeground
	return value
end

export type StreamDeckButtonFullBarForeground = {}
--- Creates a StreamDeckButtonFullBarForeground value.
function module.StreamDeckButtonFullBarForeground(
	value: StreamDeckButtonFullBarForeground
): StreamDeckButtonFullBarForeground
	return value
end

export type StreamDeckButtonKnobForeground = {}
--- Creates a StreamDeckButtonKnobForeground value.
function module.StreamDeckButtonKnobForeground(value: StreamDeckButtonKnobForeground): StreamDeckButtonKnobForeground
	return value
end

export type StreamDeckButtonImageBackground = {
	path: string,
}
--- Creates a StreamDeckButtonImageBackground value.
function module.StreamDeckButtonImageBackground(value: StreamDeckButtonImageBackground): StreamDeckButtonImageBackground
	return value
end

export type StreamDeckButtonColorBackground = {}
--- Creates a StreamDeckButtonColorBackground value.
function module.StreamDeckButtonColorBackground(value: StreamDeckButtonColorBackground): StreamDeckButtonColorBackground
	return value
end

export type Keystroke = {
	modifiers: number,
	key: number,
}
--- Creates a Keystroke value.
function module.Keystroke(value: Keystroke): Keystroke
	return value
end

export type VirtualSource = {
	id: VirtualControlElementId,
	character: VirtualControlElementCharacter?,
}
--- Creates a VirtualSource value.
function module.VirtualSource(value: VirtualSource): VirtualSource
	return value
end

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

--- Which target invocations to observe, based on causality. E.g. only those not triggered by
--- ReaLearn (would pick up invocations triggered by mouse interaction with REAPER but not by
--- ReaLearn mapping control).
export type TargetTouchCause = "Any" | "Realearn" | "Reaper"

export type Target_Mouse = { kind: "Mouse", unit: TargetUnit?, action: MouseAction }

export type Target_LastTouched = {
	kind: "LastTouched",
	unit: TargetUnit?,
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}

export type Target_AutomationModeOverride = {
	kind: "AutomationModeOverride",
	unit: TargetUnit?,
	override_value: AutomationModeOverride?,
}

export type Target_ReaperAction = {
	kind: "ReaperAction",
	unit: TargetUnit?,
	scope: ActionScope?,
	command: ReaperCommand?,
	invocation: ActionInvocationKind?,
	track: TrackDescriptor?,
}

export type Target_TransportAction = { kind: "TransportAction", unit: TargetUnit?, action: TransportAction }

export type Target_AnyOn = { kind: "AnyOn", unit: TargetUnit?, parameter: AnyOnParameter }

export type Target_BrowseTracks = {
	kind: "BrowseTracks",
	unit: TargetUnit?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
	mode: BrowseTracksMode?,
}

export type Target_Seek = {
	kind: "Seek",
	unit: TargetUnit?,
	use_time_selection: boolean?,
	use_loop_points: boolean?,
	use_regions: boolean?,
	use_project: boolean?,
	move_view: boolean?,
	seek_play: boolean?,
	feedback_resolution: FeedbackResolution?,
	behavior: SeekBehavior?,
}

export type Target_PlayRate = { kind: "PlayRate", unit: TargetUnit? }

export type Target_Tempo = { kind: "Tempo", unit: TargetUnit? }

export type Target_GoToBookmark = {
	kind: "GoToBookmark",
	unit: TargetUnit?,
	bookmark: BookmarkDescriptor,
	set_time_selection: boolean?,
	set_loop_points: boolean?,
	seek_behavior: SeekBehavior?,
}

export type Target_TrackArmState = {
	kind: "TrackArmState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackParentSendState = {
	kind: "TrackParentSendState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
}

export type Target_AllTrackFxOnOffState = {
	kind: "AllTrackFxOnOffState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
}

export type Target_TrackMuteState = {
	kind: "TrackMuteState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackPeak = { kind: "TrackPeak", unit: TargetUnit?, track: TrackDescriptor? }

export type Target_TrackPhase = {
	kind: "TrackPhase",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackSelectionState = {
	kind: "TrackSelectionState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
}

export type Target_TrackAutomationMode = {
	kind: "TrackAutomationMode",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: AutomationMode,
}

export type Target_TrackMonitoringMode = {
	kind: "TrackMonitoringMode",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: MonitoringMode,
	use_selection_ganging: boolean?,
}

export type Target_TrackAutomationTouchState = {
	kind: "TrackAutomationTouchState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
	touched_parameter: TouchedTrackParameter,
}

export type Target_TrackPan = {
	kind: "TrackPan",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackWidth = {
	kind: "TrackWidth",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackVolume = {
	kind: "TrackVolume",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_TrackTool = {
	kind: "TrackTool",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	action: TrackToolAction?,
	instance_tags: { string }?,
}

export type Target_TrackVisibility = {
	kind: "TrackVisibility",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	area: TrackArea,
}

export type Target_TrackSoloState = {
	kind: "TrackSoloState",
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	behavior: SoloBehavior?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}

export type Target_BrowseFxChain = {
	kind: "BrowseFxChain",
	unit: TargetUnit?,
	chain: FxChainDescriptor,
	display_kind: FxDisplayKind?,
}

export type Target_FxOnOffState = { kind: "FxOnOffState", unit: TargetUnit?, fx: FxDescriptor? }

export type Target_FxOnlineOfflineState = { kind: "FxOnlineOfflineState", unit: TargetUnit?, fx: FxDescriptor? }

export type Target_LoadFxSnapshot = {
	kind: "LoadFxSnapshot",
	unit: TargetUnit?,
	fx: FxDescriptor?,
	snapshot: FxSnapshot?,
}

export type Target_BrowseFxPresets = { kind: "BrowseFxPresets", unit: TargetUnit?, fx: FxDescriptor? }

export type Target_FxTool = {
	kind: "FxTool",
	unit: TargetUnit?,
	fx: FxDescriptor?,
	action: FxToolAction?,
	instance_tags: { string }?,
}

export type Target_FxVisibility = {
	kind: "FxVisibility",
	unit: TargetUnit?,
	fx: FxDescriptor?,
	display_kind: FxDisplayKind?,
}

export type Target_FxParameterValue = {
	kind: "FxParameterValue",
	unit: TargetUnit?,
	parameter: FxParameterDescriptor,
	poll_for_feedback: boolean?,
	retrigger: boolean?,
	real_time: boolean?,
}

export type Target_FxParameterAutomationTouchState = {
	kind: "FxParameterAutomationTouchState",
	unit: TargetUnit?,
	parameter: FxParameterDescriptor,
}

export type Target_RouteAutomationMode = {
	kind: "RouteAutomationMode",
	unit: TargetUnit?,
	route: RouteDescriptor,
	mode: AutomationMode,
	poll_for_feedback: boolean?,
}

export type Target_RouteMonoState = {
	kind: "RouteMonoState",
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type Target_RouteMuteState = {
	kind: "RouteMuteState",
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type Target_RoutePhase = {
	kind: "RoutePhase",
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}

export type Target_RoutePan = { kind: "RoutePan", unit: TargetUnit?, route: RouteDescriptor }

export type Target_RouteVolume = { kind: "RouteVolume", unit: TargetUnit?, route: RouteDescriptor }

export type Target_RouteTouchState = {
	kind: "RouteTouchState",
	unit: TargetUnit?,
	route: RouteDescriptor,
	touched_parameter: TouchedRouteParameter,
}

export type Target_PlaytimeSlotTransportAction = {
	kind: "PlaytimeSlotTransportAction",
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	action: PlaytimeSlotTransportAction,
	stop_column_if_slot_empty: boolean?,
}

export type Target_PlaytimeColumnAction = {
	kind: "PlaytimeColumnAction",
	unit: TargetUnit?,
	column: PlaytimeColumnDescriptor,
	action: PlaytimeColumnAction,
}

export type Target_PlaytimeRowAction = {
	kind: "PlaytimeRowAction",
	unit: TargetUnit?,
	row: PlaytimeRowDescriptor,
	action: PlaytimeRowAction,
}

export type Target_PlaytimeMatrixAction = {
	kind: "PlaytimeMatrixAction",
	unit: TargetUnit?,
	action: PlaytimeMatrixAction,
}

export type Target_PlaytimeControlUnitScroll = { kind: "PlaytimeControlUnitScroll", unit: TargetUnit?, axis: Axis }

export type Target_PlaytimeBrowseCells = { kind: "PlaytimeBrowseCells", unit: TargetUnit?, axis: Axis }

export type Target_PlaytimeSlotSeek = {
	kind: "PlaytimeSlotSeek",
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	feedback_resolution: FeedbackResolution?,
}

export type Target_PlaytimeSlotVolume = { kind: "PlaytimeSlotVolume", unit: TargetUnit?, slot: PlaytimeSlotDescriptor }

export type Target_PlaytimeSlotManagementAction = {
	kind: "PlaytimeSlotManagementAction",
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	action: PlaytimeSlotManagementAction,
}

export type Target_SendMidi = { kind: "SendMidi", unit: TargetUnit?, message: string?, destination: SendMidiDestination? }

export type Target_SendOsc = {
	kind: "SendOsc",
	unit: TargetUnit?,
	address: string?,
	argument: OscArgument?,
	destination: OscDestination?,
}

export type Target_Dummy = { kind: "Dummy", unit: TargetUnit? }

export type Target_EnableInstances = {
	kind: "EnableInstances",
	unit: TargetUnit?,
	tags: { string }?,
	exclusivity: InstanceExclusivity?,
}

export type Target_EnableMappings = {
	kind: "EnableMappings",
	unit: TargetUnit?,
	tags: { string }?,
	exclusivity: MappingExclusivity?,
}

export type Target_ModifyMapping = {
	kind: "ModifyMapping",
	unit: TargetUnit?,
	session: string?,
	mapping: string?,
	modification: MappingModification,
}

export type Target_CompartmentParameterValue = {
	kind: "CompartmentParameterValue",
	unit: TargetUnit?,
	parameter: CompartmentParameterDescriptor,
}

export type Target_LoadMappingSnapshot = {
	kind: "LoadMappingSnapshot",
	unit: TargetUnit?,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: MappingSnapshotDescForLoad?,
	default_value: TargetValue?,
}

export type Target_TakeMappingSnapshot = {
	kind: "TakeMappingSnapshot",
	unit: TargetUnit?,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: BackwardCompatibleMappingSnapshotDescForTake,
}

export type Target_BrowseGroupMappings = {
	kind: "BrowseGroupMappings",
	unit: TargetUnit?,
	exclusivity: GroupMappingExclusivity?,
	group: string?,
}

export type Target_BrowsePotFilterItems = { kind: "BrowsePotFilterItems", unit: TargetUnit?, item_kind: PotFilterKind? }

export type Target_BrowsePotPresets = { kind: "BrowsePotPresets", unit: TargetUnit? }

export type Target_PreviewPotPreset = { kind: "PreviewPotPreset", unit: TargetUnit? }

export type Target_LoadPotPreset = { kind: "LoadPotPreset", unit: TargetUnit?, fx: FxDescriptor? }

export type Target_StreamDeckBrightness = { kind: "StreamDeckBrightness", unit: TargetUnit? }

export type Target_Virtual = {
	kind: "Virtual",
	id: VirtualControlElementId,
	character: VirtualControlElementCharacter?,
	learnable: boolean?,
}
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
	| Target_PlaytimeBrowseCells
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
	| Target_StreamDeckBrightness
	| Target_Virtual

--- A type that represents all possible kinds of Target.
export type TargetKind =
	"Mouse"
	| "LastTouched"
	| "AutomationModeOverride"
	| "ReaperAction"
	| "TransportAction"
	| "AnyOn"
	| "BrowseTracks"
	| "Seek"
	| "PlayRate"
	| "Tempo"
	| "GoToBookmark"
	| "TrackArmState"
	| "TrackParentSendState"
	| "AllTrackFxOnOffState"
	| "TrackMuteState"
	| "TrackPeak"
	| "TrackPhase"
	| "TrackSelectionState"
	| "TrackAutomationMode"
	| "TrackMonitoringMode"
	| "TrackAutomationTouchState"
	| "TrackPan"
	| "TrackWidth"
	| "TrackVolume"
	| "TrackTool"
	| "TrackVisibility"
	| "TrackSoloState"
	| "BrowseFxChain"
	| "FxOnOffState"
	| "FxOnlineOfflineState"
	| "LoadFxSnapshot"
	| "BrowseFxPresets"
	| "FxTool"
	| "FxVisibility"
	| "FxParameterValue"
	| "FxParameterAutomationTouchState"
	| "RouteAutomationMode"
	| "RouteMonoState"
	| "RouteMuteState"
	| "RoutePhase"
	| "RoutePan"
	| "RouteVolume"
	| "RouteTouchState"
	| "PlaytimeSlotTransportAction"
	| "PlaytimeColumnAction"
	| "PlaytimeRowAction"
	| "PlaytimeMatrixAction"
	| "PlaytimeControlUnitScroll"
	| "PlaytimeBrowseCells"
	| "PlaytimeSlotSeek"
	| "PlaytimeSlotVolume"
	| "PlaytimeSlotManagementAction"
	| "SendMidi"
	| "SendOsc"
	| "Dummy"
	| "EnableInstances"
	| "EnableMappings"
	| "ModifyMapping"
	| "CompartmentParameterValue"
	| "LoadMappingSnapshot"
	| "TakeMappingSnapshot"
	| "BrowseGroupMappings"
	| "BrowsePotFilterItems"
	| "BrowsePotPresets"
	| "PreviewPotPreset"
	| "LoadPotPreset"
	| "StreamDeckBrightness"
	| "Virtual"

--- Helper table to create Target values of different kinds.
module.Target = {}

--- Creates a Target of kind Mouse.
function module.Target.Mouse(value: MouseTarget): Target_Mouse
	local t: any = table.clone(value)
	t.kind = "Mouse"
	return t
end

--- Creates a Target of kind LastTouched.
function module.Target.LastTouched(value: LastTouchedTarget): Target_LastTouched
	local t: any = table.clone(value)
	t.kind = "LastTouched"
	return t
end

--- Creates a Target of kind AutomationModeOverride.
function module.Target.AutomationModeOverride(value: AutomationModeOverrideTarget): Target_AutomationModeOverride
	local t: any = table.clone(value)
	t.kind = "AutomationModeOverride"
	return t
end

--- Creates a Target of kind ReaperAction.
function module.Target.ReaperAction(value: ReaperActionTarget): Target_ReaperAction
	local t: any = table.clone(value)
	t.kind = "ReaperAction"
	return t
end

--- Creates a Target of kind TransportAction.
function module.Target.TransportAction(value: TransportActionTarget): Target_TransportAction
	local t: any = table.clone(value)
	t.kind = "TransportAction"
	return t
end

--- Creates a Target of kind AnyOn.
function module.Target.AnyOn(value: AnyOnTarget): Target_AnyOn
	local t: any = table.clone(value)
	t.kind = "AnyOn"
	return t
end

--- Creates a Target of kind BrowseTracks.
function module.Target.BrowseTracks(value: BrowseTracksTarget): Target_BrowseTracks
	local t: any = table.clone(value)
	t.kind = "BrowseTracks"
	return t
end

--- Creates a Target of kind Seek.
function module.Target.Seek(value: SeekTarget): Target_Seek
	local t: any = table.clone(value)
	t.kind = "Seek"
	return t
end

--- Creates a Target of kind PlayRate.
function module.Target.PlayRate(value: PlayRateTarget): Target_PlayRate
	local t: any = table.clone(value)
	t.kind = "PlayRate"
	return t
end

--- Creates a Target of kind Tempo.
function module.Target.Tempo(value: TempoTarget): Target_Tempo
	local t: any = table.clone(value)
	t.kind = "Tempo"
	return t
end

--- Creates a Target of kind GoToBookmark.
function module.Target.GoToBookmark(value: GoToBookmarkTarget): Target_GoToBookmark
	local t: any = table.clone(value)
	t.kind = "GoToBookmark"
	return t
end

--- Creates a Target of kind TrackArmState.
function module.Target.TrackArmState(value: TrackArmStateTarget): Target_TrackArmState
	local t: any = table.clone(value)
	t.kind = "TrackArmState"
	return t
end

--- Creates a Target of kind TrackParentSendState.
function module.Target.TrackParentSendState(value: TrackParentSendStateTarget): Target_TrackParentSendState
	local t: any = table.clone(value)
	t.kind = "TrackParentSendState"
	return t
end

--- Creates a Target of kind AllTrackFxOnOffState.
function module.Target.AllTrackFxOnOffState(value: AllTrackFxOnOffStateTarget): Target_AllTrackFxOnOffState
	local t: any = table.clone(value)
	t.kind = "AllTrackFxOnOffState"
	return t
end

--- Creates a Target of kind TrackMuteState.
function module.Target.TrackMuteState(value: TrackMuteStateTarget): Target_TrackMuteState
	local t: any = table.clone(value)
	t.kind = "TrackMuteState"
	return t
end

--- Creates a Target of kind TrackPeak.
function module.Target.TrackPeak(value: TrackPeakTarget): Target_TrackPeak
	local t: any = table.clone(value)
	t.kind = "TrackPeak"
	return t
end

--- Creates a Target of kind TrackPhase.
function module.Target.TrackPhase(value: TrackPhaseTarget): Target_TrackPhase
	local t: any = table.clone(value)
	t.kind = "TrackPhase"
	return t
end

--- Creates a Target of kind TrackSelectionState.
function module.Target.TrackSelectionState(value: TrackSelectionStateTarget): Target_TrackSelectionState
	local t: any = table.clone(value)
	t.kind = "TrackSelectionState"
	return t
end

--- Creates a Target of kind TrackAutomationMode.
function module.Target.TrackAutomationMode(value: TrackAutomationModeTarget): Target_TrackAutomationMode
	local t: any = table.clone(value)
	t.kind = "TrackAutomationMode"
	return t
end

--- Creates a Target of kind TrackMonitoringMode.
function module.Target.TrackMonitoringMode(value: TrackMonitoringModeTarget): Target_TrackMonitoringMode
	local t: any = table.clone(value)
	t.kind = "TrackMonitoringMode"
	return t
end

--- Creates a Target of kind TrackAutomationTouchState.
function module.Target.TrackAutomationTouchState(
	value: TrackAutomationTouchStateTarget
): Target_TrackAutomationTouchState
	local t: any = table.clone(value)
	t.kind = "TrackAutomationTouchState"
	return t
end

--- Creates a Target of kind TrackPan.
function module.Target.TrackPan(value: TrackPanTarget): Target_TrackPan
	local t: any = table.clone(value)
	t.kind = "TrackPan"
	return t
end

--- Creates a Target of kind TrackWidth.
function module.Target.TrackWidth(value: TrackWidthTarget): Target_TrackWidth
	local t: any = table.clone(value)
	t.kind = "TrackWidth"
	return t
end

--- Creates a Target of kind TrackVolume.
function module.Target.TrackVolume(value: TrackVolumeTarget): Target_TrackVolume
	local t: any = table.clone(value)
	t.kind = "TrackVolume"
	return t
end

--- Creates a Target of kind TrackTool.
function module.Target.TrackTool(value: TrackToolTarget): Target_TrackTool
	local t: any = table.clone(value)
	t.kind = "TrackTool"
	return t
end

--- Creates a Target of kind TrackVisibility.
function module.Target.TrackVisibility(value: TrackVisibilityTarget): Target_TrackVisibility
	local t: any = table.clone(value)
	t.kind = "TrackVisibility"
	return t
end

--- Creates a Target of kind TrackSoloState.
function module.Target.TrackSoloState(value: TrackSoloStateTarget): Target_TrackSoloState
	local t: any = table.clone(value)
	t.kind = "TrackSoloState"
	return t
end

--- Creates a Target of kind BrowseFxChain.
function module.Target.BrowseFxChain(value: BrowseFxChainTarget): Target_BrowseFxChain
	local t: any = table.clone(value)
	t.kind = "BrowseFxChain"
	return t
end

--- Creates a Target of kind FxOnOffState.
function module.Target.FxOnOffState(value: FxOnOffStateTarget): Target_FxOnOffState
	local t: any = table.clone(value)
	t.kind = "FxOnOffState"
	return t
end

--- Creates a Target of kind FxOnlineOfflineState.
function module.Target.FxOnlineOfflineState(value: FxOnlineOfflineStateTarget): Target_FxOnlineOfflineState
	local t: any = table.clone(value)
	t.kind = "FxOnlineOfflineState"
	return t
end

--- Creates a Target of kind LoadFxSnapshot.
function module.Target.LoadFxSnapshot(value: LoadFxSnapshotTarget): Target_LoadFxSnapshot
	local t: any = table.clone(value)
	t.kind = "LoadFxSnapshot"
	return t
end

--- Creates a Target of kind BrowseFxPresets.
function module.Target.BrowseFxPresets(value: BrowseFxPresetsTarget): Target_BrowseFxPresets
	local t: any = table.clone(value)
	t.kind = "BrowseFxPresets"
	return t
end

--- Creates a Target of kind FxTool.
function module.Target.FxTool(value: FxToolTarget): Target_FxTool
	local t: any = table.clone(value)
	t.kind = "FxTool"
	return t
end

--- Creates a Target of kind FxVisibility.
function module.Target.FxVisibility(value: FxVisibilityTarget): Target_FxVisibility
	local t: any = table.clone(value)
	t.kind = "FxVisibility"
	return t
end

--- Creates a Target of kind FxParameterValue.
function module.Target.FxParameterValue(value: FxParameterValueTarget): Target_FxParameterValue
	local t: any = table.clone(value)
	t.kind = "FxParameterValue"
	return t
end

--- Creates a Target of kind FxParameterAutomationTouchState.
function module.Target.FxParameterAutomationTouchState(
	value: FxParameterAutomationTouchStateTarget
): Target_FxParameterAutomationTouchState
	local t: any = table.clone(value)
	t.kind = "FxParameterAutomationTouchState"
	return t
end

--- Creates a Target of kind RouteAutomationMode.
function module.Target.RouteAutomationMode(value: RouteAutomationModeTarget): Target_RouteAutomationMode
	local t: any = table.clone(value)
	t.kind = "RouteAutomationMode"
	return t
end

--- Creates a Target of kind RouteMonoState.
function module.Target.RouteMonoState(value: RouteMonoStateTarget): Target_RouteMonoState
	local t: any = table.clone(value)
	t.kind = "RouteMonoState"
	return t
end

--- Creates a Target of kind RouteMuteState.
function module.Target.RouteMuteState(value: RouteMuteStateTarget): Target_RouteMuteState
	local t: any = table.clone(value)
	t.kind = "RouteMuteState"
	return t
end

--- Creates a Target of kind RoutePhase.
function module.Target.RoutePhase(value: RoutePhaseTarget): Target_RoutePhase
	local t: any = table.clone(value)
	t.kind = "RoutePhase"
	return t
end

--- Creates a Target of kind RoutePan.
function module.Target.RoutePan(value: RoutePanTarget): Target_RoutePan
	local t: any = table.clone(value)
	t.kind = "RoutePan"
	return t
end

--- Creates a Target of kind RouteVolume.
function module.Target.RouteVolume(value: RouteVolumeTarget): Target_RouteVolume
	local t: any = table.clone(value)
	t.kind = "RouteVolume"
	return t
end

--- Creates a Target of kind RouteTouchState.
function module.Target.RouteTouchState(value: RouteTouchStateTarget): Target_RouteTouchState
	local t: any = table.clone(value)
	t.kind = "RouteTouchState"
	return t
end

--- Creates a Target of kind PlaytimeSlotTransportAction.
function module.Target.PlaytimeSlotTransportAction(
	value: PlaytimeSlotTransportActionTarget
): Target_PlaytimeSlotTransportAction
	local t: any = table.clone(value)
	t.kind = "PlaytimeSlotTransportAction"
	return t
end

--- Creates a Target of kind PlaytimeColumnAction.
function module.Target.PlaytimeColumnAction(value: PlaytimeColumnActionTarget): Target_PlaytimeColumnAction
	local t: any = table.clone(value)
	t.kind = "PlaytimeColumnAction"
	return t
end

--- Creates a Target of kind PlaytimeRowAction.
function module.Target.PlaytimeRowAction(value: PlaytimeRowActionTarget): Target_PlaytimeRowAction
	local t: any = table.clone(value)
	t.kind = "PlaytimeRowAction"
	return t
end

--- Creates a Target of kind PlaytimeMatrixAction.
function module.Target.PlaytimeMatrixAction(value: PlaytimeMatrixActionTarget): Target_PlaytimeMatrixAction
	local t: any = table.clone(value)
	t.kind = "PlaytimeMatrixAction"
	return t
end

--- Creates a Target of kind PlaytimeControlUnitScroll.
function module.Target.PlaytimeControlUnitScroll(
	value: PlaytimeControlUnitScrollTarget
): Target_PlaytimeControlUnitScroll
	local t: any = table.clone(value)
	t.kind = "PlaytimeControlUnitScroll"
	return t
end

--- Creates a Target of kind PlaytimeBrowseCells.
function module.Target.PlaytimeBrowseCells(value: PlaytimeBrowseCellsTarget): Target_PlaytimeBrowseCells
	local t: any = table.clone(value)
	t.kind = "PlaytimeBrowseCells"
	return t
end

--- Creates a Target of kind PlaytimeSlotSeek.
function module.Target.PlaytimeSlotSeek(value: PlaytimeSlotSeekTarget): Target_PlaytimeSlotSeek
	local t: any = table.clone(value)
	t.kind = "PlaytimeSlotSeek"
	return t
end

--- Creates a Target of kind PlaytimeSlotVolume.
function module.Target.PlaytimeSlotVolume(value: PlaytimeSlotVolumeTarget): Target_PlaytimeSlotVolume
	local t: any = table.clone(value)
	t.kind = "PlaytimeSlotVolume"
	return t
end

--- Creates a Target of kind PlaytimeSlotManagementAction.
function module.Target.PlaytimeSlotManagementAction(
	value: PlaytimeSlotManagementActionTarget
): Target_PlaytimeSlotManagementAction
	local t: any = table.clone(value)
	t.kind = "PlaytimeSlotManagementAction"
	return t
end

--- Creates a Target of kind SendMidi.
function module.Target.SendMidi(value: SendMidiTarget): Target_SendMidi
	local t: any = table.clone(value)
	t.kind = "SendMidi"
	return t
end

--- Creates a Target of kind SendOsc.
function module.Target.SendOsc(value: SendOscTarget): Target_SendOsc
	local t: any = table.clone(value)
	t.kind = "SendOsc"
	return t
end

--- Creates a Target of kind Dummy.
function module.Target.Dummy(value: DummyTarget): Target_Dummy
	local t: any = table.clone(value)
	t.kind = "Dummy"
	return t
end

--- Creates a Target of kind EnableInstances.
function module.Target.EnableInstances(value: EnableInstancesTarget): Target_EnableInstances
	local t: any = table.clone(value)
	t.kind = "EnableInstances"
	return t
end

--- Creates a Target of kind EnableMappings.
function module.Target.EnableMappings(value: EnableMappingsTarget): Target_EnableMappings
	local t: any = table.clone(value)
	t.kind = "EnableMappings"
	return t
end

--- Creates a Target of kind ModifyMapping.
function module.Target.ModifyMapping(value: ModifyMappingTarget): Target_ModifyMapping
	local t: any = table.clone(value)
	t.kind = "ModifyMapping"
	return t
end

--- Creates a Target of kind CompartmentParameterValue.
function module.Target.CompartmentParameterValue(
	value: CompartmentParameterValueTarget
): Target_CompartmentParameterValue
	local t: any = table.clone(value)
	t.kind = "CompartmentParameterValue"
	return t
end

--- Creates a Target of kind LoadMappingSnapshot.
function module.Target.LoadMappingSnapshot(value: LoadMappingSnapshotTarget): Target_LoadMappingSnapshot
	local t: any = table.clone(value)
	t.kind = "LoadMappingSnapshot"
	return t
end

--- Creates a Target of kind TakeMappingSnapshot.
function module.Target.TakeMappingSnapshot(value: TakeMappingSnapshotTarget): Target_TakeMappingSnapshot
	local t: any = table.clone(value)
	t.kind = "TakeMappingSnapshot"
	return t
end

--- Creates a Target of kind BrowseGroupMappings.
function module.Target.BrowseGroupMappings(value: BrowseGroupMappingsTarget): Target_BrowseGroupMappings
	local t: any = table.clone(value)
	t.kind = "BrowseGroupMappings"
	return t
end

--- Creates a Target of kind BrowsePotFilterItems.
function module.Target.BrowsePotFilterItems(value: BrowsePotFilterItemsTarget): Target_BrowsePotFilterItems
	local t: any = table.clone(value)
	t.kind = "BrowsePotFilterItems"
	return t
end

--- Creates a Target of kind BrowsePotPresets.
function module.Target.BrowsePotPresets(value: BrowsePotPresetsTarget): Target_BrowsePotPresets
	local t: any = table.clone(value)
	t.kind = "BrowsePotPresets"
	return t
end

--- Creates a Target of kind PreviewPotPreset.
function module.Target.PreviewPotPreset(value: PreviewPotPresetTarget): Target_PreviewPotPreset
	local t: any = table.clone(value)
	t.kind = "PreviewPotPreset"
	return t
end

--- Creates a Target of kind LoadPotPreset.
function module.Target.LoadPotPreset(value: LoadPotPresetTarget): Target_LoadPotPreset
	local t: any = table.clone(value)
	t.kind = "LoadPotPreset"
	return t
end

--- Creates a Target of kind StreamDeckBrightness.
function module.Target.StreamDeckBrightness(value: StreamDeckBrightnessTarget): Target_StreamDeckBrightness
	local t: any = table.clone(value)
	t.kind = "StreamDeckBrightness"
	return t
end

--- Creates a Target of kind Virtual.
function module.Target.Virtual(value: VirtualTarget): Target_Virtual
	local t: any = table.clone(value)
	t.kind = "Virtual"
	return t
end

export type TargetCommons = {
	unit: TargetUnit?,
}
--- Creates a TargetCommons value.
function module.TargetCommons(value: TargetCommons): TargetCommons
	return value
end

export type TargetUnit = "Native" | "Percent"

export type MouseTarget = {
	unit: TargetUnit?,
	action: MouseAction,
}
--- Creates a MouseTarget value.
function module.MouseTarget(value: MouseTarget): MouseTarget
	return value
end

export type LastTouchedTarget = {
	unit: TargetUnit?,
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}
--- Creates a LastTouchedTarget value.
function module.LastTouchedTarget(value: LastTouchedTarget): LastTouchedTarget
	return value
end

export type AutomationModeOverrideTarget = {
	unit: TargetUnit?,
	override_value: AutomationModeOverride?,
}
--- Creates a AutomationModeOverrideTarget value.
function module.AutomationModeOverrideTarget(value: AutomationModeOverrideTarget): AutomationModeOverrideTarget
	return value
end

export type ReaperActionTarget = {
	unit: TargetUnit?,
	scope: ActionScope?,
	command: ReaperCommand?,
	invocation: ActionInvocationKind?,
	track: TrackDescriptor?,
}
--- Creates a ReaperActionTarget value.
function module.ReaperActionTarget(value: ReaperActionTarget): ReaperActionTarget
	return value
end

export type ActionScope = "Main" | "ActiveMidiEditor" | "ActiveMidiEventListEditor" | "MediaExplorer"

export type TransportActionTarget = {
	unit: TargetUnit?,
	action: TransportAction,
}
--- Creates a TransportActionTarget value.
function module.TransportActionTarget(value: TransportActionTarget): TransportActionTarget
	return value
end

export type AnyOnTarget = {
	unit: TargetUnit?,
	parameter: AnyOnParameter,
}
--- Creates a AnyOnTarget value.
function module.AnyOnTarget(value: AnyOnTarget): AnyOnTarget
	return value
end

export type BrowseTracksTarget = {
	unit: TargetUnit?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
	mode: BrowseTracksMode?,
}
--- Creates a BrowseTracksTarget value.
function module.BrowseTracksTarget(value: BrowseTracksTarget): BrowseTracksTarget
	return value
end

export type SeekTarget = {
	unit: TargetUnit?,
	use_time_selection: boolean?,
	use_loop_points: boolean?,
	use_regions: boolean?,
	use_project: boolean?,
	move_view: boolean?,
	seek_play: boolean?,
	feedback_resolution: FeedbackResolution?,
	behavior: SeekBehavior?,
}
--- Creates a SeekTarget value.
function module.SeekTarget(value: SeekTarget): SeekTarget
	return value
end

export type PlayRateTarget = {
	unit: TargetUnit?,
}
--- Creates a PlayRateTarget value.
function module.PlayRateTarget(value: PlayRateTarget): PlayRateTarget
	return value
end

export type TempoTarget = {
	unit: TargetUnit?,
}
--- Creates a TempoTarget value.
function module.TempoTarget(value: TempoTarget): TempoTarget
	return value
end

export type GoToBookmarkTarget = {
	unit: TargetUnit?,
	bookmark: BookmarkDescriptor,
	set_time_selection: boolean?,
	set_loop_points: boolean?,
	seek_behavior: SeekBehavior?,
}
--- Creates a GoToBookmarkTarget value.
function module.GoToBookmarkTarget(value: GoToBookmarkTarget): GoToBookmarkTarget
	return value
end

export type TrackArmStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackArmStateTarget value.
function module.TrackArmStateTarget(value: TrackArmStateTarget): TrackArmStateTarget
	return value
end

export type TrackParentSendStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
}
--- Creates a TrackParentSendStateTarget value.
function module.TrackParentSendStateTarget(value: TrackParentSendStateTarget): TrackParentSendStateTarget
	return value
end

export type AllTrackFxOnOffStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
}
--- Creates a AllTrackFxOnOffStateTarget value.
function module.AllTrackFxOnOffStateTarget(value: AllTrackFxOnOffStateTarget): AllTrackFxOnOffStateTarget
	return value
end

export type TrackMuteStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackMuteStateTarget value.
function module.TrackMuteStateTarget(value: TrackMuteStateTarget): TrackMuteStateTarget
	return value
end

export type TrackPeakTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
}
--- Creates a TrackPeakTarget value.
function module.TrackPeakTarget(value: TrackPeakTarget): TrackPeakTarget
	return value
end

export type TrackPhaseTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackPhaseTarget value.
function module.TrackPhaseTarget(value: TrackPhaseTarget): TrackPhaseTarget
	return value
end

export type TrackSelectionStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	scroll_arrange_view: boolean?,
	scroll_mixer: boolean?,
}
--- Creates a TrackSelectionStateTarget value.
function module.TrackSelectionStateTarget(value: TrackSelectionStateTarget): TrackSelectionStateTarget
	return value
end

export type TrackAutomationModeTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: AutomationMode,
}
--- Creates a TrackAutomationModeTarget value.
function module.TrackAutomationModeTarget(value: TrackAutomationModeTarget): TrackAutomationModeTarget
	return value
end

export type TrackMonitoringModeTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	mode: MonitoringMode,
	use_selection_ganging: boolean?,
}
--- Creates a TrackMonitoringModeTarget value.
function module.TrackMonitoringModeTarget(value: TrackMonitoringModeTarget): TrackMonitoringModeTarget
	return value
end

export type TrackAutomationTouchStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
	touched_parameter: TouchedTrackParameter,
}
--- Creates a TrackAutomationTouchStateTarget value.
function module.TrackAutomationTouchStateTarget(value: TrackAutomationTouchStateTarget): TrackAutomationTouchStateTarget
	return value
end

export type TrackPanTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackPanTarget value.
function module.TrackPanTarget(value: TrackPanTarget): TrackPanTarget
	return value
end

export type TrackWidthTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackWidthTarget value.
function module.TrackWidthTarget(value: TrackWidthTarget): TrackWidthTarget
	return value
end

export type TrackVolumeTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackVolumeTarget value.
function module.TrackVolumeTarget(value: TrackVolumeTarget): TrackVolumeTarget
	return value
end

export type TrackToolTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	action: TrackToolAction?,
	instance_tags: { string }?,
}
--- Creates a TrackToolTarget value.
function module.TrackToolTarget(value: TrackToolTarget): TrackToolTarget
	return value
end

export type TrackToolAction = "DoNothing" | "SetAsUnitTrack" | "PinAsUnitTrack"

--- Mouse position on the given axis.
---
--- Control:
---
--- - Move by (using relative control value)
--- - Move to (use absolute control value)
--- - Move to absolute coordinates before clicking (2 mappings for moving, 1 for clicking)
---
--- Feedback:
---
--- - Reflect position of the mouse cursor
---
--- Future extension possibilities:
---
--- - Canvas: Relative to all screens, current screen, REAPER window or focused window
--- - Pixel density: Take pixel density into account
export type MouseAction_MoveTo = { kind: "MoveTo", axis: Axis? }

--- Like [`Self::Move`] but prefers relative control, so the glue section will feed it
--- with relative control values whenever possible. Could make a little difference.
export type MouseAction_MoveBy = { kind: "MoveBy", axis: Axis? }

--- Button state.
---
--- Control:
---
--- - Press and release a mouse button
--- - Press a mouse button and keep it pressed (press-only filter)
--- - Just release a mouse button (release-only filter, e.g. for manual drag control)
---
--- Feedback:
---
--- - Whether the button is down or up
---
--- Future extension possibilities:
---
--- - Click or double-click a mouse button (press and immediate release, this could be a generic
---   "Glue" option because it could be useful for other on/off targets as well).
export type MouseAction_PressOrRelease = { kind: "PressOrRelease", button: MouseButton? }

--- Scroll wheel.
---
--- Control:
---
--- - Invoke scroll wheel
---
--- Feedback: None
export type MouseAction_Scroll = { kind: "Scroll", axis: Axis? }
export type MouseAction = MouseAction_MoveTo | MouseAction_MoveBy | MouseAction_PressOrRelease | MouseAction_Scroll

--- A type that represents all possible kinds of MouseAction.
export type MouseActionKind = "MoveTo" | "MoveBy" | "PressOrRelease" | "Scroll"

--- Helper table to create MouseAction values of different kinds.
module.MouseAction = {}

--- Creates a MouseAction of kind MoveTo.
--- Mouse position on the given axis.
---
--- Control:
---
--- - Move by (using relative control value)
--- - Move to (use absolute control value)
--- - Move to absolute coordinates before clicking (2 mappings for moving, 1 for clicking)
---
--- Feedback:
---
--- - Reflect position of the mouse cursor
---
--- Future extension possibilities:
---
--- - Canvas: Relative to all screens, current screen, REAPER window or focused window
--- - Pixel density: Take pixel density into account
function module.MouseAction.MoveTo(value: { axis: Axis? }): MouseAction_MoveTo
	local t: any = table.clone(value)
	t.kind = "MoveTo"
	return t
end

--- Creates a MouseAction of kind MoveBy.
--- Like [`Self::Move`] but prefers relative control, so the glue section will feed it
--- with relative control values whenever possible. Could make a little difference.
function module.MouseAction.MoveBy(value: { axis: Axis? }): MouseAction_MoveBy
	local t: any = table.clone(value)
	t.kind = "MoveBy"
	return t
end

--- Creates a MouseAction of kind PressOrRelease.
--- Button state.
---
--- Control:
---
--- - Press and release a mouse button
--- - Press a mouse button and keep it pressed (press-only filter)
--- - Just release a mouse button (release-only filter, e.g. for manual drag control)
---
--- Feedback:
---
--- - Whether the button is down or up
---
--- Future extension possibilities:
---
--- - Click or double-click a mouse button (press and immediate release, this could be a generic
---   "Glue" option because it could be useful for other on/off targets as well).
function module.MouseAction.PressOrRelease(value: { button: MouseButton? }): MouseAction_PressOrRelease
	local t: any = table.clone(value)
	t.kind = "PressOrRelease"
	return t
end

--- Creates a MouseAction of kind Scroll.
--- Scroll wheel.
---
--- Control:
---
--- - Invoke scroll wheel
---
--- Feedback: None
function module.MouseAction.Scroll(value: { axis: Axis? }): MouseAction_Scroll
	local t: any = table.clone(value)
	t.kind = "Scroll"
	return t
end

export type Axis = "X" | "Y"

export type MouseButton = "Left" | "Middle" | "Right"

export type TrackVisibilityTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	poll_for_feedback: boolean?,
	area: TrackArea,
}
--- Creates a TrackVisibilityTarget value.
function module.TrackVisibilityTarget(value: TrackVisibilityTarget): TrackVisibilityTarget
	return value
end

export type TrackSoloStateTarget = {
	unit: TargetUnit?,
	track: TrackDescriptor?,
	exclusivity: TrackExclusivity?,
	behavior: SoloBehavior?,
	use_track_grouping: boolean?,
	use_selection_ganging: boolean?,
}
--- Creates a TrackSoloStateTarget value.
function module.TrackSoloStateTarget(value: TrackSoloStateTarget): TrackSoloStateTarget
	return value
end

export type BrowseFxChainTarget = {
	unit: TargetUnit?,
	chain: FxChainDescriptor,
	display_kind: FxDisplayKind?,
}
--- Creates a BrowseFxChainTarget value.
function module.BrowseFxChainTarget(value: BrowseFxChainTarget): BrowseFxChainTarget
	return value
end

export type FxOnOffStateTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
}
--- Creates a FxOnOffStateTarget value.
function module.FxOnOffStateTarget(value: FxOnOffStateTarget): FxOnOffStateTarget
	return value
end

export type FxOnlineOfflineStateTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
}
--- Creates a FxOnlineOfflineStateTarget value.
function module.FxOnlineOfflineStateTarget(value: FxOnlineOfflineStateTarget): FxOnlineOfflineStateTarget
	return value
end

export type LoadFxSnapshotTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
	snapshot: FxSnapshot?,
}
--- Creates a LoadFxSnapshotTarget value.
function module.LoadFxSnapshotTarget(value: LoadFxSnapshotTarget): LoadFxSnapshotTarget
	return value
end

export type BrowseFxPresetsTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
}
--- Creates a BrowseFxPresetsTarget value.
function module.BrowseFxPresetsTarget(value: BrowseFxPresetsTarget): BrowseFxPresetsTarget
	return value
end

export type FxToolTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
	action: FxToolAction?,
	instance_tags: { string }?,
}
--- Creates a FxToolTarget value.
function module.FxToolTarget(value: FxToolTarget): FxToolTarget
	return value
end

export type FxToolAction = "DoNothing" | "SetAsUnitFx" | "PinAsUnitFx"

export type FxVisibilityTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
	display_kind: FxDisplayKind?,
}
--- Creates a FxVisibilityTarget value.
function module.FxVisibilityTarget(value: FxVisibilityTarget): FxVisibilityTarget
	return value
end

export type FxParameterValueTarget = {
	unit: TargetUnit?,
	parameter: FxParameterDescriptor,
	poll_for_feedback: boolean?,
	retrigger: boolean?,
	real_time: boolean?,
}
--- Creates a FxParameterValueTarget value.
function module.FxParameterValueTarget(value: FxParameterValueTarget): FxParameterValueTarget
	return value
end

export type CompartmentParameterValueTarget = {
	unit: TargetUnit?,
	parameter: CompartmentParameterDescriptor,
}
--- Creates a CompartmentParameterValueTarget value.
function module.CompartmentParameterValueTarget(value: CompartmentParameterValueTarget): CompartmentParameterValueTarget
	return value
end

export type FxParameterAutomationTouchStateTarget = {
	unit: TargetUnit?,
	parameter: FxParameterDescriptor,
}
--- Creates a FxParameterAutomationTouchStateTarget value.
function module.FxParameterAutomationTouchStateTarget(
	value: FxParameterAutomationTouchStateTarget
): FxParameterAutomationTouchStateTarget
	return value
end

export type RouteAutomationModeTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
	mode: AutomationMode,
	poll_for_feedback: boolean?,
}
--- Creates a RouteAutomationModeTarget value.
function module.RouteAutomationModeTarget(value: RouteAutomationModeTarget): RouteAutomationModeTarget
	return value
end

export type RouteMonoStateTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
--- Creates a RouteMonoStateTarget value.
function module.RouteMonoStateTarget(value: RouteMonoStateTarget): RouteMonoStateTarget
	return value
end

export type RouteMuteStateTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
--- Creates a RouteMuteStateTarget value.
function module.RouteMuteStateTarget(value: RouteMuteStateTarget): RouteMuteStateTarget
	return value
end

export type RoutePhaseTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
	poll_for_feedback: boolean?,
}
--- Creates a RoutePhaseTarget value.
function module.RoutePhaseTarget(value: RoutePhaseTarget): RoutePhaseTarget
	return value
end

export type RoutePanTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
}
--- Creates a RoutePanTarget value.
function module.RoutePanTarget(value: RoutePanTarget): RoutePanTarget
	return value
end

export type RouteVolumeTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
}
--- Creates a RouteVolumeTarget value.
function module.RouteVolumeTarget(value: RouteVolumeTarget): RouteVolumeTarget
	return value
end

export type RouteTouchStateTarget = {
	unit: TargetUnit?,
	route: RouteDescriptor,
	touched_parameter: TouchedRouteParameter,
}
--- Creates a RouteTouchStateTarget value.
function module.RouteTouchStateTarget(value: RouteTouchStateTarget): RouteTouchStateTarget
	return value
end

export type PlaytimeSlotTransportActionTarget = {
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	action: PlaytimeSlotTransportAction,
	stop_column_if_slot_empty: boolean?,
}
--- Creates a PlaytimeSlotTransportActionTarget value.
function module.PlaytimeSlotTransportActionTarget(
	value: PlaytimeSlotTransportActionTarget
): PlaytimeSlotTransportActionTarget
	return value
end

export type PlaytimeColumnActionTarget = {
	unit: TargetUnit?,
	column: PlaytimeColumnDescriptor,
	action: PlaytimeColumnAction,
}
--- Creates a PlaytimeColumnActionTarget value.
function module.PlaytimeColumnActionTarget(value: PlaytimeColumnActionTarget): PlaytimeColumnActionTarget
	return value
end

export type PlaytimeRowActionTarget = {
	unit: TargetUnit?,
	row: PlaytimeRowDescriptor,
	action: PlaytimeRowAction,
}
--- Creates a PlaytimeRowActionTarget value.
function module.PlaytimeRowActionTarget(value: PlaytimeRowActionTarget): PlaytimeRowActionTarget
	return value
end

export type PlaytimeMatrixActionTarget = {
	unit: TargetUnit?,
	action: PlaytimeMatrixAction,
}
--- Creates a PlaytimeMatrixActionTarget value.
function module.PlaytimeMatrixActionTarget(value: PlaytimeMatrixActionTarget): PlaytimeMatrixActionTarget
	return value
end

export type PlaytimeControlUnitScrollTarget = {
	unit: TargetUnit?,
	axis: Axis,
}
--- Creates a PlaytimeControlUnitScrollTarget value.
function module.PlaytimeControlUnitScrollTarget(value: PlaytimeControlUnitScrollTarget): PlaytimeControlUnitScrollTarget
	return value
end

export type PlaytimeBrowseCellsTarget = {
	unit: TargetUnit?,
	axis: Axis,
}
--- Creates a PlaytimeBrowseCellsTarget value.
function module.PlaytimeBrowseCellsTarget(value: PlaytimeBrowseCellsTarget): PlaytimeBrowseCellsTarget
	return value
end

export type PlaytimeSlotSeekTarget = {
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	feedback_resolution: FeedbackResolution?,
}
--- Creates a PlaytimeSlotSeekTarget value.
function module.PlaytimeSlotSeekTarget(value: PlaytimeSlotSeekTarget): PlaytimeSlotSeekTarget
	return value
end

export type PlaytimeSlotVolumeTarget = {
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
}
--- Creates a PlaytimeSlotVolumeTarget value.
function module.PlaytimeSlotVolumeTarget(value: PlaytimeSlotVolumeTarget): PlaytimeSlotVolumeTarget
	return value
end

export type PlaytimeSlotManagementActionTarget = {
	unit: TargetUnit?,
	slot: PlaytimeSlotDescriptor,
	action: PlaytimeSlotManagementAction,
}
--- Creates a PlaytimeSlotManagementActionTarget value.
function module.PlaytimeSlotManagementActionTarget(
	value: PlaytimeSlotManagementActionTarget
): PlaytimeSlotManagementActionTarget
	return value
end

export type PlaytimeSlotManagementAction =
	"ClearSlot"
	| "FillSlotWithSelectedItem"
	| "EditClip"
	| "CopyOrPasteClip"
	| "DoubleClipSectionLength"
	| "HalveClipSectionLength"
	| "QuantizationOnOffState"
	| "Duplicate"
	| "Activate"

export type AdjustClipSectionLengthAction = {
	factor: number,
}
--- Creates a AdjustClipSectionLengthAction value.
function module.AdjustClipSectionLengthAction(value: AdjustClipSectionLengthAction): AdjustClipSectionLengthAction
	return value
end

export type SendMidiTarget = {
	unit: TargetUnit?,
	message: string?,
	destination: SendMidiDestination?,
}
--- Creates a SendMidiTarget value.
function module.SendMidiTarget(value: SendMidiTarget): SendMidiTarget
	return value
end

export type DummyTarget = {
	unit: TargetUnit?,
}
--- Creates a DummyTarget value.
function module.DummyTarget(value: DummyTarget): DummyTarget
	return value
end

export type SendOscTarget = {
	unit: TargetUnit?,
	address: string?,
	argument: OscArgument?,
	destination: OscDestination?,
}
--- Creates a SendOscTarget value.
function module.SendOscTarget(value: SendOscTarget): SendOscTarget
	return value
end

export type EnableInstancesTarget = {
	unit: TargetUnit?,
	tags: { string }?,
	exclusivity: InstanceExclusivity?,
}
--- Creates a EnableInstancesTarget value.
function module.EnableInstancesTarget(value: EnableInstancesTarget): EnableInstancesTarget
	return value
end

export type EnableMappingsTarget = {
	unit: TargetUnit?,
	tags: { string }?,
	exclusivity: MappingExclusivity?,
}
--- Creates a EnableMappingsTarget value.
function module.EnableMappingsTarget(value: EnableMappingsTarget): EnableMappingsTarget
	return value
end

export type ModifyMappingTarget = {
	unit: TargetUnit?,
	session: string?,
	mapping: string?,
	modification: MappingModification,
}
--- Creates a ModifyMappingTarget value.
function module.ModifyMappingTarget(value: ModifyMappingTarget): ModifyMappingTarget
	return value
end

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

--- A type that represents all possible kinds of MappingModification.
export type MappingModificationKind = "LearnTarget" | "SetTargetToLastTouched"

--- Helper table to create MappingModification values of different kinds.
module.MappingModification = {}

--- Creates a MappingModification of kind LearnTarget.
function module.MappingModification.LearnTarget(value: LearnTargetMappingModification): MappingModification_LearnTarget
	local t: any = table.clone(value)
	t.kind = "LearnTarget"
	return t
end

--- Creates a MappingModification of kind SetTargetToLastTouched.
function module.MappingModification.SetTargetToLastTouched(
	value: SetTargetToLastTouchedMappingModification
): MappingModification_SetTargetToLastTouched
	local t: any = table.clone(value)
	t.kind = "SetTargetToLastTouched"
	return t
end

export type LearnTargetMappingModification = {
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}
--- Creates a LearnTargetMappingModification value.
function module.LearnTargetMappingModification(value: LearnTargetMappingModification): LearnTargetMappingModification
	return value
end

export type SetTargetToLastTouchedMappingModification = {
	included_targets: { LearnableTargetKind }?,
	touch_cause: TargetTouchCause?,
}
--- Creates a SetTargetToLastTouchedMappingModification value.
function module.SetTargetToLastTouchedMappingModification(
	value: SetTargetToLastTouchedMappingModification
): SetTargetToLastTouchedMappingModification
	return value
end

export type LoadMappingSnapshotTarget = {
	unit: TargetUnit?,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: MappingSnapshotDescForLoad?,
	default_value: TargetValue?,
}
--- Creates a LoadMappingSnapshotTarget value.
function module.LoadMappingSnapshotTarget(value: LoadMappingSnapshotTarget): LoadMappingSnapshotTarget
	return value
end

export type TakeMappingSnapshotTarget = {
	unit: TargetUnit?,
	tags: { string }?,
	active_mappings_only: boolean?,
	snapshot: BackwardCompatibleMappingSnapshotDescForTake,
}
--- Creates a TakeMappingSnapshotTarget value.
function module.TakeMappingSnapshotTarget(value: TakeMappingSnapshotTarget): TakeMappingSnapshotTarget
	return value
end

export type BackwardCompatibleMappingSnapshotDescForTake_Old = string

export type BackwardCompatibleMappingSnapshotDescForTake_New = MappingSnapshotDescForTake
export type BackwardCompatibleMappingSnapshotDescForTake =
	BackwardCompatibleMappingSnapshotDescForTake_Old
	| BackwardCompatibleMappingSnapshotDescForTake_New

--- Creates a BackwardCompatibleMappingSnapshotDescForTake value.
function module.BackwardCompatibleMappingSnapshotDescForTake(
	value: BackwardCompatibleMappingSnapshotDescForTake
): BackwardCompatibleMappingSnapshotDescForTake
	return value
end

export type MappingSnapshotDescForLoad_Initial = { kind: "Initial" }

export type MappingSnapshotDescForLoad_ById = { kind: "ById", id: string }
export type MappingSnapshotDescForLoad = MappingSnapshotDescForLoad_Initial | MappingSnapshotDescForLoad_ById

--- A type that represents all possible kinds of MappingSnapshotDescForLoad.
export type MappingSnapshotDescForLoadKind = "Initial" | "ById"

--- Helper table to create MappingSnapshotDescForLoad values of different kinds.
module.MappingSnapshotDescForLoad = {}

--- Creates a MappingSnapshotDescForLoad of kind Initial.
function module.MappingSnapshotDescForLoad.Initial(): MappingSnapshotDescForLoad_Initial
	return {
		kind = "Initial",
	}
end

--- Creates a MappingSnapshotDescForLoad of kind ById.
function module.MappingSnapshotDescForLoad.ById(value: { id: string }): MappingSnapshotDescForLoad_ById
	local t: any = table.clone(value)
	t.kind = "ById"
	return t
end

export type MappingSnapshotDescForTake_LastLoaded = { kind: "LastLoaded" }

export type MappingSnapshotDescForTake_ById = { kind: "ById", id: string }
export type MappingSnapshotDescForTake = MappingSnapshotDescForTake_LastLoaded | MappingSnapshotDescForTake_ById

--- A type that represents all possible kinds of MappingSnapshotDescForTake.
export type MappingSnapshotDescForTakeKind = "LastLoaded" | "ById"

--- Helper table to create MappingSnapshotDescForTake values of different kinds.
module.MappingSnapshotDescForTake = {}

--- Creates a MappingSnapshotDescForTake of kind LastLoaded.
function module.MappingSnapshotDescForTake.LastLoaded(): MappingSnapshotDescForTake_LastLoaded
	return {
		kind = "LastLoaded",
	}
end

--- Creates a MappingSnapshotDescForTake of kind ById.
function module.MappingSnapshotDescForTake.ById(value: { id: string }): MappingSnapshotDescForTake_ById
	local t: any = table.clone(value)
	t.kind = "ById"
	return t
end

export type BrowseGroupMappingsTarget = {
	unit: TargetUnit?,
	exclusivity: GroupMappingExclusivity?,
	group: string?,
}
--- Creates a BrowseGroupMappingsTarget value.
function module.BrowseGroupMappingsTarget(value: BrowseGroupMappingsTarget): BrowseGroupMappingsTarget
	return value
end

export type BrowsePotFilterItemsTarget = {
	unit: TargetUnit?,
	item_kind: PotFilterKind?,
}
--- Creates a BrowsePotFilterItemsTarget value.
function module.BrowsePotFilterItemsTarget(value: BrowsePotFilterItemsTarget): BrowsePotFilterItemsTarget
	return value
end

export type BrowsePotPresetsTarget = {
	unit: TargetUnit?,
}
--- Creates a BrowsePotPresetsTarget value.
function module.BrowsePotPresetsTarget(value: BrowsePotPresetsTarget): BrowsePotPresetsTarget
	return value
end

export type PreviewPotPresetTarget = {
	unit: TargetUnit?,
}
--- Creates a PreviewPotPresetTarget value.
function module.PreviewPotPresetTarget(value: PreviewPotPresetTarget): PreviewPotPresetTarget
	return value
end

export type LoadPotPresetTarget = {
	unit: TargetUnit?,
	fx: FxDescriptor?,
}
--- Creates a LoadPotPresetTarget value.
function module.LoadPotPresetTarget(value: LoadPotPresetTarget): LoadPotPresetTarget
	return value
end

export type StreamDeckBrightnessTarget = {
	unit: TargetUnit?,
}
--- Creates a StreamDeckBrightnessTarget value.
function module.StreamDeckBrightnessTarget(value: StreamDeckBrightnessTarget): StreamDeckBrightnessTarget
	return value
end

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
	learnable: boolean?,
}
--- Creates a VirtualTarget value.
function module.VirtualTarget(value: VirtualTarget): VirtualTarget
	return value
end

export type AutomationModeOverride_Bypass = { kind: "Bypass" }

export type AutomationModeOverride_Mode = { kind: "Mode", mode: AutomationMode }
export type AutomationModeOverride = AutomationModeOverride_Bypass | AutomationModeOverride_Mode

--- A type that represents all possible kinds of AutomationModeOverride.
export type AutomationModeOverrideKind = "Bypass" | "Mode"

--- Helper table to create AutomationModeOverride values of different kinds.
module.AutomationModeOverride = {}

--- Creates a AutomationModeOverride of kind Bypass.
function module.AutomationModeOverride.Bypass(): AutomationModeOverride_Bypass
	return {
		kind = "Bypass",
	}
end

--- Creates a AutomationModeOverride of kind Mode.
function module.AutomationModeOverride.Mode(value: { mode: AutomationMode }): AutomationModeOverride_Mode
	local t: any = table.clone(value)
	t.kind = "Mode"
	return t
end

export type AutomationMode = "TrimRead" | "Read" | "Touch" | "Write" | "Latch" | "LatchPreview"

export type MonitoringMode = "Off" | "Normal" | "TapeStyle"

export type TransportAction = "PlayStop" | "PlayPause" | "Stop" | "Pause" | "Record" | "Repeat"

export type PlaytimeSlotTransportAction =
	"Trigger"
	| "PlayStop"
	| "PlayPause"
	| "Stop"
	| "Pause"
	| "RecordStop"
	| "RecordPlayStop"
	| "OverdubPlay"
	| "Looped"

export type PlaytimeColumnAction = "Stop" | "ArmState" | "ArmStateExclusive" | "Activate"

export type PlaytimeRowAction = "PlayScene" | "BuildScene" | "ClearScene" | "CopyOrPasteScene" | "Activate"

export type PlaytimeMatrixAction =
	"Stop"
	| "Undo"
	| "Redo"
	| "BuildScene"
	| "SetRecordLengthMode"
	| "SetCustomRecordLengthInBars"
	| "ClickOnOffState"
	| "MidiAutoQuantizationOnOffState"
	| "SmartRecord"
	| "StartOrStopPlayback"
	| "SilenceModeOnOffState"
	| "Panic"
	| "SequencerRecordOnOffState"
	| "SequencerPlayOnOffState"
	| "TapTempo"

export type AnyOnParameter = "TrackSolo" | "TrackMute" | "TrackArm" | "TrackSelection"

export type ActionInvocationKind = "Trigger" | "Absolute14Bit" | "Absolute7Bit" | "Relative"

export type ReaperCommand_Id = number

export type ReaperCommand_Name = string
export type ReaperCommand = ReaperCommand_Id | ReaperCommand_Name

--- Creates a ReaperCommand value.
function module.ReaperCommand(value: ReaperCommand): ReaperCommand
	return value
end

--- Resolves to the track on which this ReaLearn instance is installed.
---
--- Doesn't make sense if ReaLearn is installed on the monitoring FX chain.
export type TrackDescriptor_This = { address: "This", track_must_be_selected: boolean? }

export type TrackDescriptor_Master = { address: "Master", track_must_be_selected: boolean? }

export type TrackDescriptor_Instance = { address: "Instance", track_must_be_selected: boolean? }

export type TrackDescriptor_Selected = { address: "Selected", allow_multiple: boolean? }

export type TrackDescriptor_Dynamic = {
	address: "Dynamic",
	track_must_be_selected: boolean?,
	expression: string,
	scope: TrackScope?,
}

export type TrackDescriptor_ById = { address: "ById", track_must_be_selected: boolean?, id: string? }

export type TrackDescriptor_ByIndex = {
	address: "ByIndex",
	track_must_be_selected: boolean?,
	index: number,
	scope: TrackScope?,
}

export type TrackDescriptor_ByName = {
	address: "ByName",
	track_must_be_selected: boolean?,
	name: string,
	allow_multiple: boolean?,
}

export type TrackDescriptor_FromClipColumn = {
	address: "FromClipColumn",
	track_must_be_selected: boolean?,
	column: PlaytimeColumnDescriptor,
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

--- A type that represents all possible kinds of TrackDescriptor.
export type TrackDescriptorAddress =
	"This"
	| "Master"
	| "Instance"
	| "Selected"
	| "Dynamic"
	| "ById"
	| "ByIndex"
	| "ByName"
	| "FromClipColumn"

--- Helper table to create TrackDescriptor values of different kinds.
module.TrackDescriptor = {}

--- Creates a TrackDescriptor of kind This.
--- Resolves to the track on which this ReaLearn instance is installed.
---
--- Doesn't make sense if ReaLearn is installed on the monitoring FX chain.
function module.TrackDescriptor.This(value: { track_must_be_selected: boolean? }): TrackDescriptor_This
	local t: any = table.clone(value)
	t.address = "This"
	return t
end

--- Creates a TrackDescriptor of kind Master.
function module.TrackDescriptor.Master(value: { track_must_be_selected: boolean? }): TrackDescriptor_Master
	local t: any = table.clone(value)
	t.address = "Master"
	return t
end

--- Creates a TrackDescriptor of kind Instance.
function module.TrackDescriptor.Instance(value: { track_must_be_selected: boolean? }): TrackDescriptor_Instance
	local t: any = table.clone(value)
	t.address = "Instance"
	return t
end

--- Creates a TrackDescriptor of kind Selected.
function module.TrackDescriptor.Selected(value: { allow_multiple: boolean? }): TrackDescriptor_Selected
	local t: any = table.clone(value)
	t.address = "Selected"
	return t
end

--- Creates a TrackDescriptor of kind Dynamic.
function module.TrackDescriptor.Dynamic(
	value: { track_must_be_selected: boolean?, expression: string, scope: TrackScope? }
): TrackDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

--- Creates a TrackDescriptor of kind ById.
function module.TrackDescriptor.ById(value: { track_must_be_selected: boolean?, id: string? }): TrackDescriptor_ById
	local t: any = table.clone(value)
	t.address = "ById"
	return t
end

--- Creates a TrackDescriptor of kind ByIndex.
function module.TrackDescriptor.ByIndex(
	value: { track_must_be_selected: boolean?, index: number, scope: TrackScope? }
): TrackDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a TrackDescriptor of kind ByName.
function module.TrackDescriptor.ByName(
	value: { track_must_be_selected: boolean?, name: string, allow_multiple: boolean? }
): TrackDescriptor_ByName
	local t: any = table.clone(value)
	t.address = "ByName"
	return t
end

--- Creates a TrackDescriptor of kind FromClipColumn.
function module.TrackDescriptor.FromClipColumn(value: {
	track_must_be_selected: boolean?,
	column: PlaytimeColumnDescriptor,
	context: ClipColumnTrackContext,
}): TrackDescriptor_FromClipColumn
	local t: any = table.clone(value)
	t.address = "FromClipColumn"
	return t
end

export type ClipColumnTrackContext = "Playback" | "Recording"

export type TrackDescriptorCommons = {
	track_must_be_selected: boolean?,
}
--- Creates a TrackDescriptorCommons value.
function module.TrackDescriptorCommons(value: TrackDescriptorCommons): TrackDescriptorCommons
	return value
end

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

--- A type that represents all possible kinds of BookmarkDescriptor.
export type BookmarkDescriptorKind = "Marker" | "Region"

--- Helper table to create BookmarkDescriptor values of different kinds.
module.BookmarkDescriptor = {}

--- Creates a BookmarkDescriptor of kind Marker.
function module.BookmarkDescriptor.Marker(value: BookmarkRef): BookmarkDescriptor_Marker
	local t: any = table.clone(value)
	t.kind = "Marker"
	return t
end

--- Creates a BookmarkDescriptor of kind Region.
function module.BookmarkDescriptor.Region(value: BookmarkRef): BookmarkDescriptor_Region
	local t: any = table.clone(value)
	t.kind = "Region"
	return t
end

export type BookmarkRef_ById = { id: number }

export type BookmarkRef_ByIndex = { index: number }
export type BookmarkRef = BookmarkRef_ById | BookmarkRef_ByIndex

--- Creates a BookmarkRef value.
function module.BookmarkRef(value: BookmarkRef): BookmarkRef
	return value
end

export type FxDescriptorCommons = {
	fx_must_have_focus: boolean?,
}
--- Creates a FxDescriptorCommons value.
function module.FxDescriptorCommons(value: FxDescriptorCommons): FxDescriptorCommons
	return value
end

export type FxDescriptor_This = { address: "This", fx_must_have_focus: boolean? }

export type FxDescriptor_Focused = { address: "Focused" }

export type FxDescriptor_Instance = { address: "Instance", fx_must_have_focus: boolean? }

export type FxDescriptor_Dynamic = {
	address: "Dynamic",
	fx_must_have_focus: boolean?,
	chain: FxChainDescriptor,
	expression: string,
}

export type FxDescriptor_ById = { address: "ById", fx_must_have_focus: boolean?, chain: FxChainDescriptor, id: string? }

export type FxDescriptor_ByIndex = {
	address: "ByIndex",
	fx_must_have_focus: boolean?,
	chain: FxChainDescriptor,
	index: number,
}

export type FxDescriptor_ByName = {
	address: "ByName",
	fx_must_have_focus: boolean?,
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

--- A type that represents all possible kinds of FxDescriptor.
export type FxDescriptorAddress = "This" | "Focused" | "Instance" | "Dynamic" | "ById" | "ByIndex" | "ByName"

--- Helper table to create FxDescriptor values of different kinds.
module.FxDescriptor = {}

--- Creates a FxDescriptor of kind This.
function module.FxDescriptor.This(value: { fx_must_have_focus: boolean? }): FxDescriptor_This
	local t: any = table.clone(value)
	t.address = "This"
	return t
end

--- Creates a FxDescriptor of kind Focused.
function module.FxDescriptor.Focused(): FxDescriptor_Focused
	return {
		address = "Focused",
	}
end

--- Creates a FxDescriptor of kind Instance.
function module.FxDescriptor.Instance(value: { fx_must_have_focus: boolean? }): FxDescriptor_Instance
	local t: any = table.clone(value)
	t.address = "Instance"
	return t
end

--- Creates a FxDescriptor of kind Dynamic.
function module.FxDescriptor.Dynamic(
	value: { fx_must_have_focus: boolean?, chain: FxChainDescriptor, expression: string }
): FxDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

--- Creates a FxDescriptor of kind ById.
function module.FxDescriptor.ById(
	value: { fx_must_have_focus: boolean?, chain: FxChainDescriptor, id: string? }
): FxDescriptor_ById
	local t: any = table.clone(value)
	t.address = "ById"
	return t
end

--- Creates a FxDescriptor of kind ByIndex.
function module.FxDescriptor.ByIndex(
	value: { fx_must_have_focus: boolean?, chain: FxChainDescriptor, index: number }
): FxDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a FxDescriptor of kind ByName.
function module.FxDescriptor.ByName(value: {
	fx_must_have_focus: boolean?,
	chain: FxChainDescriptor,
	name: string,
	allow_multiple: boolean?,
}): FxDescriptor_ByName
	local t: any = table.clone(value)
	t.address = "ByName"
	return t
end

export type FxChainDescriptor_Track = { address: "Track", track: TrackDescriptor?, chain: TrackFxChain? }
export type FxChainDescriptor = FxChainDescriptor_Track

--- A type that represents all possible kinds of FxChainDescriptor.
export type FxChainDescriptorAddress = "Track"

--- Helper table to create FxChainDescriptor values of different kinds.
module.FxChainDescriptor = {}

--- Creates a FxChainDescriptor of kind Track.
function module.FxChainDescriptor.Track(
	value: { track: TrackDescriptor?, chain: TrackFxChain? }
): FxChainDescriptor_Track
	local t: any = table.clone(value)
	t.address = "Track"
	return t
end

export type TrackFxChain = "Normal" | "Input"

export type FxDisplayKind = "FloatingWindow" | "Chain"

export type FxSnapshot = {
	fx_kind: string?,
	fx_name: string?,
	preset_name: string?,
	content: FxSnapshotContent,
}
--- Creates a FxSnapshot value.
function module.FxSnapshot(value: FxSnapshot): FxSnapshot
	return value
end

export type FxSnapshotContent_Chunk = { kind: "Chunk", chunk: string }
export type FxSnapshotContent = FxSnapshotContent_Chunk

--- A type that represents all possible kinds of FxSnapshotContent.
export type FxSnapshotContentKind = "Chunk"

--- Helper table to create FxSnapshotContent values of different kinds.
module.FxSnapshotContent = {}

--- Creates a FxSnapshotContent of kind Chunk.
function module.FxSnapshotContent.Chunk(value: { chunk: string }): FxSnapshotContent_Chunk
	local t: any = table.clone(value)
	t.kind = "Chunk"
	return t
end

export type FxParameterDescriptor_Dynamic = { address: "Dynamic", fx: FxDescriptor?, expression: string }

export type FxParameterDescriptor_ById = { address: "ById", fx: FxDescriptor?, index: number }

export type FxParameterDescriptor_ByIndex = { address: "ByIndex", fx: FxDescriptor?, index: number }

export type FxParameterDescriptor_ByName = { address: "ByName", fx: FxDescriptor?, name: string }
export type FxParameterDescriptor =
	FxParameterDescriptor_Dynamic
	| FxParameterDescriptor_ById
	| FxParameterDescriptor_ByIndex
	| FxParameterDescriptor_ByName

--- A type that represents all possible kinds of FxParameterDescriptor.
export type FxParameterDescriptorAddress = "Dynamic" | "ById" | "ByIndex" | "ByName"

--- Helper table to create FxParameterDescriptor values of different kinds.
module.FxParameterDescriptor = {}

--- Creates a FxParameterDescriptor of kind Dynamic.
function module.FxParameterDescriptor.Dynamic(
	value: { fx: FxDescriptor?, expression: string }
): FxParameterDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

--- Creates a FxParameterDescriptor of kind ById.
function module.FxParameterDescriptor.ById(value: { fx: FxDescriptor?, index: number }): FxParameterDescriptor_ById
	local t: any = table.clone(value)
	t.address = "ById"
	return t
end

--- Creates a FxParameterDescriptor of kind ByIndex.
function module.FxParameterDescriptor.ByIndex(
	value: { fx: FxDescriptor?, index: number }
): FxParameterDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a FxParameterDescriptor of kind ByName.
function module.FxParameterDescriptor.ByName(value: { fx: FxDescriptor?, name: string }): FxParameterDescriptor_ByName
	local t: any = table.clone(value)
	t.address = "ByName"
	return t
end

export type CompartmentParameterDescriptor_ById = { address: "ById", index: number }
export type CompartmentParameterDescriptor = CompartmentParameterDescriptor_ById

--- A type that represents all possible kinds of CompartmentParameterDescriptor.
export type CompartmentParameterDescriptorAddress = "ById"

--- Helper table to create CompartmentParameterDescriptor values of different kinds.
module.CompartmentParameterDescriptor = {}

--- Creates a CompartmentParameterDescriptor of kind ById.
function module.CompartmentParameterDescriptor.ById(value: { index: number }): CompartmentParameterDescriptor_ById
	local t: any = table.clone(value)
	t.address = "ById"
	return t
end

export type RouteDescriptorCommons = {
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
}
--- Creates a RouteDescriptorCommons value.
function module.RouteDescriptorCommons(value: RouteDescriptorCommons): RouteDescriptorCommons
	return value
end

export type RouteDescriptor_Dynamic = {
	address: "Dynamic",
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
	expression: string,
}

export type RouteDescriptor_ById = { address: "ById", track: TrackDescriptor?, route_kind: TrackRouteKind?, id: string? }

export type RouteDescriptor_ByIndex = {
	address: "ByIndex",
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
	index: number,
}

export type RouteDescriptor_ByName = {
	address: "ByName",
	track: TrackDescriptor?,
	route_kind: TrackRouteKind?,
	name: string,
}
export type RouteDescriptor =
	RouteDescriptor_Dynamic
	| RouteDescriptor_ById
	| RouteDescriptor_ByIndex
	| RouteDescriptor_ByName

--- A type that represents all possible kinds of RouteDescriptor.
export type RouteDescriptorAddress = "Dynamic" | "ById" | "ByIndex" | "ByName"

--- Helper table to create RouteDescriptor values of different kinds.
module.RouteDescriptor = {}

--- Creates a RouteDescriptor of kind Dynamic.
function module.RouteDescriptor.Dynamic(
	value: { track: TrackDescriptor?, route_kind: TrackRouteKind?, expression: string }
): RouteDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

--- Creates a RouteDescriptor of kind ById.
function module.RouteDescriptor.ById(
	value: { track: TrackDescriptor?, route_kind: TrackRouteKind?, id: string? }
): RouteDescriptor_ById
	local t: any = table.clone(value)
	t.address = "ById"
	return t
end

--- Creates a RouteDescriptor of kind ByIndex.
function module.RouteDescriptor.ByIndex(
	value: { track: TrackDescriptor?, route_kind: TrackRouteKind?, index: number }
): RouteDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a RouteDescriptor of kind ByName.
function module.RouteDescriptor.ByName(
	value: { track: TrackDescriptor?, route_kind: TrackRouteKind?, name: string }
): RouteDescriptor_ByName
	local t: any = table.clone(value)
	t.address = "ByName"
	return t
end

export type TrackRouteKind = "Send" | "Receive" | "HardwareOutput"

export type PlaytimeSlotDescriptor_Active = { address: "Active" }

export type PlaytimeSlotDescriptor_ByIndex = { address: "ByIndex", column_index: number, row_index: number }

export type PlaytimeSlotDescriptor_Dynamic = { address: "Dynamic", column_expression: string, row_expression: string }
export type PlaytimeSlotDescriptor =
	PlaytimeSlotDescriptor_Active
	| PlaytimeSlotDescriptor_ByIndex
	| PlaytimeSlotDescriptor_Dynamic

--- A type that represents all possible kinds of PlaytimeSlotDescriptor.
export type PlaytimeSlotDescriptorAddress = "Active" | "ByIndex" | "Dynamic"

--- Helper table to create PlaytimeSlotDescriptor values of different kinds.
module.PlaytimeSlotDescriptor = {}

--- Creates a PlaytimeSlotDescriptor of kind Active.
function module.PlaytimeSlotDescriptor.Active(): PlaytimeSlotDescriptor_Active
	return {
		address = "Active",
	}
end

--- Creates a PlaytimeSlotDescriptor of kind ByIndex.
function module.PlaytimeSlotDescriptor.ByIndex(value: playtime.SlotAddress): PlaytimeSlotDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a PlaytimeSlotDescriptor of kind Dynamic.
function module.PlaytimeSlotDescriptor.Dynamic(
	value: { column_expression: string, row_expression: string }
): PlaytimeSlotDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

export type PlaytimeSlotDescriptorKind = "Active" | "ByIndex" | "Dynamic"

export type PlaytimeColumnDescriptor_Active = { address: "Active" }

export type PlaytimeColumnDescriptor_ByIndex = { address: "ByIndex", index: number }

export type PlaytimeColumnDescriptor_Dynamic = { address: "Dynamic", expression: string }
export type PlaytimeColumnDescriptor =
	PlaytimeColumnDescriptor_Active
	| PlaytimeColumnDescriptor_ByIndex
	| PlaytimeColumnDescriptor_Dynamic

--- A type that represents all possible kinds of PlaytimeColumnDescriptor.
export type PlaytimeColumnDescriptorAddress = "Active" | "ByIndex" | "Dynamic"

--- Helper table to create PlaytimeColumnDescriptor values of different kinds.
module.PlaytimeColumnDescriptor = {}

--- Creates a PlaytimeColumnDescriptor of kind Active.
function module.PlaytimeColumnDescriptor.Active(): PlaytimeColumnDescriptor_Active
	return {
		address = "Active",
	}
end

--- Creates a PlaytimeColumnDescriptor of kind ByIndex.
function module.PlaytimeColumnDescriptor.ByIndex(value: playtime.ColumnAddress): PlaytimeColumnDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a PlaytimeColumnDescriptor of kind Dynamic.
function module.PlaytimeColumnDescriptor.Dynamic(value: { expression: string }): PlaytimeColumnDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

export type PlaytimeColumnDescriptorKind = "Active" | "ByIndex" | "Dynamic"

export type PlaytimeRowDescriptor_Active = { address: "Active" }

export type PlaytimeRowDescriptor_ByIndex = { address: "ByIndex", index: number }

export type PlaytimeRowDescriptor_Dynamic = { address: "Dynamic", expression: string }
export type PlaytimeRowDescriptor =
	PlaytimeRowDescriptor_Active
	| PlaytimeRowDescriptor_ByIndex
	| PlaytimeRowDescriptor_Dynamic

--- A type that represents all possible kinds of PlaytimeRowDescriptor.
export type PlaytimeRowDescriptorAddress = "Active" | "ByIndex" | "Dynamic"

--- Helper table to create PlaytimeRowDescriptor values of different kinds.
module.PlaytimeRowDescriptor = {}

--- Creates a PlaytimeRowDescriptor of kind Active.
function module.PlaytimeRowDescriptor.Active(): PlaytimeRowDescriptor_Active
	return {
		address = "Active",
	}
end

--- Creates a PlaytimeRowDescriptor of kind ByIndex.
function module.PlaytimeRowDescriptor.ByIndex(value: playtime.RowAddress): PlaytimeRowDescriptor_ByIndex
	local t: any = table.clone(value)
	t.address = "ByIndex"
	return t
end

--- Creates a PlaytimeRowDescriptor of kind Dynamic.
function module.PlaytimeRowDescriptor.Dynamic(value: { expression: string }): PlaytimeRowDescriptor_Dynamic
	local t: any = table.clone(value)
	t.address = "Dynamic"
	return t
end

export type PlaytimeRowDescriptorKind = "Active" | "ByIndex" | "Dynamic"

export type SendMidiDestination_FxOutput = { kind: "FxOutput" }

export type SendMidiDestination_FeedbackOutput = { kind: "FeedbackOutput" }

export type SendMidiDestination_InputDevice = { kind: "InputDevice", device_id: number? }
export type SendMidiDestination =
	SendMidiDestination_FxOutput
	| SendMidiDestination_FeedbackOutput
	| SendMidiDestination_InputDevice

--- A type that represents all possible kinds of SendMidiDestination.
export type SendMidiDestinationKind = "FxOutput" | "FeedbackOutput" | "InputDevice"

--- Helper table to create SendMidiDestination values of different kinds.
module.SendMidiDestination = {}

--- Creates a SendMidiDestination of kind FxOutput.
function module.SendMidiDestination.FxOutput(): SendMidiDestination_FxOutput
	return {
		kind = "FxOutput",
	}
end

--- Creates a SendMidiDestination of kind FeedbackOutput.
function module.SendMidiDestination.FeedbackOutput(): SendMidiDestination_FeedbackOutput
	return {
		kind = "FeedbackOutput",
	}
end

--- Creates a SendMidiDestination of kind InputDevice.
function module.SendMidiDestination.InputDevice(value: InputDeviceMidiDestination): SendMidiDestination_InputDevice
	local t: any = table.clone(value)
	t.kind = "InputDevice"
	return t
end

export type InputDeviceMidiDestination = {
	device_id: number?,
}
--- Creates a InputDeviceMidiDestination value.
function module.InputDeviceMidiDestination(value: InputDeviceMidiDestination): InputDeviceMidiDestination
	return value
end

export type OscDestination_FeedbackOutput = { kind: "FeedbackOutput" }

export type OscDestination_Device = { kind: "Device", id: string }
export type OscDestination = OscDestination_FeedbackOutput | OscDestination_Device

--- A type that represents all possible kinds of OscDestination.
export type OscDestinationKind = "FeedbackOutput" | "Device"

--- Helper table to create OscDestination values of different kinds.
module.OscDestination = {}

--- Creates a OscDestination of kind FeedbackOutput.
function module.OscDestination.FeedbackOutput(): OscDestination_FeedbackOutput
	return {
		kind = "FeedbackOutput",
	}
end

--- Creates a OscDestination of kind Device.
function module.OscDestination.Device(value: { id: string }): OscDestination_Device
	local t: any = table.clone(value)
	t.kind = "Device"
	return t
end

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

--- A type that represents all possible kinds of TargetValue.
export type TargetValueKind = "Unit" | "Discrete"

--- Helper table to create TargetValue values of different kinds.
module.TargetValue = {}

--- Creates a TargetValue of kind Unit.
function module.TargetValue.Unit(value: { value: number }): TargetValue_Unit
	local t: any = table.clone(value)
	t.kind = "Unit"
	return t
end

--- Creates a TargetValue of kind Discrete.
function module.TargetValue.Discrete(value: { value: number }): TargetValue_Discrete
	local t: any = table.clone(value)
	t.kind = "Discrete"
	return t
end

return module

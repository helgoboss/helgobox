export interface Mapping {
  key?: string;
  name?: string;
  tags?: Array<string>;
  group?: string;
  visible_in_projection?: boolean;
  enabled?: boolean;
  control_enabled?: boolean;
  feedback_enabled?: boolean;
  active?: Active;
  feedback_behavior?: FeedbackBehavior;
  on_activate?: Lifecycle;
  on_deactivate?: Lifecycle;
  source?: Source;
  glue?: Glue;
  target?: Target;
}

export type Lifecycle = "Normal";

export type FeedbackBehavior =
  | "Normal"
  | "SendFeedbackAfterControl"
  | "PreventEchoFeedback";

export type Active = "Always";

export type Source =
  | { type: "None" }
  | { type: "MidiDeviceChanges" } & MidiDeviceChangesSource
  | { type: "RealearnInstanceStart" } & RealearnInstanceStartSource
  | { type: "MidiNoteVelocity" } & MidiNoteVelocitySource
  | { type: "MidiNoteKeyNumber" } & MidiNoteKeyNumberSource
  | { type: "MidiPolyphonicKeyPressureAmount" }
    & MidiPolyphonicKeyPressureAmountSource
  | { type: "MidiControlChangeValue" } & MidiControlChangeValueSource
  | { type: "MidiProgramChangeNumber" } & MidiProgramChangeNumberSource
  | { type: "MidiChannelPressureAmount" } & MidiChannelPressureAmountSource
  | { type: "MidiPitchBendChangeValue" } & MidiPitchBendChangeValueSource
  | { type: "MidiParameterNumberValue" } & MidiParameterNumberValueSource
  | { type: "MidiClockTempo" } & MidiClockTempoSource
  | { type: "MidiClockTransport" } & MidiClockTransportSource
  | { type: "MidiRaw" } & MidiRawSource
  | { type: "MidiScript" } & MidiScriptSource
  | { type: "MackieLcd" } & MackieLcd
  | { type: "MackieSevenSegmentDisplay" } & MackieSevenSegmentDisplay
  | { type: "SiniConE24Display" } & SiniConE24Display
  | { type: "LaunchpadProScrollingTextDisplay" }
    & LaunchpadProScrollingTextDisplay
  | { type: "Osc" } & OscSource
  | { type: "Virtual" } & VirtualSource;

export interface MidiNoteVelocitySource {
  channel?: number;
  key_number?: number;
}

export interface MidiNoteKeyNumberSource {
  channel?: number;
}

export interface MidiPolyphonicKeyPressureAmountSource {
  channel?: number;
  key_number?: number;
}

export interface MidiControlChangeValueSource {
  channel?: number;
  controller_number?: number;
  character?: SourceCharacter;
  fourteen_bit?: boolean;
}

export interface MidiProgramChangeNumberSource {
  channel?: number;
}

export interface MidiChannelPressureAmountSource {
  channel?: number;
}

export interface MidiPitchBendChangeValueSource {
  channel?: number;
}

export interface MidiParameterNumberValueSource {
  channel?: number;
  number?: number;
  fourteen_bit?: boolean;
  registered?: boolean;
  character?: SourceCharacter;
}

export interface MidiClockTempoSource {
  reserved?: string;
}

export interface MidiClockTransportSource {
  message?: MidiClockTransportMessage;
}

export interface MidiRawSource {
  pattern?: string;
  character?: SourceCharacter;
}

export interface MidiScriptSource {
  script?: string;
}

export type SourceCharacter =
  | "Range"
  | "Button"
  | "Relative1"
  | "Relative2"
  | "Relative3"
  | "StatefulButton";

export type MidiClockTransportMessage = "Start" | "Continue" | "Stop";

export interface MackieLcd {
  channel?: number;
  line?: number;
}

export interface MackieSevenSegmentDisplay {
  scope?: MackieSevenSegmentDisplayScope;
}

export type MackieSevenSegmentDisplayScope =
  | "All"
  | "Assignment"
  | "Tc"
  | "TcHoursBars"
  | "TcMinutesBeats"
  | "TcSecondsSub"
  | "TcFramesTicks";

export interface SiniConE24Display {
  cell_index?: number;
  item_index?: number;
}

export interface OscSource {
  address?: string;
  argument?: OscArgument;
}

export interface VirtualSource {
  id: VirtualControlElementId;
  kind: VirtualControlElementKind;
}

export interface Glue {
  source_interval?: [number, number];
}

export interface Target {
  unit?: TargetUnit;
}

export type TargetUnit = "Native" | "Percent";
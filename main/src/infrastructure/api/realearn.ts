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

export type FeedbackBehavior = "Normal";

export type Active = "Always";

export type Source =
  | { type: "None" }
  | { type: "Reaper" } & ReaperSource
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
  | { type: "MidiDisplay" } & MidiDisplaySource
  | { type: "Osc" } & OscSource
  | { type: "Virtual" } & VirtualSource;

export type ReaperSource = "MidiDeviceChanges" | "RealearnInstanceStart";

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

export interface MidiDisplaySource {
  spec?: MidiDisplaySpec;
}

export type SourceCharacter =
  | "Range"
  | "Button"
  | "Relative1"
  | "Relative2"
  | "Relative3"
  | "StatefulButton";

export type MidiClockTransportMessage = "Start" | "Continue" | "Stop";

export type MidiDisplaySpec =
  | { type: "MackieLcd" } & MackieLcdSpec
  | { type: "MackieSevenSegmentDisplay" } & MackieSevenSegmentDisplaySpec
  | { type: "SiniConE24" } & SiniConE24Spec;

export interface MackieLcdSpec {
  channel?: number;
  line?: number;
}

export interface MackieSevenSegmentDisplaySpec {
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

export interface SiniConE24Spec {
  cell_index?: number;
  item_index?: number;
}

export interface OscSource {
  reserved?: number;
}

export interface VirtualSource {
  reserved?: number;
}

export interface Glue {
  source_interval?: [number, number];
}

export interface Target {
  unit?: TargetUnit;
}

export type TargetUnit = "Native" | "Percent";
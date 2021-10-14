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
  | "None"
  | ReaperSource
  | MidiSource
  | OscSource
  | VirtualSource;

export type ReaperSource = "MidiDeviceChanges" | "RealearnInstanceStart";

export type MidiSource =
  | NoteVelocitySource
  | NoteKeyNumberSource
  | PolyphonicKeyPressureAmountSource
  | ControlChangeValueSource
  | ProgramChangeNumberSource
  | ChannelPressureAmountSource
  | PitchBendChangeValueSource
  | ParameterNumberValueSource
  | ClockTempoSource
  | ClockTransportSource
  | RawSource
  | ScriptSource
  | DisplaySource;

export interface NoteVelocitySource {
  channel: number | null;
  key_number: number | null;
}

export interface NoteKeyNumberSource {
  channel: number | null;
}

export interface PolyphonicKeyPressureAmountSource {
  channel: number | null;
  key_number: number | null;
}

export interface ControlChangeValueSource {
  channel: number | null;
  controller_number: number | null;
  character: SourceCharacter | null;
  fourteen_bit: boolean | null;
}

export interface ProgramChangeNumberSource {
  channel: number | null;
}

export interface ChannelPressureAmountSource {
  channel: number | null;
}

export interface PitchBendChangeValueSource {
  channel: number | null;
}

export interface ParameterNumberValueSource {
  channel: number | null;
  number: number | null;
  fourteen_bit: boolean | null;
  registered: boolean | null;
  character: SourceCharacter | null;
}

export interface ClockTempoSource {
  reserved: string | null;
}

export interface ClockTransportSource {
  message: MidiClockTransportMessage | null;
}

export interface RawSource {
  pattern: string | null;
  character: SourceCharacter | null;
}

export interface ScriptSource {
  script: string | null;
}

export interface DisplaySource {
  spec: DisplaySpec | null;
}

export type SourceCharacter =
  | "Range"
  | "Button"
  | "Relative1"
  | "Relative2"
  | "Relative3"
  | "StatefulButton";

export type MidiClockTransportMessage = "Start" | "Continue" | "Stop";

export type DisplaySpec =
  | MackieLcdSpec
  | MackieSevenSegmentDisplaySpec
  | SiniConE24Spec;

export interface MackieLcdSpec {
  channel: number | null;
  line: number | null;
}

export interface MackieSevenSegmentDisplaySpec {
  scope: MackieSevenSegmentDisplayScope | null;
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
  cell_index: number | null;
  item_index: number | null;
}

export interface OscSource {
  number: number;
}

export interface VirtualSource {
  number: number;
}

export interface Glue {
  source_interval: [number, number];
}

export interface Target {
  unit: TargetUnit | null;
}

export type TargetUnit = "Native" | "Percent";
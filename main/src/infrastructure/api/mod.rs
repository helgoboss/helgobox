mod mapping {
    use super::glue::*;
    use super::source::*;
    use super::target::*;
    use schemars::JsonSchema;
    use serde::Serialize;
    use ts_rs::TS;

    #[derive(Default, Serialize, JsonSchema, TS)]
    pub struct Mapping {
        /// An optional key that you can assign to this mapping in order to refer
        /// to it from somewhere else.
        ///
        /// This key should be unique within this list of mappings.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub group: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub visible_in_projection: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub control_enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub active: Option<Active>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub on_activate: Option<Lifecycle>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub on_deactivate: Option<Lifecycle>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source: Option<Source>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub glue: Option<Glue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub target: Option<Target>,
    }

    #[derive(Serialize, JsonSchema, TS)]
    pub enum Lifecycle {
        Normal,
    }

    #[derive(Serialize, JsonSchema, TS)]
    pub enum FeedbackBehavior {
        Normal,
    }

    #[derive(Serialize, JsonSchema, TS)]
    pub enum Active {
        Always,
    }
}

mod source {
    use midi::*;
    use osc::*;
    use reaper::*;
    use schemars::JsonSchema;
    use serde::Serialize;
    use ts_rs::TS;
    use virt::*;

    #[derive(Serialize, JsonSchema, TS)]
    pub enum Source {
        None,
        Reaper(ReaperSource),
        Midi(MidiSource),
        Osc(OscSource),
        Virtual(VirtualSource),
    }

    pub mod reaper {
        use schemars::JsonSchema;
        use serde::Serialize;
        use ts_rs::TS;

        #[derive(Serialize, JsonSchema, TS)]
        pub enum ReaperSource {
            MidiDeviceChanges,
            RealearnInstanceStart,
        }
    }

    pub mod midi {
        use schemars::JsonSchema;
        use serde::Serialize;
        use ts_rs::TS;

        #[derive(Serialize, JsonSchema, TS)]
        pub enum MidiSource {
            NoteVelocity(NoteVelocitySource),
            NoteKeyNumber(NoteKeyNumberSource),
            PolyphonicKeyPressureAmount(PolyphonicKeyPressureAmountSource),
            ControlChangeValue(ControlChangeValueSource),
            ProgramChangeNumber(ProgramChangeNumberSource),
            ChannelPressureAmount(ChannelPressureAmountSource),
            PitchBendChangeValue(PitchBendChangeValueSource),
            ParameterNumberValue(ParameterNumberValueSource),
            ClockTempo(ClockTempoSource),
            ClockTransport(ClockTransportSource),
            Raw(RawSource),
            Script(ScriptSource),
            Display(DisplaySource),
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct NoteVelocitySource {
            pub channel: Option<u8>,
            pub key_number: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct NoteKeyNumberSource {
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct PolyphonicKeyPressureAmountSource {
            pub channel: Option<u8>,
            pub key_number: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ControlChangeValueSource {
            pub channel: Option<u8>,
            pub controller_number: Option<u8>,
            pub character: Option<SourceCharacter>,
            pub fourteen_bit: Option<bool>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ProgramChangeNumberSource {
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ChannelPressureAmountSource {
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct PitchBendChangeValueSource {
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ParameterNumberValueSource {
            pub channel: Option<u8>,
            pub number: Option<u16>,
            pub fourteen_bit: Option<bool>,
            pub registered: Option<bool>,
            pub character: Option<SourceCharacter>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ClockTempoSource {
            reserved: Option<String>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ClockTransportSource {
            pub message: Option<MidiClockTransportMessage>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct RawSource {
            pub pattern: Option<String>,
            pub character: Option<SourceCharacter>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct ScriptSource {
            pub script: Option<String>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct DisplaySource {
            pub spec: Option<DisplaySpec>,
        }

        #[derive(Serialize, JsonSchema, TS)]
        pub enum SourceCharacter {
            Range,
            Button,
            Relative1,
            Relative2,
            Relative3,
            StatefulButton,
        }

        #[derive(Serialize, JsonSchema, TS)]
        pub enum MidiClockTransportMessage {
            Start,
            Continue,
            Stop,
        }

        #[derive(Serialize, JsonSchema, TS)]
        pub enum DisplaySpec {
            MackieLcd(MackieLcdSpec),
            MackieSevenSegmentDisplay(MackieSevenSegmentDisplaySpec),
            SiniConE24(SiniConE24Spec),
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct MackieLcdSpec {
            pub channel: Option<u8>,
            pub line: Option<u8>,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct MackieSevenSegmentDisplaySpec {
            pub scope: Option<MackieSevenSegmentDisplayScope>,
        }

        #[derive(Serialize, JsonSchema, TS)]
        pub enum MackieSevenSegmentDisplayScope {
            All,
            Assignment,
            Tc,
            TcHoursBars,
            TcMinutesBeats,
            TcSecondsSub,
            TcFramesTicks,
        }

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct SiniConE24Spec {
            pub cell_index: Option<u8>,
            pub item_index: Option<u8>,
        }
    }

    pub mod osc {
        use schemars::JsonSchema;
        use serde::Serialize;
        use ts_rs::TS;

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct OscSource {
            pub number: u8,
        }
    }

    pub mod virt {
        use schemars::JsonSchema;
        use serde::Serialize;
        use ts_rs::TS;

        #[derive(Default, Serialize, JsonSchema, TS)]
        pub struct VirtualSource {
            pub number: u8,
        }
    }
}

mod glue {
    use schemars::JsonSchema;
    use serde::Serialize;
    use ts_rs::TS;

    #[derive(Default, Serialize, JsonSchema, TS)]
    pub struct Glue {
        pub source_interval: (f64, f64),
    }
}

mod target {
    use schemars::JsonSchema;
    use serde::Serialize;
    use ts_rs::TS;

    #[derive(Default, Serialize, JsonSchema, TS)]
    pub struct Target {
        pub unit: Option<TargetUnit>,
    }

    #[derive(Serialize, JsonSchema, TS)]
    pub enum TargetUnit {
        Native,
        Percent,
    }
}

#[cfg(test)]
mod tests {
    use super::glue::*;
    use super::mapping::*;
    use super::source::midi::*;
    use super::source::osc::*;
    use super::source::reaper::*;
    use super::source::virt::*;
    use super::source::Source;
    use super::target::*;

    ts_rs::export! {
        // Mapping
        Mapping, Lifecycle, FeedbackBehavior, Active,
        // Source
        Source,
        // REAPER source
        ReaperSource,
        // MIDI source
        MidiSource, NoteVelocitySource, NoteKeyNumberSource,
        PolyphonicKeyPressureAmountSource, ControlChangeValueSource, ProgramChangeNumberSource,
        ChannelPressureAmountSource, PitchBendChangeValueSource, ParameterNumberValueSource,
        ClockTempoSource, ClockTransportSource, RawSource, ScriptSource, DisplaySource,
        SourceCharacter, MidiClockTransportMessage, DisplaySpec, MackieLcdSpec,
        MackieSevenSegmentDisplaySpec, MackieSevenSegmentDisplayScope, SiniConE24Spec,
        // OSC source
        OscSource,
        // Virtual source
        VirtualSource,
        // Glue
        Glue,
        // Target
        Target, TargetUnit

        => "src/infrastructure/api/realearn.ts"
    }

    #[test]
    fn export_json_schema() {
        let settings = schemars::gen::SchemaSettings::draft07().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = false;
        });
        let gen = settings.into_generator();
        let schema = gen.into_root_schema_for::<Mapping>();
        let schema_json = serde_json::to_string_pretty(&schema).unwrap();
        std::fs::write("src/infrastructure/api/realearn.schema.json", schema_json).unwrap();
    }

    #[test]
    fn example() {
        let mapping = Mapping {
            key: Some("volume".to_string()),
            name: Some("Volume".to_string()),
            tags: Some(vec!["mix".to_string(), "master".to_string()]),
            group: Some("faders".to_string()),
            visible_in_projection: Some(true),
            enabled: Some(true),
            control_enabled: Some(true),
            feedback_enabled: Some(true),
            active: Some(Active::Always),
            feedback_behavior: Some(FeedbackBehavior::Normal),
            on_activate: Some(Lifecycle::Normal),
            on_deactivate: Some(Lifecycle::Normal),
            source: Some(Source::Midi(MidiSource::ControlChangeValue(
                ControlChangeValueSource {
                    channel: Some(0),
                    controller_number: Some(64),
                    character: Some(SourceCharacter::Button),
                    fourteen_bit: Some(false),
                },
            ))),
            glue: Some(Glue {
                source_interval: (0.3, 0.7),
            }),
            target: Some(Target {
                unit: Some(TargetUnit::Percent),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/infrastructure/api/example.json", json).unwrap();
    }
}

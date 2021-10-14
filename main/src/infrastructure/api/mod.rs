mod mapping {
    use super::glue::*;
    use super::source::*;
    use super::target::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
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

    #[derive(Serialize, Deserialize, JsonSchema, TS)]
    pub enum Lifecycle {
        Normal,
    }

    #[derive(Serialize, Deserialize, JsonSchema, TS)]
    pub enum FeedbackBehavior {
        Normal,
    }

    #[derive(Serialize, Deserialize, JsonSchema, TS)]
    pub enum Active {
        Always,
    }
}

mod source {
    use midi::*;
    use osc::*;
    use reaper::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;
    use virt::*;

    #[derive(Serialize, Deserialize, JsonSchema, TS)]
    #[serde(tag = "type")]
    pub enum Source {
        None,
        Reaper(ReaperSource),
        // MIDI
        MidiNoteVelocity(MidiNoteVelocitySource),
        MidiNoteKeyNumber(MidiNoteKeyNumberSource),
        MidiPolyphonicKeyPressureAmount(MidiPolyphonicKeyPressureAmountSource),
        MidiControlChangeValue(MidiControlChangeValueSource),
        MidiProgramChangeNumber(MidiProgramChangeNumberSource),
        MidiChannelPressureAmount(MidiChannelPressureAmountSource),
        MidiPitchBendChangeValue(MidiPitchBendChangeValueSource),
        MidiParameterNumberValue(MidiParameterNumberValueSource),
        MidiClockTempo(MidiClockTempoSource),
        MidiClockTransport(MidiClockTransportSource),
        MidiRaw(MidiRawSource),
        MidiScript(MidiScriptSource),
        MidiDisplay(MidiDisplaySource),
        // OSC
        Osc(OscSource),
        Virtual(VirtualSource),
    }

    pub mod reaper {
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};
        use ts_rs::TS;

        #[derive(Serialize, Deserialize, JsonSchema, TS)]
        pub enum ReaperSource {
            MidiDeviceChanges,
            RealearnInstanceStart,
        }
    }

    pub mod midi {
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};
        use ts_rs::TS;

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiNoteVelocitySource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub key_number: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiNoteKeyNumberSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiPolyphonicKeyPressureAmountSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub key_number: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiControlChangeValueSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub controller_number: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub character: Option<SourceCharacter>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub fourteen_bit: Option<bool>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiProgramChangeNumberSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiChannelPressureAmountSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiPitchBendChangeValueSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiParameterNumberValueSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub number: Option<u16>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub fourteen_bit: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub registered: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub character: Option<SourceCharacter>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiClockTempoSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            reserved: Option<String>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiClockTransportSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub message: Option<MidiClockTransportMessage>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiRawSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub pattern: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub character: Option<SourceCharacter>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiScriptSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub script: Option<String>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MidiDisplaySource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub spec: Option<MidiDisplaySpec>,
        }

        #[derive(Serialize, Deserialize, JsonSchema, TS)]
        pub enum SourceCharacter {
            Range,
            Button,
            Relative1,
            Relative2,
            Relative3,
            StatefulButton,
        }

        #[derive(Serialize, Deserialize, JsonSchema, TS)]
        pub enum MidiClockTransportMessage {
            Start,
            Continue,
            Stop,
        }

        #[derive(Serialize, Deserialize, JsonSchema, TS)]
        #[serde(tag = "type")]
        pub enum MidiDisplaySpec {
            MackieLcd(MackieLcdSpec),
            MackieSevenSegmentDisplay(MackieSevenSegmentDisplaySpec),
            SiniConE24(SiniConE24Spec),
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MackieLcdSpec {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub channel: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub line: Option<u8>,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct MackieSevenSegmentDisplaySpec {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub scope: Option<MackieSevenSegmentDisplayScope>,
        }

        #[derive(Serialize, Deserialize, JsonSchema, TS)]
        pub enum MackieSevenSegmentDisplayScope {
            All,
            Assignment,
            Tc,
            TcHoursBars,
            TcMinutesBeats,
            TcSecondsSub,
            TcFramesTicks,
        }

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct SiniConE24Spec {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub cell_index: Option<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub item_index: Option<u8>,
        }
    }

    pub mod osc {
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};
        use ts_rs::TS;

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct OscSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub reserved: Option<u8>,
        }
    }

    pub mod virt {
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};
        use ts_rs::TS;

        #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
        pub struct VirtualSource {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub reserved: Option<u8>,
        }
    }
}

mod glue {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
    pub struct Glue {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source_interval: Option<(f64, f64)>,
    }
}

mod target {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    #[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
    pub struct Target {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub unit: Option<TargetUnit>,
    }

    #[derive(Serialize, Deserialize, JsonSchema, TS)]
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
        Mapping,
        Lifecycle,
        FeedbackBehavior,
        Active,
        // Source
        Source,
        // REAPER source
        ReaperSource,
        // MIDI source
        MidiNoteVelocitySource,
        MidiNoteKeyNumberSource,
        MidiPolyphonicKeyPressureAmountSource,
        MidiControlChangeValueSource,
        MidiProgramChangeNumberSource,
        MidiChannelPressureAmountSource,
        MidiPitchBendChangeValueSource,
        MidiParameterNumberValueSource,
        MidiClockTempoSource,
        MidiClockTransportSource,
        MidiRawSource,
        MidiScriptSource,
        MidiDisplaySource,
        SourceCharacter,
        MidiClockTransportMessage,
        MidiDisplaySpec,
        MackieLcdSpec,
        MackieSevenSegmentDisplaySpec,
        MackieSevenSegmentDisplayScope,
        SiniConE24Spec,
        // OSC source
        OscSource,
        // Virtual source
        VirtualSource,
        // Glue
        Glue,
        // Target
        Target,
        TargetUnit

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
            source: Some(Source::MidiControlChangeValue(
                MidiControlChangeValueSource {
                    channel: Some(0),
                    controller_number: Some(64),
                    character: Some(SourceCharacter::Button),
                    fourteen_bit: Some(false),
                },
            )),
            glue: Some(Glue {
                source_interval: Some((0.3, 0.7)),
            }),
            target: Some(Target {
                unit: Some(TargetUnit::Percent),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/infrastructure/api/example.json", json).unwrap();
    }

    #[test]
    fn example_from_lua() {
        use mlua::{Lua, LuaSerdeExt};
        let lua = Lua::new();
        let value = lua.load(include_str!("example.lua")).eval().unwrap();
        let mapping: Mapping = lua.from_value(value).unwrap();
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/infrastructure/api/example_from_lua.json", json).unwrap();
    }
}

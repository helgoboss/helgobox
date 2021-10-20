mod compartment;
mod glue;
mod group;
mod mapping;
mod parameter;
mod source;
mod target;

pub use compartment::*;
pub use glue::*;
pub use group::*;
pub use mapping::*;
pub use parameter::*;
pub use source::*;
pub use target::*;

#[cfg(test)]
mod tests {
    use super::*;

    ts_rs::export! {
        // Mapping
        Mapping,
        Lifecycle,
        FeedbackBehavior,
        ActivationCondition,
        // Source
        Source,
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
        SourceCharacter,
        MidiClockTransportMessage,
        MackieLcd,
        MackieSevenSegmentDisplay,
        MackieSevenSegmentDisplayScope,
        SiniConE24Display,
        // OSC source
        OscSource,
        // Virtual source
        VirtualSource,
        // Glue
        Glue,
        // Target
        Target,
        TargetUnit

        => "src/infrastructure/api/schema/generated/realearn.ts"
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
        std::fs::write(
            "src/infrastructure/api/schema/generated/realearn.schema.json",
            schema_json,
        )
        .unwrap();
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
            activation_condition: None,
            feedback_behavior: Some(FeedbackBehavior::Normal),
            on_activate: Some(Lifecycle::Todo),
            on_deactivate: Some(Lifecycle::Todo),
            source: Some(Source::MidiControlChangeValue(
                MidiControlChangeValueSource {
                    channel: Some(0),
                    controller_number: Some(64),
                    character: Some(SourceCharacter::Button),
                    fourteen_bit: Some(false),
                },
            )),
            glue: Some(Glue {
                source_interval: Some(Interval(0.3, 0.7)),
                ..Default::default()
            }),
            target: Some(Target {
                unit: Some(TargetUnit::Percent),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/infrastructure/api/schema/test/example.json", json).unwrap();
    }

    #[test]
    fn example_from_lua() {
        use mlua::{Lua, LuaSerdeExt};
        let lua = Lua::new();
        let value = lua.load(include_str!("test/example.lua")).eval().unwrap();
        let mapping: Mapping = lua.from_value(value).unwrap();
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write(
            "src/infrastructure/api/schema/test/example_from_lua.json",
            json,
        )
        .unwrap();
    }
}

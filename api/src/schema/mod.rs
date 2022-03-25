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

use playtime_api::Matrix;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub value: T,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ApiObject {
    ClipMatrix(Envelope<Box<Option<Matrix>>>),
    MainCompartment(Envelope<Box<Compartment>>),
    ControllerCompartment(Envelope<Box<Compartment>>),
    Mappings(Envelope<Vec<Mapping>>),
    Mapping(Envelope<Box<Mapping>>),
}

impl ApiObject {
    pub fn into_mappings(self) -> Option<Vec<Mapping>> {
        match self {
            ApiObject::Mappings(Envelope { value: mappings }) => Some(mappings),
            ApiObject::Mapping(Envelope { value: m }) => Some(vec![*m]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_json_schema() {
        let settings = schemars::gen::SchemaSettings::draft07().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = false;
        });
        let gen = settings.into_generator();
        let schema = gen.into_root_schema_for::<Compartment>();
        let schema_json = serde_json::to_string_pretty(&schema).unwrap();
        std::fs::write("src/schema/generated/realearn.schema.json", schema_json).unwrap();
    }

    #[test]
    fn example() {
        let mapping = Mapping {
            id: Some("volume".to_string()),
            name: Some("Volume".to_string()),
            tags: Some(vec!["mix".to_string(), "master".to_string()]),
            group: Some("faders".to_string()),
            visible_in_projection: Some(true),
            enabled: Some(true),
            control_enabled: Some(true),
            feedback_enabled: Some(true),
            activation_condition: None,
            source: Some(Source::MidiControlChangeValue(
                MidiControlChangeValueSource {
                    feedback_behavior: Some(FeedbackBehavior::Normal),
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
            target: None,
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/schema/test/example.json", json).unwrap();
    }

    #[test]
    fn example_from_lua() {
        use mlua::{Lua, LuaSerdeExt};
        let lua = Lua::new();
        let value = lua.load(include_str!("test/example.lua")).eval().unwrap();
        let mapping: Mapping = lua.from_value(value).unwrap();
        let json = serde_json::to_string_pretty(&mapping).unwrap();
        std::fs::write("src/schema/test/example_from_lua.json", json).unwrap();
    }
}

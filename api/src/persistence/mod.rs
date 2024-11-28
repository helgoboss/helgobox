mod compartment;
mod controller;
mod glue;
mod group;
mod instance;
mod mapping;
mod parameter;
mod preset;
mod root;
mod session;
mod source;
mod target;

pub use compartment::*;
pub use controller::*;
pub use glue::*;
pub use group::*;
pub use instance::*;
pub use mapping::*;
pub use parameter::*;
pub use preset::*;
pub use root::*;
pub use session::*;
pub use source::*;
pub use target::*;

use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope<T> {
    #[serde(default)]
    pub version: Option<Version>,
    pub value: T,
}

impl<T> Envelope<T> {
    pub fn new(version: Option<Version>, value: T) -> Self {
        Self { version, value }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ApiObject {
    /// A Playtime matrix.
    ClipMatrix(Envelope<Box<Option<playtime_api::persistence::FlexibleMatrix>>>),
    /// Main compartment.
    MainCompartment(Envelope<Box<Compartment>>),
    /// Controller compartment.
    ControllerCompartment(Envelope<Box<Compartment>>),
    /// A flat list of mappings.
    Mappings(Envelope<Vec<Mapping>>),
    /// A single mapping.
    Mapping(Envelope<Box<Mapping>>),
}

impl ApiObject {
    pub fn into_mappings(self) -> Option<Envelope<Vec<Mapping>>> {
        match self {
            ApiObject::Mappings(Envelope {
                value: mappings,
                version,
            }) => Some(Envelope::new(version, mappings)),
            ApiObject::Mapping(Envelope { value: m, version }) => {
                Some(Envelope::new(version, vec![*m]))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_to_json() {
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
        serde_json::to_string_pretty(&mapping).unwrap();
        // std::fs::write("src/schema/test/example.json", json).unwrap();
    }

    #[test]
    fn example_from_lua() {
        use mlua::{Lua, LuaSerdeExt};
        let lua = Lua::new();
        let value = lua.load(include_str!("test/example.lua")).eval().unwrap();
        let mapping: Mapping = lua.from_value(value).unwrap();
        serde_json::to_string_pretty(&mapping).unwrap();
        // std::fs::write("src/schema/test/example_from_lua.json", json).unwrap();
    }
}

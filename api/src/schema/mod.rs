mod compartment;
mod glue;
mod group;
mod mapping;
mod parameter;
mod root;
mod session;
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
    use crate::schema::root::ReaLearn;
    use okapi::openapi3::{Components, OpenApi};
    use schemars::schema::{InstanceType, SchemaObject, SingleOrVec};

    #[test]
    fn export_json_schema() {
        let settings = schemars::gen::SchemaSettings::draft07().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = false;
        });
        let gen = settings.into_generator();
        let schema = gen.into_root_schema_for::<ReaLearn>();
        let schema_json = serde_json::to_string_pretty(&schema).unwrap();
        std::fs::write("src/schema/json-schema/realearn.schema.json", schema_json).unwrap();
    }

    #[test]
    fn export_openapi() {
        let settings = schemars::gen::SchemaSettings::openapi3().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = false;
        });
        let gen = settings.into_generator();
        let schema = gen.into_root_schema_for::<ReaLearn>();
        let mut openapi: OpenApi =
            serde_yaml::from_str(include_str!("realearn.template.yaml")).unwrap();
        openapi.info.version = env!("CARGO_PKG_VERSION").to_owned();
        openapi.components = {
            let components = Components {
                schemas: schema
                    .definitions
                    .into_iter()
                    .map(|(key, schema)| {
                        let mut schema_object = schema.into_object();
                        transform_schema_object(&mut schema_object);
                        (key, schema_object)
                    })
                    .collect(),
                ..Default::default()
            };
            Some(components)
        };
        let schema_yaml = serde_yaml::to_string(&openapi).unwrap();
        std::fs::write("src/schema/openapi/realearn.yaml", schema_yaml).unwrap();
    }

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

    /// Transforms a schema to reflect our needs.
    fn transform_schema_object(schema_object: &mut SchemaObject) {
        if schema_object.has_type(InstanceType::Array) {
            apply_tuple_transformation(schema_object);
        }
    }

    /// Applies the tuple transformation.
    ///
    /// When schemars serializes a Rust tuple, `items` will obtain an array value.
    /// The problem is that openapi 3.0.0 expects an object value. It doesn't support
    /// specifying the type for each array element separately. This is supported in
    /// openapi 3.1.0 by using `prefixItems` instead of `items`. But this isn't supported
    /// by `swagger-dart-code-generator`. So we simply do the following transformation:
    ///
    /// Input:
    ///
    /// ```yaml
    /// RgbColor:
    ///   type: array
    ///   items:
    ///     - type: integer
    ///       format: uint8
    ///       minimum: 0.0
    ///     - type: integer
    ///       format: uint8
    ///       minimum: 0.0
    ///     - type: integer
    ///       format: uint8
    ///       minimum: 0.0
    ///   maxItems: 3
    ///   minItems: 3
    /// ```
    ///
    /// Output:
    ///
    /// ```yaml
    /// RgbColor:
    ///   type: array
    ///   items:
    ///     type: integer
    ///     format: uint8
    ///     minimum: 0.0
    ///   maxItems: 3
    ///   minItems: 3
    /// ```
    fn apply_tuple_transformation(schema_object: &mut SchemaObject) {
        if let Some(array) = schema_object.array.as_mut() {
            let items = array.items.take().unwrap();
            let new_items = if let SingleOrVec::Vec(vec) = items {
                let first_item_schema = vec.into_iter().next().unwrap();
                SingleOrVec::Single(Box::new(first_item_schema))
            } else {
                items
            };
            array.items = Some(new_items);
        }
    }
}

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
    use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};
    use schemars::visit::{visit_schema_object, Visitor};
    use std::collections::{HashMap, HashSet};

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
    fn export_openapi_normal() {
        export_openapi_internal("realearn.yaml", |openapi, root_schema| {
            openapi.info.description = Some("ReaLearn API".to_owned());
            apply_tuple_transformation(root_schema);
        });
    }

    #[test]
    fn export_openapi_simplified() {
        export_openapi_internal("realearn-simplified.yaml", |openapi, root_schema| {
            openapi.info.description =
                Some("ReaLearn API without aliased types (for compatibility with swagger-dart-code-generator)".to_owned());
            apply_tuple_transformation(root_schema);
            apply_type_alias_transformation(root_schema);
        });
    }

    fn export_openapi_internal(
        file_name: &str,
        apply_transformations: impl FnOnce(&mut OpenApi, &mut RootSchema),
    ) {
        let settings = schemars::gen::SchemaSettings::openapi3().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = false;
        });
        let gen = settings.into_generator();
        let mut root_schema = gen.into_root_schema_for::<ReaLearn>();
        let mut openapi: OpenApi =
            serde_yaml::from_str(include_str!("realearn.template.yaml")).unwrap();
        apply_transformations(&mut openapi, &mut root_schema);
        openapi.info.version = env!("CARGO_PKG_VERSION").to_owned();
        openapi.components = {
            let components = Components {
                schemas: {
                    root_schema
                        .definitions
                        .into_iter()
                        .map(|(key, schema)| (key, schema.into_object()))
                        .collect()
                },
                ..Default::default()
            };
            Some(components)
        };
        let schema_yaml = serde_yaml::to_string(&openapi).unwrap();
        std::fs::write(format!("src/schema/openapi/{}", file_name), schema_yaml).unwrap();
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

    /// Applies the type alias transformation.
    ///
    /// schemars correctly generates an
    /// [aliased type](https://goswagger.io/use/models/schemas.html#type-aliasing) for Rust
    /// newtypes. We have a couple of those, e.g. `Bpm` and I like them because they convey
    /// meaning and enforce additional value range constraints. But unfortunately,
    /// `swagger-dart-code-generator` doesn't generate `typedef`s as a result. It generates wrapper
    /// objects, which is incorrect.
    ///
    /// This transformation simply resolves the aliases.
    ///
    /// Input:
    ///
    /// ```yaml
    /// Section:
    ///   type: object
    ///   properties:
    ///     length:
    ///       allOf:
    ///         - $ref: "#/components/schemas/PositiveSecond"
    /// PositiveSecond:
    ///   type: number
    ///   format: double
    /// ```
    ///
    /// Output:
    ///
    /// ```yaml
    /// Section:
    ///   type: object
    ///   properties:
    ///     length:
    ///       type: number
    ///       format: double
    /// ```
    fn apply_type_alias_transformation(root_schema: &mut RootSchema) {
        // Remove type alias schemas
        let type_alias_keys: HashSet<_> = root_schema
            .definitions
            .iter()
            .filter_map(|(key, schema)| match schema {
                Schema::Object(SchemaObject {
                    instance_type: Some(SingleOrVec::Single(b)),
                    enum_values,
                    ..
                }) => {
                    let instance_type = **b;
                    let is_type_alias_for_string =
                        instance_type == InstanceType::String && enum_values.is_none();
                    let is_type_alias_for_number = instance_type == InstanceType::Number;
                    let is_type_alias = is_type_alias_for_string || is_type_alias_for_number;
                    if is_type_alias {
                        Some(key.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        let type_aliases = type_alias_keys
            .into_iter()
            .map(|key| {
                let reference = format!("#/components/schemas/{}", key);
                let schema = root_schema.definitions.remove(&key).unwrap();
                (reference, schema.into_object())
            })
            .collect();
        let mut transformer = TypeAliasTransformer {
            aliases: type_aliases,
        };
        // Modify remaining schemas
        apply_transformation_to_all_schema_objects(root_schema, |obj| {
            apply_type_alias_transformation_to_schema_object(obj, &mut transformer)
        });
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
    fn apply_tuple_transformation(root_schema: &mut RootSchema) {
        apply_transformation_to_all_schema_objects(
            root_schema,
            apply_tuple_transformation_to_schema_object,
        );
    }

    fn apply_transformation_to_all_schema_objects(
        root_schema: &mut RootSchema,
        mut f: impl FnMut(&mut SchemaObject),
    ) {
        for schema in root_schema.definitions.values_mut() {
            match schema {
                Schema::Bool(_) => {}
                Schema::Object(obj) => {
                    f(obj);
                }
            }
        }
    }

    fn apply_tuple_transformation_to_schema_object(schema_object: &mut SchemaObject) {
        if !schema_object.has_type(InstanceType::Array) {
            return;
        }
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

    fn apply_type_alias_transformation_to_schema_object(
        schema_object: &mut SchemaObject,
        transformer: &mut TypeAliasTransformer,
    ) {
        visit_schema_object(transformer, schema_object);
    }

    struct TypeAliasTransformer {
        aliases: HashMap<String, SchemaObject>,
    }

    impl TypeAliasTransformer {
        fn extract_type_alias_schema(&self, schema_object: &SchemaObject) -> Option<SchemaObject> {
            if let Some(reference) = &schema_object.reference {
                // Reference to type alias not as part of "allOf"
                if let Some(type_alias_schema) = self.aliases.get(reference) {
                    Some(type_alias_schema.clone())
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    impl Visitor for TypeAliasTransformer {
        fn visit_schema_object(&mut self, schema: &mut SchemaObject) {
            if let Some(all_of) = &mut schema.subschemas().all_of {
                // Reference to type alias as part of "allOf"
                if let [Schema::Object(single)] = all_of.as_mut_slice() {
                    if let Some(type_alias_schema) = self.extract_type_alias_schema(single) {
                        // Reference to type alias not as part of "allOf"
                        schema.subschemas = None;
                        schema.instance_type = type_alias_schema.instance_type;
                        schema.format = type_alias_schema.format;
                        return;
                    }
                }
            } else if let Some(type_alias_schema) = self.extract_type_alias_schema(&schema) {
                // Reference to type alias not as part of "allOf"
                *schema = type_alias_schema;
                return;
            }
            visit_schema_object(self, schema);
        }
    }
}

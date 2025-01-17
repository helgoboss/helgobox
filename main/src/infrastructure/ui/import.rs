use anyhow::{bail, Context};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
use std::os::raw::c_void;

use serde::{Deserialize, Serialize};

use crate::base::notification;
use crate::domain::{CompartmentKind, FsDirLuaModuleFinder, LuaModuleContainer, SafeLua};
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use crate::infrastructure::api::convert::{from_data, to_data};
use crate::infrastructure::data::{
    parse_lua_frontmatter, ActivationConditionData, CompartmentModelData, InstanceData,
    MappingModelData, ModeModelData, SourceModelData, TargetModelData, UnitData,
};
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::lua_serializer;
use crate::infrastructure::ui::util::open_in_browser;
use base::hash_util::NonCryptoHashSet;
use helgobox_api::persistence;
use helgobox_api::persistence::{ApiObject, CommonPresetMetaData, Envelope};
use mlua::prelude::LuaError;
use mlua::Value;
use playtime_api::persistence::FlexibleMatrix;
use reaper_high::Reaper;
use semver::Version;

pub enum UntaggedDataObject {
    Tagged(DataObject),
    PresetLike(CommonPresetData),
}

impl UntaggedDataObject {
    pub fn version(&self) -> Option<&Version> {
        match self {
            UntaggedDataObject::Tagged(o) => o.version(),
            UntaggedDataObject::PresetLike(d) => d.version.as_ref(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DataObject {
    /// A complete Helgobox instance.
    Instance(Envelope<Box<InstanceData>>),
    /// A Helgobox unit (within an instance).
    #[serde(alias = "Session")]
    Unit(Envelope<Box<UnitData>>),
    /// A Playtime matrix.
    ClipMatrix(Envelope<Box<Option<playtime_api::persistence::FlexibleMatrix>>>),
    /// Main compartment.
    MainCompartment(Envelope<Box<CompartmentModelData>>),
    /// Controller compartment.
    ControllerCompartment(Envelope<Box<CompartmentModelData>>),
    /// Flat list of mappings.
    Mappings(Envelope<Vec<MappingModelData>>),
    /// Single mapping.
    Mapping(Envelope<Box<MappingModelData>>),
    /// Mapping source.
    Source(Envelope<Box<SourceModelData>>),
    /// Mapping glue.
    #[serde(alias = "Mode")]
    Glue(Envelope<Box<ModeModelData>>),
    /// Mapping target.
    Target(Envelope<Box<TargetModelData>>),
    /// Mapping activation condition.
    ActivationCondition(Envelope<Box<ActivationConditionData>>),
}

/// This corresponds to the way controller and main presets are structured.
///
/// They don't have an envelope. We also want to be able to import their data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonPresetData {
    #[serde(default)]
    pub version: Option<Version>,
    pub name: String,
    #[serde(flatten)]
    pub data: Box<CompartmentModelData>,
}

impl DataObject {
    pub fn try_from_api_object(
        api_object: ApiObject,
        conversion_context: &impl ApiToDataConversionContext,
    ) -> anyhow::Result<Self> {
        let data_object = match api_object {
            ApiObject::ClipMatrix(envelope) => {
                if let Some(FlexibleMatrix::Unsigned(m)) = &*envelope.value {
                    warn_about_unknown_props("importing Playtime matrix", &m.unknown_props);
                }
                DataObject::ClipMatrix(envelope)
            }
            ApiObject::MainCompartment(Envelope { value: c, version }) => {
                warn_about_unknown_props("importing main compartment", &c.unknown_props);
                let data_compartment = to_data::convert_compartment(CompartmentKind::Main, *c)?;
                DataObject::MainCompartment(Envelope::new(version, Box::new(data_compartment)))
            }
            ApiObject::ControllerCompartment(Envelope { value: c, version }) => {
                warn_about_unknown_props("importing controller compartment", &c.unknown_props);
                let data_compartment =
                    to_data::convert_compartment(CompartmentKind::Controller, *c)?;
                DataObject::ControllerCompartment(Envelope::new(
                    version,
                    Box::new(data_compartment),
                ))
            }
            ApiObject::Mappings(Envelope {
                value: mappings,
                version,
            }) => {
                let data_mappings = Self::try_from_api_mappings(mappings, conversion_context);
                DataObject::Mappings(Envelope::new(version, data_mappings?))
            }
            ApiObject::Mapping(Envelope { value: m, version }) => {
                let data_mapping = to_data::convert_mapping(*m, conversion_context)?;
                DataObject::Mapping(Envelope::new(version, Box::new(data_mapping)))
            }
        };
        Ok(data_object)
    }

    pub fn try_from_api_mappings(
        api_mappings: Vec<persistence::Mapping>,
        conversion_context: &impl ApiToDataConversionContext,
    ) -> anyhow::Result<Vec<MappingModelData>> {
        api_mappings
            .into_iter()
            .map(|m| to_data::convert_mapping(m, conversion_context))
            .collect()
    }

    pub fn try_into_api_object(
        self,
        conversion_style: ConversionStyle,
    ) -> anyhow::Result<ApiObject> {
        let api_object = match self {
            DataObject::Unit(Envelope { .. }) => todo!("session API not yet implemented"),
            DataObject::ClipMatrix(envelope) => ApiObject::ClipMatrix(envelope),
            DataObject::MainCompartment(Envelope { value: c, version }) => {
                let api_compartment = from_data::convert_compartment(*c, conversion_style)?;
                ApiObject::MainCompartment(Envelope::new(version, Box::new(api_compartment)))
            }
            DataObject::ControllerCompartment(Envelope { value: c, version }) => {
                let api_compartment = from_data::convert_compartment(*c, conversion_style)?;
                ApiObject::ControllerCompartment(Envelope::new(version, Box::new(api_compartment)))
            }
            DataObject::Mappings(Envelope {
                value: mappings,
                version,
            }) => {
                let api_mappings: Result<Vec<_>, _> = mappings
                    .into_iter()
                    .map(|m| from_data::convert_mapping(m, conversion_style))
                    .collect();
                ApiObject::Mappings(Envelope::new(version, api_mappings?))
            }
            DataObject::Mapping(Envelope { value: m, version }) => {
                let api_mapping = from_data::convert_mapping(*m, conversion_style)?;
                ApiObject::Mapping(Envelope::new(version, Box::new(api_mapping)))
            }
            _ => {
                bail!("conversion from source/mode/target data object not supported at the moment");
            }
        };
        Ok(api_object)
    }

    pub fn version(&self) -> Option<&Version> {
        use DataObject::*;
        match self {
            Instance(v) => v.version.as_ref(),
            Unit(v) => v.version.as_ref(),
            ClipMatrix(v) => v.version.as_ref(),
            MainCompartment(v) => v.version.as_ref(),
            ControllerCompartment(v) => v.version.as_ref(),
            Mappings(v) => v.version.as_ref(),
            Mapping(v) => v.version.as_ref(),
            Source(v) => v.version.as_ref(),
            Glue(v) => v.version.as_ref(),
            Target(v) => v.version.as_ref(),
            ActivationCondition(v) => v.version.as_ref(),
        }
    }
}

/// Attempts to deserialize a data object supporting both JSON and Lua.
pub fn deserialize_data_object(
    text: &str,
    conversion_context: &impl ApiToDataConversionContext,
) -> anyhow::Result<UntaggedDataObject> {
    // Try as JSON data object
    let json_data_object_result =
        serde_json::from_str::<DataObject>(text).map(UntaggedDataObject::Tagged);
    let json_data_object_error = match json_data_object_result {
        Ok(object) => return Ok(object),
        Err(e) => e,
    };
    // That didn't work. Try as JSON preset.
    let json_preset_result =
        serde_json::from_str::<CommonPresetData>(text).map(UntaggedDataObject::PresetLike);
    let json_preset_error = match json_preset_result {
        Ok(object) => return Ok(object),
        Err(e) => e,
    };
    // That didn't work. Execute as Lua.
    let lua = SafeLua::new()?;
    let lua_execution_result =
        execute_lua_import_script(&lua, text, conversion_context.compartment(), true);
    let (lua_execution_error, lua_api_object_error, lua_preset_error) = match lua_execution_result {
        Err(e) => (Some(e), None, None),
        Ok(value) => {
            // At first try deserializing as Lua API object
            let lua_api_object_result = SafeLua::from_value::<ApiObject>(value.clone())
                .and_then(|api_object| {
                    DataObject::try_from_api_object(api_object, conversion_context)
                        .context("converting API object to data object")
                })
                .map(UntaggedDataObject::Tagged);
            let lua_api_object_error = match lua_api_object_result {
                Ok(object) => return Ok(object),
                Err(e) => e,
            };
            // That wasn't it. Try deserializing as Lua preset.
            // We don't need the full metadata here (controller/main-preset specific), just the common one.
            // Actually only the version is important because it might influence import behavior.
            let lua_preset_result = parse_lua_frontmatter::<CommonPresetMetaData>(text)
                .and_then(|meta_data| {
                    let compartment = SafeLua::from_value::<persistence::Compartment>(value)?;
                    warn_about_unknown_props("importing as Lua preset", &compartment.unknown_props);
                    // When importing a Lua preset, we expect at least the "mappings" property. It's not strictly necessary
                    // for a preset to have mappings, but when importing stuff it's important that we have good error reporting.
                    // If we accept pretty much all possible tables as valid Lua preset, the user will never see an error
                    // message when he made a grave error, e.g. providing a completely different data structure with only
                    // unknown properties.
                    compartment
                        .mappings
                        .as_ref()
                        .context("property \"mappings\" not provided")?;
                    Ok((meta_data, compartment))
                })
                .and_then(|(meta_data, compartment)| {
                    let compartment_data = to_data::convert_compartment(
                        conversion_context.compartment(),
                        compartment,
                    )?;
                    let common_preset_data = CommonPresetData {
                        version: meta_data.realearn_version,
                        name: meta_data.name,
                        data: Box::new(compartment_data),
                    };
                    Ok(common_preset_data)
                })
                .map(UntaggedDataObject::PresetLike);
            let lua_preset_error = match lua_preset_result {
                Ok(object) => return Ok(object),
                Err(e) => e,
            };
            (None, Some(lua_api_object_error), Some(lua_preset_error))
        }
    };
    // Nothing fits :(
    bail!(
        "Clipboard content doesn't look like proper ReaLearn import data:\n\n\
        Invalid JSON API object:\n\
        {json_data_object_error}\n\n\
        Invalid JSON preset:\n\
        {json_preset_error:#}\n\n\
        Invalid Lua code:\n\
        {lua_execution_error:#?}\n\n\
        Invalid Lua API object:\n\
        {lua_api_object_error:#?}\n\n\
        Invalid Lua preset:\n\
        {lua_preset_error:#?}"
    );
}

pub fn deserialize_data_object_from_json(text: &str) -> Result<DataObject, Box<dyn Error>> {
    Ok(serde_json::from_str(text)?)
}

pub fn serialize_data_object_to_json(object: DataObject) -> anyhow::Result<String> {
    serde_json::to_string_pretty(&object).context("couldn't serialize object")
}

/// Runs without importing the result and also doesn't have an execution time limit.
pub fn dry_run_lua_script(text: &str, active_compartment: CompartmentKind) -> anyhow::Result<()> {
    let lua = SafeLua::new()?;
    let value = execute_lua_import_script(&lua, text, active_compartment, false)?;
    let json = serde_json::to_string_pretty(&value)?;
    match BackboneShell::get_temp_dir() {
        None => {
            Reaper::get().show_console_msg(json);
        }
        Some(dir) => {
            let json_file = dir.path().join("dry-run.json");
            std::fs::write(&json_file, json)?;
            open_in_browser(&json_file.to_string_lossy())
        }
    }
    Ok(())
}

pub enum SerializationFormat {
    JsonDataObject,
    LuaApiObject(ConversionStyle),
}

pub fn serialize_data_object(
    data_object: DataObject,
    format: SerializationFormat,
) -> anyhow::Result<String> {
    match format {
        SerializationFormat::JsonDataObject => serialize_data_object_to_json(data_object),
        SerializationFormat::LuaApiObject(style) => {
            serialize_data_object_to_lua(data_object, style)
        }
    }
}

pub fn serialize_data_object_to_lua(
    data_object: DataObject,
    conversion_style: ConversionStyle,
) -> anyhow::Result<String> {
    let api_object = data_object.try_into_api_object(conversion_style)?;
    Ok(lua_serializer::to_string(&api_object)?)
}

pub fn deserialize_api_object_from_lua(
    text: &str,
    active_compartment: CompartmentKind,
) -> anyhow::Result<ApiObject> {
    let lua = SafeLua::new()?;
    let value = execute_lua_import_script(&lua, text, active_compartment, true)?;
    SafeLua::from_value(value)
}

fn verify_no_recursive_tables(value: &Value) -> Result<(), LuaError> {
    verify_no_recursive_tables_internal(value, &mut Default::default(), &mut Default::default())
}

fn verify_no_recursive_tables_internal(
    value: &Value,
    visited_tables: &mut NonCryptoHashSet<*const c_void>,
    key_stack: &mut Vec<String>,
) -> Result<(), LuaError> {
    if let Value::Table(t) = value {
        let table_pointer = t.to_pointer();
        if !visited_tables.insert(table_pointer) {
            let msg = format!("Detected recursive table at {key_stack:?}");
            return Err(LuaError::runtime(msg));
        }
        t.for_each(|key: Value, value: Value| {
            key_stack.push(key.to_string().unwrap_or_default());
            verify_no_recursive_tables_internal(&key, visited_tables, key_stack)?;
            verify_no_recursive_tables_internal(&value, visited_tables, key_stack)?;
            key_stack.pop();
            Ok(())
        })?;
        visited_tables.remove(&table_pointer);
    }
    Ok(())
}

fn execute_lua_import_script(
    lua: &SafeLua,
    code: &str,
    active_compartment: CompartmentKind,
    limit_execution_time: bool,
) -> anyhow::Result<mlua::Value> {
    if limit_execution_time {
        lua.start_execution_time_limit_countdown();
    }
    // Add support for require, but only for the logged-in user's presets. That means the module root will be the
    // subdirectory within the preset directory that has the name as the logged-in user's name.
    let preset_dir = BackboneShell::realearn_compartment_preset_dir_path(active_compartment);
    let module_finder = FsDirLuaModuleFinder::new(preset_dir.join(whoami::username()));
    let module_container = LuaModuleContainer::new(Ok(module_finder));
    let value =
        module_container.execute_as_module(lua.as_ref(), None, "Import".to_string(), code)?;
    // Recursive tables are always forbidden in import scenarios, so we check for them right here
    verify_no_recursive_tables(&value)?;
    Ok(value)
}

fn warn_about_unknown_props(
    label: &str,
    unknown_props: &Option<BTreeMap<String, serde_json::Value>>,
) {
    let Some(unknown_props) = unknown_props.as_ref() else {
        return;
    };
    if unknown_props.is_empty() {
        return;
    }
    let keys: Vec<_> = unknown_props.keys().collect();
    let msg = format!("The following imported properties were ignored when {label}: {keys:?}");
    notification::warn(msg);
}

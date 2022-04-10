use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use derive_more::Display;
use mlua::{ChunkMode, HookTriggers, Table};
use playtime_api::Matrix;
use serde::{Deserialize, Serialize};

use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use crate::infrastructure::api::convert::{from_data, to_data};
use crate::infrastructure::data::{
    CompartmentModelData, MappingModelData, ModeModelData, SessionData, SourceModelData,
    TargetModelData,
};
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::lua_serializer;
use crate::infrastructure::ui::util::open_in_browser;
use mlua::{Lua, LuaSerdeExt, Value};
use realearn_api::schema;
use realearn_api::schema::{ApiObject, Envelope};
use realearn_csi::{deserialize_csi_object_from_csi, AnnotatedResult, CsiObject};
use reaper_high::Reaper;

#[derive(Deserialize)]
#[serde(untagged)]
pub enum UntaggedDataObject {
    Tagged(DataObject),
    PresetLike(CommonPresetData),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DataObject {
    Session(Envelope<Box<SessionData>>),
    ClipMatrix(Envelope<Box<Option<Matrix>>>),
    MainCompartment(Envelope<Box<CompartmentModelData>>),
    ControllerCompartment(Envelope<Box<CompartmentModelData>>),
    Mappings(Envelope<Vec<MappingModelData>>),
    Mapping(Envelope<Box<MappingModelData>>),
    Source(Envelope<Box<SourceModelData>>),
    Mode(Envelope<Box<ModeModelData>>),
    Target(Envelope<Box<TargetModelData>>),
}

/// This corresponds to the way controller and main presets are structured.
///
/// They don't have an envelope. We also want to be able to import their data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonPresetData {
    pub name: String,
    #[serde(flatten)]
    pub data: Box<CompartmentModelData>,
}

impl DataObject {
    pub fn try_from_api_object(
        api_object: ApiObject,
        conversion_context: &impl ApiToDataConversionContext,
    ) -> Result<Self, Box<dyn Error>> {
        let data_object = match api_object {
            ApiObject::ClipMatrix(envelope) => DataObject::ClipMatrix(envelope),
            ApiObject::MainCompartment(Envelope { value: c }) => {
                let data_compartment = to_data::convert_compartment(*c)?;
                DataObject::MainCompartment(Envelope {
                    value: Box::new(data_compartment),
                })
            }
            ApiObject::ControllerCompartment(Envelope { value: c }) => {
                let data_compartment = to_data::convert_compartment(*c)?;
                DataObject::ControllerCompartment(Envelope {
                    value: Box::new(data_compartment),
                })
            }
            ApiObject::Mappings(Envelope { value: mappings }) => {
                let data_mappings = Self::try_from_api_mappings(mappings, conversion_context);
                DataObject::Mappings(Envelope {
                    value: data_mappings?,
                })
            }
            ApiObject::Mapping(Envelope { value: m }) => {
                let data_mapping = to_data::convert_mapping(*m, conversion_context)?;
                DataObject::Mapping(Envelope {
                    value: Box::new(data_mapping),
                })
            }
        };
        Ok(data_object)
    }

    pub fn try_from_api_mappings(
        api_mappings: Vec<schema::Mapping>,
        conversion_context: &impl ApiToDataConversionContext,
    ) -> Result<Vec<MappingModelData>, Box<dyn Error>> {
        api_mappings
            .into_iter()
            .map(|m| to_data::convert_mapping(m, conversion_context))
            .collect()
    }

    pub fn try_into_api_object(
        self,
        conversion_style: ConversionStyle,
    ) -> Result<ApiObject, Box<dyn Error>> {
        let api_object =
            match self {
                DataObject::Session(Envelope { .. }) => todo!("session API not yet implemented"),
                DataObject::ClipMatrix(envelope) => ApiObject::ClipMatrix(envelope),
                DataObject::MainCompartment(Envelope { value: c }) => {
                    let api_compartment = from_data::convert_compartment(*c, conversion_style)?;
                    ApiObject::MainCompartment(Envelope {
                        value: Box::new(api_compartment),
                    })
                }
                DataObject::ControllerCompartment(Envelope { value: c }) => {
                    let api_compartment = from_data::convert_compartment(*c, conversion_style)?;
                    ApiObject::ControllerCompartment(Envelope {
                        value: Box::new(api_compartment),
                    })
                }
                DataObject::Mappings(Envelope { value: mappings }) => {
                    let api_mappings: Result<Vec<_>, _> = mappings
                        .into_iter()
                        .map(|m| from_data::convert_mapping(m, conversion_style))
                        .collect();
                    ApiObject::Mappings(Envelope {
                        value: api_mappings?,
                    })
                }
                DataObject::Mapping(Envelope { value: m }) => {
                    let api_mapping = from_data::convert_mapping(*m, conversion_style)?;
                    ApiObject::Mapping(Envelope {
                        value: Box::new(api_mapping),
                    })
                }
                _ => return Err(
                    "conversion from source/mode/target data object not supported at the moment"
                        .into(),
                ),
            };
        Ok(api_object)
    }
}

/// Attempts to deserialize a data object supporting both JSON and Lua.
pub fn deserialize_data_object(
    text: &str,
    conversion_context: &impl ApiToDataConversionContext,
) -> Result<AnnotatedResult<UntaggedDataObject>, Box<dyn Error>> {
    let json_err = match deserialize_untagged_data_object_from_json(text) {
        Ok(o) => {
            return Ok(AnnotatedResult::without_annotations(o));
        }
        Err(e) => e,
    };
    let lua_err = match deserialize_data_object_from_lua(text, conversion_context) {
        Ok(o) => {
            return Ok(AnnotatedResult::without_annotations(
                UntaggedDataObject::Tagged(o),
            ));
        }
        Err(e) => e,
    };
    let csi_err = match deserialize_data_object_from_csi(text, conversion_context) {
        Ok(r) => {
            let untagged_data_object = UntaggedDataObject::Tagged(r.value);
            let annotated_result = AnnotatedResult {
                value: untagged_data_object,
                annotations: r.annotations,
            };
            return Ok(annotated_result);
        }
        Err(e) => e,
    };
    let msg = format!(
        "Clipboard content doesn't look like proper ReaLearn import data:\n\n\
        Invalid JSON: \n\
        {}\n\n\
        Invalid Lua: \n\
        {}\n\n\
        Invalid CSI: \n\
        {}",
        json_err, lua_err, csi_err
    );
    Err(msg.into())
}

pub fn deserialize_data_object_from_json(text: &str) -> Result<DataObject, Box<dyn Error>> {
    Ok(serde_json::from_str(text)?)
}

pub fn deserialize_untagged_data_object_from_json(
    text: &str,
) -> Result<UntaggedDataObject, Box<dyn Error>> {
    Ok(serde_json::from_str(text)?)
}

pub fn deserialize_data_object_from_csi(
    text: &str,
    conversion_context: &impl ApiToDataConversionContext,
) -> Result<AnnotatedResult<DataObject>, Box<dyn Error>> {
    let csi_object = deserialize_csi_object_from_csi(text)?;
    let api_object_res = CsiObject::try_into_api_object(csi_object)?;
    let res = AnnotatedResult {
        value: DataObject::try_from_api_object(api_object_res.value, conversion_context)?,
        annotations: api_object_res.annotations,
    };
    Ok(res)
}

pub fn deserialize_data_object_from_lua(
    text: &str,
    conversion_context: &impl ApiToDataConversionContext,
) -> Result<DataObject, Box<dyn Error>> {
    let api_object = deserialize_api_object_from_lua(text)?;
    let data_object = DataObject::try_from_api_object(api_object, conversion_context)?;
    Ok(data_object)
}

pub fn serialize_data_object_to_json(object: DataObject) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string_pretty(&object).map_err(|_| "couldn't serialize object")?)
}

pub fn dry_run_lua_script(text: &str) -> Result<(), Box<dyn Error>> {
    let lua = Lua::new();
    let value = execute_lua_script(&lua, text)?;
    let json = serde_json::to_string_pretty(&value)?;
    match App::get_temp_dir() {
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
) -> Result<String, Box<dyn Error>> {
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
) -> Result<String, Box<dyn Error>> {
    let api_object = data_object.try_into_api_object(conversion_style)?;
    Ok(lua_serializer::to_string(&api_object)?)
}

pub fn deserialize_api_object_from_lua(text: &str) -> Result<ApiObject, Box<dyn Error>> {
    let lua = Lua::new();
    let value = execute_lua_script(&lua, text)?;
    Ok(lua.from_value(value)?)
}

fn execute_lua_script<'a>(lua: &'a Lua, text: &str) -> Result<mlua::Value<'a>, Box<dyn Error>> {
    let instant = Instant::now();
    // Try to prevent code from taking too long to execute.
    lua.set_hook(
        HookTriggers::every_nth_instruction(10),
        move |_lua, _debug| {
            if instant.elapsed() > Duration::from_millis(200) {
                Err(mlua::Error::ExternalError(Arc::new(
                    RealearnScriptError::Timeout,
                )))
            } else {
                Ok(())
            }
        },
    )?;
    // Make sure we execute in a sort of sandbox.
    let env = build_safe_lua_env(&lua)?;
    // Add some useful functions (hidden, undocumented, subject to change!)
    let realearn_table = {
        let table = lua.create_table()?;
        let get_track_guid_by_index = lua.create_function(|_, index: u32| {
            let guid = Reaper::get()
                .current_project()
                .track_by_index(index)
                .map(|t| t.guid().to_string_without_braces());
            Ok(guid)
        })?;
        table.set("get_track_guid_by_index", get_track_guid_by_index)?;
        let print = lua.create_function(|_, arg: mlua::Value| {
            let text: String = match arg {
                Value::String(s) => format!("{}\n", s.to_string_lossy()),
                arg => format!("{:?}\n", arg),
            };
            Reaper::get().show_console_msg(text);
            Ok(())
        })?;
        table.set("print", print)?;
        table
    };
    env.set("realearn", realearn_table)?;
    // Load and evaluate script
    let lua_chunk = lua
        .load(text)
        .set_name("Import")?
        .set_mode(ChunkMode::Text)
        .set_environment(env)?;
    let value = lua_chunk.eval().map_err(|e| match e {
        mlua::Error::CallbackError { cause, .. } => {
            let boxed: Box<dyn Error> = Box::new(cause);
            boxed
        }
        e => Box::new(e),
    })?;
    Ok(value)
}

#[derive(Debug, Display)]
enum RealearnScriptError {
    #[display(fmt = "ReaLearn script took too long to execute")]
    Timeout,
}

impl Error for RealearnScriptError {}

/// Creates a Lua environment in which we can't execute potentially malicious code
/// (by only including safe functions according to http://lua-users.org/wiki/SandBoxes).
fn build_safe_lua_env(lua: &Lua) -> Result<Table, Box<dyn Error>> {
    let original_env = lua.globals();
    let safe_env = lua.create_table()?;
    for var in SAFE_LUA_VARS {
        copy_var_to_table(lua, &safe_env, &original_env, var)?;
    }
    Ok(safe_env)
}

fn copy_var_to_table(
    lua: &Lua,
    dest_table: &Table,
    src_table: &Table,
    var: &str,
) -> Result<(), Box<dyn Error>> {
    if let Some(dot_index) = var.find('.') {
        // Nested variable
        let parent_var = &var[0..dot_index];
        let nested_dest_table = if let Ok(t) = dest_table.get::<_, Table>(parent_var) {
            t
        } else {
            let new_table = lua.create_table()?;
            dest_table.set(parent_var, new_table.clone())?;
            new_table
        };
        let nested_src_table: Table = src_table.get(parent_var)?;
        let child_var = &var[dot_index + 1..];
        copy_var_to_table(lua, &nested_dest_table, &nested_src_table, child_var)?;
        Ok(())
    } else {
        // Leaf variable
        let original_value: Value = src_table.get(var)?;
        dest_table.set(var, original_value)?;
        Ok(())
    }
}

/// Safe Lua vars according to http://lua-users.org/wiki/SandBoxes.
///
/// Even a bit more restrictive because we don't include `io` and `coroutine`.
const SAFE_LUA_VARS: &[&str] = &[
    "assert",
    "error",
    "ipairs",
    "next",
    "pairs",
    "pcall",
    "print",
    "select",
    "tonumber",
    "tostring",
    "type",
    "unpack",
    "_VERSION",
    "xpcall",
    "string.byte",
    "string.char",
    "string.find",
    "string.format",
    "string.gmatch",
    "string.gsub",
    "string.len",
    "string.lower",
    "string.match",
    "string.rep",
    "string.reverse",
    "string.sub",
    "string.upper",
    "table.insert",
    "table.maxn",
    "table.remove",
    "table.sort",
    "math.abs",
    "math.acos",
    "math.asin",
    "math.atan",
    "math.atan2",
    "math.ceil",
    "math.cos",
    "math.cosh",
    "math.deg",
    "math.exp",
    "math.floor",
    "math.fmod",
    "math.frexp",
    "math.huge",
    "math.ldexp",
    "math.log",
    "math.log10",
    "math.max",
    "math.min",
    "math.modf",
    "math.pi",
    "math.pow",
    "math.rad",
    "math.random",
    "math.sin",
    "math.sinh",
    "math.sqrt",
    "math.tan",
    "math.tanh",
    "os.clock",
    "os.difftime",
    "os.time",
];

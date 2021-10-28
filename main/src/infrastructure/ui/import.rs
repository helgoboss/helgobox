use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use derive_more::Display;
use mlua::{ChunkMode, HookTriggers};
use serde::{Deserialize, Serialize};

use crate::infrastructure::api::convert::from_data::DataToApiConversionContext;
use crate::infrastructure::api::convert::to_data::ApiToDataConversionContext;
use crate::infrastructure::api::convert::{from_data, to_data};
use crate::infrastructure::api::schema;
use crate::infrastructure::data::{
    MappingModelData, ModeModelData, QualifiedCompartmentModelData, SessionData, SourceModelData,
    TargetModelData,
};
use crate::infrastructure::ui::lua_serializer;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum DataObject {
    Session(Box<SessionData>),
    Compartment(Box<QualifiedCompartmentModelData>),
    Mappings(Vec<MappingModelData>),
    Mapping(Box<MappingModelData>),
    Source(Box<SourceModelData>),
    Mode(Box<ModeModelData>),
    Target(Box<TargetModelData>),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiObject {
    Compartment(Box<schema::Compartment>),
    Mappings(Vec<schema::Mapping>),
    Mapping(Box<schema::Mapping>),
}

impl DataObject {
    pub fn try_from_api_object(
        api_object: ApiObject,
        conversion_context: &impl ApiToDataConversionContext,
    ) -> Result<Self, Box<dyn Error>> {
        let data_object = match api_object {
            ApiObject::Compartment(c) => {
                let data_compartment = to_data::convert_compartment(*c)?;
                DataObject::Compartment(Box::new(data_compartment))
            }
            ApiObject::Mappings(mappings) => {
                let data_mappings = Self::try_from_api_mappings(mappings, conversion_context);
                DataObject::Mappings(data_mappings?)
            }
            ApiObject::Mapping(m) => {
                let data_mapping = to_data::convert_mapping(*m, conversion_context)?;
                DataObject::Mapping(Box::new(data_mapping))
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
}

impl ApiObject {
    pub fn try_from_data_object(
        data_object: DataObject,
        conversion_context: &impl DataToApiConversionContext,
    ) -> Result<Self, Box<dyn Error>> {
        let api_object = match data_object {
            DataObject::Compartment(c) => {
                let api_compartment = from_data::convert_compartment(*c, conversion_context)?;
                ApiObject::Compartment(Box::new(api_compartment))
            }
            DataObject::Session(_) => todo!("session API not yet implemented"),
            DataObject::Mappings(mappings) => {
                let api_mappings: Result<Vec<_>, _> = mappings
                    .into_iter()
                    .map(|m| from_data::convert_mapping(m, conversion_context))
                    .collect();
                ApiObject::Mappings(api_mappings?)
            }
            DataObject::Mapping(m) => {
                let api_mapping = from_data::convert_mapping(*m, conversion_context)?;
                ApiObject::Mapping(Box::new(api_mapping))
            }
            _ => Err("conversion from source/mode/target data object not supported at the moment")?,
        };
        Ok(api_object)
    }

    pub fn into_mappings(self) -> Option<Vec<schema::Mapping>> {
        match self {
            ApiObject::Mappings(mappings) => Some(mappings),
            ApiObject::Mapping(m) => Some(vec![*m]),
            _ => None,
        }
    }
}

/// Attempts to deserialize a data object supporting both JSON and Lua.
pub fn deserialize_data_object(
    text: &str,
    conversion_context: &impl ApiToDataConversionContext,
) -> Result<DataObject, Box<dyn Error>> {
    let json_err = match deserialize_data_object_from_json(text) {
        Ok(o) => {
            return Ok(o);
        }
        Err(e) => e,
    };
    let lua_err = match deserialize_data_object_from_lua(text, conversion_context) {
        Ok(o) => {
            return Ok(o);
        }
        Err(e) => e,
    };
    let msg = format!(
        "Clipboard content doesn't look like proper ReaLearn import data:\n\n\
        Invalid JSON: \n\
        {}\n\n\
        Invalid Lua: \n\
        {}",
        json_err, lua_err
    );
    Err(msg.into())
}

pub fn deserialize_data_object_from_json(text: &str) -> Result<DataObject, Box<dyn Error>> {
    Ok(serde_json::from_str(&text)?)
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

pub fn serialize_data_object_to_lua(
    data_object: DataObject,
    conversion_context: &impl DataToApiConversionContext,
) -> Result<String, Box<dyn Error>> {
    let api_object = ApiObject::try_from_data_object(data_object, conversion_context)?;
    Ok(lua_serializer::to_string(&api_object)?)
}

pub fn deserialize_api_object_from_lua(text: &str) -> Result<ApiObject, Box<dyn Error>> {
    use mlua::{Lua, LuaSerdeExt};
    let lua = Lua::new();
    let instant = Instant::now();
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
    let env = lua.create_table()?;
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
    Ok(lua.from_value(value)?)
}

#[derive(Debug, Display)]
enum RealearnScriptError {
    #[display(fmt = "ReaLearn script took too long to execute")]
    Timeout,
}

impl Error for RealearnScriptError {}

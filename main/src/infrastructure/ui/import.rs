use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, Instant};

use derive_more::Display;
use mlua::{ChunkMode, HookTriggers};
use serde::{Deserialize, Serialize};

use crate::infrastructure::api;
use crate::infrastructure::data::{QualifiedCompartmentModelData, SessionData};

pub enum ImportData {
    Session(SessionData),
    Compartment(QualifiedCompartmentModelData),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiData {
    Compartment(api::schema::Compartment),
}

pub fn read_import(text: &str) -> Result<ImportData, Box<dyn Error>> {
    let json_import_err = match read_import_from_json(text) {
        Ok(import_data) => {
            return Ok(import_data);
        }
        Err(e) => e,
    };
    let lua_import_err = match read_import_from_lua(text) {
        Ok(import_data) => {
            return Ok(import_data);
        }
        Err(e) => e,
    };
    let msg = format!(
        "Clipboard content doesn't look like proper ReaLearn import data:\n\n\
        Invalid JSON: \n\
        {}\n\n\
        Invalid Lua script: \n\
        {}",
        json_import_err, lua_import_err
    );
    Err(msg.into())
}

fn read_import_from_json(text: &str) -> Result<ImportData, Box<dyn Error>> {
    let session_data: SessionData = serde_json::from_str(text)?;
    Ok(ImportData::Session(session_data))
}

fn read_import_from_lua(text: &str) -> Result<ImportData, Box<dyn Error>> {
    use crate::infrastructure::api::convert;
    use mlua::{Lua, LuaSerdeExt};
    let lua = Lua::new();
    let instant = Instant::now();
    lua.set_hook(
        HookTriggers::every_nth_instruction(10),
        move |_lua, debug| {
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
    let api_data: ApiData = lua.from_value(value)?;
    let import_data = match api_data {
        ApiData::Compartment(c) => {
            let compartment_data = convert::to_data::convert_compartment(c)?;
            ImportData::Compartment(compartment_data)
        }
    };
    Ok(import_data)
}

#[derive(Debug, Display)]
enum RealearnScriptError {
    #[display(fmt = "ReaLearn script took too long to execute")]
    Timeout,
}

impl Error for RealearnScriptError {}

use crate::domain::GroupId;
use crate::infrastructure::api::convert::from_data;
use crate::infrastructure::api::schema;
use crate::infrastructure::data::{
    MappingModelData, ModeModelData, SourceModelData, TargetModelData,
};
use crate::infrastructure::ui::lua_serializer;
use arboard::Clipboard;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardObject {
    Mappings(Vec<MappingModelData>),
    Mapping(Box<MappingModelData>),
    Source(Box<SourceModelData>),
    Mode(Box<ModeModelData>),
    Target(Box<TargetModelData>),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum LuaObject {
    Mappings(Vec<schema::Mapping>),
    Mapping(Box<schema::Mapping>),
    Source(Box<schema::Source>),
    Mode(Box<schema::Glue>),
    Target(Box<schema::Target>),
}

pub fn copy_object_to_clipboard(
    object: ClipboardObject,
    as_lua: bool,
    group_key_by_id: impl Fn(GroupId) -> Option<String> + Copy,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = if as_lua {
        use ClipboardObject::*;
        let lua_object = match object {
            Source(s) => LuaObject::Source(Box::new(from_data::convert_source(*s)?)),
            Mappings(mappings) => {
                let lua_mappings: Result<Vec<_>, _> = mappings
                    .into_iter()
                    .map(|m| from_data::convert_mapping(m, group_key_by_id))
                    .collect();
                LuaObject::Mappings(lua_mappings?)
            }
            Mapping(m) => {
                LuaObject::Mapping(Box::new(from_data::convert_mapping(*m, group_key_by_id)?))
            }
            Mode(m) => LuaObject::Mode(Box::new(from_data::convert_glue(*m)?)),
            Target(t) => {
                LuaObject::Target(Box::new(from_data::convert_target(*t, group_key_by_id)?))
            }
        };
        lua_serializer::to_string(&lua_object)?
    } else {
        serde_json::to_string_pretty(&object).map_err(|_| "couldn't serialize object")?
    };
    copy_text_to_clipboard(text);
    Ok(())
}

pub fn get_object_from_clipboard() -> Option<ClipboardObject> {
    let json = get_text_from_clipboard()?;
    serde_json::from_str(&json).ok()?
}

pub fn copy_text_to_clipboard(text: String) {
    let mut clipboard = Clipboard::new().expect("couldn't create clipboard");
    clipboard
        .set_text(text)
        .expect("couldn't set clipboard contents");
}

pub fn get_text_from_clipboard() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    clipboard.get_text().ok()
}

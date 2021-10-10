use crate::infrastructure::data::{
    MappingModelData, ModeModelData, SourceModelData, TargetModelData,
};
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

pub fn copy_object_to_clipboard(object: ClipboardObject) -> Result<(), &'static str> {
    let json = serde_json::to_string_pretty(&object).map_err(|_| "couldn't serialize object")?;
    copy_text_to_clipboard(json);
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

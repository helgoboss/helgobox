use crate::application::{GroupId, SharedSession};
use crate::domain::{MappingCompartment, MappingId};
use crate::infrastructure::data::{
    MappingModelData, ModeModelData, SourceModelData, TargetModelData,
};
use clipboard::{ClipboardContext, ClipboardProvider};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardObject {
    Mapping(MappingModelData),
    Source(SourceModelData),
    Mode(ModeModelData),
    Target(TargetModelData),
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
    let mut clipboard: ClipboardContext =
        ClipboardProvider::new().expect("couldn't create clipboard");
    clipboard
        .set_contents(text)
        .expect("couldn't set clipboard contents");
}

pub fn get_text_from_clipboard() -> Option<String> {
    let mut clipboard: ClipboardContext = ClipboardProvider::new().ok()?;
    clipboard.get_contents().ok()
}

use crate::persistence::{CommonPresetMetaData, ControllerPresetMetaData, MainPresetMetaData};
use serde::Serialize;

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize)]
pub struct MainPreset {
    pub id: String,
    pub common: CommonPresetMetaData,
    pub specific: MainPresetMetaData,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize)]
pub struct ControllerPreset {
    pub id: String,
    pub common: CommonPresetMetaData,
    pub specific: ControllerPresetMetaData,
}

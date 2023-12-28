use crate::infrastructure::data::UnitData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InstanceOrUnitData {
    InstanceData(InstanceData),
    UnitData(UnitData),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceData {
    pub units: Vec<UnitData>,
}

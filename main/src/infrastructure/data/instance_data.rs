use crate::infrastructure::data::UnitData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InstanceOrUnitData {
    InstanceData(InstanceData),
    /// For backward compatibility.
    UnitData(UnitData),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceData {
    pub main_unit: UnitData,
    pub additional_units: Vec<UnitData>,
}

impl Default for InstanceOrUnitData {
    fn default() -> Self {
        Self::InstanceData(InstanceData::default())
    }
}

impl InstanceOrUnitData {
    pub fn into_instance_data(self) -> InstanceData {
        match self {
            InstanceOrUnitData::InstanceData(d) => d,
            InstanceOrUnitData::UnitData(d) => InstanceData {
                main_unit: d,
                additional_units: vec![],
            },
        }
    }
}

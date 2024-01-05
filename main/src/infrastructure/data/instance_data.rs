use crate::infrastructure::data::{ClipMatrixRefData, UnitData};
use base::default_util::{deserialize_null_default, is_default};
use realearn_api::persistence::InstanceSettings;
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
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub settings: InstanceSettings,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub pot_state: pot::PersistentState,
    #[cfg(feature = "playtime")]
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_matrix: Option<ClipMatrixRefData>,
}

impl Default for InstanceOrUnitData {
    fn default() -> Self {
        Self::InstanceData(InstanceData::default())
    }
}

impl InstanceOrUnitData {
    #[allow(deprecated)]
    pub fn into_instance_data(self) -> InstanceData {
        match self {
            InstanceOrUnitData::InstanceData(d) => d,
            InstanceOrUnitData::UnitData(mut d) => InstanceData {
                // Migrate pot state from unit data
                pot_state: d.pot_state.take().unwrap_or_default(),
                // Migrate clip matrix state from unit data
                clip_matrix: d.clip_matrix.take(),
                main_unit: d,
                additional_units: vec![],
                settings: Default::default(),
            },
        }
    }
}

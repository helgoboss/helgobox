use crate::application::CompartmentPresetModel;
use crate::domain::Compartment;
use crate::infrastructure::data::CompartmentModelData;
use base::default_util::{deserialize_null_default, is_default};

use crate::infrastructure::plugin::BackboneShell;
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompartmentPresetData {
    // Since ReaLearn 1.12.0-pre18
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    version: Option<Version>,
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(flatten)]
    data: CompartmentModelData,
}

impl CompartmentPresetData {
    pub fn from_model(preset: &CompartmentPresetModel) -> CompartmentPresetData {
        CompartmentPresetData {
            version: Some(BackboneShell::version().clone()),
            id: Some(preset.id().to_string()),
            data: CompartmentModelData::from_model(preset.model()),
            name: preset.name().to_string(),
        }
    }

    pub fn to_model(
        &self,
        id: String,
        compartment: Compartment,
    ) -> anyhow::Result<CompartmentPresetModel> {
        let preset = CompartmentPresetModel::new(
            id,
            self.name.clone(),
            compartment,
            self.data
                .to_model(self.version.as_ref(), compartment, None)?,
        );
        Ok(preset)
    }

    pub fn clear_id(&mut self) {
        self.id = None;
    }

    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}

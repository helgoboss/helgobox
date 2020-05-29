use crate::domain::ModeModel;
use serde::{Deserialize, Serialize};
use validator::ValidationErrors;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeModelData {}

impl Default for ModeModelData {
    fn default() -> Self {
        Self {}
    }
}

impl ModeModelData {
    pub fn from_model(model: &ModeModel) -> Self {
        // TODO
        Self {}
    }

    pub fn apply_to_model(&self, model: &mut ModeModel) -> Result<(), ValidationErrors> {
        // TODO
        Ok(())
    }
}

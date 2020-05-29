use crate::domain::TargetModel;
use serde::{Deserialize, Serialize};
use validator::ValidationErrors;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetModelData {}

impl Default for TargetModelData {
    fn default() -> Self {
        Self {}
    }
}

impl TargetModelData {
    pub fn from_model(model: &TargetModel) -> Self {
        // TODO
        Self {}
    }

    pub fn apply_to_model(&self, model: &mut TargetModel) -> Result<(), ValidationErrors> {
        // TODO
        Ok(())
    }
}

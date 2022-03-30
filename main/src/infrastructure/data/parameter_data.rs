use crate::base::default_util::is_default;
use crate::domain::ParameterSetting;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterData {
    #[serde(flatten)]
    pub settings: ParameterSetting,
    #[serde(default, skip_serializing_if = "is_default")]
    pub value: f32,
}

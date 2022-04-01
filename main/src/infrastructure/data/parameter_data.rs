use crate::base::default_util::is_default;
use crate::domain::ParamSetting;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterData {
    #[serde(flatten)]
    pub setting: ParamSetting,
    #[serde(default, skip_serializing_if = "is_default")]
    pub value: f32,
}

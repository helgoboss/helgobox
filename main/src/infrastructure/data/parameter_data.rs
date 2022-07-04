use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::ParamSetting;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterData {
    #[serde(flatten)]
    pub setting: ParamSetting,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub value: f32,
}

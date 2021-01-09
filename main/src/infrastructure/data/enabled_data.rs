use crate::core::default_util::{bool_true, is_bool_true};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledData {
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub control_is_enabled: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub feedback_is_enabled: bool,
}

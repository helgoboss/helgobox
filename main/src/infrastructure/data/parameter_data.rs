use crate::base::default_util::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub value: f32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
}

impl ParameterData {
    pub fn key_matches(&self, key: &str) -> bool {
        if let Some(k) = self.key.as_ref() {
            k == key
        } else {
            false
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParameterData {
    pub value: f32,
    pub name: Option<String>,
}

impl Default for ParameterData {
    fn default() -> Self {
        Self {
            value: 0.0,
            name: None,
        }
    }
}

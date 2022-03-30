use crate::base::default_util::is_default;
use helgoboss_learn::UnitValue;
use serde::{Deserialize, Serialize};

pub const PLUGIN_PARAMETER_COUNT: u32 = 200;
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;
pub type ParameterArray = [f32; PLUGIN_PARAMETER_COUNT as usize];
pub type ParameterSlice = [f32];
pub const ZEROED_PLUGIN_PARAMETERS: ParameterArray = [0.0f32; PLUGIN_PARAMETER_COUNT as usize];

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterSetting {
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub max_value: Option<f64>,
}

impl ParameterSetting {
    pub fn is_default(&self) -> bool {
        self.name.is_empty()
    }

    pub fn key_matches(&self, key: &str) -> bool {
        if let Some(k) = self.key.as_ref() {
            k == key
        } else {
            false
        }
    }

    pub fn convert_to_effective_value(&self, raw_value: f32) -> f64 {
        let raw_value = UnitValue::new_clamped(raw_value as _);
        if let Some(max_value) = self.max_value {
            raw_value.get() * max_value
        } else {
            raw_value.get()
        }
    }

    pub fn convert_to_raw_value(&self, effective_value: f64) -> f32 {
        let raw_value = if let Some(max_value) = self.max_value {
            effective_value / max_value
        } else {
            effective_value
        };
        UnitValue::new_clamped(raw_value).get() as f32
    }

    pub fn parse_to_raw_value(&self, text: &str) -> Result<f32, &'static str> {
        let effective_value: f64 = text.parse().map_err(|_| "couldn't parse as number")?;
        Ok(self.convert_to_raw_value(effective_value))
    }
}

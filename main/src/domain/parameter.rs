use crate::base::default_util::is_default;
use helgoboss_learn::UnitValue;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::ops::Range;

pub const PLUGIN_PARAMETER_COUNT: u32 = 200;
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;
pub type ParameterValueArray = [f32; PLUGIN_PARAMETER_COUNT as usize];
pub type ParameterSettingArray = [ParameterSetting; PLUGIN_PARAMETER_COUNT as usize];
pub const ZEROED_PLUGIN_PARAMETERS: ParameterValueArray = [0.0f32; PLUGIN_PARAMETER_COUNT as usize];

#[derive(Copy, Clone, Debug)]
pub struct Parameters<'a> {
    values: &'a [f32],
    settings: &'a [ParameterSetting],
}

impl<'a> Parameters<'a> {
    pub fn new(values: &'a [f32], settings: &'a [ParameterSetting]) -> Self {
        assert_eq!(values.len(), settings.len());
        Self { values, settings }
    }

    pub fn values(&self) -> &[f32] {
        self.values
    }

    pub fn raw_value_at(&self, index: u32) -> Option<f32> {
        self.values.get(index as usize).copied()
    }

    pub fn setting_at(&self, index: u32) -> Option<&ParameterSetting> {
        self.settings.get(index as usize)
    }

    pub fn slice(&self, range: Range<u32>) -> Self {
        let range = range.start as usize..range.end as usize;
        Self {
            values: &self.values[range.clone()],
            settings: &self.settings[range],
        }
    }

    pub fn get_effective_value(&self, index: u32) -> Option<f64> {
        let raw_value = *self.values.get(index as usize)?;
        let effective_value = self.settings[index as usize].convert_to_effective_value(raw_value);
        Some(effective_value)
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterSetting {
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub value_count: Option<NonZeroU32>,
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
        if let Some(value_count) = self.value_count {
            (raw_value.get() * (value_count.get() - 1) as f64).round()
        } else {
            raw_value.get()
        }
    }

    pub fn convert_to_raw_value(&self, effective_value: f64) -> f32 {
        let raw_value = if let Some(value_count) = self.value_count {
            (effective_value / (value_count.get() - 1) as f64).round()
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

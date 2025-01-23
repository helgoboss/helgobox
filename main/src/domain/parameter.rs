use crate::domain::CompartmentKind;
use base::default_util::{deserialize_null_default, is_default};
use derive_more::Display;
use enum_map::EnumMap;
use helgoboss_learn::UnitValue;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU32;
use std::ops::{Add, RangeInclusive};

/// Total number of parameters of the plug-in.
pub const PLUGIN_PARAMETER_COUNT: u32 = 200;

/// Number of parameters per compartment.
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;

/// Returns an iterator over the range of compartment parameter indices.
pub fn compartment_param_index_iter() -> impl Iterator<Item = CompartmentParamIndex> {
    convert_compartment_param_index_range_to_iter(&compartment_param_index_range())
}

/// Returns the range of compartment parameter indices.
pub fn compartment_param_index_range() -> RangeInclusive<CompartmentParamIndex> {
    CompartmentParamIndex(0)..=CompartmentParamIndex(COMPARTMENT_PARAMETER_COUNT - 1)
}

/// We need this because the `step_trait` is not stabilized yet.
pub fn convert_plugin_param_index_range_to_iter(
    range: &RangeInclusive<PluginParamIndex>,
) -> impl Iterator<Item = PluginParamIndex> {
    (range.start().get()..=range.end().get()).map(PluginParamIndex)
}

/// We need this because the `step_trait` is not stabilized yet.
pub fn convert_compartment_param_index_range_to_iter(
    range: &RangeInclusive<CompartmentParamIndex>,
) -> impl Iterator<Item = CompartmentParamIndex> {
    (range.start().get()..=range.end().get()).map(CompartmentParamIndex)
}

/// Raw parameter value.
pub type RawParamValue = f32;

/// Effective parameter value.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum EffectiveParamValue {
    Continuous(f64),
    Discrete(u32),
}

impl Display for EffectiveParamValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EffectiveParamValue::Continuous(v) => write!(f, "{:.3}", *v),
            EffectiveParamValue::Discrete(v) => v.fmt(f),
        }
    }
}

impl From<EffectiveParamValue> for f64 {
    fn from(v: EffectiveParamValue) -> Self {
        match v {
            EffectiveParamValue::Continuous(v) => v,
            EffectiveParamValue::Discrete(v) => v as f64,
        }
    }
}

/// Parameter setting.
#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSetting {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub key: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub value_count: Option<NonZeroU32>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub value_labels: Vec<String>,
}

impl ParamSetting {
    fn is_default(&self) -> bool {
        self.key.is_none() && self.name.is_empty() && self.value_count.is_none()
    }

    pub fn discrete_values(&self) -> Option<impl Iterator<Item = Cow<str>> + '_> {
        let value_count = self.value_count?;
        let iter = (0..value_count.get()).map(|v| {
            self.find_label_for_value(v)
                .map(|v| v.into())
                .unwrap_or_else(|| v.to_string().into())
        });
        Some(iter)
    }

    /// Checks if the given key matches the key of this parameter (if a key is defined).
    pub fn key_matches(&self, key: &str) -> bool {
        if let Some(k) = self.key.as_ref() {
            k == key
        } else {
            false
        }
    }

    pub fn with_raw_value(&self, value: RawParamValue) -> impl Display + '_ {
        SettingAndValue {
            setting: self,
            value,
        }
    }

    pub fn convert_to_value(&self, raw_value: RawParamValue) -> EffectiveParamValue {
        let raw_value = UnitValue::new_clamped(raw_value as _);
        if let Some(value_count) = self.value_count {
            let scaled = raw_value.get() * (value_count.get() - 1) as f64;
            EffectiveParamValue::Discrete(scaled.round() as u32)
        } else {
            EffectiveParamValue::Continuous(raw_value.get())
        }
    }

    fn convert_to_raw_value(&self, effective_value: f64) -> RawParamValue {
        let raw_value = if let Some(value_count) = self.value_count {
            effective_value / (value_count.get() - 1) as f64
        } else {
            effective_value
        };
        UnitValue::new_clamped(raw_value).get() as RawParamValue
    }

    pub fn find_label_for_value(&self, value: u32) -> Option<&str> {
        self.value_labels.get(value as usize).map(|s| s.as_str())
    }

    /// Attempts to parse the given text to a raw parameter value.
    pub fn parse_to_raw_value(&self, text: &str) -> Result<RawParamValue, &'static str> {
        let effective_value: f64 = text.parse().map_err(|_| "couldn't parse as number")?;
        Ok(self.convert_to_raw_value(effective_value))
    }
}

struct SettingAndValue<'a> {
    setting: &'a ParamSetting,
    value: RawParamValue,
}

impl Display for SettingAndValue<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let effective_value = self.setting.convert_to_value(self.value);
        if let EffectiveParamValue::Discrete(v) = effective_value {
            if let Some(label) = self.setting.find_label_for_value(v) {
                return label.fmt(f);
            }
        }
        effective_value.fmt(f)
    }
}

/// Setting and value combined.
#[derive(Clone, Debug, Default)]
pub struct Param {
    setting: ParamSetting,
    value: RawParamValue,
}

impl Param {
    /// Creates a new parameter.
    pub fn new(setting: ParamSetting, value: RawParamValue) -> Self {
        Self { setting, value }
    }

    /// Returns the effective parameter value (taking the parameter setting into account).
    pub fn effective_value(&self) -> EffectiveParamValue {
        self.setting.convert_to_value(self.value)
    }

    /// Returns the setting of this parameter.
    pub fn setting(&self) -> &ParamSetting {
        &self.setting
    }

    /// Sets the setting of this parameter.
    pub fn set_setting(&mut self, setting: ParamSetting) {
        self.setting = setting;
    }

    /// Returns the raw value of this parameter.
    pub fn raw_value(&self) -> RawParamValue {
        self.value
    }

    /// Sets the raw value of this parameter.
    pub fn set_raw_value(&mut self, value: RawParamValue) {
        self.value = value;
    }
}

impl Display for Param {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let setting_and_value = SettingAndValue {
            setting: &self.setting,
            value: self.value,
        };
        setting_and_value.fmt(f)
    }
}

/// All parameters for a particular compartment.
#[derive(Clone, Debug)]
pub struct CompartmentParams(Vec<Param>);

impl Default for CompartmentParams {
    fn default() -> Self {
        let vector = vec![Default::default(); COMPARTMENT_PARAMETER_COUNT as usize];
        Self(vector)
    }
}

impl CompartmentParams {
    /// Returns the parameter at the given index.
    pub fn at(&self, index: CompartmentParamIndex) -> &Param {
        self.0.get(index.get() as usize).unwrap()
    }

    /// Returns the parameter at the given index, mutable.
    pub fn at_mut(&mut self, index: CompartmentParamIndex) -> &mut Param {
        self.0.get_mut(index.get() as usize).unwrap()
    }

    /// Returns the name of the parameter including its position.
    pub fn get_parameter_name(&self, index: CompartmentParamIndex) -> Cow<String> {
        let setting = &self.at(index).setting;
        if setting.name.is_empty() {
            Cow::Owned(format!("Param {}", index.get() + 1))
        } else {
            Cow::Borrowed(&setting.name)
        }
    }

    /// Returns a map of all parameter settings that don't correspond to the defaults.
    pub fn non_default_settings(&self) -> Vec<(CompartmentParamIndex, ParamSetting)> {
        self.0
            .iter()
            .map(|p| &p.setting)
            .enumerate()
            .filter(|(_, s)| !s.is_default())
            .map(|(i, s)| {
                (
                    CompartmentParamIndex::try_from(i as u32).unwrap(),
                    s.clone(),
                )
            })
            .collect()
    }

    /// Applies the given settings.
    pub fn apply_given_settings(&mut self, settings: Vec<(CompartmentParamIndex, ParamSetting)>) {
        for (i, setting) in settings {
            self.at_mut(i).setting = setting;
        }
    }

    /// Resets all settings and values to the defaults.
    pub fn reset_all(&mut self) {
        *self = Default::default();
    }

    pub fn find_setting_by_key(&self, key: &str) -> Option<(CompartmentParamIndex, &ParamSetting)> {
        self.0
            .iter()
            .enumerate()
            .find(|(_, s)| s.setting.key.as_ref().map(|k| k == key).unwrap_or(false))
            .map(|(i, s)| {
                (
                    CompartmentParamIndex::try_from(i as u32).unwrap(),
                    &s.setting,
                )
            })
    }
}

/// All parameters for the complete plug-in.
#[derive(Clone, Debug, Default)]
pub struct PluginParams {
    compartment_params: EnumMap<CompartmentKind, CompartmentParams>,
}

impl PluginParams {
    /// Returns the parameter at the given index.
    pub fn at(&self, index: PluginParamIndex) -> &Param {
        let (compartment, index) = CompartmentKind::translate_plugin_param_index(index);
        self.compartment_params(compartment).at(index)
    }

    /// Returns the parameter at the given index, mutable.
    pub fn at_mut(&mut self, index: PluginParamIndex) -> &mut Param {
        let (compartment, index) = CompartmentKind::translate_plugin_param_index(index);
        self.compartment_params_mut(compartment).at_mut(index)
    }

    /// Returns the parameter for the given compartment.
    pub fn compartment_params(&self, compartment: CompartmentKind) -> &CompartmentParams {
        &self.compartment_params[compartment]
    }

    /// Returns the parameter for the given compartment, mutable.
    pub fn compartment_params_mut(
        &mut self,
        compartment: CompartmentKind,
    ) -> &mut CompartmentParams {
        &mut self.compartment_params[compartment]
    }

    /// Returns the parameter name prefixed with compartment label.
    pub fn build_qualified_parameter_name(&self, index: PluginParamIndex) -> String {
        let (compartment, index) = CompartmentKind::translate_plugin_param_index(index);
        let compartment_param_name = self
            .compartment_params(compartment)
            .get_parameter_name(index);
        let compartment_label = match compartment {
            CompartmentKind::Controller => "Ctrl",
            CompartmentKind::Main => "Main",
        };
        format!(
            "{} p{}: {}",
            compartment_label,
            index.get() + 1,
            compartment_param_name
        )
    }
}

/// Refers to a parameter within the complete set of plug-in parameters.
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Hash, Debug, Default, Display)]
pub struct PluginParamIndex(u32);

impl PluginParamIndex {
    /// Returns the raw index.
    pub fn get(&self) -> u32 {
        self.0
    }
}

impl Add<u32> for PluginParamIndex {
    type Output = Option<Self>;

    fn add(self, rhs: u32) -> Self::Output {
        Self::try_from(self.0 + rhs).ok()
    }
}

impl TryFrom<u32> for PluginParamIndex {
    type Error = &'static str;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value >= PLUGIN_PARAMETER_COUNT {
            return Err("invalid plug-in parameter index");
        }
        Ok(Self(value))
    }
}

/// Refers to a parameter within one compartment.
#[derive(
    Copy, Clone, Eq, PartialEq, PartialOrd, Hash, Debug, Default, Serialize, Deserialize, Display,
)]
#[serde(try_from = "u32")]
pub struct CompartmentParamIndex(u32);

impl CompartmentParamIndex {
    /// Returns the raw index.
    pub fn get(&self) -> u32 {
        self.0
    }
}

impl Add<u32> for CompartmentParamIndex {
    type Output = Option<Self>;

    fn add(self, rhs: u32) -> Self::Output {
        Self::try_from(self.0 + rhs).ok()
    }
}

impl TryFrom<u32> for CompartmentParamIndex {
    type Error = &'static str;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value >= COMPARTMENT_PARAMETER_COUNT {
            return Err("invalid compartment parameter index");
        }
        Ok(Self(value))
    }
}

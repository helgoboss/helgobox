use crate::base::default_util::is_default;
use crate::domain::MappingCompartment;
use derive_more::Display;
use helgoboss_learn::UnitValue;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU32;
use std::ops::Range;

/// Total number of parameters of the plug-in.
pub const PLUGIN_PARAMETER_COUNT: u32 = 200;

/// Number of parameters per compartment.
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;

/// Owner of all plug-in parameter settings and values.
#[derive(Debug, Default)]
pub struct OwnedPluginParams {
    settings: OwnedPluginParamSettings,
    values: OwnedPluginParamValues,
}

/// Owner of all plug-in parameter settings.
#[derive(Debug)]
pub struct OwnedPluginParamSettings {
    array: PluginParamSettingArray,
}

/// Owned compartment parameter settings.
#[derive(Debug)]
pub struct OwnedCompartmentParamSettings(Vec<ParamSetting>);

impl OwnedCompartmentParamSettings {
    /// Creates empty settings.
    pub fn new() -> Self {
        Self(vec![
            Default::default();
            COMPARTMENT_PARAMETER_COUNT as usize
        ])
    }

    /// Updates the setting at the given position.
    pub fn update_at(&mut self, index: CompartmentParamIndex, setting: ParamSetting) {
        self.0[index.0 as usize] = setting;
    }
}

/// Owner of all plug-in parameter values.
#[derive(Clone, Debug)]
pub struct OwnedPluginParamValues {
    array: PluginParamValueArray,
}

/// Borrowed parameter values of the complete plug-in.
#[derive(Copy, Clone, Debug)]
pub struct PluginParamValues<'a>(ParamValues<'a>);

/// Borrowed parameter values of a specific compartment.
#[derive(Copy, Clone, Debug)]
pub struct CompartmentParamValues<'a>(ParamValues<'a>);

/// Borrowed parameter settings of the complete plug-in.
#[derive(Copy, Clone, Debug)]
pub struct PluginParamSettings<'a>(ParamSettings<'a>);

/// Borrowed parameter settings and values of the complete plug-in.
#[derive(Copy, Clone, Debug)]
pub struct PluginParams<'a> {
    params: Params<'a>,
}

/// Borrowed parameter settings for a particular compartment.
#[derive(Copy, Clone, Debug)]
pub struct CompartmentParamSettings<'a> {
    compartment: MappingCompartment,
    settings: ParamSettings<'a>,
}

/// Borrowed parameter settings and values for a particular compartment.
#[derive(Copy, Clone, Debug)]
pub struct CompartmentParams<'a> {
    compartment: MappingCompartment,
    params: Params<'a>,
}

/// Borrowed combination of parameter setting and value.
pub struct Param<'a> {
    setting: &'a ParamSetting,
    value: RawParamValue,
}

/// Parameter setting.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSetting {
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub value_count: Option<NonZeroU32>,
}

/// Raw parameter value.
pub type RawParamValue = f32;

/// Refers to a parameter within the complete set of plug-in parameters.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Display)]
pub struct PluginParamIndex(u32);

/// Refers to a parameter within one compartment.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize, Display)]
#[serde(try_from = "u32")]
pub struct CompartmentParamIndex(u32);

/// Borrowed parameter settings and values, either of the complete plug-in or just one compartment.
#[derive(Copy, Clone, Debug)]
struct Params<'a> {
    settings: ParamSettings<'a>,
    values: ParamValues<'a>,
}

/// Borrowed parameter settings, either of the complete plug-in or just one compartment.
#[derive(Copy, Clone, Debug)]
struct ParamSettings<'a>(&'a [ParamSetting]);

impl<'a> ParamSettings<'a> {
    fn count(&self) -> u32 {
        self.0.len() as _
    }

    fn get_parameter_name(&self, index: u32) -> Option<Cow<String>> {
        let setting = self.at(index)?;
        let name = if setting.name.is_empty() {
            Cow::Owned(format!("Param {}", index + 1))
        } else {
            Cow::Borrowed(&setting.name)
        };
        Some(name)
    }

    fn at(&self, index: u32) -> Option<&ParamSetting> {
        self.0.get(index as usize)
    }
}

/// Borrowed parameter values, either of the complete plug-in or just one compartment.
#[derive(Copy, Clone, Debug)]
struct ParamValues<'a>(&'a [RawParamValue]);

impl<'a> ParamValues<'a> {
    fn count(&self) -> u32 {
        self.0.len() as _
    }

    fn at(&self, index: u32) -> Option<RawParamValue> {
        self.0.get(index as usize).copied()
    }
}

impl Default for OwnedPluginParamSettings {
    fn default() -> Self {
        Self {
            array: [Default::default(); PLUGIN_PARAMETER_COUNT as usize],
        }
    }
}

impl Default for OwnedPluginParamValues {
    fn default() -> Self {
        Self {
            array: [0.0f32; PLUGIN_PARAMETER_COUNT as usize],
        }
    }
}

impl OwnedPluginParams {
    /// Returns a borrowed version of this data.
    pub fn borrow(&self) -> PluginParams {
        PluginParams::new(&self.settings.array, &self.values.array).unwrap()
    }

    /// Returns a reference to the values.
    pub fn values(&self) -> PluginParamValues {
        PluginParamValues(ParamValues(&self.values.array))
    }

    /// Returns a mutable reference to the values.
    pub fn values_mut(&mut self) -> &mut OwnedPluginParamValues {
        &mut self.values
    }

    /// Returns a reference to the settings.
    pub fn settings(&self) -> PluginParamSettings {
        PluginParamSettings(ParamSettings(&self.settings.array))
    }

    /// Returns a mutable reference to the settings.
    pub fn settings_mut(&mut self) -> &mut OwnedPluginParamSettings {
        &mut self.settings
    }
}

impl OwnedPluginParamSettings {
    /// Merges settings and values.
    pub fn merge_with_values<'a>(&'a self, values: PluginParamValues<'a>) -> PluginParams<'a> {
        PluginParams::new(&self.array, values.0 .0).unwrap()
    }

    /// Returns the parameter name prefixed with compartment label.
    pub fn build_qualified_parameter_name(&self, index: PluginParamIndex) -> String {
        let cmpt = MappingCompartment::by_plugin_param_index(index);
        let cmpt_param_settings = self.slice(cmpt);
        let cmpt_param_index = cmpt.to_compartment_param_index(index);
        let cmpt_param_name = cmpt_param_settings.get_parameter_name(cmpt_param_index);
        // TODO-medium Do this in borrowed struct
        let cmpt_label = match cmpt {
            MappingCompartment::ControllerMappings => "Ctrl",
            MappingCompartment::MainMappings => "Main",
        };
        format!(
            "{} p{}: {}",
            cmpt_label,
            cmpt_param_index.get() + 1,
            cmpt_param_name
        )
    }

    /// Resets all settings in the given compartment to the defaults.
    pub fn reset_all_in_compartment(&mut self, compartment: MappingCompartment) {
        todo!()
    }

    /// Updates particular settings within the given compartment.
    pub fn update_certain_settings_within_compartment(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = (CompartmentParamIndex, ParamSetting)>,
    ) {
        let compartment_settings = self.compartment_settings_mut(compartment);
        for (i, s) in settings {
            compartment_settings[i.get() as usize] = s;
        }
    }

    /// Updates all settings
    pub fn update_all_settings_within_compartment(
        &mut self,
        compartment: MappingCompartment,
        settings: OwnedCompartmentParamSettings,
    ) {
        todo!()
    }

    fn compartment_settings_mut(&mut self, compartment: MappingCompartment) -> &mut [ParamSetting] {
        &mut self.array[compartment.param_range_for_indexing()]
    }

    fn as_slice(&self) -> &[ParamSetting] {
        &self.array
    }

    fn slice(&self, compartment: MappingCompartment) -> CompartmentParamSettings {
        let param_range = compartment.param_range_for_indexing();
        CompartmentParamSettings {
            compartment,
            settings: ParamSettings(&self.array[param_range]),
        }
    }
}

impl<'a> CompartmentParamSettings<'a> {
    fn get_parameter_name(&self, index: CompartmentParamIndex) -> Cow<String> {
        self.settings.get_parameter_name(index.get()).unwrap()
    }

    /// Returns a map of all parameter settings that don't correspond to the defaults.
    pub fn non_default_settings(&self) -> HashMap<CompartmentParamIndex, ParamSetting> {
        self.settings
            .0
            .iter()
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
}

impl<'a> PluginParams<'a> {
    fn new(
        settings: &'a [ParamSetting],
        values: &'a [RawParamValue],
    ) -> Result<Self, &'static str> {
        let params = Params::new(ParamValues(values), ParamSettings(settings))?;
        Ok(Self { params })
    }

    /// Returns a reference to the values.
    pub fn values(&self) -> PluginParamValues {
        PluginParamValues(self.params.values)
    }

    /// Returns a reference to the settings.
    pub fn settings(&self) -> PluginParamSettings {
        PluginParamSettings(self.params.settings)
    }

    /// Returns the parameter at the given index.
    pub fn at(&self, index: PluginParamIndex) -> Param {
        self.params.at(index.get()).unwrap()
    }

    fn raw_value_slice(&self) -> &[RawParamValue] {
        self.params.values.0
    }

    /// Returns the parameters for the given compartment.
    pub fn slice_to_compartment(&self, compartment: MappingCompartment) -> CompartmentParams {
        let param_range = compartment.param_range();
        CompartmentParams {
            compartment,
            params: self.params.slice(param_range),
        }
    }

    fn setting_and_raw_value_at(&self, index: PluginParamIndex) -> (&ParamSetting, RawParamValue) {
        self.params.setting_and_raw_value_at(index.get()).unwrap()
    }
}

impl OwnedPluginParamValues {
    /// Updates the value at the given index.
    pub fn update_at(&mut self, index: PluginParamIndex, value: RawParamValue) {
        self.array[index.get() as usize] = value;
    }

    /// Returns a borrowed version of this data.
    pub fn borrow(&self) -> PluginParamValues {
        PluginParamValues(ParamValues(&self.array))
    }
}

impl<'a> PluginParamValues<'a> {
    /// Returns the parameter value at the given index.
    pub fn at(&self, index: PluginParamIndex) -> RawParamValue {
        self.0.at(index.get()).unwrap()
    }
}

impl<'a> PluginParamSettings<'a> {
    /// Merges settings and values.
    pub fn merge_with_values<'b>(&'b self, values: PluginParamValues<'b>) -> PluginParams<'b> {
        PluginParams::new(&self.0 .0, values.0 .0).unwrap()
    }

    /// Returns the settings for the given compartment.
    pub fn slice_to_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> CompartmentParamSettings {
        let param_range = compartment.param_range();
        // CompartmentParamSettings {
        //     compartment,
        //     params: self.params.slice(param_range),
        //     settings: self.0.
        // }
        todo!()
    }
}

impl<'a> CompartmentParamValues<'a> {
    /// Returns the parameter value at the given index.
    pub fn at(&self, index: CompartmentParamIndex) -> RawParamValue {
        self.0.at(index.get()).unwrap()
    }
}

impl<'a> CompartmentParams<'a> {
    /// Returns the parameter at the given index.
    pub fn at(&self, index: CompartmentParamIndex) -> Param {
        self.params.at(index.get()).unwrap()
    }

    /// Returns a reference to the values.
    pub fn values(&self) -> CompartmentParamValues {
        CompartmentParamValues(self.params.values)
    }

    fn raw_value_slice(&self) -> &[RawParamValue] {
        self.params.values.0
    }

    fn setting_and_raw_value_at(
        &self,
        index: CompartmentParamIndex,
    ) -> (&ParamSetting, RawParamValue) {
        self.params.setting_and_raw_value_at(index.get()).unwrap()
    }
}

impl<'a> Param<'a> {
    fn new(setting: &'a ParamSetting, value: RawParamValue) -> Self {
        Self { setting, value }
    }

    /// Returns the effective value of this parameter (taking the count into account).
    pub fn effective_value(&self) -> f64 {
        self.setting.convert_to_effective_value(self.value)
    }

    /// Returns the setting of this parameter.
    pub fn setting(&self) -> &ParamSetting {
        &self.setting
    }

    /// Returns the raw value of this parameter.
    pub fn raw_value(&self) -> RawParamValue {
        self.value
    }

    /// Attempts to parse the given text to a raw parameter value.
    pub fn parse(&self, text: &str) -> Result<RawParamValue, &'static str> {
        self.setting.parse_to_raw_value(text)
    }
}

impl<'a> Display for Param<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let effective_value = self.effective_value();
        write!(f, "{:.3}", effective_value)
    }
}

impl PluginParamIndex {
    /// Returns the raw index.
    pub fn get(&self) -> u32 {
        self.0
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

impl CompartmentParamIndex {
    /// Returns the raw index.
    pub fn get(&self) -> u32 {
        self.0
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

/// Array that holds *all* plug-in parameter values.
type PluginParamValueArray = [RawParamValue; PLUGIN_PARAMETER_COUNT as usize];

/// Array that holds *all* plug-in parameter settings.
type PluginParamSettingArray = [ParamSetting; PLUGIN_PARAMETER_COUNT as usize];

// ---------------------------------------------------------

impl OwnedPluginParamSettings {
    fn get_setting(&self, compartment: MappingCompartment, index: u32) -> &ParamSetting {
        &self.compartment_parameter_settings(compartment)[index as usize]
    }

    // fn compartment_parameters(
    //     &self,
    //     compartment: MappingCompartment,
    //     all_values: &[RawParamValue],
    // ) -> Params {
    //     self.all_parameters(all_values)
    //         .slice(compartment.param_range())
    // }
    //
    // fn all_parameters(&self, all_values: &[RawParamValue]) -> Params {
    //     Params::new(all_values, &self.array)
    // }

    fn set_compartment_parameter_settings_without_notification(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: Vec<ParamSetting>,
    ) {
        self.set_compartment_parameter_settings_without_notification_from_iter(
            compartment,
            parameter_settings.into_iter(),
        )
    }

    fn set_compartment_parameter_settings_without_notification_from_iter(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = ParamSetting>,
    ) {
        let compartment_settings = self.compartment_settings_mut(compartment);
        for (i, s) in settings.enumerate() {
            compartment_settings[i] = s;
        }
    }

    fn set_compartment_parameter_settings_without_notification_from_indexed_iter(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = (u32, ParamSetting)>,
    ) {
        let compartment_settings = self.compartment_settings_mut(compartment);
        for (i, s) in settings {
            compartment_settings[i as usize] = s;
        }
    }

    fn set_parameter_settings_from_non_default(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: HashMap<u32, ParamSetting>,
    ) {
        let mut settings = empty_parameter_settings();
        for (i, s) in parameter_settings {
            settings[i as usize] = s;
        }
        self.set_compartment_parameter_settings_without_notification_from_iter(
            compartment,
            settings.into_iter(),
        );
    }

    fn find_parameter_setting_by_key(
        &self,
        compartment: MappingCompartment,
        key: &str,
    ) -> Option<(u32, &ParamSetting)> {
        self.compartment_parameter_settings(compartment)
            .iter()
            .enumerate()
            .find(|(_, s)| s.key.as_ref().map(|k| k == key).unwrap_or(false))
            .map(|(i, s)| (i as u32, s))
    }

    fn compartment_parameter_settings(&self, compartment: MappingCompartment) -> &[ParamSetting] {
        &self.array[compartment.param_range_for_indexing()]
    }
}

fn empty_parameter_settings() -> [ParamSetting; COMPARTMENT_PARAMETER_COUNT as usize] {
    [Default::default(); COMPARTMENT_PARAMETER_COUNT as usize]
}

impl<'a> Params<'a> {
    fn new(values: ParamValues<'a>, settings: ParamSettings<'a>) -> Result<Self, &'static str> {
        if values.count() != settings.count() {
            return Err("parameter value and settings size mismatch");
        }
        Ok(Self { values, settings })
    }

    pub fn at(&self, index: u32) -> Option<Param> {
        Some(Param::new(self.settings.at(index)?, self.values.at(index)?))
    }

    fn slice(&self, range: Range<u32>) -> Self {
        let range = range.start as usize..range.end as usize;
        Self {
            values: ParamValues(&self.values.0[range.clone()]),
            settings: ParamSettings(&self.settings.0[range]),
        }
    }

    fn setting_and_raw_value_at(&self, index: u32) -> Option<(&ParamSetting, RawParamValue)> {
        let raw_value = self.values.at(index)?;
        let setting = self.settings.at(index).unwrap();
        Some((setting, raw_value))
    }
}

impl ParamSetting {
    fn is_default(&self) -> bool {
        self.name.is_empty()
    }

    /// Checks if the given key matches the key of this parameter (if a key is defined).
    pub fn key_matches(&self, key: &str) -> bool {
        if let Some(k) = self.key.as_ref() {
            k == key
        } else {
            false
        }
    }

    fn convert_to_effective_value(&self, raw_value: RawParamValue) -> f64 {
        let raw_value = UnitValue::new_clamped(raw_value as _);
        if let Some(value_count) = self.value_count {
            (raw_value.get() * (value_count.get() - 1) as f64).round()
        } else {
            raw_value.get()
        }
    }

    fn convert_to_raw_value(&self, effective_value: f64) -> RawParamValue {
        let raw_value = if let Some(value_count) = self.value_count {
            (effective_value / (value_count.get() - 1) as f64).round()
        } else {
            effective_value
        };
        UnitValue::new_clamped(raw_value).get() as RawParamValue
    }

    fn parse_to_raw_value(&self, text: &str) -> Result<RawParamValue, &'static str> {
        let effective_value: f64 = text.parse().map_err(|_| "couldn't parse as number")?;
        Ok(self.convert_to_raw_value(effective_value))
    }
}

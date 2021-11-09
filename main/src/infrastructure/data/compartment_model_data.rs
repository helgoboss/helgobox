use crate::application::{CompartmentModel, GroupModel, ParameterSetting};
use crate::base::default_util::is_default;
use crate::domain::{GroupId, GroupKey, MappingCompartment};
use crate::infrastructure::data::{
    DataToModelConversionContext, GroupModelData, MappingModelData, MigrationDescriptor,
    ModelToDataConversionContext,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompartmentModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub default_group: Option<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub groups: Vec<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub mappings: Vec<MappingModelData>,
    // String key workaround because otherwise deserialization doesn't work with flattening.
    // (https://github.com/serde-rs/serde/issues/1183)
    #[serde(default, skip_serializing_if = "is_default")]
    pub parameters: HashMap<String, ParameterSetting>,
}

impl ModelToDataConversionContext for CompartmentModel {
    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        let group = self.groups.iter().find(|g| g.id() == group_id)?;
        Some(group.key().clone())
    }
}

impl CompartmentModelData {
    pub fn from_model(model: &CompartmentModel) -> Self {
        Self {
            default_group: Some(GroupModelData::from_model(&model.default_group)),
            groups: model
                .groups
                .iter()
                .map(|g| GroupModelData::from_model(g))
                .collect(),
            mappings: model
                .mappings
                .iter()
                .map(|m| MappingModelData::from_model(m, model))
                .collect(),
            parameters: model
                .parameters
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        }
    }

    pub fn to_model(
        &self,
        version: Option<&Version>,
        compartment: MappingCompartment,
    ) -> CompartmentModel {
        struct ConversionContext {
            groups: Vec<GroupModel>,
        }
        impl DataToModelConversionContext for ConversionContext {
            fn non_default_group_id_by_key(&self, key: &GroupKey) -> Option<GroupId> {
                let group = self.groups.iter().find(|g| g.key() == key)?;
                Some(group.id())
            }
        }
        let migration_descriptor = MigrationDescriptor::new(version);
        let final_default_group = self
            .default_group
            .as_ref()
            .map(|g| g.to_model(compartment, true))
            .unwrap_or_else(|| GroupModel::default_for_compartment(compartment));
        let groups = self
            .groups
            .iter()
            .map(|g| g.to_model(compartment, false))
            .collect();
        let conversion_context = ConversionContext { groups };
        CompartmentModel {
            default_group: final_default_group,
            mappings: self
                .mappings
                .iter()
                .map(|m| {
                    m.to_model_for_preset(
                        compartment,
                        &migration_descriptor,
                        version,
                        &conversion_context,
                    )
                })
                .collect(),
            parameters: self
                .parameters
                .iter()
                .filter_map(|(key, value)| Some((key.parse::<u32>().ok()?, value.clone())))
                .collect(),
            groups: conversion_context.groups,
        }
    }
}

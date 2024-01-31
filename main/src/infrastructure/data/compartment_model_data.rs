use crate::application::{CompartmentInSession, CompartmentModel, GroupModel, UnitModel};
use crate::domain::{
    CompartmentKind, CompartmentParamIndex, GroupId, GroupKey, MappingId, MappingKey, ParamSetting,
    UnitId,
};
use crate::infrastructure::data::{
    GroupModelData, MappingModelData, MigrationDescriptor, ModelToDataConversionContext,
    SimpleDataToModelConversionContext,
};
use crate::infrastructure::plugin::BackboneShell;
use base::default_util::{deserialize_null_default, is_default};
use base::hash_util::NonCryptoHashMap;
use base::validation_util::{ensure_no_duplicate, ValidationError};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompartmentModelData {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub default_group: Option<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub groups: Vec<GroupModelData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mappings: Vec<MappingModelData>,
    // String key workaround because otherwise deserialization doesn't work with flattening.
    // (https://github.com/serde-rs/serde/issues/1183)
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub parameters: NonCryptoHashMap<String, ParamSetting>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub common_lua: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub custom_data: HashMap<String, serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub notes: String,
}

impl ModelToDataConversionContext for CompartmentModel {
    fn non_default_group_key_by_id(&self, group_id: GroupId) -> Option<GroupKey> {
        let group = self.groups.iter().find(|g| g.id() == group_id)?;
        Some(group.key().clone())
    }

    fn mapping_key_by_id(&self, mapping_id: MappingId) -> Option<MappingKey> {
        let mapping = self.mappings.iter().find(|m| m.id() == mapping_id)?;
        Some(mapping.key().clone())
    }

    fn session_id_by_instance_id(&self, instance_id: UnitId) -> Option<String> {
        BackboneShell::get().find_session_id_by_instance_id(instance_id)
    }
}

impl CompartmentModelData {
    pub fn from_model(model: &CompartmentModel) -> Self {
        Self {
            default_group: Some(GroupModelData::from_model(&model.default_group, model)),
            groups: model
                .groups
                .iter()
                .map(|g| GroupModelData::from_model(g, model))
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
            common_lua: model.common_lua.clone(),
            custom_data: model.custom_data.clone(),
            notes: model.notes.clone(),
        }
    }

    /// Pass a session if you want to create mapping and group models with IDs of the current
    /// sessions (instead of creating new random ones).
    pub fn to_model(
        &self,
        version: Option<&Version>,
        compartment: CompartmentKind,
        session: Option<&UnitModel>,
    ) -> anyhow::Result<CompartmentModel> {
        ensure_no_duplicate_compartment_data(
            &self.mappings,
            &self.groups,
            self.parameters.values(),
        )?;
        let migration_descriptor = MigrationDescriptor::new(version);
        let conversion_context = SimpleDataToModelConversionContext::from_session_or_random(
            &self.groups,
            &self.mappings,
            session.map(|s| CompartmentInSession::new(s, compartment)),
        );
        let final_default_group = self
            .default_group
            .as_ref()
            .map(|g| g.to_model(compartment, true, &conversion_context))
            .unwrap_or_else(|| GroupModel::default_for_compartment(compartment));
        let groups = self
            .groups
            .iter()
            .map(|g| g.to_model(compartment, false, &conversion_context))
            .collect();
        let mappings: anyhow::Result<Vec<_>> = self
            .mappings
            .iter()
            .map(|m| {
                m.to_model_for_preset(
                    compartment,
                    &migration_descriptor,
                    version,
                    &conversion_context,
                )
                .map_err(anyhow::Error::msg)
            })
            .collect();
        let model = CompartmentModel {
            default_group: final_default_group,
            mappings: mappings?,
            parameters: self
                .parameters
                .iter()
                .filter_map(|(key, value)| {
                    let index: CompartmentParamIndex = key.parse::<u32>().ok()?.try_into().ok()?;
                    Some((index, value.clone()))
                })
                .collect(),
            groups,
            common_lua: self.common_lua.clone(),
            custom_data: self.custom_data.clone(),
            notes: self.notes.clone(),
        };
        Ok(model)
    }
}

pub fn ensure_no_duplicate_compartment_data<'a>(
    mappings: &[MappingModelData],
    groups: &[GroupModelData],
    parameters: impl Iterator<Item = &'a ParamSetting>,
) -> Result<(), ValidationError> {
    ensure_no_duplicate("mapping IDs", mappings.iter().filter_map(|m| m.id.as_ref()))?;
    ensure_no_duplicate(
        "group IDs",
        groups
            .iter()
            .filter_map(|g| if g.id.is_empty() { None } else { Some(&g.id) }),
    )?;
    ensure_no_duplicate("parameter IDs", parameters.filter_map(|p| p.key.as_ref()))?;
    Ok(())
}

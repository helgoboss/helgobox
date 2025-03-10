use crate::infrastructure::data::UnitData;
use anyhow::Context;
use base::default_util::{deserialize_null_default, is_default};
use base::hash_util::NonCryptoHashMap;
use helgobox_api::persistence::InstanceSettings;
use playtime_api::persistence::FlexibleMatrix;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceData {
    pub main_unit: UnitData,
    pub additional_units: Vec<UnitData>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub settings: InstanceSettings,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub pot_state: pot::PersistentState,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub clip_matrix: Option<FlexibleMatrix>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub custom_data: NonCryptoHashMap<String, serde_json::Value>,
}

impl InstanceData {
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        let instance_data: Self = match serde_json::from_slice(data) {
            Ok(d) => d,
            Err(instance_data_error) => {
                // Couldn't parse as instance data. Unhappy path. Let's make some inspections.
                let json: serde_json::Value = serde_json::from_slice(data)
                    .with_context(|| create_parse_error_msg(data, "parsing as JSON"))?;
                // Parsing was successful
                if json.get("mainUnit").is_some() {
                    // It's really meant as InstanceData. Fail!
                    Err(instance_data_error).with_context(|| {
                        create_parse_error_msg(data, "interpreting JSON as instance data")
                    })?;
                }
                // Ah, this could be a preset for the pre-2.16 era, meant as data for a single unit.
                let data: UnitData = serde_json::from_value(json).with_context(|| {
                    create_parse_error_msg(data, "interpreting JSON as unit data")
                })?;
                convert_old_unit_to_instance_data(data)
            }
        };
        Ok(instance_data)
    }
}

fn create_parse_error_msg(data: &[u8], label: &str) -> String {
    let data_as_str = std::str::from_utf8(data).unwrap_or("<UTF-8 decoding error>");
    format!(
        "Helgobox couldn't restore this instance while {label}. Please attach the following text if you want to report this: \n\n\
        {data_as_str}\n\n"
    )
}

#[allow(deprecated)]
fn convert_old_unit_to_instance_data(mut d: UnitData) -> InstanceData {
    InstanceData {
        // Migrate pot state from unit data
        pot_state: d.pot_state.take().unwrap_or_default(),
        // Migrate Playtime matrix state from unit data
        clip_matrix: d.clip_matrix.take(),
        main_unit: d,
        additional_units: vec![],
        settings: Default::default(),
        custom_data: Default::default(),
    }
}

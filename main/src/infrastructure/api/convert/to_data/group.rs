use crate::domain::GroupId;
use crate::infrastructure::api::convert::to_data::{convert_activation, convert_tags};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::{EnabledData, GroupModelData};
use realearn_api::schema::*;

pub fn convert_group(
    g: Group,
    is_default_group: bool,
    param_index_by_key: impl Fn(&str) -> Option<u32>,
) -> ConversionResult<GroupModelData> {
    let data = GroupModelData {
        id: if is_default_group {
            GroupId::default()
        } else {
            GroupId::random()
        },
        key: g.key,
        name: g.name.unwrap_or_default(),
        tags: convert_tags(g.tags.unwrap_or_default())?,
        enabled_data: {
            EnabledData {
                control_is_enabled: g.control_enabled.unwrap_or(defaults::GROUP_CONTROL_ENABLED),
                feedback_is_enabled: g
                    .feedback_enabled
                    .unwrap_or(defaults::GROUP_FEEDBACK_ENABLED),
            }
        },
        activation_condition_data: if let Some(cond) = g.activation_condition {
            convert_activation(cond, &param_index_by_key)?
        } else {
            Default::default()
        },
    };
    Ok(data)
}

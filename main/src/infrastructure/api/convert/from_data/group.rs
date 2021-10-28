use crate::infrastructure::api::convert::from_data::{convert_activation_condition, convert_tags};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::data::GroupModelData;

pub fn convert_group(data: GroupModelData) -> ConversionResult<schema::Group> {
    let group = schema::Group {
        key: data.key,
        name: Some(data.name),
        tags: convert_tags(&data.tags),
        control_enabled: Some(data.enabled_data.control_is_enabled),
        feedback_enabled: Some(data.enabled_data.feedback_is_enabled),
        activation_condition: convert_activation_condition(data.activation_condition_data),
    };
    Ok(group)
}

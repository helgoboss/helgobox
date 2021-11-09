use crate::infrastructure::api::convert::from_data::{
    convert_activation_condition, convert_tags, ConversionStyle,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::GroupModelData;
use realearn_api::schema;

pub fn convert_group(
    data: GroupModelData,
    style: ConversionStyle,
) -> ConversionResult<schema::Group> {
    let group = schema::Group {
        id: {
            if data.id.is_default() {
                None
            } else {
                Some(data.id.into())
            }
        },
        name: style.required_value(data.name),
        tags: convert_tags(&data.tags, style),
        control_enabled: style.required_value_with_default(
            data.enabled_data.control_is_enabled,
            defaults::GROUP_CONTROL_ENABLED,
        ),
        feedback_enabled: style.required_value_with_default(
            data.enabled_data.feedback_is_enabled,
            defaults::GROUP_FEEDBACK_ENABLED,
        ),
        activation_condition: convert_activation_condition(data.activation_condition_data),
    };
    Ok(group)
}

use crate::domain::ParamSetting;
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::api::convert::ConversionResult;
use helgobox_api::persistence;

pub fn convert_parameter(
    index: String,
    data: ParamSetting,
    style: ConversionStyle,
) -> ConversionResult<persistence::Parameter> {
    let p = persistence::Parameter {
        index: index.parse()?,
        id: data.key,
        name: Some(data.name),
        value_count: data.value_count,
        value_labels: style.required_value(data.value_labels),
    };
    Ok(p)
}

use crate::domain::ParamSetting;
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::api::convert::ConversionResult;
use realearn_api::schema;

pub fn convert_parameter(
    index: String,
    data: ParamSetting,
    style: ConversionStyle,
) -> ConversionResult<schema::Parameter> {
    let p = schema::Parameter {
        index: index.parse()?,
        id: data.key,
        name: Some(data.name),
        value_count: data.value_count,
        value_labels: style.required_value(data.value_labels),
    };
    Ok(p)
}

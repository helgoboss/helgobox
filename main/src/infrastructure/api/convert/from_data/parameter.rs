use crate::domain::ParameterSetting;
use crate::infrastructure::api::convert::ConversionResult;
use realearn_api::schema;

pub fn convert_parameter(
    index: String,
    data: ParameterSetting,
) -> ConversionResult<schema::Parameter> {
    let p = schema::Parameter {
        index: index.parse()?,
        id: data.key,
        name: Some(data.name),
        max_value: data.max_value,
    };
    Ok(p)
}

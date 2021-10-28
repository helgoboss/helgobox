use crate::application::ParameterSetting;
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;

pub fn convert_parameter(
    index: String,
    data: ParameterSetting,
) -> ConversionResult<schema::Parameter> {
    let p = schema::Parameter {
        index: index.parse()?,
        key: data.key,
        name: Some(data.name),
    };
    Ok(p)
}

use crate::domain::ParamSetting;
use crate::infrastructure::api::convert::ConversionResult;
use realearn_api::schema;

pub fn convert_parameter(index: String, data: ParamSetting) -> ConversionResult<schema::Parameter> {
    let p = schema::Parameter {
        index: index.parse()?,
        id: data.key,
        name: Some(data.name),
        value_count: data.value_count,
    };
    Ok(p)
}

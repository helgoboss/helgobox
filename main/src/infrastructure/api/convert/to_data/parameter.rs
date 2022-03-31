use crate::domain::ParameterSetting;
use crate::infrastructure::api::convert::ConversionResult;
use realearn_api::schema::*;

pub fn convert_parameter(p: Parameter) -> ConversionResult<ParameterSetting> {
    let data = ParameterSetting {
        key: p.id,
        name: p.name.unwrap_or_default(),
        value_count: p.value_count,
    };
    Ok(data)
}

use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::ParameterData;

pub fn convert_parameter(p: Parameter) -> ConversionResult<ParameterData> {
    let data = ParameterData {
        key: p.key,
        value: p.value.unwrap_or_default(),
        name: p.name.unwrap_or_default(),
    };
    Ok(data)
}

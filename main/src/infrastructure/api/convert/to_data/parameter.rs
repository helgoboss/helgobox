use crate::application::ParameterSetting;
use crate::infrastructure::api::convert::to_data::ConversionResult;
use crate::infrastructure::api::schema::*;

pub fn convert_parameter(p: Parameter) -> ConversionResult<ParameterSetting> {
    let data = ParameterSetting {
        key: p.key,
        name: p.name.unwrap_or_default(),
    };
    Ok(data)
}

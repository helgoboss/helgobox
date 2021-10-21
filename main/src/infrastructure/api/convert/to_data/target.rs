use crate::infrastructure::api::convert::to_data::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::TargetModelData;

pub fn convert_target(t: Target) -> ConversionResult<TargetModelData> {
    let data = TargetModelData::default();
    Ok(data)
}

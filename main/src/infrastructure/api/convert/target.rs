use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::TargetModelData;

pub fn convert_target(t: Target) -> ConversionResult<TargetModelData> {
    Err("Target is invalid")?
}

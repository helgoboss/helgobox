use crate::infrastructure::api::convert::from_data::{
    convert_group, convert_mapping, convert_parameter, DataToApiConversionContext,
};
use crate::infrastructure::api::convert::{convert_multiple, ConversionResult};
use crate::infrastructure::api::schema;
use crate::infrastructure::data::CompartmentModelData;

pub fn convert_compartment(
    data: CompartmentModelData,
    context: &impl DataToApiConversionContext,
) -> ConversionResult<schema::Compartment> {
    let compartment = schema::Compartment {
        default_group: {
            if let Some(group_data) = data.default_group {
                Some(convert_group(group_data)?)
            } else {
                None
            }
        },
        parameters: {
            let v: Result<Vec<_>, _> = data
                .parameters
                .into_iter()
                .map(|(key, value)| convert_parameter(key, value))
                .collect();
            Some(v?)
        },
        groups: {
            let v = convert_multiple(data.groups, |g| convert_group(g))?;
            Some(v)
        },
        mappings: {
            let v = convert_multiple(data.mappings, |m| convert_mapping(m, context))?;
            Some(v)
        },
    };
    Ok(compartment)
}

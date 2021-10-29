use crate::infrastructure::api::convert::from_data::{
    convert_group, convert_mapping, convert_parameter, ConversionStyle, DataToApiConversionContext,
};
use crate::infrastructure::api::convert::{convert_multiple, ConversionResult};
use crate::infrastructure::api::schema;
use crate::infrastructure::data::CompartmentModelData;

pub fn convert_compartment(
    data: CompartmentModelData,
    context: &impl DataToApiConversionContext,
    style: ConversionStyle,
) -> ConversionResult<schema::Compartment> {
    let compartment = schema::Compartment {
        default_group: {
            let v = if let Some(group_data) = data.default_group {
                Some(convert_group(group_data, style)?)
            } else {
                None
            };
            style.optional_value(v)
        },
        parameters: {
            let v: Result<Vec<_>, _> = data
                .parameters
                .into_iter()
                .map(|(key, value)| convert_parameter(key, value))
                .collect();
            style.required_value(v?)
        },
        groups: {
            let v = convert_multiple(data.groups, |g| convert_group(g, style))?;
            style.required_value(v)
        },
        mappings: {
            let v = convert_multiple(data.mappings, |m| convert_mapping(m, context, style))?;
            style.required_value(v)
        },
    };
    Ok(compartment)
}

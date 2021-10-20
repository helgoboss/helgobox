use crate::infrastructure::api::convert::group::convert_group;
use crate::infrastructure::api::convert::parameter::convert_parameter;
use crate::infrastructure::api::convert::{convert_mapping, convert_multiple, ConversionResult};
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{CompartmentModelData, ParameterData};
use std::collections::HashMap;

pub fn convert_compartment(c: Compartment) -> ConversionResult<CompartmentModelData> {
    let parameters: HashMap<u32, ParameterData> = {
        let res: ConversionResult<HashMap<_, _>> = c
            .parameters
            .unwrap_or_default()
            .into_iter()
            .map(|p| Ok((p.index, convert_parameter(p)?)))
            .collect();
        res?
    };
    let param_by_key = |key: &str| {
        parameters
            .iter()
            .find(|(_, p)| p.key_matches(key))
            .map(|(i, _)| *i)
    };
    let groups = convert_multiple(c.groups.unwrap_or_default(), |g| {
        convert_group(g, &param_by_key)
    })?;
    let group_by_key = |key: &str| groups.iter().find(|g| g.key_matches(key)).map(|g| g.id);
    let data = CompartmentModelData {
        kind: {
            use crate::domain::MappingCompartment as T;
            use CompartmentKind::*;
            match c.kind {
                Controller => T::ControllerMappings,
                Main => T::MainMappings,
            }
        },
        default_group: convert_group(c.default_group.unwrap_or_default(), &param_by_key)?,
        mappings: convert_multiple(c.mappings.unwrap_or_default(), |m| {
            convert_mapping(m, group_by_key, &param_by_key)
        })?,
        parameters,
        groups,
    };
    Ok(data)
}

use crate::application::ParameterSetting;
use crate::infrastructure::api::convert::to_data::group::convert_group;
use crate::infrastructure::api::convert::to_data::parameter::convert_parameter;
use crate::infrastructure::api::convert::to_data::{
    convert_mapping, convert_multiple, ConversionResult,
};
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{CompartmentModelData, QualifiedCompartmentModelData};
use std::collections::HashMap;

pub fn convert_compartment(c: Compartment) -> ConversionResult<QualifiedCompartmentModelData> {
    let parameters: HashMap<u32, ParameterSetting> = {
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
        convert_group(g, false, &param_by_key)
    })?;
    let group_by_key = |key: &str| groups.iter().find(|g| g.key_matches(key)).map(|g| g.id);
    let data = QualifiedCompartmentModelData {
        kind: {
            use crate::domain::MappingCompartment as T;
            use CompartmentKind::*;
            match c.kind {
                Controller => T::ControllerMappings,
                Main => T::MainMappings,
            }
        },
        data: CompartmentModelData {
            default_group: Some(convert_group(
                c.default_group.unwrap_or_default(),
                true,
                &param_by_key,
            )?),
            mappings: convert_multiple(c.mappings.unwrap_or_default(), |m| {
                convert_mapping(m, group_by_key, &param_by_key)
            })?,
            parameters: parameters
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
            groups,
        },
    };
    Ok(data)
}

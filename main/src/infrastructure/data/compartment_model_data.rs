use crate::domain::MappingCompartment;
use crate::infrastructure::data::{GroupModelData, MappingModelData, ParameterData};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct CompartmentModelData {
    pub kind: MappingCompartment,
    pub default_group: GroupModelData,
    pub parameters: HashMap<u32, ParameterData>,
    pub groups: Vec<GroupModelData>,
    pub mappings: Vec<MappingModelData>,
}

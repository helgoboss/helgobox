use crate::application::{
    GroupModel, GroupProp, GroupPropVal, MappingModel, MappingProp, MappingPropVal,
    ParameterSetting,
};
use crate::domain::{GroupId, MappingId};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct CompartmentModel {
    pub parameters: HashMap<u32, ParameterSetting>,
    pub default_group: GroupModel,
    pub groups: Vec<GroupModel>,
    pub mappings: Vec<MappingModel>,
}

pub enum CompartmentPropVal {
    GroupProp(GroupId, GroupPropVal),
    MappingProp(MappingId, MappingPropVal),
}

impl CompartmentPropVal {
    pub fn prop(&self) -> CompartmentProp {
        use CompartmentProp as P;
        use CompartmentPropVal as V;
        match self {
            V::GroupProp(id, val) => P::GroupProp(*id, val.prop()),
            V::MappingProp(id, val) => P::MappingProp(*id, val.prop()),
        }
    }
}

#[derive(Copy, Clone)]
pub enum CompartmentProp {
    GroupProp(GroupId, GroupProp),
    MappingProp(MappingId, MappingProp),
}

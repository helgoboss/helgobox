use crate::application::{
    Affected, GroupModel, GroupProp, MappingCommand, MappingModel, MappingProp,
};
use crate::domain::{GroupId, MappingId, ParameterSetting};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct CompartmentModel {
    pub parameters: HashMap<u32, ParameterSetting>,
    pub default_group: GroupModel,
    pub groups: Vec<GroupModel>,
    pub mappings: Vec<MappingModel>,
}

pub enum CompartmentCommand {
    ChangeMapping(MappingId, MappingCommand),
}

pub enum CompartmentProp {
    InGroup(GroupId, Affected<GroupProp>),
    InMapping(MappingId, Affected<MappingProp>),
}

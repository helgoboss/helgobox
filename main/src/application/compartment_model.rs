use crate::application::{
    GroupCommand, GroupModel, GroupProp, MappingCommand, MappingModel, MappingProp,
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

pub enum CompartmentCommand {
    ChangeGroup(GroupId, GroupCommand),
    ChangeMapping(MappingId, MappingCommand),
}

#[derive(Copy, Clone)]
pub enum CompartmentProp {
    /// `None` means that the complete group is affected.
    GroupProp(GroupId, Option<GroupProp>),
    /// `None` means that the complete mapping is affected.
    MappingProp(MappingId, Option<MappingProp>),
}

use crate::application::{
    Affected, GroupModel, GroupProp, MappingCommand, MappingModel, MappingProp,
};
use crate::domain::{CompartmentParamIndex, GroupId, MappingId, ParamSetting};

#[derive(Clone, Debug)]
pub struct CompartmentModel {
    pub parameters: Vec<(CompartmentParamIndex, ParamSetting)>,
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

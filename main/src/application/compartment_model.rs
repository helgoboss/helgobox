use crate::application::{
    Affected, GroupModel, GroupProp, MappingCommand, MappingModel, MappingProp,
};
use crate::domain::{CompartmentParamIndex, GroupId, MappingId, ParamSetting};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct CompartmentModel {
    pub parameters: Vec<(CompartmentParamIndex, ParamSetting)>,
    pub default_group: GroupModel,
    pub groups: Vec<GroupModel>,
    pub mappings: Vec<MappingModel>,
    /// At the moment, custom data is only used in the controller compartment.
    pub custom_data: HashMap<String, serde_json::Value>,
}

pub enum CompartmentCommand {
    ChangeMapping(MappingId, MappingCommand),
}

pub enum CompartmentProp {
    InGroup(GroupId, Affected<GroupProp>),
    InMapping(MappingId, Affected<MappingProp>),
}

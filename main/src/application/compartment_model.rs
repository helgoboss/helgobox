use crate::application::{
    Affected, GroupModel, GroupProp, MappingCommand, MappingModel, MappingProp,
};
use crate::domain::{CompartmentParamIndex, GroupId, MappingId, ParamSetting};
use base::hash_util::NonCryptoHashMap;

#[derive(Clone, Debug)]
pub struct CompartmentModel {
    pub parameters: Vec<(CompartmentParamIndex, ParamSetting)>,
    pub default_group: GroupModel,
    pub groups: Vec<GroupModel>,
    pub mappings: Vec<MappingModel>,
    pub common_lua: String,
    pub custom_data: NonCryptoHashMap<String, serde_json::Value>,
    pub notes: String,
}

pub enum CompartmentCommand {
    SetNotes(String),
    SetCommonLua(String),
    ChangeMapping(MappingId, Box<MappingCommand>),
}

pub enum CompartmentProp {
    Notes,
    CommonLua,
    InGroup(GroupId, Affected<GroupProp>),
    InMapping(MappingId, Affected<MappingProp>),
}

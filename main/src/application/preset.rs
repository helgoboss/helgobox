use crate::application::{GroupModel, MappingModel, SharedGroup, SharedMapping};
use std::fmt;
use std::fmt::Debug;

pub trait Preset: Clone + Debug {
    fn id(&self) -> &str;
    fn default_group(&self) -> &GroupModel;
    fn groups(&self) -> &Vec<GroupModel>;
    fn mappings(&self) -> &Vec<MappingModel>;
}

pub trait PresetManager: fmt::Debug {
    type PresetType;

    fn find_by_id(&self, id: &str) -> Option<Self::PresetType>;

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool;

    fn groups_are_dirty(
        &self,
        id: &str,
        default_group: &SharedGroup,
        groups: &[SharedGroup],
    ) -> bool;
}

use crate::domain::Tag;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct MappingScope {
    /// The mapping in question should have at least one of these tags.
    pub tags: HashSet<Tag>,
}

impl MappingScope {
    pub fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }
}

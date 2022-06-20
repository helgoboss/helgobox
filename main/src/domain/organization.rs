use crate::domain::{Exclusivity, Tag};
use std::collections::HashSet;
use std::hash::Hash;

#[derive(Clone, Debug, PartialEq)]
pub struct TagScope {
    pub tags: HashSet<Tag>,
}

impl TagScope {
    pub fn determine_enable_disable_change(
        &self,
        exclusivity: Exclusivity,
        tags: &[Tag],
        is_enable: bool,
    ) -> Option<bool> {
        use Exclusivity::*;
        if exclusivity == Exclusive || (exclusivity == ExclusiveOnOnly && is_enable) {
            // Exclusive
            if self.has_tags() {
                // Set mappings that match the scope tags and unset all others as long as they
                // have tags!
                if tags.is_empty() {
                    None
                } else {
                    Some(has_any_of(&self.tags, tags))
                }
            } else {
                // Scope doesn't define any tags. Unset *all* mappings as long as
                // they have tags.
                if tags.is_empty() {
                    None
                } else {
                    Some(false)
                }
            }
        } else {
            // Non-exclusive
            if !self.has_tags() || has_any_of(&self.tags, tags) {
                // Non-exclusive, so we just add to or remove from mappings that are
                // currently active (= relative).
                Some(true)
            } else {
                // Don't touch mappings that don't match the tags.
                None
            }
        }
    }

    pub fn any_tag_matches(&self, other_tags: &[Tag]) -> bool {
        has_any_of(&self.tags, other_tags)
    }

    pub fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }
}

fn has_any_of<'a, T: 'a + Eq + Hash>(
    self_tags: &HashSet<T>,
    other_tags: impl IntoIterator<Item = &'a T>,
) -> bool {
    other_tags.into_iter().any(|t| self_tags.contains(t))
}

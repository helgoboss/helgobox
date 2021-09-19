use crate::domain::{Exclusivity, Tag};
use std::collections::HashSet;
use std::hash::Hash;

#[derive(Clone, Debug, PartialEq)]
pub struct TagScope {
    pub tags: HashSet<Tag>,
}

impl TagScope {
    pub fn determine_change(&self, exclusivity: Exclusivity, tags: &[Tag]) -> Option<bool> {
        match exclusivity {
            Exclusivity::Exclusive => {
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
            }
            Exclusivity::NonExclusive => {
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

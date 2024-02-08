use crate::domain::{MappingId, Tag, TagScope, VirtualMappingSnapshotIdForLoad};
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use base::{convert_to_identifier, SmallAsciiString};
use helgoboss_learn::AbsoluteValue;
use std::str::FromStr;

#[derive(Debug, Default)]
pub struct MappingSnapshotContainer {
    snapshots: NonCryptoHashMap<MappingSnapshotId, MappingSnapshot>,
    active_snapshot_id_by_tag: NonCryptoHashMap<Tag, MappingSnapshotId>,
}

impl MappingSnapshotContainer {
    /// Creates the container.
    pub fn new(
        snapshots: NonCryptoHashMap<MappingSnapshotId, MappingSnapshot>,
        active_snapshot_id_by_tag: NonCryptoHashMap<Tag, MappingSnapshotId>,
    ) -> Self {
        Self {
            snapshots,
            active_snapshot_id_by_tag,
        }
    }

    /// Updates the contents of the given snapshot.
    pub fn update_snapshot(&mut self, id: MappingSnapshotId, snapshot: MappingSnapshot) {
        self.snapshots.insert(id, snapshot);
    }

    /// Returns the last loaded snapshot ID for the given tags.
    ///
    /// If there's no record of a last loaded snapshot for any of the given tags, it returns `None`.
    ///
    /// If the last loaded snapshot IDs differ between the tags, it also returns `None`.
    pub fn last_loaded_snapshot_id(&self, scope: &TagScope) -> Option<MappingSnapshotId> {
        let active_snapshot_ids: NonCryptoHashSet<_> = scope
            .tags
            .iter()
            .map(|tag| self.active_snapshot_id_by_tag.get(tag))
            .collect();
        if active_snapshot_ids.len() > 1 {
            // Active snapshots differ.
            return None;
        }
        let single_active_snapshot_id = active_snapshot_ids.iter().next()?.cloned()?;
        Some(single_active_snapshot_id)
    }

    /// Marks the given snapshot as the active one for all tags in the given scope.
    pub fn mark_snapshot_active(
        &mut self,
        tag_scope: &TagScope,
        snapshot_id: &VirtualMappingSnapshotIdForLoad,
    ) {
        for tag in &tag_scope.tags {
            match snapshot_id {
                VirtualMappingSnapshotIdForLoad::Initial => {
                    self.active_snapshot_id_by_tag.remove(tag);
                }
                VirtualMappingSnapshotIdForLoad::ById(id) => {
                    self.active_snapshot_id_by_tag
                        .insert(tag.clone(), id.clone());
                }
            }
        }
    }

    /// Returns `true` if for all tags in the given scope the given snapshot is the currently
    /// active one.
    pub fn snapshot_is_active(
        &self,
        tag_scope: &TagScope,
        snapshot_id: &VirtualMappingSnapshotIdForLoad,
    ) -> bool {
        tag_scope.tags.iter().all(|tag| {
            if let Some(active_snapshot_id) = self.active_snapshot_id_by_tag.get(tag) {
                if let VirtualMappingSnapshotIdForLoad::ById(snapshot_id) = snapshot_id {
                    snapshot_id == active_snapshot_id
                } else {
                    false
                }
            } else {
                matches!(snapshot_id, VirtualMappingSnapshotIdForLoad::Initial)
            }
        })
    }

    pub fn active_snapshot_id_by_tag(&self) -> &NonCryptoHashMap<Tag, MappingSnapshotId> {
        &self.active_snapshot_id_by_tag
    }

    /// Returns the snapshot contents associated with the given snapshot ID.
    pub fn find_snapshot_by_id(&self, id: &MappingSnapshotId) -> Option<&MappingSnapshot> {
        self.snapshots.get(id)
    }

    /// Returns all snapshots in this container.
    pub fn snapshots(&self) -> impl Iterator<Item = (&MappingSnapshotId, &MappingSnapshot)> {
        self.snapshots.iter()
    }
}

#[derive(Debug, Default)]
pub struct MappingSnapshot {
    target_values: NonCryptoHashMap<MappingId, AbsoluteValue>,
}

impl MappingSnapshot {
    pub fn new(target_values: NonCryptoHashMap<MappingId, AbsoluteValue>) -> Self {
        Self { target_values }
    }

    pub fn find_target_value_by_mapping_id(&self, id: MappingId) -> Option<AbsoluteValue> {
        self.target_values.get(&id).copied()
    }

    pub fn target_values(&self) -> impl Iterator<Item = (MappingId, AbsoluteValue)> + '_ {
        self.target_values
            .iter()
            .map(|(id, target_value)| (*id, *target_value))
    }
}

#[derive(
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Debug,
    Hash,
    derive_more::Display,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
)]
pub struct MappingSnapshotId(SmallAsciiString);

impl FromStr for MappingSnapshotId {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let small_ascii_string = convert_to_identifier(text)?;
        Ok(Self(small_ascii_string))
    }
}

use crate::domain::{convert_to_identifier, MappingId, SmallAsciiString};
use helgoboss_learn::AbsoluteValue;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Default)]
pub struct MappingSnapshotContainer {
    snapshots: HashMap<MappingSnapshotId, MappingSnapshot>,
}

impl MappingSnapshotContainer {
    pub fn new(snapshots: HashMap<MappingSnapshotId, MappingSnapshot>) -> Self {
        Self { snapshots }
    }

    pub fn update_snapshot(&mut self, id: MappingSnapshotId, snapshot: MappingSnapshot) {
        self.snapshots.insert(id, snapshot);
    }

    pub fn find_snapshot_by_id(&self, id: &MappingSnapshotId) -> Option<&MappingSnapshot> {
        self.snapshots.get(id)
    }

    pub fn snapshots(&self) -> impl Iterator<Item = (&MappingSnapshotId, &MappingSnapshot)> {
        self.snapshots.iter()
    }
}

#[derive(Debug, Default)]
pub struct MappingSnapshot {
    target_values: HashMap<MappingId, AbsoluteValue>,
}

impl MappingSnapshot {
    pub fn new(target_values: HashMap<MappingId, AbsoluteValue>) -> Self {
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

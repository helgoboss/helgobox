//! Usually we use Protocol Buffers for the runtime API but there are a few things that are
//! not performance-critical and better expressed in a Rust-first manner.
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SimpleMappingContainer {
    pub mappings: Vec<SimpleMapping>,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SimpleMapping {
    pub source: SimpleSource,
    pub target: SimpleMappingTarget,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SimpleSource {
    Note(NoteSource),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct NoteSource {
    pub channel: u8,
    pub number: u8,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SimpleMappingTarget {
    TriggerMatrix,
    TriggerColumn(ColumnAddress),
    TriggerRow(RowAddress),
    TriggerSlot(SlotAddress),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ColumnAddress {
    pub index: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct RowAddress {
    pub index: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct SlotAddress {
    pub column_index: u32,
    pub row_index: u32,
}

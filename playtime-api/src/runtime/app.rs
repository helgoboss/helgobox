//! Usually we use Protocol Buffers for the runtime app API but there are a few things that are
//! not performance-critical and better expressed in a Rust-first manner.
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

// We don't really need a tagged enum here but it's an easy way to transmit the event as a
// JSON object (vs. just a string) ... which is better for some clients. Plus, we might want
// to deliver some additional payloads in future.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InfoEvent {
    RecordedMatrixSequence,
    DiscardedMatrixSequenceBecauseEmpty,
    RemovedMatrixSequence,
    WroteMatrixSequenceToArrangement,
}

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
    MoreComplicated,
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
    pub index: usize,
}

impl Display for ColumnAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column {}", self.index + 1)
    }
}

impl ColumnAddress {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct RowAddress {
    pub index: usize,
}

impl Display for RowAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Row {}", self.index + 1)
    }
}

impl RowAddress {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct SlotAddress {
    pub column_index: usize,
    pub row_index: usize,
}

impl SlotAddress {
    pub fn new(column: usize, row: usize) -> Self {
        Self {
            column_index: column,
            row_index: row,
        }
    }

    pub fn column(&self) -> usize {
        self.column_index
    }

    pub fn row(&self) -> usize {
        self.row_index
    }
}

impl Display for SlotAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Slot {}/{}", self.column_index + 1, self.row_index + 1)
    }
}

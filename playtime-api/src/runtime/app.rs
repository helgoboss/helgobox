//! Usually we use Protocol Buffers for the runtime app API but there are a few things that are
//! not performance-critical and better expressed in a Rust-first manner.
use crate::persistence::{ColumnAddress, RowAddress, SlotAddress};
use serde::{Deserialize, Serialize};

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
    SmartRecord,
    EnterSilenceModeOrPlayIgnited,
    SequencerRecordOnOffState,
    SequencerPlayOnOffState,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CellAddress {
    pub column_index: Option<usize>,
    pub row_index: Option<usize>,
}

impl CellAddress {
    pub fn new(column_index: Option<usize>, row_index: Option<usize>) -> Self {
        Self {
            column_index,
            row_index,
        }
    }

    pub fn matrix() -> Self {
        Self::new(None, None)
    }

    pub fn column(column_index: usize) -> Self {
        Self {
            column_index: Some(column_index),
            row_index: None,
        }
    }

    pub fn row(row_index: usize) -> Self {
        Self {
            column_index: None,
            row_index: Some(row_index),
        }
    }

    pub fn slot(column_index: usize, row_index: usize) -> Self {
        Self {
            column_index: Some(column_index),
            row_index: Some(row_index),
        }
    }
}

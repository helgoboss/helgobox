//! Usually we use Protocol Buffers for the runtime app API but there are a few things that are
//! not performance-critical and better expressed in a Rust-first manner.
use crate::persistence::{ColumnAddress, RowAddress, SlotAddress};
use serde::{Deserialize, Serialize};

// We don't really need a tagged enum here but it's an easy way to transmit the event as a
// JSON object (vs. just a string) ... which is better for some clients. Plus, we might want
// to deliver some additional payloads in the future.
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InfoEvent {
    Generic(GenericInfoEvent),
    RecordedMatrixSequence,
    DiscardedMatrixSequenceBecauseEmpty,
    RemovedMatrixSequence,
    WroteMatrixSequenceToArrangement,
}

impl InfoEvent {
    /// Creates an info event with a generic message. This is displayed as toast in the app.
    pub fn generic(message: impl Into<String>) -> Self {
        Self::Generic(GenericInfoEvent {
            message: message.into(),
        })
    }

    /// Creates an info event that would ideally be treated like a warning on the app side.
    pub fn warning(message: impl Into<String>) -> Self {
        // In the future, this could set a special error flag, so that it could be displayed
        // in a different way in the app.
        Self::generic(message)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct GenericInfoEvent {
    pub message: String,
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
    TapTempo,
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

    pub fn to_slot_address(&self) -> Option<SlotAddress> {
        Some(SlotAddress::new(self.column_index?, self.row_index?))
    }
}
